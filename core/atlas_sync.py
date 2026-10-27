"""
Atlas sync: swaps notes and timer time with the Atlas app (phone or PC) over Wi-Fi.

Luminescent only ever hosts. It shows this computer's address and a 6 digit code, Atlas connects, compares
both sides and shows every change in both directions before anything is written. Timer time adds up: an
hour on the PC timer and 20 minutes on the phone shows 1h 20m on both. Notes go both ways, to-do lists
travel as checklist notes ("- [x] milk") and come back as to-do lists.

The wire format is Atlas's own (see Atlas's sync/SyncWire.kt):
  one TCP connection per request, each way one frame:  b"ATLS" | 1 byte type | 4 byte big endian length | body
  type 0 is JSON sealed with AES-GCM (12 byte nonce first, tag last) under PBKDF2-SHA256(code, "atlas-sync-1",
  120 000 rounds); type 1 is a plain error message. A wrong code can not open the frame.

Every row is a record {kind, uid, fields}. The device that connects does all the comparing, so this side only
has to list what it has and write what it is sent. To keep fingerprints stable, the exact fields of every
record as last synced are kept in data/atlas_sync.json ("shadow") and sent back unchanged until the note or
timer they came from changes here.
"""
import base64
import hashlib
import json
import os
import re
import secrets
import socket
import struct
import threading
import time
import uuid
from datetime import datetime

from cryptography.hazmat.primitives.ciphers.aead import AESGCM

PORT = 47830
MAGIC = b"ATLS"
SEALED, ERROR = 0, 1
SALT = b"atlas-sync-1"
ROUNDS = 120_000
MAX_FRAME = 512 * 1024 * 1024
PROTOCOL = 1

# what Luminescent can keep, Atlas leaves everything else out of the comparison
ACCEPTS = {"kinds": ["item", "log"], "items": ["NOTE", "TRACKER:TIMER"]}
IGNORED_IN_FINGERPRINT = {"updated", "created"}


# ── wire ────────────────────────────────────────────────────────────────────

class SyncError(Exception):
    pass


def new_code() -> str:
    return f"{secrets.randbelow(1_000_000):06d}"


def derive_key(code: str) -> bytes:
    return hashlib.pbkdf2_hmac("sha256", code.encode(), SALT, ROUNDS, 32)


def seal(key: bytes, plain: bytes) -> bytes:
    nonce = os.urandom(12)
    return nonce + AESGCM(key).encrypt(nonce, plain, None)


def unseal(key: bytes, data: bytes) -> bytes:
    if len(data) <= 12:
        raise ValueError("too short")
    return AESGCM(key).decrypt(data[:12], data[12:], None)


def _read_exact(sock, n: int) -> bytes:
    buf = bytearray()
    while len(buf) < n:
        part = sock.recv(min(65536, n - len(buf)))
        if not part:
            raise SyncError("The other device hung up")
        buf += part
    return bytes(buf)


def read_frame(sock, progress=None) -> tuple[int, bytes]:
    head = _read_exact(sock, 9)
    if head[:4] != MAGIC:
        raise SyncError("That is not an Atlas device")
    kind, length = head[4], struct.unpack(">i", head[5:9])[0]
    if length < 0 or length > MAX_FRAME:
        raise SyncError("Too big")
    body = bytearray()
    while len(body) < length:
        part = sock.recv(min(65536, length - len(body)))
        if not part:
            raise SyncError("The other device hung up")
        body += part
        if progress:
            progress(len(body) / length)
    return kind, bytes(body)


def write_frame(sock, kind: int, body: bytes):
    sock.sendall(MAGIC + bytes([kind]) + struct.pack(">i", len(body)) + body)


# ── fingerprints, the same text Atlas makes for the same data ───────────────

