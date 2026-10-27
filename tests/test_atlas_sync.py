"""
Checks the Atlas sync host without Qt: python tests/test_atlas_sync.py
A small stand-in for the Atlas app connects over a real socket, like the phone would.
"""
import json
import os
import socket
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from core import atlas_sync as s  # noqa: E402

PORT = 47931


def call(code, request):
    key = s.derive_key(code)
    with socket.create_connection(("127.0.0.1", PORT), timeout=10) as conn:
        s.write_frame(conn, s.SEALED, s.seal(key, json.dumps(request).encode()))
        kind, body = s.read_frame(conn)
        if kind == s.ERROR:
            raise s.SyncError(body.decode())
        return json.loads(s.unseal(key, body))


def main():
    home = tempfile.mkdtemp()
    store = s.Store(os.path.join(home, "atlas_sync.json"))
    notes = [{"id": "n1", "name": "Ideas", "type": "note", "content": "build a boat", "items": [], "colour": "#E5484D",
              "pinned": True, "tags": ["Work"]},
             {"id": "n2", "name": "Shop", "type": "todo", "content": "", "tags": [], "colour": "", "pinned": False,
              "items": [{"id": "a", "text": "milk", "checked": True, "due": ""}, {"id": "b", "text": "eggs", "checked": False, "due": ""}]}]
    activity = [{"t": "2026-10-26T10:30:00", "kind": "timer", "label": "Coding", "secs": 1800.0},
                {"t": "2026-10-26T11:00:00", "kind": "run", "label": "Firefox"}]
    added_lines = []

    def write(fn):
        added_lines.extend(fn(notes))

    host = s.Host(store, "test-pc", local=lambda: (notes, activity + added_lines), write=write)
    code = host.start(PORT)
    try:
        hello = call(code, {"action": "hello", "name": "Pixel", "kind": "Phone", "deviceId": "phone"})
        assert hello["app"] == "luminescent" and hello["accepts"]["items"] == ["NOTE", "TRACKER:TIMER"]

        try:
            call("000000" if code != "000000" else "111111", {"action": "hello"})
            raise AssertionError("a wrong code got in")
        except s.SyncError as e:
            assert str(e) == "Wrong code"

        got = call(code, {"action": "records", "peerId": "phone", "name": "Pixel"})
        recs = {s.key_of(r): r for r in got["records"]}
        kinds = sorted(r["kind"] + ":" + r["fields"].get("type", "") for r in recs.values())
        assert kinds == ["item:NOTE", "item:NOTE", "item:TRACKER", "log:"], kinds
        shop = recs["item/lum-n2"]["fields"]["note"]["text"]
        assert shop == "- [x] milk\n- [ ] eggs", shop
        log = next(r for r in recs.values() if r["kind"] == "log")
        assert log["fields"]["durationSeconds"] == 1800 and log["fields"]["value"] == 1800.0

        # the phone sends a new note, 20 minutes on the same timer, and an edit to the shopping list
        tracker_uid = log["fields"]["trackerId"]
        shop_edit = json.loads(json.dumps(recs["item/lum-n2"]))
        shop_edit["fields"]["note"]["text"] = "- [x] milk\n- [x] eggs\n- [ ] bread"
        new_note = {"kind": "item", "uid": "atlas-note-1", "fields": dict(s.item_fields("NOTE", "From phone"), note={"text": "hi"})}
        phone_log = {"kind": "log", "uid": "atlas-log-1", "fields": {"trackerId": tracker_uid, "timestamp": 1_761_480_000_000,
                                                                     "value": 1200.0, "durationSeconds": 1200, "note": None}}
        upserts = [shop_edit, new_note, phone_log]
        final = {k: s.fingerprint(r) for k, r in recs.items()}
        for r in upserts:
            final[s.key_of(r)] = s.fingerprint(r)
        ok = call(code, {"action": "apply", "syncId": "x1", "peerId": "phone", "name": "Pixel", "kind": "Phone",
                         "upserts": upserts, "deletes": [], "relinks": [], "baseline": final})
        assert ok == {"ok": True}

        assert [n["name"] for n in notes] == ["Ideas", "Shop", "From phone"]
        assert [(i["text"], i["checked"]) for i in notes[1]["items"]] == [("milk", True), ("eggs", True), ("bread", False)]
        assert notes[1]["items"][0]["id"] == "a"                     # unchanged items keep their ids
        assert added_lines and added_lines[0]["label"] == "Coding" and added_lines[0]["secs"] == 1200.0

        # straight after a sync nothing here looks changed, so the next compare is quiet
        again = call(code, {"action": "records", "peerId": "phone", "name": "Pixel"})
        now = {s.key_of(r): s.fingerprint(r) for r in again["records"]}
        assert now == final, set(now.items()) ^ set(final.items())
        assert again["baseDigest"] == s.baseline_digest(final)

        # an edit here shows up as a change, and only that one
        notes[0]["content"] = "build a bigger boat"
        changed = {s.key_of(r): s.fingerprint(r) for r in call(code, {"action": "records", "peerId": "phone"})["records"]}
        assert [k for k in changed if changed[k] != final.get(k)] == ["item/lum-n1"]

        # deleted in Atlas: the note goes, a deleted timer log never comes back
        call(code, {"action": "apply", "syncId": "x2", "peerId": "phone", "name": "Pixel", "upserts": [],
                    "deletes": [{"kind": "item", "uid": "atlas-note-1"}, {"kind": "log", "uid": log["uid"]}],
                    "relinks": [], "baseline": {k: v for k, v in changed.items() if k not in ("item/atlas-note-1", s.key_of(log))}})
        assert [n["name"] for n in notes] == ["Ideas", "Shop"]
        after = {s.key_of(r) for r in call(code, {"action": "records", "peerId": "phone"})["records"]}
        assert s.key_of(log) not in after and "item/atlas-note-1" not in after

        assert s.int_to_colour(s.colour_to_int("#E5484D")) == "#E5484D"
    finally:
        host.stop()
    print("atlas sync: all good")


if __name__ == "__main__":
    main()