def canonical(value) -> str:
    if isinstance(value, dict):
        return "{" + ",".join(json.dumps(k, ensure_ascii=False) + ":" + canonical(value[k]) for k in sorted(value)) + "}"
    if isinstance(value, list):
        return "[" + ",".join(canonical(v) for v in value) + "]"
    return json.dumps(value, ensure_ascii=False)


def fingerprint(record: dict) -> str:
    fields = {k: v for k, v in record["fields"].items() if k not in IGNORED_IN_FINGERPRINT}
    digest = hashlib.sha256(record["kind"].encode() + canonical(fields).encode()).digest()
    return base64.urlsafe_b64encode(digest).decode().rstrip("=")[:22]


def baseline_digest(baseline: dict) -> str:
    digest = hashlib.sha256(canonical(baseline).encode()).digest()
    return base64.urlsafe_b64encode(digest).decode().rstrip("=")


def key_of(record: dict) -> str:
    return f"{record['kind']}/{record['uid']}"


# ── turning Luminescent's notes and timers into records and back ────────────

def now_ms() -> int:
    return int(time.time() * 1000)


def colour_to_int(colour: str):
    """'#E5484D' -> the signed ARGB int Atlas keeps."""
    if not colour or not re.fullmatch(r"#?[0-9a-fA-F]{6}", colour):
        return None
    value = 0xFF000000 | int(colour.lstrip("#"), 16)
    return value - (1 << 32)


def int_to_colour(value) -> str:
    if value is None:
        return ""
    return f"#{int(value) & 0xFFFFFF:06X}"


TODO_LINE = re.compile(r"^- \[( |x|X)\] (.*?)(?: \(due (\d{4}-\d{2}-\d{2}(?:[ T]\d{2}:\d{2})?)\))?$")


def note_text(note: dict) -> str:
    if note.get("type") == "todo":
        lines = []
        for it in note.get("items", []):
            due = f" (due {it['due'].replace('T', ' ')[:16]})" if it.get("due") else ""
            lines.append(f"- [{'x' if it.get('checked') else ' '}] {it.get('text', '')}{due}")
        return "\n".join(lines)
    return note.get("content", "")


def is_checklist(text: str) -> bool:
    lines = [ln for ln in text.splitlines() if ln.strip()]
    return bool(lines) and all(TODO_LINE.match(ln) for ln in lines)


def checklist_items(text: str, old_items: list) -> list:
    """Checklist lines back into to-do items, keeping the ids of items whose text did not change."""
    by_text = {it.get("text"): it for it in old_items}
    items = []
    for ln in text.splitlines():
        m = TODO_LINE.match(ln)
        if not m:
            continue
        old = by_text.get(m.group(2), {})
        due = (m.group(3) or "").replace(" ", "T")
        items.append({"id": old.get("id") or uuid.uuid4().hex[:10], "text": m.group(2),
                      "checked": m.group(1) in "xX", "due": due})
    return items


def item_fields(kind_of_item: str, name: str) -> dict:
    t = now_ms()
    return {"type": kind_of_item, "parentId": None, "name": name, "color": None, "icon": None, "sortOrder": 0,
            "pinned": False, "created": t, "updated": t, "archived": False, "deletedAt": None}


def note_signature(fields: dict) -> tuple:
    """What a note looks like from Luminescent's side, to tell whether it changed here."""
    text = (fields.get("note") or {}).get("text", "")
    return (fields.get("name", ""), text, int_to_colour(fields.get("color")), bool(fields.get("pinned")),
            tuple(sorted(fields.get("tags") or [])))


def local_signature(note: dict) -> tuple:
    return (note.get("name", ""), note_text(note), (note.get("colour") or "").upper(), bool(note.get("pinned")),
            tuple(sorted(t.lower() for t in note.get("tags", []))))


class Store:
    """Everything sync remembers, in data/atlas_sync.json."""

    def __init__(self, path: str):
        self.path = path
        try:
            with open(path, "r", encoding="utf-8") as f:
                self.state = json.load(f)
        except (OSError, ValueError):
            self.state = {}
        self.state.setdefault("device_id", uuid.uuid4().hex)
        self.state.setdefault("peers", {})
        self.state.setdefault("shadow", {})      # key -> record as last synced
        self.state.setdefault("timers", {})      # timer label -> tracker uid
        self.state.setdefault("alias", {})       # a uid Luminescent made -> the one both sides use now
        self.state.setdefault("gone", [])        # timer log uids deleted from Atlas, never offered again
        self.last_out: dict = {}

    def save(self):
        tmp = self.path + ".tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(self.state, f)
        os.replace(tmp, self.path)

    def baseline(self, peer: str) -> dict:
        return self.state["peers"].get(peer, {}).get("baseline", {})

    def _uid(self, uid: str) -> str:
        return self.state["alias"].get(uid, uid)

    def note_uid(self, note: dict) -> str:
        return note.get("atlas_uid") or self._uid("lum-" + note["id"])

    def timer_uid(self, label: str) -> str:
        if label not in self.state["timers"]:
            slug = re.sub(r"[^a-z0-9]+", "-", label.lower()).strip("-") or "timer"
            self.state["timers"][label] = f"lum-timer-{slug}-{uuid.uuid4().hex[:6]}"
        return self._uid(self.state["timers"][label])

    # ── what this side has ──
    def records(self, notes: list, activity: list) -> list:
        shadow = self.state["shadow"]
        out = dict(shadow)
        gone = set(self.state["gone"])

        seen = set()
        for note in notes:
            uid = self.note_uid(note)
            key = f"item/{uid}"
            seen.add(key)
            base = shadow.get(key)
            if base and note_signature(base["fields"]) == local_signature(note):
                continue                                     # unchanged here, the shadow copy goes as it is
            fields = dict(base["fields"]) if base else item_fields("NOTE", note.get("name", ""))
            fields.update({"name": note.get("name", ""), "color": colour_to_int(note.get("colour")),
                           "pinned": bool(note.get("pinned")), "note": {"text": note_text(note)}, "updated": now_ms()})
            tags = sorted({t.lower() for t in note.get("tags", [])})
            if tags:
                fields["tags"] = tags
            else:
                fields.pop("tags", None)
            out[key] = {"kind": "item", "uid": uid, "fields": fields}
        # a note that was synced and is now gone here was deleted here, unless Atlas only put it in its trash
        for key, rec in shadow.items():
            f = rec["fields"]
            if rec["kind"] == "item" and f.get("type") == "NOTE" and key not in seen and f.get("deletedAt") is None:
                out.pop(key, None)

        for line in activity:
            if line.get("kind") != "timer" or not line.get("secs") or not line.get("label"):
                continue
            uid = line.get("atlas") or self._uid("lum-act-" + hashlib.sha1(
                json.dumps(line, sort_keys=True, ensure_ascii=False).encode()).hexdigest()[:16])
            key = f"log/{uid}"
            if uid in gone or key in out:
                continue
            tracker = self.timer_uid(line["label"])
            if f"item/{tracker}" not in out:
                fields = item_fields("TRACKER", line["label"])
                fields["tracker"] = {"kind": "TIMER", "unit": None, "dailyGoal": None, "showOnHome": True,
                                     "aggregate": None, "showOnCalendar": True}
                out[f"item/{tracker}"] = {"kind": "item", "uid": tracker, "fields": fields}
            secs = int(round(float(line["secs"])))
            try:
                ended = datetime.fromisoformat(line["t"]).timestamp()
            except (KeyError, ValueError):
                continue
            out[key] = {"kind": "log", "uid": uid, "fields": {
                "trackerId": tracker, "timestamp": int((ended - secs) * 1000), "value": float(secs),
                "durationSeconds": secs, "note": None}}

        self.last_out = out
        return list(out.values())

    # ── writing what Atlas sent ──
    def apply(self, notes: list, upserts: list, deletes: list, relinks: list) -> list:
        """Changes `notes` in place and returns activity lines to add for timer time logged in Atlas."""
        shadow = self.state["shadow"]
        for r in relinks:
            self.state["alias"][r["from"]] = r["to"]
            for label, uid in list(self.state["timers"].items()):
                if uid == r["from"]:
                    self.state["timers"][label] = r["to"]
            old = shadow.pop(f"{r['kind']}/{r['from']}", None)
            if old:
                shadow[f"{r['kind']}/{r['to']}"] = dict(old, uid=r["to"])

        by_uid = {self.note_uid(n): n for n in notes}
        for d in deletes:
            key = f"{d['kind']}/{d['uid']}"
            shadow.pop(key, None)
            if d["kind"] == "item" and d["uid"] in by_uid:
                notes.remove(by_uid.pop(d["uid"]))
            elif d["kind"] == "log":
                self.state["gone"].append(d["uid"])

        new_lines = []
        known = list(self.last_out.values()) + list(shadow.values()) + upserts
        names = {r["uid"]: r["fields"].get("name", "") for r in known if r["kind"] == "item"}
        for rec in upserts:
            f = rec["fields"]
            if rec["kind"] == "item" and f.get("type") == "NOTE":
                note = by_uid.get(rec["uid"])
                if f.get("deletedAt") is not None:
                    if note is not None:
                        notes.remove(note)
                        by_uid.pop(rec["uid"], None)
                else:
                    if note is None:
                        note = {"id": uuid.uuid4().hex[:10], "atlas_uid": rec["uid"], "items": [], "content": "",
                                "type": "note", "tags": []}
                        notes.append(note)
                        by_uid[rec["uid"]] = note
                    text = (f.get("note") or {}).get("text", "")
                    note["name"] = f.get("name", "") or "Note"
                    note["colour"] = int_to_colour(f.get("color"))
                    note["pinned"] = bool(f.get("pinned"))
                    note["tags"] = list(f.get("tags") or [])
                    if note.get("type") == "todo" or (not note.get("content") and is_checklist(text)):
                        note["type"] = "todo"
                        note["items"] = checklist_items(text, note.get("items", []))
                    else:
                        note["content"] = text
            elif rec["kind"] == "item" and f.get("type") == "TRACKER":
                self.state["timers"].setdefault(f.get("name", ""), rec["uid"])
            elif rec["kind"] == "log" and not rec["uid"].startswith("lum-act-"):
                secs = int(f.get("durationSeconds") or 0)
                label = names.get(f.get("trackerId"), "")
                if secs > 0 and label:
                    ended = datetime.fromtimestamp((int(f["timestamp"]) / 1000) + secs)
                    new_lines.append({"t": ended.isoformat(timespec="seconds"), "kind": "timer", "label": label,
                                      "secs": float(secs), "atlas": rec["uid"]})
            shadow[key_of(rec)] = rec
        return new_lines

    def remember(self, peer: str, name: str, kind: str, baseline: dict, upserts: list):
        """After a sync both sides keep the same picture, the shadow becomes exactly that."""
        sent = {key_of(r): r for r in upserts}
        shadow = {}
        for key in baseline:
            rec = sent.get(key) or self.last_out.get(key) or self.state["shadow"].get(key)
            if rec:
                shadow[key] = rec
        self.state["shadow"] = shadow
        self.state["peers"][peer] = {"name": name, "kind": kind, "last": now_ms(), "baseline": baseline}
        self.save()


# ── hosting ─────────────────────────────────────────────────────────────────

def local_addresses() -> list:
    """This computer's addresses on the local network."""
    found = []
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        s.connect(("192.168.255.255", 1))           # nothing is sent, it just picks the outgoing interface
        found.append(s.getsockname()[0])
        s.close()
    except OSError:
        pass
    try:
        for info in socket.getaddrinfo(socket.gethostname(), None, socket.AF_INET):
            found.append(info[4][0])
    except OSError:
        pass
    private = [a for a in dict.fromkeys(found) if a.startswith(("10.", "192.168.")) or re.match(r"172\.(1[6-9]|2\d|3[01])\.", a)]
    return private


class Host:
    """Listens for Atlas while hosting is on. `local()` returns (notes, activity) and `write(fn)` runs fn
    with the notes list where it is safe to change it (the Qt thread), both are given by the module."""

    def __init__(self, store: Store, name: str, local, write, status=lambda text, frac=None: None):
        self.store, self.name, self.local, self.write, self.status = store, name, local, write, status
        self.code = None
        self._key = None
        self._sock = None
        self._thread = None
        self._wrong = 0
        self._applied = set()

    def start(self, port: int = PORT) -> str:
        self.code = new_code()
        self._key = derive_key(self.code)
        self._wrong = 0
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        sock.bind(("0.0.0.0", port))
        sock.listen(2)
        self._sock = sock
        self._thread = threading.Thread(target=self._loop, name="atlas-sync", daemon=True)
        self._thread.start()
        self.status("Waiting for Atlas…")
        return self.code

    def stop(self):
        sock, self._sock = self._sock, None
        if sock:
            try:
                sock.close()
            except OSError:
                pass
        self.code = None

    @property
    def running(self) -> bool:
        return self._sock is not None

    def _loop(self):
        while self._sock is not None:
            try:
                conn, _ = self._sock.accept()
            except OSError:
                break
            with conn:
                conn.settimeout(300)
                try:
                    self._serve(conn)
                except Exception as e:           # one bad request never stops hosting
                    try:
                        write_frame(conn, ERROR, str(e).encode() or b"Something went wrong")
                    except OSError:
                        pass

    def _serve(self, conn):
        kind, body = read_frame(conn)
        if self._wrong >= 10:
            write_frame(conn, ERROR, b"Too many wrong codes. Turn hosting off and on again in Luminescent.")
            return
        try:
            request = json.loads(unseal(self._key, body)) if kind == SEALED else None
        except Exception:
            request = None
        if request is None:
            self._wrong += 1
            write_frame(conn, ERROR, b"Wrong code")
            if self._wrong >= 10:
                self.status("Too many wrong codes, hosting stopped")
                self.stop()
            return
        reply = self.answer(request)
        write_frame(conn, SEALED, seal(self._key, json.dumps(reply, ensure_ascii=False).encode()))

    def answer(self, request: dict) -> dict:
        action = request.get("action")
        who = request.get("name") or "Atlas"
        hello = {"app": "luminescent", "protocol": PROTOCOL, "deviceId": self.store.state["device_id"],
                 "name": self.name, "kind": "PC", "accepts": ACCEPTS}
        if action == "hello":
            self.status(f"{who} connected")
            return hello
        if action == "records":
            self.status(f"Comparing with {who}…")
            notes, activity = self.local()
            records = self.store.records(notes, activity)
            return {"records": records, "baseDigest": baseline_digest(self.store.baseline(request.get("peerId", "")))}
        if action == "apply":
            sync_id = request.get("syncId", "")
            if sync_id and sync_id in self._applied:
                return {"ok": True}
            self.status(f"Saving what {who} sent…", 1.0)
            upserts, deletes = request.get("upserts", []), request.get("deletes", [])

            def change(notes):
                return self.store.apply(notes, upserts, deletes, request.get("relinks", []))

            self.write(change)
            self.store.remember(request.get("peerId", ""), who, request.get("kind", "Device"),
                                request.get("baseline", {}), upserts)
            if sync_id:
                self._applied.add(sync_id)
            n = len(upserts) + len(deletes)
            self.status(f"Synced with {who}" + (f" · {n} change{'s' if n != 1 else ''} received" if n else " · nothing new"))
            return {"ok": True}
        raise SyncError("Luminescent doesn't know that request")
