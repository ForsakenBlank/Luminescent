"""
Automation: nodes wired together on the Automation page, running in the background.

    data/automation.json
    {"version": 1, "active": workspace id, "vars": {name: text},
     "workspaces": [{"id", "name", "enabled", "view": {"x", "y", "zoom"},
                     "nodes": [{"id", "type", "x", "y", "title", "off", "fields": {key: value}, "state": {}}],
                     "wires": [{"id", "src": [node id, port], "dst": [node id, port]}]}]}

A node has flow ports and value ports. A pulse leaves a flow output ("out:done") and arrives at the
flow inputs wired to it ("in:run"), carrying a bit of text along: its input. A value output ("val:text")
is read when a node needs it: a field with a ◆ next to it ("field:commands") takes its value from the
wire plugged into it instead of its box.

In any text field {input} is the text that came in with the pulse, {var:name} a variable, and {date},
{time}, {now}, {clipboard} and {newline} what they say. Other braces are left alone (so shell and awk
code is safe).

The node types are at the bottom: each is a Node subclass with a @node(...) line saying what it looks
like. Pulses are delivered on the next event-loop pass, so a node wired back into itself can never
recurse, and a workspace that fires more than `automation.max_rate` times a second is switched off.
"""
import os
import re
import json
import time
import shlex
import random
import shutil
import logging
import tempfile
import threading
import subprocess
from dataclasses import dataclass, field as dc_field
from datetime import datetime, date, timedelta

from PyQt6.QtCore import QObject, QTimer, QProcess, QFileSystemWatcher, pyqtSignal
from PyQt6.QtWidgets import QApplication

from core import DATA_DIR, IS_WIN, save_json, load_json, events, runner, clip, sounds, hypr, system

log = logging.getLogger("automation")

PATH = os.path.join(DATA_DIR, "automation.json")
MAX_TEXT = 200_000              # what a pulse may carry
_TOKEN = re.compile(r"\{(input|date|time|now|clipboard|newline|var:[^{}\n]+)\}")


def _id() -> str:
    import uuid
    return uuid.uuid4().hex[:10]


# ── what a node type looks like ───────────────────────────────────────────────
@dataclass
class F:
    """One field on a node. kind: text multiline number bool choice hotkey time date file folder."""
    key: str
    label: str
    kind: str = "text"
    default: object = ""
    choices: object = None          # [(value, label)] or a function(engine) -> [(value, label)]
    pin: bool = False               # can take its value from a wire (a ◆ on its left)
    hint: str = ""                  # placeholder text


@dataclass
class NodeType:
    id: str
    title: str
    cat: str
    cls: type
    ins: list = dc_field(default_factory=list)
    outs: list = dc_field(default_factory=list)
    values: list = dc_field(default_factory=list)
    fields: list = dc_field(default_factory=list)
    help: str = ""
    width: int = 260


TYPES: dict[str, NodeType] = {}
CATEGORIES = [("trigger", "Triggers", "#F2B705"), ("action", "Actions", "#7B6CF6"), ("logic", "Logic", "#3FBF74"),
              ("value", "Values & text", "#4CA7F5"), ("other", "Other", "#8A8A96")]


def node(tid: str, title: str, cat: str, ins=(), outs=(), values=(), fields=(), help: str = "", width: int = 260):
    def deco(cls):
        TYPES[tid] = NodeType(tid, title, cat, cls, list(ins), list(outs), list(values), list(fields), help, width)
        cls.type_id = tid
        return cls
    return deco


def default_fields(tid: str) -> dict:
    return {f.key: f.default for f in TYPES[tid].fields}


def choices_of(f: F, eng) -> list:
    try:
        return list(f.choices(eng) if callable(f.choices) else f.choices or [])
    except Exception:
        log.exception("choices for %s failed", f.key)
        return []


# ── the engine ────────────────────────────────────────────────────────────────
class AutomationEngine(QObject):
    pulse = pyqtSignal(str, str, str)        # workspace id, wire id, text: for the editor's animation
    node_changed = pyqtSignal(str, str)      # workspace id, node id: its status line changed
    changed = pyqtSignal()                   # workspaces added, removed, renamed, turned on or off
    vars_changed = pyqtSignal()

    def __init__(self, launcher):
        super().__init__()
        self.launcher = launcher
        self.settings = launcher.settings
        self.data = self._load()
        self.rt: dict[str, dict[str, "Node"]] = {}           # workspace id -> node id -> running node
        self._out: dict[str, dict[tuple, list]] = {}         # workspace id -> (node, port) -> wires
        self._in: dict[str, dict[tuple, dict]] = {}          # workspace id -> (node, field port) -> wire
        self._rate: dict[str, list] = {}
        self._depth = 0
        self._save_timer = QTimer(self)
        self._save_timer.setSingleShot(True)
        self._save_timer.setInterval(600)
        self._save_timer.timeout.connect(self.save_now)
        self.clock = QTimer(self)                            # the time triggers' shared once-a-second tick
        self.clock.setInterval(1000)
        self.watch = Watchers(self)
        events.bus().fired.connect(self._event)
        self.started = False

    # ── storage ────────────────────────────────────────────────────────────
    def _load(self) -> dict:
        d = load_json(PATH, None)
        if not isinstance(d, dict) or not isinstance(d.get("workspaces"), list):
            d = {"version": 1, "vars": {}, "workspaces": [self._example()]}
        d["workspaces"] = [w for w in d["workspaces"] if isinstance(w, dict)] or [self._blank("Workspace")]
        for ws in d["workspaces"]:
            ws.setdefault("id", _id())
            ws.setdefault("name", "Workspace")
            ws.setdefault("enabled", True)
            ws.setdefault("view", {"x": 0, "y": 0, "zoom": 1.0})
            ws["nodes"] = [n for n in ws.get("nodes", []) if isinstance(n, dict) and n.get("type") in TYPES]
            ids = {n.setdefault("id", _id()) for n in ws["nodes"]}
            for n in ws["nodes"]:
                n.setdefault("x", 0)
                n.setdefault("y", 0)
                n.setdefault("title", "")
                n.setdefault("off", False)
                n["fields"] = {**default_fields(n["type"]), **(n.get("fields") or {})}
                if not isinstance(n.get("state"), dict):
                    n["state"] = {}
            ws["wires"] = [w for w in ws.get("wires", []) if isinstance(w, dict) and w.get("src", [None])[0] in ids
                           and w.get("dst", [None])[0] in ids]
            for w in ws["wires"]:
                w.setdefault("id", _id())
        if not isinstance(d.get("vars"), dict):
            d["vars"] = {}
        if not any(w["id"] == d.get("active") for w in d["workspaces"]):
            d["active"] = d["workspaces"][0]["id"]
        return d

    @staticmethod
    def _blank(name: str) -> dict:
        return {"id": _id(), "name": name, "enabled": True, "view": {"x": 0, "y": 0, "zoom": 1.0},
                "nodes": [], "wires": []}

    def _example(self) -> dict:
        """The first start: a small example, switched off, to show how wiring works."""
        ws = self._blank("Example")
        ws["enabled"] = False
        a = {"id": _id(), "type": "interval", "x": -420, "y": -120, "title": "", "off": False, "state": {},
             "fields": {**default_fields("interval"), "seconds": 60}}
        b = {"id": _id(), "type": "now", "x": -420, "y": 120, "title": "", "off": False, "state": {},
             "fields": {**default_fields("now"), "format": "%H:%M"}}
        c = {"id": _id(), "type": "notify", "x": -40, "y": -120, "title": "", "off": False, "state": {},
             "fields": {**default_fields("notify"), "title": "Automation", "message": "Another minute went by."}}
        note = {"id": _id(), "type": "comment", "x": 340, "y": -120, "title": "", "off": False, "state": {},
                "fields": {"text": "This workspace is off (the switch at the bottom). Turn it on and a card "
                                   "pops up every minute.\n\nDrag from a port to another to wire them. "
                                   "Right-click or double-click the space to add nodes."}}
        ws["nodes"] = [a, b, c, note]
        ws["wires"] = [{"id": _id(), "src": [a["id"], "out:tick"], "dst": [c["id"], "in:run"]},
                       {"id": _id(), "src": [b["id"], "val:text"], "dst": [c["id"], "field:title"]}]
        return ws

    def schedule_save(self):
        self._save_timer.start()

    def save_now(self):
        self._save_timer.stop()
        try:
            save_json(PATH, self.data, indent=2)
        except OSError:
            log.exception("could not save automation.json")

    # ── workspaces ─────────────────────────────────────────────────────────
    def workspaces(self) -> list[dict]:
        return self.data["workspaces"]

    def ws(self, wid: str) -> dict | None:
        return next((w for w in self.data["workspaces"] if w["id"] == wid), None)

    def active_ws(self) -> dict:
        return self.ws(self.data.get("active")) or self.data["workspaces"][0]

    def set_active(self, wid: str):
        if self.ws(wid) is not None:
            self.data["active"] = wid
            self.schedule_save()

    def add_ws(self, name: str, copy_from: dict | None = None) -> dict:
        if copy_from is not None:
            ws = json.loads(json.dumps(copy_from))
            ws["id"], ws["name"] = _id(), name
            remap = {n["id"]: _id() for n in ws["nodes"]}
            for n in ws["nodes"]:
                n["id"] = remap[n["id"]]
                n["state"] = {}
            for w in ws["wires"]:
                w["id"], w["src"][0], w["dst"][0] = _id(), remap[w["src"][0]], remap[w["dst"][0]]
            ws["enabled"] = False
        else:
            ws = self._blank(name)
        self.data["workspaces"].append(ws)
        self.data["active"] = ws["id"]
        self._build(ws)
        self.schedule_save()
        self.changed.emit()
        return ws

    def import_ws(self, ws: dict) -> dict:
        """A workspace read from a file: new ids, switched off until you look at it."""
        return self.add_ws(ws.get("name") or "Imported", copy_from={**self._blank(""), **ws})

    def rename_ws(self, wid: str, name: str):
        ws = self.ws(wid)
        if ws and name.strip():
            ws["name"] = name.strip()
            self.schedule_save()
            self.changed.emit()

    def delete_ws(self, wid: str):
        if len(self.data["workspaces"]) < 2:
            return
        self._stop_ws(wid)
        self.data["workspaces"] = [w for w in self.data["workspaces"] if w["id"] != wid]
        if self.data.get("active") == wid:
            self.data["active"] = self.data["workspaces"][0]["id"]
        self._register_hotkeys()
        self.schedule_save()
        self.changed.emit()

    def set_enabled(self, wid: str, on: bool):
        ws = self.ws(wid)
        if ws is None or ws["enabled"] == on:
            return
        ws["enabled"] = on
        self._build(ws)
        self._register_hotkeys()
        self.schedule_save()
        self.changed.emit()

    @property
    def paused(self) -> bool:
        return not self.settings.get("automation.enabled", True)

    def toggle_paused(self):
        self.settings.set("automation.enabled", self.paused)
        self.settings.save_quiet()
        self.apply_settings()
        self.launcher.notify("Automation", "Automations paused." if self.paused else "Automations running again.",
                             2000)

    # ── running ────────────────────────────────────────────────────────────
    def start(self):
        self.started = True
        self.clock.start()
        for ws in self.workspaces():
            self._build(ws)
        self._register_hotkeys()
        log.info("automation: %d workspace(s), %d running", len(self.workspaces()),
                 sum(1 for w in self.workspaces() if self.running(w)))

    def shutdown(self):
        for wid in list(self.rt):
            self._stop_ws(wid)
        self.clock.stop()
        self.save_now()

    def apply_settings(self):
        for ws in self.workspaces():
            self._build(ws)
        self._register_hotkeys()
        self.changed.emit()

    def running(self, ws: dict) -> bool:
        return self.started and bool(ws.get("enabled")) and not self.paused

    def _stop_ws(self, wid: str):
        for n in self.rt.pop(wid, {}).values():
            self._stop_node(n)
        self._out.pop(wid, None)
        self._in.pop(wid, None)

    @staticmethod
    def _stop_node(n: "Node"):
        try:
            n.alive = False
            n.stop()
        except Exception:
            log.exception("stopping %s failed", n.spec.id)

    def _build(self, ws: dict):
        """Bring the running nodes in line with the workspace: start new ones, stop removed ones, re-index the
        wires. Nodes that stay keep running (an interval is not restarted by adding a wire)."""
        wid = ws["id"]
        if not self.running(ws):
            self._stop_ws(wid)
            return
        live = self.rt.setdefault(wid, {})
        want = {n["id"]: n for n in ws["nodes"] if not n.get("off")}
        for nid in list(live):
            if nid not in want or live[nid].data is not want[nid]:
                self._stop_node(live.pop(nid))
        out, inn = {}, {}
        for w in ws["wires"]:
            out.setdefault(tuple(w["src"]), []).append(w)
            if w["dst"][1].startswith("field:"):
                inn[tuple(w["dst"])] = w
        self._out[wid], self._in[wid] = out, inn
        for nid, nd in want.items():
            if nid not in live:
                n = TYPES[nd["type"]].cls(self, ws, nd)
                live[nid] = n
                try:
                    n.start()
                except Exception as e:
                    log.exception("starting %s failed", nd["type"])
                    n.set_status(f"⚠ {e}")

    def edited(self, wid: str, structural: bool = True, nid: str = ""):
        """The editor changed a workspace. A field change restarts only that node, and only if it is a trigger
        (an interval with a new time); other nodes read their fields when they run."""
        ws = self.ws(wid)
        if ws is None:
            return
        self.schedule_save()
        if structural:
            self._build(ws)
            self._register_hotkeys()
            return
        n = self.rt.get(wid, {}).get(nid)
        if n is not None and n.spec.cat == "trigger":
            self._stop_node(n)
            del self.rt[wid][nid]
            self._build(ws)
            if n.spec.id in ("hotkey", "manual"):
                self._register_hotkeys()

    def node_rt(self, wid: str, nid: str) -> "Node | None":
        return self.rt.get(wid, {}).get(nid)

    def run_node(self, wid: str, nid: str, text: str = ""):
        """▶ on a node (or the palette, the tray): a trigger fires, anything else runs as if a pulse came in."""
        ws = self.ws(wid)
        if ws is None:
            return
        if not self.running(ws):
            why = "Automations are paused" if self.paused else f"“{ws['name']}” is switched off"
            self.launcher.notify("Automation", f"{why}. Turn it on at the bottom of the Automation page.")
            return
        n = self.node_rt(wid, nid)
        if n is not None:
            n.run_now(text)

    def fire(self, n: "Node", port: str, text: str = ""):
        if not n.alive:
            return
        wid = n.ws["id"]
        text = str(text if text is not None else "")[:MAX_TEXT]
        n.last = text
        for w in self._out.get(wid, {}).get((n.id, "out:" + port), []):
            if not self._rate_ok(n.ws):
                return
            self.pulse.emit(wid, w["id"], text[:300])
            dst = self.rt.get(wid, {}).get(w["dst"][0])
            if dst is not None:
                QTimer.singleShot(0, lambda d=dst, p=w["dst"][1].split(":", 1)[1], t=text: self._deliver(d, p, t))

    def _deliver(self, n: "Node", port: str, text: str):
        if not n.alive:
            return
        try:
            n.receive(port, text)
        except Exception as e:
            log.exception("%s failed", n.spec.id)
            n.set_status(f"⚠ {type(e).__name__}: {e}")

    def _rate_ok(self, ws: dict) -> bool:
        now = time.monotonic()
        r = self._rate.setdefault(ws["id"], [now, 0])
        if now - r[0] > 1.0:
            r[0], r[1] = now, 0
        r[1] += 1
        limit = int(self.settings.get("automation.max_rate", 200) or 200)
        if r[1] > limit:
            log.warning("workspace %s fired more than %d times a second: switched off", ws["name"], limit)
            QTimer.singleShot(0, lambda: self.set_enabled(ws["id"], False))
            self.launcher.notify("Automation", f"“{ws['name']}” fired more than {limit} times in a second, so it "
                                               "was switched off. Look for a wire loop.", 8000)
            return False
        return True

    def pull(self, n: "Node", key: str, inp: str) -> str | None:
        """The value wired into field `key` of node `n`, or None when nothing is wired there."""
        w = self._in.get(n.ws["id"], {}).get((n.id, "field:" + key))
        if w is None:
            return None
        src = self.rt.get(n.ws["id"], {}).get(w["src"][0])
        if src is None or self._depth > 40:
            return ""
        self._depth += 1
        try:
            return str(src.value(w["src"][1].split(":", 1)[1], inp) or "")
        except Exception:
            log.exception("reading %s failed", src.spec.id)
            return ""
        finally:
            self._depth -= 1

    # ── text ───────────────────────────────────────────────────────────────
    def expand(self, text: str, inp: str = "") -> str:
        if "{" not in text:
            return text

        def sub(m):
            k = m.group(1)
            if k == "input":
                return inp
            if k == "newline":
                return "\n"
            now = datetime.now()
            if k == "date":
                return now.strftime("%Y-%m-%d")
            if k == "time":
                return now.strftime("%H:%M")
            if k == "now":
                return now.strftime("%Y-%m-%d %H:%M:%S")
            if k == "clipboard":
                return clipboard_text()
            return str(self.data["vars"].get(k[4:].strip(), ""))
        return _TOKEN.sub(sub, text)

    # ── variables ──────────────────────────────────────────────────────────
    def get_var(self, name: str) -> str:
        return str(self.data["vars"].get(name.strip(), ""))

    def set_var(self, name: str, value: str):
        name = name.strip()
        if not name:
            return
        self.data["vars"][name] = str(value)[:MAX_TEXT]
        self.schedule_save()
        self.vars_changed.emit()

    def delete_var(self, name: str):
        if self.data["vars"].pop(name, None) is not None:
            self.schedule_save()
            self.vars_changed.emit()

    # ── hooks into the rest of Luminescent ─────────────────────────────────
    def _event(self, name: str, text: str):
        for nodes in list(self.rt.values()):
            for n in list(nodes.values()):
                if isinstance(n, EventTrigger):
                    n.event(name, text)

    def _register_hotkeys(self):
        hk = self.launcher.hotkeys
        hk.unregister_all("automation")
        combo = self.settings.get("automation.pause_hotkey", "")
        if combo:
            hk.register("automation", combo, self.toggle_paused, "pause automations")
        problems = []
        for wid, nodes in self.rt.items():
            for n in nodes.values():
                if isinstance(n, HotkeyTrigger) and n.raw("combo"):
                    err = hk.register("automation", n.raw("combo"), n.pressed, f"automation {n.label()}")
                    n.set_status(f"⚠ {err}" if err else "")
                    if err:
                        problems.append(f"{n.label()}: {err}")
        if problems:
            self.launcher.notify("Automation hotkeys", "\n".join(problems))

    def manual_nodes(self) -> list[tuple[dict, dict]]:
        return [(ws, n) for ws in self.workspaces() for n in ws["nodes"]
                if n["type"] == "manual" and not n.get("off")]

    def palette_entries(self, Entry) -> list:
        out = []
        for ws, n in self.manual_nodes():
            label = n["fields"].get("label") or "Run"
            out.append(Entry(f"auto:{n['id']}", f"▶ {label}", lambda w=ws["id"], i=n["id"]: self.run_node(w, i),
                             f"Automation · {ws['name']}" + ("" if self.running(ws) else " (off)"), "automation"))
        out.append(Entry("auto:pause", "Resume automations" if self.paused else "Pause all automations",
                         self.toggle_paused, "Automation", "automation"))
        return out

    def tray_menu(self, menu):
        sub = menu.addMenu("⚡  Automation")
        sub.addAction("Resume automations" if self.paused else "Pause all automations", self.toggle_paused)
        manual = self.manual_nodes()
        if manual:
            sub.addSeparator()
            for ws, n in manual:
                a = sub.addAction(f"▶  {n['fields'].get('label') or 'Run'}   ({ws['name']})",
                                  lambda w=ws["id"], i=n["id"]: self.run_node(w, i))
                a.setEnabled(self.running(ws))
        sub.addSeparator()
        for ws in self.workspaces():
            a = sub.addAction(ws["name"], lambda w=ws["id"]: self.set_enabled(w, not self.ws(w)["enabled"]))
            a.setCheckable(True)
            a.setChecked(bool(ws["enabled"]))
        sub.addSeparator()
        sub.addAction("Open the Automation page", lambda: self.launcher.overlay.open("automation"))


# ── background watchers shared by every node that needs them ──────────────────
class Watchers(QObject):
    """Polls that only run while some node wants them: programs, idle time, CPU/RAM/battery, the clipboard."""

    def __init__(self, eng: AutomationEngine):
        super().__init__()
        self.eng = eng
        self.subs: dict[str, set] = {"proc": set(), "idle": set(), "sys": set(), "clip": set(), "win": set(),
                                     "net": set()}
        self.timer = QTimer(self)
        self.timer.setInterval(2000)
        self.timer.timeout.connect(self._tick)
        self._cpu_last = None
        self._cursor = (None, time.monotonic())
        self._clip_last = None
        self._procs: set[str] | None = None
        QApplication.clipboard().dataChanged.connect(self._clip_changed)

    def add(self, kind: str, n):
        self.subs[kind].add(n)
        if not self.timer.isActive():
            self.timer.start()
            QTimer.singleShot(0, self._tick)

    def remove(self, kind: str, n):
        self.subs[kind].discard(n)
        if not any(self.subs.values()):
            self.timer.stop()

    def _tick(self):
        if self.subs["proc"]:
            self._procs = process_names()
            for n in list(self.subs["proc"]):
                n.check(self._procs)
        if self.subs["idle"]:
            secs = self.idle_seconds()
            for n in list(self.subs["idle"]):
                n.check(secs)
        if self.subs["sys"]:
            want = {n.raw("metric") for n in self.subs["sys"]}
            vals = {"cpu": self.cpu(), "ram": ram_percent(), "battery": battery_percent()}
            if "cpu_temp" in want:
                vals["cpu_temp"] = system.cpu_temp()
            if want & {"gpu", "gpu_temp"}:
                g = system.gpu.read() or {}
                vals["gpu"], vals["gpu_temp"] = g.get("load"), g.get("temp")
            for n in list(self.subs["sys"]):
                n.check(vals)
        if self.subs["win"]:
            app, title = system.focused_window()
            for n in list(self.subs["win"]):
                n.check(app, title)
        if self.subs["net"]:
            up = system.online.read()
            for n in list(self.subs["net"]):
                n.check(up)
        if self.subs["clip"] and hypr.wayland():
            # an unfocused Wayland client is not told about clipboard changes: look every two seconds
            self._clip_changed(poll=True)

    def _clip_changed(self, poll: bool = False):
        if not self.subs["clip"]:
            return
        text = clipboard_text()
        if text == self._clip_last:
            return
        first = self._clip_last is None and poll
        self._clip_last = text
        if first:
            return
        for n in list(self.subs["clip"]):
            n.changed(text)
        events.emit("clipboard.changed", text[:200])

    def cpu(self) -> float | None:
        if IS_WIN:
            try:
                import psutil
                return float(psutil.cpu_percent(interval=None))
            except Exception:
                return None
        try:
            with open("/proc/stat") as f:
                parts = [float(x) for x in f.readline().split()[1:]]
            idle, total = parts[3] + (parts[4] if len(parts) > 4 else 0), sum(parts)
            last, self._cpu_last = self._cpu_last, (idle, total)
            if last is None or total == last[1]:
                return None
            return 100.0 * (1 - (idle - last[0]) / (total - last[1]))
        except (OSError, ValueError, IndexError):
            return None

    def idle_seconds(self) -> float:
        if IS_WIN:
            import ctypes

            class _LII(ctypes.Structure):
                _fields_ = [("cbSize", ctypes.c_uint), ("dwTime", ctypes.c_uint)]
            lii = _LII()
            lii.cbSize = ctypes.sizeof(_LII)
            if ctypes.windll.user32.GetLastInputInfo(ctypes.byref(lii)):
                return (ctypes.windll.kernel32.GetTickCount() - lii.dwTime) / 1000.0
            return 0.0
        exe = shutil.which("xprintidle")
        if exe and os.environ.get("DISPLAY") and not hypr.wayland():
            try:
                return int(subprocess.run([exe], capture_output=True, text=True, timeout=2).stdout.strip()) / 1000
            except (OSError, ValueError, subprocess.SubprocessError):
                pass
        # Wayland tells no program about key presses elsewhere: the mouse standing still is the best guess
        pos = hypr.cursor_pos()
        p = (pos.x(), pos.y())
        if p != self._cursor[0]:
            self._cursor = (p, time.monotonic())
        return time.monotonic() - self._cursor[1]


def clipboard_text() -> str:
    try:
        if clip._wl():
            return clip._paste("text/plain;charset=utf-8").decode("utf-8", "replace") or \
                clip._paste("text/plain").decode("utf-8", "replace")
    except Exception:
        pass
    return QApplication.clipboard().text()


def process_names() -> set[str]:
    names = set()
    if IS_WIN:
        try:
            import psutil
            for p in psutil.process_iter(["name"]):
                n = (p.info.get("name") or "").lower()
                if n:
                    names.add(n)
                    names.add(n[:-4] if n.endswith(".exe") else n)
        except Exception:
            log.debug("process list failed", exc_info=True)
        return names
    for pid in os.listdir("/proc") if os.path.isdir("/proc") else []:
        if not pid.isdigit():
            continue
        try:
            with open(f"/proc/{pid}/comm") as f:
                names.add(f.read().strip().lower())
            exe = os.path.basename(os.readlink(f"/proc/{pid}/exe")).lower()
            names.add(exe)
        except OSError:
            continue
    return names


def ram_percent() -> float | None:
    if IS_WIN:
        try:
            import psutil
            return float(psutil.virtual_memory().percent)
        except Exception:
            return None
    try:
        info = {}
        with open("/proc/meminfo") as f:
            for line in f:
                k, _, v = line.partition(":")
                info[k] = float(v.split()[0])
        return 100.0 * (1 - info["MemAvailable"] / info["MemTotal"])
    except (OSError, KeyError, ValueError, ZeroDivisionError):
        return None


def battery_percent() -> float | None:
    if IS_WIN:
        try:
            import psutil
            b = psutil.sensors_battery()
            return float(b.percent) if b else None
        except Exception:
            return None
    base = "/sys/class/power_supply"
    try:
        for name in sorted(os.listdir(base)):
            if name.startswith("BAT"):
                with open(os.path.join(base, name, "capacity")) as f:
                    return float(f.read().strip())
    except (OSError, ValueError):
        pass
    return None


# ── nodes ─────────────────────────────────────────────────────────────────────
class Node:
    type_id = ""
    live = False                # its status moves by itself (a countdown): the editor repaints it

    def __init__(self, eng: AutomationEngine, ws: dict, data: dict):
        self.eng = eng
        self.ws = ws
        self.data = data
        self.id = data["id"]
        self.spec = TYPES[data["type"]]
        self.alive = True
        self.status = ""
        self.progress: float | None = None
        self.last = ""              # the last text it sent on

    # fields
    def raw(self, key: str):
        return self.data["fields"].get(key, next((f.default for f in self.spec.fields if f.key == key), ""))

    def f(self, key: str, inp: str = "") -> str:
        got = self.eng.pull(self, key, inp)
        if got is not None:
            return got
        v = self.raw(key)
        return self.eng.expand(v, inp) if isinstance(v, str) else v

    def num(self, key: str, inp: str = "", default: float = 0.0) -> float:
        try:
            return float(str(self.f(key, inp)).strip().replace(",", "."))
        except (TypeError, ValueError):
            return default

    def label(self) -> str:
        return self.data.get("title") or self.spec.title

    # running
    def start(self):
        pass

    def stop(self):
        pass

    def receive(self, port: str, inp: str):
        pass

    def value(self, port: str, inp: str) -> str:
        return ""

    def fire(self, port: str, text: str = ""):
        self.eng.fire(self, port, text)

    def run_now(self, text: str = ""):
        if self.spec.cat == "trigger" and self.spec.outs:
            self.fire(self.spec.outs[0], text)
        elif self.spec.ins:
            self.eng._deliver(self, self.spec.ins[0], text)

    def set_status(self, text: str, progress: float | None = None):
        self.status, self.progress = text, progress
        self.eng.node_changed.emit(self.ws["id"], self.id)

    def info(self) -> tuple[str, float | None]:
        """What the editor shows under the node: a status line and a progress bar (None = none)."""
        return self.status, self.progress

    def save_state(self, **kw):
        self.data["state"].update(kw)
        self.eng.schedule_save()


def _hm(text: str) -> tuple[int, int] | None:
    m = re.match(r"^\s*(\d{1,2})[:.](\d{2})\s*$", str(text))
    if not m or int(m.group(1)) > 23 or int(m.group(2)) > 59:
        return None
    return int(m.group(1)), int(m.group(2))


_DAYS = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"]


def _days(text: str) -> set[int]:
    t = str(text or "").strip().lower()
    if t in ("", "every day", "daily", "all"):
        return set(range(7))
    if t in ("weekdays", "workdays"):
        return set(range(5))
    if t in ("weekends", "weekend"):
        return {5, 6}
    out = set()
    for part in re.split(r"[,\s]+", t):
        if "-" in part:
            a, _, b = part.partition("-")
            if a[:3] in _DAYS and b[:3] in _DAYS:
                i, j = _DAYS.index(a[:3]), _DAYS.index(b[:3])
                out |= set(range(i, j + 1)) if i <= j else set(range(i, 7)) | set(range(0, j + 1))
        elif part[:3] in _DAYS:
            out.add(_DAYS.index(part[:3]))
    return out or set(range(7))


# ── triggers ──────────────────────────────────────────────────────────────────
@node("interval", "Every…", "trigger", ins=["start", "stop"], outs=["tick"], values=["count"], fields=[
    F("seconds", "Every (seconds)", "number", 5, pin=True),
    F("autostart", "Running from the start", "bool", True),
    F("fire_now", "Also fire straight away", "bool", False)],
    help="Fires again and again. Its input says how many times it fired. Start and stop it with a wire.")
class Interval(Node):
    live = True

    def start(self):
        self.timer = QTimer()
        self.timer.timeout.connect(self._tick)
        self.count = 0
        self.t0, self.ms = 0.0, 0
        if self.raw("autostart"):
            self._go()
        else:
            self.set_status("stopped")

    def _go(self):
        self.ms = max(100, int(self.num("seconds", default=5) * 1000))
        self.t0 = time.monotonic()
        self.timer.start(self.ms)
        if self.raw("fire_now"):
            QTimer.singleShot(0, self._tick)

    def _tick(self):
        self.count += 1
        self.t0 = time.monotonic()
        self.fire("tick", str(self.count))

    def receive(self, port, inp):
        if port == "start" and not self.timer.isActive():
            self._go()
        elif port == "stop":
            self.timer.stop()
            self.set_status("stopped")

    def value(self, port, inp):
        return str(self.count)

    def stop(self):
        self.timer.stop()

    def info(self):
        if not self.timer.isActive():
            return "stopped", None
        left = max(0.0, self.ms / 1000 - (time.monotonic() - self.t0))
        return f"next in {left:.0f} s" if left >= 1 else "now", 1 - left / max(0.1, self.ms / 1000)


@node("at_time", "At a time of day", "trigger", outs=["fire"], fields=[
    F("time", "Time", "time", "16:00"),
    F("days", "Days", "text", "every day", hint="every day · weekdays · weekends · mon,wed,fri")],
    help="Fires at that time on those days. Its input is the time.")
class AtTime(Node):
    live = True

    def start(self):
        self._last = ""
        self.eng.clock.timeout.connect(self._tick)

    def stop(self):
        try:
            self.eng.clock.timeout.disconnect(self._tick)
        except TypeError:
            pass

    def _next(self) -> datetime | None:
        hm = _hm(self.raw("time"))
        if hm is None:
            return None
        days = _days(self.raw("days"))
        now = datetime.now()
        for d in range(8):
            t = (now + timedelta(days=d)).replace(hour=hm[0], minute=hm[1], second=0, microsecond=0)
            if t.weekday() in days and t > now - timedelta(seconds=59):
                return t
        return None

    def _tick(self):
        now = datetime.now()
        hm = _hm(self.raw("time"))
        key = now.strftime("%Y-%m-%d %H:%M")
        if hm and (now.hour, now.minute) == hm and now.weekday() in _days(self.raw("days")) and key != self._last:
            self._last = key
            self.fire("fire", now.strftime("%H:%M"))

    def info(self):
        t = self._next()
        if t is None:
            return "⚠ write the time like 16:00", None
        secs = (t - datetime.now()).total_seconds()
        if secs < 60:
            return "now", None
        day = "today" if t.date() == date.today() else ("tomorrow" if t.date() == date.today() + timedelta(1)
                                                         else t.strftime("%a"))
        return f"next: {day} {t:%H:%M} (in {_left(secs)})", None


def _left(secs: float) -> str:
    secs = int(secs)
    if secs >= 86400:
        return f"{secs // 86400}d {secs % 86400 // 3600}h"
    if secs >= 3600:
        return f"{secs // 3600}h {secs % 3600 // 60}m"
    if secs >= 60:
        return f"{secs // 60}m"
    return f"{secs}s"


@node("on_date", "On a date", "trigger", outs=["fire"], fields=[
    F("date", "Date", "date", ""),
    F("time", "Time", "time", "09:00"),
    F("catch_up", "If Luminescent was off then, fire when it starts", "bool", True)],
    help="Fires once, on that day at that time (e.g. 3 October 2028, 09:00).")
class OnDate(Node):
    live = True

    def start(self):
        self.eng.clock.timeout.connect(self._tick)

    def stop(self):
        try:
            self.eng.clock.timeout.disconnect(self._tick)
        except TypeError:
            pass

    def _target(self) -> datetime | None:
        hm = _hm(self.raw("time")) or (0, 0)
        try:
            d = datetime.strptime(str(self.raw("date")).strip(), "%Y-%m-%d")
        except ValueError:
            return None
        return d.replace(hour=hm[0], minute=hm[1])

    def _tick(self):
        t = self._target()
        if t is None or self.data["state"].get("fired") == t.isoformat():
            return
        now = datetime.now()
        late = (now - t).total_seconds()
        if late >= 0 and (late < 90 or self.raw("catch_up")):
            self.save_state(fired=t.isoformat())
            self.fire("fire", t.strftime("%Y-%m-%d %H:%M"))

    def info(self):
        t = self._target()
        if t is None:
            return "⚠ pick a date", None
        if self.data["state"].get("fired") == t.isoformat():
            return "done (fired once)", None
        secs = (t - datetime.now()).total_seconds()
        return (f"in {_left(secs)}" if secs > 0 else "now"), None


@node("hotkey", "Hotkey", "trigger", outs=["pressed"], fields=[F("combo", "Hotkey", "hotkey", "")],
      help="Fires when you press the hotkey, anywhere.", width=330)
class HotkeyTrigger(Node):
    def pressed(self):
        self.fire("pressed", self.raw("combo"))


@node("manual", "Button", "trigger", outs=["fire"], fields=[F("label", "Name", "text", "Run")],
      help="Fires when you click ▶ on it, or pick it in the command palette or the tray menu (Automation).")
class Manual(Node):
    pass


@node("event", "Luminescent event", "trigger", outs=["fire"], fields=[
    F("event", "When", "choice", "luminescent.started", choices=events.NAMES),
    F("filter", "Only if its text contains", "text", "", hint="(anything)")],
    help="Fires when something happens in Luminescent. Its input says what (the timer's name, the theme…).")
class EventTrigger(Node):
    def event(self, name: str, text: str):
        if name == self.raw("event") and str(self.raw("filter") or "").lower() in text.lower():
            self.fire("fire", text)


@node("process", "Program starts / stops", "trigger", outs=["started", "stopped"], values=["running"], fields=[
    F("name", "Program", "text", "", hint="firefox, steam, notepad.exe…")],
    help="Watches for a program by its process name (looked up every two seconds).")
class ProcessTrigger(Node):
    def start(self):
        self.up = None
        self.eng.watch.add("proc", self)

    def stop(self):
        self.eng.watch.remove("proc", self)

    def check(self, names: set[str]):
        want = str(self.raw("name") or "").strip().lower()
        if not want:
            self.set_status("⚠ type a program name")
            return
        up = want in names or (want + ".exe") in names or want.removesuffix(".exe") in names
        if self.up is not None and up != self.up:
            self.fire("started" if up else "stopped", want)
        if up != self.up:
            self.set_status("running" if up else "not running")
        self.up = up

    def value(self, port, inp):
        return "yes" if self.up else "no"


@node("file_watch", "File or folder changes", "trigger", outs=["changed"], fields=[
    F("path", "File or folder", "file", "")],
    help="Fires when the file, or anything directly in the folder, changes. Its input is the path.")
class FileWatch(Node):
    def start(self):
        self.w = QFileSystemWatcher()
        self.w.fileChanged.connect(self._hit)
        self.w.directoryChanged.connect(self._hit)
        self._debounce = QTimer()
        self._debounce.setSingleShot(True)
        self._debounce.setInterval(400)
        self._debounce.timeout.connect(lambda: self.fire("changed", self._path))
        self._path = os.path.expanduser(str(self.raw("path") or "").strip())
        if self._path and os.path.exists(self._path):
            self.w.addPath(self._path)
            self.set_status("watching")
        else:
            self.set_status("⚠ that path does not exist" if self._path else "pick a file or folder")

    def _hit(self, path: str):
        if os.path.exists(self._path) and self._path not in self.w.files() + self.w.directories():
            self.w.addPath(self._path)          # editors save by replacing the file: watch the new one
        self._debounce.start()

    def stop(self):
        self._debounce.stop()
        self.w.deleteLater()


@node("clipboard", "Clipboard changes", "trigger", outs=["changed"],
      help="Fires when something new is copied. Its input is the copied text.")
class ClipTrigger(Node):
    def start(self):
        self.eng.watch.add("clip", self)

    def stop(self):
        self.eng.watch.remove("clip", self)

    def changed(self, text: str):
        self.fire("changed", text)


@node("idle", "Idle / back", "trigger", outs=["idle", "back"], fields=[
    F("minutes", "Idle for (minutes)", "number", 5)],
    help="Fires when the PC has not been used for that long, and again when you are back. "
         "On Wayland only the mouse can be seen, so it counts the mouse standing still.")
class IdleTrigger(Node):
    def start(self):
        self.away = False
        self.eng.watch.add("idle", self)

    def stop(self):
        self.eng.watch.remove("idle", self)

    def check(self, secs: float):
        limit = max(0.1, self.num("minutes", default=5)) * 60
        if not self.away and secs >= limit:
            self.away = True
            self.set_status("away")
            self.fire("idle", f"{secs / 60:.0f}")
        elif self.away and secs < 5:
            self.away = False
            self.set_status("")
            self.fire("back")


METRICS = [("cpu", "CPU load (%)"), ("cpu_temp", "CPU temperature (°C)"), ("gpu", "GPU load (%)"),
           ("gpu_temp", "GPU temperature (°C)"), ("ram", "Memory (RAM, %)"), ("battery", "Battery (%)")]
_UNIT = {"cpu_temp": " °C", "gpu_temp": " °C"}


@node("threshold", "CPU / GPU / temperature / battery", "trigger", outs=["crossed", "back"], values=["percent"],
      fields=[
    F("metric", "Watch", "choice", "cpu", choices=METRICS),
    F("op", "When it is", "choice", "above", choices=[("above", "above"), ("below", "below")]),
    F("value", "Line (% or °C)", "number", 90),
    F("for_s", "For at least (seconds)", "number", 10)],
    help="Fires when the level crosses your line and stays there, and again when it is back. Temperatures "
         "and GPU load come from the sensors (Linux; on Windows only an NVIDIA GPU can be read). Wire it to "
         "\"Warning on screen\" for a proper heads-up, e.g. CPU temperature above 80 °C.")
class ThresholdTrigger(Node):
    def start(self):
        self.since = None
        self.over = False
        self.level = None
        self.eng.watch.add("sys", self)

    def stop(self):
        self.eng.watch.remove("sys", self)

    def check(self, vals: dict):
        metric = self.raw("metric")
        v = vals.get(metric)
        if v is None:
            self.set_status("⚠ not available here" if metric in ("battery", "cpu_temp", "gpu", "gpu_temp")
                            else "…")
            return
        self.level = v
        unit = _UNIT.get(metric, " %")
        line = self.num("value", default=90)
        bad = v > line if self.raw("op") == "above" else v < line
        now = time.monotonic()
        if bad and not self.over:
            self.since = self.since or now
            if now - self.since >= self.num("for_s", default=10):
                self.over = True
                self.fire("crossed", f"{v:.0f}")
        elif not bad:
            self.since = None
            if self.over:
                self.over = False
                self.fire("back", f"{v:.0f}")
        self.set_status(f"{v:.0f}{unit}")

    def value(self, port, inp):
        return f"{self.level:.0f}" if self.level is not None else ""


@node("window", "Window focused", "trigger", outs=["focused", "left"], values=["title"], fields=[
    F("match", "App or title contains", "text", "", hint="firefox, Visual Studio Code, YouTube…")],
    help="Fires when you switch to a window whose app or title contains the text, and when you leave it. "
         "Its input is the window title." + ("" if IS_WIN else " Needs Hyprland."))
class WindowTrigger(Node):
    def start(self):
        self.inside = None
        self.title = ""
        self.eng.watch.add("win", self)

    def stop(self):
        self.eng.watch.remove("win", self)

    def check(self, app: str, title: str):
        want = str(self.raw("match") or "").strip().lower()
        if not want:
            self.set_status("⚠ type part of an app name or title")
            return
        inside = want in app.lower() or want in title.lower()
        if inside:
            self.title = title
        if self.inside is not None and inside != self.inside:
            self.fire("focused" if inside else "left", title)
        if inside != self.inside:
            self.set_status("in it" if inside else "elsewhere")
        self.inside = inside

    def value(self, port, inp):
        return self.title


@node("network", "Internet on / off", "trigger", outs=["online", "offline"], values=["online"],
      help="Fires when the internet goes away and when it comes back (checked every 10 seconds).")
class NetTrigger(Node):
    def start(self):
        self.up = None
        self.eng.watch.add("net", self)

    def stop(self):
        self.eng.watch.remove("net", self)

    def check(self, up: bool | None):
        if up is None:
            return
        if self.up is not None and up != self.up:
            self.fire("online" if up else "offline")
        if up != self.up:
            self.set_status("online" if up else "offline")
        self.up = up

    def value(self, port, inp):
        return "yes" if self.up else "no"


# ── actions ───────────────────────────────────────────────────────────────────
def _shell() -> str:
    return "cmd" if IS_WIN else "bash"


@node("command", "Run command", "action", ins=["run", "stop"], outs=["done", "failed", "line"], values=["output"],
      fields=[
    F("commands", "Commands (one per line, each after the last one finishes)", "multiline", "", pin=True,
      hint="echo hello"),
    F("terminal", "Show it in a terminal window", "bool", False),
    F("stop_on_error", "Stop at a command that fails", "bool", True),
    F("folder", "Run in folder", "folder", "", hint="your home folder")],
    help="Runs the commands one after another (in the background, or in a terminal you can watch). "
         "Done/failed carry the output; line fires for every line it prints.", width=320)
class Command(Node):
    def start(self):
        self.proc: QProcess | None = None
        self.output = ""
        self._queue: list[str] = []
        self._poll = None

    def receive(self, port, inp):
        if port == "stop":
            self._queue.clear()
            self._kill()
            return
        if self.proc is not None or self._poll is not None:
            if len(self._queue) < 20:
                self._queue.append(inp)          # one run at a time; the next waits its turn
            return
        lines = [ln for ln in str(self.f("commands", inp)).splitlines() if ln.strip()]
        if not lines:
            self.set_status("⚠ no commands")
            return
        cwd = os.path.expanduser(str(self.raw("folder") or "").strip() or "~")
        if not os.path.isdir(cwd):
            cwd = os.path.expanduser("~")
        if self.raw("terminal"):
            self._in_terminal(lines, cwd)
            return
        self._lines, self._cwd, self._out, self._failed, self._code = lines, cwd, [], False, 0
        self._next()

    def _argv(self, line: str) -> tuple[str, list[str]]:
        sh = self.eng.settings.get("automation.shell", _shell())
        if IS_WIN:
            if sh == "powershell":
                return "powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", line]
            return "cmd.exe", []
        exe = shutil.which(sh) or "/bin/sh"
        return exe, ["-c", line]

    def _next(self):
        if not self._lines:
            self._finish()
            return
        line = self._lines.pop(0)
        p = QProcess()
        p.setProcessChannelMode(QProcess.ProcessChannelMode.MergedChannels)
        p.setWorkingDirectory(self._cwd)
        prog, args = self._argv(line)
        p.setProgram(prog)
        if IS_WIN and prog == "cmd.exe":
            p.setNativeArguments(f'/d /s /c "{line}"')
        else:
            p.setArguments(args)
        p.readyReadStandardOutput.connect(lambda: self._read(p))
        p.finished.connect(lambda code, _st: self._ended(p, code))
        p.errorOccurred.connect(lambda err: self._error(p, err))
        self.proc = p
        self.set_status(f"running: {line[:40]}")
        p.start()
        secs = int(self.eng.settings.get("automation.command_timeout", 0) or 0)
        if secs > 0:
            QTimer.singleShot(secs * 1000, lambda: self._timeout(p))

    def _read(self, p: QProcess):
        chunk = bytes(p.readAllStandardOutput()).decode("utf-8", "replace")
        self._out.append(chunk)
        for ln in chunk.splitlines():
            if ln.strip():
                self.fire("line", ln)

    def _ended(self, p: QProcess, code: int):
        if p is not self.proc:
            return
        self.proc = None
        p.deleteLater()
        if code != 0:
            self._failed, self._code = True, code
            if self.raw("stop_on_error"):
                self._lines = []
        self._next()

    def _error(self, p: QProcess, err):
        if err == QProcess.ProcessError.FailedToStart and p is self.proc:
            self._out.append(f"could not start: {p.errorString()}\n")
            self._ended(p, 127)

    def _timeout(self, p: QProcess):
        if p is self.proc and p.state() != QProcess.ProcessState.NotRunning:
            self._out.append("\n[stopped: took too long]\n")
            p.kill()

    def _kill(self):
        if self.proc is not None:
            self._lines = []
            self.proc.kill()

    def _finish(self):
        self.output = "".join(self._out).strip()[-MAX_TEXT:]
        self.set_status(f"failed (exit code {self._code})" if self._failed else "done")
        self.fire("failed" if self._failed else "done", self.output)
        self._after()

    def _after(self):
        if self._queue:
            nxt = self._queue.pop(0)
            QTimer.singleShot(0, lambda: self.receive("run", nxt))

    def _in_terminal(self, lines: list[str], cwd: str):
        """A script in a terminal window you can watch; a marker file says when it ended and how."""
        fd, marker = tempfile.mkstemp(prefix="lum-auto-", suffix=".code")
        os.close(fd)
        os.remove(marker)
        stop = bool(self.raw("stop_on_error"))
        sh = self.eng.settings.get("automation.shell", _shell())
        if IS_WIN and sh == "powershell":
            body = "\n".join(ln + ("\nif ($LASTEXITCODE) { exit $LASTEXITCODE }" if stop else "") for ln in lines)
            script = _temp_script(body, ".ps1")
            cmd = (f"powershell -NoProfile -ExecutionPolicy Bypass -File '{script}'; "
                   f"$LASTEXITCODE | Out-File -Encoding ascii '{marker}'")
        elif IS_WIN:
            body = "@echo off\n" + "\n".join(ln + ("\nif errorlevel 1 exit /b %errorlevel%" if stop else "")
                                             for ln in lines)
            script = _temp_script(body, ".cmd")
            cmd = f"cmd /c '{script}'; $LASTEXITCODE | Out-File -Encoding ascii '{marker}'"
        else:
            exe = shutil.which(sh) or "sh"
            if os.path.basename(exe) == "fish":
                body = (" && " if stop else "\n").join(lines)
            else:
                body = ("set -e\n" if stop else "") + "\n".join(lines)
            script = _temp_script(body, ".sh")
            cmd = f"{shlex.quote(exe)} {shlex.quote(script)}; echo $? > {shlex.quote(marker)}"
        err = runner.open_terminal(cwd, cmd, self.eng.settings.get("dashboard.terminal", ""))
        if err:
            self.set_status(f"⚠ {err}")
            self.fire("failed", err)
            return
        self.set_status("running in a terminal")
        self._poll = QTimer()
        started = time.monotonic()

        def check():
            if os.path.exists(marker):
                try:
                    with open(marker, encoding="utf-8", errors="replace") as fh:
                        code = int((fh.read().strip() or "0").split()[0])
                except (OSError, ValueError):
                    code = 0
                for f in (marker, script):
                    try:
                        os.remove(f)
                    except OSError:
                        pass
                self._poll.stop()
                self._poll = None
                self.output = str(code)
                self.set_status(f"failed (exit code {code})" if code else "done")
                self.fire("failed" if code else "done", str(code))
                self._after()
            elif time.monotonic() - started > 86400:
                self._poll.stop()
                self._poll = None
        self._poll.timeout.connect(check)
        self._poll.start(500)

    def value(self, port, inp):
        return self.output

    def stop(self):
        self._queue.clear()
        if self.proc is not None:
            self.proc.kill()
        if self._poll is not None:
            self._poll.stop()


def _temp_script(body: str, ext: str) -> str:
    fd, path = tempfile.mkstemp(prefix="lum-auto-", suffix=ext)
    with os.fdopen(fd, "w", encoding="utf-8", newline="\r\n" if ext in (".cmd", ".ps1") else "\n") as f:
        f.write(body + "\n")
    return path


@node("open", "Open a program, file or URL", "action", ins=["run"], outs=["done"], fields=[
    F("target", "Open", "file", "", pin=True, hint="firefox · ~/notes.txt · https://…"),
    F("args", "Arguments", "text", "", pin=True)],
    help="Starts a program, opens a file or folder in its app, or a web page in the browser.")
class OpenTarget(Node):
    def receive(self, port, inp):
        target = str(self.f("target", inp)).strip()
        if not target:
            self.set_status("⚠ nothing to open")
            return
        s = self.eng.settings
        err, _p = runner.launch(target, str(self.f("args", inp)), self.label(),
                                {"use_terminal": s.get("dashboard.use_terminal", True),
                                 "keep_open": s.get("dashboard.keep_open", True),
                                 "python_path": s.get("dashboard.python_path", ""),
                                 "terminal": s.get("dashboard.terminal", "")})
        self.set_status(f"⚠ {err}" if err else "")
        if not err:
            self.fire("done", target)


@node("close_program", "Close a program", "action", ins=["run"], outs=["done", "not_running"], fields=[
    F("name", "Program", "text", "", pin=True, hint="firefox, notepad.exe…"),
    F("force", "Force it (no chance to save)", "bool", False)],
    help="Closes every running copy of the program.")
class CloseProgram(Node):
    def receive(self, port, inp):
        name = str(self.f("name", inp)).strip()
        if not name:
            return
        if IS_WIN:
            exe = name if name.lower().endswith(".exe") else name + ".exe"
            cmd = ["taskkill", "/IM", exe] + (["/F"] if self.raw("force") else [])
            kw = {"creationflags": subprocess.CREATE_NO_WINDOW}
        else:
            cmd = ["pkill"] + (["-9"] if self.raw("force") else []) + ["-x", name]
            kw = {}

        def work():
            try:
                code = subprocess.run(cmd, capture_output=True, timeout=10, **kw).returncode
            except (OSError, subprocess.SubprocessError):
                code = 1
            _later(lambda: self.fire("done" if code == 0 else "not_running", name))
        threading.Thread(target=work, daemon=True).start()


def _lum_actions(eng) -> list:
    out = []
    for key, m in eng.launcher.modules.items():
        for aid, label, _fn in m.actions():
            out.append((f"{key}:{aid}", f"{m.title}: {label}"))
    return out


@node("lum_action", "Luminescent tool", "action", ins=["run"], outs=["done"], fields=[
    F("action", "Do", "choice", "notes:toggle", choices=_lum_actions)],
    help="Anything from the Dashboard's Tools tab: show Notes, a new timer, pick a colour, snap a window…")
class LumAction(Node):
    def receive(self, port, inp):
        self.eng.launcher.run_action(str(self.raw("action")))
        self.fire("done", inp)


def _pages(eng) -> list:
    from core.settings import ALL_PAGES
    return list(ALL_PAGES)


@node("overlay", "Open / close Luminescent", "action", ins=["run"], outs=["done"], fields=[
    F("mode", "Do", "choice", "open", choices=[("open", "Open it on a page"), ("close", "Close it"),
                                              ("toggle", "Open or close it")]),
    F("page", "Page", "choice", "dashboard", choices=_pages)],
    help="Opens the overlay on a page (Dashboard, Notes, Settings…) or closes it.")
class OverlayNode(Node):
    def receive(self, port, inp):
        ov = self.eng.launcher.overlay
        mode = self.raw("mode")
        if mode == "close" or (mode == "toggle" and ov.is_visible()):
            ov.hide()
        else:
            ov.open(str(self.raw("page") or "dashboard"))
        self.fire("done", inp)


def _items(eng) -> list:
    st = eng.launcher.dashboard
    several = len(st.boards()) > 1
    return [(it["id"], (f"{b['name']} › " if several else "") + f"{t['title']} › {it['name']}")
            for b, t in st.all_tabs() for it in t["items"] if it.get("kind", "item") in ("item", "group")]


@node("dash_item", "Run a dashboard item", "action", ins=["run"], outs=["done"], fields=[
    F("item", "Item", "choice", "", choices=_items)],
    help="Runs an item from the Dashboard, as if you double-clicked it.")
class DashItem(Node):
    def receive(self, port, inp):
        self.eng.launcher.dashboard.run_item(str(self.raw("item")))
        self.fire("done", inp)


def _boards(eng) -> list:
    return [(b["id"], b["name"]) for b in eng.launcher.dashboard.boards()]


@node("switch_board", "Switch dashboard", "action", ins=["run"], outs=["done"], fields=[
    F("board", "Dashboard", "choice", "", choices=_boards)])
class SwitchBoard(Node):
    def receive(self, port, inp):
        self.eng.launcher.dashboard.switch_board(str(self.raw("board")))
        self.fire("done", inp)


def _themes(eng) -> list:
    return [("next", "The next theme")] + [(t["id"], t["name"]) for t in eng.launcher.themes.themes()]


@node("theme", "Switch theme", "action", ins=["run"], outs=["done"], fields=[
    F("theme", "Theme", "choice", "next", choices=_themes)])
class ThemeNode(Node):
    def receive(self, port, inp):
        tid = str(self.raw("theme"))
        if tid == "next":
            self.eng.launcher.next_theme()
        else:
            self.eng.launcher.apply_theme(tid)
        self.fire("done", inp)


@node("timer", "Start a timer", "action", ins=["run"], outs=["started"], fields=[
    F("time", "Time", "text", "5m", pin=True, hint="5m · 1h30 · 25m focus · at 17:30")],
    help="A Timer panel, already running. Wire the “A timer finished” event to do something when it ends.")
class TimerNode(Node):
    def receive(self, port, inp):
        m = self.eng.launcher.modules.get("timer")
        if m is None or not m.running:
            self.set_status("⚠ the Timer module is off")
            return
        from modules.timer import parse_entry
        got = parse_entry(str(self.f("time", inp)))
        if got is None:
            self.set_status("⚠ could not read that time")
            return
        ms, label = got
        cfg = {"mode": "timer" if ms > 0 else "stopwatch", "duration_ms": int(ms)}
        if label:
            cfg["label"] = label
        m._add_panel(cfg)
        m._panels[-1].play_pause()
        self.fire("started", label or str(self.f("time", inp)))


def _workspaces(eng) -> list:
    return [("", "This workspace")] + [(w["id"], w["name"]) for w in eng.workspaces()]


@node("workspace", "Workspace on / off", "action", ins=["run"], outs=["done"], fields=[
    F("workspace", "Workspace", "choice", "", choices=_workspaces),
    F("mode", "Turn it", "choice", "off", choices=[("on", "on"), ("off", "off"), ("toggle", "the other way")])],
    help="Switches a whole workspace on or off (this one too).")
class WorkspaceNode(Node):
    def receive(self, port, inp):
        wid = str(self.raw("workspace") or "") or self.ws["id"]
        ws = self.eng.ws(wid)
        if ws is None:
            return
        mode = self.raw("mode")
        on = (not ws["enabled"]) if mode == "toggle" else mode == "on"
        self.fire("done", inp)
        QTimer.singleShot(0, lambda: self.eng.set_enabled(wid, on))


@node("notify", "Show a notification", "action", ins=["run"], outs=["done"], fields=[
    F("title", "Title", "text", "Luminescent", pin=True),
    F("message", "Message", "multiline", "{input}", pin=True),
    F("style", "Show as", "choice", "card", choices=[("card", "A Luminescent card"),
                                                     ("desktop", "A desktop notification"),
                                                     ("center", "A warning in the middle of the screen")])],
    help="A small card in the corner, a desktop notification, or a big warning in the middle of the screen.")
class Notify(Node):
    def receive(self, port, inp):
        title, msg = str(self.f("title", inp)), str(self.f("message", inp))
        style = self.raw("style")
        try:
            if style == "center":
                from ui import alert
                alert.show(title, msg, "info")
            elif style == "desktop":
                self.eng.launcher.toast(title, msg)
            else:
                self.eng.launcher.toasts.show(title, msg, 6000)
        except Exception:
            log.exception("could not show the notification as %s", style)
            try:
                self.eng.launcher.toast(title, msg)          # the desktop's own notifications as a fallback
            except Exception:
                log.exception("the fallback notification failed too")
            self.set_status("⚠ notification failed (see the log)")
        self.fire("done", inp)


@node("message", "Big message", "action", ins=["run"], outs=["ok"], fields=[
    F("title", "Title", "text", "Luminescent", pin=True),
    F("text", "Message", "multiline", "{input}", pin=True)],
    help="A message box on top of everything; ok fires when you close it.")
class MessageNode(Node):
    def receive(self, port, inp):
        from PyQt6.QtWidgets import QMessageBox
        from PyQt6.QtCore import Qt
        box = QMessageBox()
        box.setWindowTitle(str(self.f("title", inp)))
        box.setText(str(self.f("text", inp)))
        box.setWindowFlags(box.windowFlags() | Qt.WindowType.WindowStaysOnTopHint)
        box.finished.connect(lambda _r: self.fire("ok", inp))
        self._box = box
        box.open()
        from core import widgets
        widgets.bring_to_front(box)


@node("ask", "Ask me", "action", ins=["run"], outs=["answer", "cancelled"], values=["answer"], fields=[
    F("question", "Question", "text", "What should I type?", pin=True),
    F("default", "Starts with", "text", "", pin=True),
    F("var", "Also keep it in variable", "text", "", hint="(none)")],
    help="Pops up a box for you to type in, then carries on with what you typed.")
class Ask(Node):
    def start(self):
        self.answer = ""

    def receive(self, port, inp):
        from PyQt6.QtWidgets import QInputDialog
        from PyQt6.QtCore import Qt
        from core import widgets
        dlg = QInputDialog()
        dlg.setWindowTitle(self.label())
        dlg.setLabelText(str(self.f("question", inp)))
        dlg.setTextValue(str(self.f("default", inp)))
        dlg.setWindowFlags(dlg.windowFlags() | Qt.WindowType.WindowStaysOnTopHint)

        def done(result):
            if result:
                self.answer = dlg.textValue()
                if str(self.raw("var") or "").strip():
                    self.eng.set_var(str(self.raw("var")), self.answer)
                self.fire("answer", self.answer)
            else:
                self.fire("cancelled", inp)
        dlg.finished.connect(done)
        self._dlg = dlg
        dlg.open()
        widgets.bring_to_front(dlg)

    def value(self, port, inp):
        return self.answer


@node("sound", "Play a sound", "action", ins=["run"], outs=["done"], fields=[
    F("sound", "Sound", "choice", "chime", choices=[("chime", "Chime"), ("alarm", "Alarm"),
                                                    ("custom", "A sound file")]),
    F("file", "Sound file", "file", "")])
class SoundNode(Node):
    def receive(self, port, inp):
        sounds.play(str(self.raw("sound")), os.path.expanduser(str(self.raw("file") or "")))
        self.fire("done", inp)


@node("speak", "Say it out loud", "action", ins=["run"], outs=["done"], fields=[
    F("text", "Say", "text", "{input}", pin=True)],
    help=("Windows reads it with its own voice. Linux needs spd-say (speech-dispatcher) or espeak-ng."
          if IS_WIN else "Needs spd-say (speech-dispatcher) or espeak-ng / espeak."))
class Speak(Node):
    def receive(self, port, inp):
        text = str(self.f("text", inp)).strip()
        if not text:
            self.fire("done", inp)
            return
        p = QProcess()
        if IS_WIN:
            safe = text.replace("'", "''")
            p.setProgram("powershell.exe")
            p.setArguments(["-NoProfile", "-Command", "Add-Type -AssemblyName System.Speech; (New-Object "
                            f"System.Speech.Synthesis.SpeechSynthesizer).Speak('{safe}')"])
        else:
            exe = shutil.which("spd-say") or shutil.which("espeak-ng") or shutil.which("espeak")
            if not exe:
                self.set_status("⚠ install speech-dispatcher or espeak-ng")
                return
            p.setProgram(exe)
            p.setArguments((["-w"] if exe.endswith("spd-say") else []) + [text])
        p.finished.connect(lambda *_: (self.fire("done", inp), p.deleteLater()))
        self._p = p
        p.start()


@node("type_text", "Type text", "action", ins=["run"], outs=["done"], fields=[
    F("text", "Type", "multiline", "", pin=True),
    F("delay_ms", "Wait first (ms)", "number", 300)],
    help=("Types into the window that has the keyboard." if IS_WIN else
          "Types into the focused window (with wtype or ydotool if installed, else it pastes)."))
class TypeText(Node):
    def receive(self, port, inp):
        text = str(self.f("text", inp))
        QTimer.singleShot(int(max(0.0, self.num("delay_ms", default=300))), lambda: self._type(text, inp))

    def _type(self, text: str, inp: str):
        try:
            if IS_WIN:
                import keyboard
                keyboard.write(text, delay=0.005)
            elif shutil.which("wtype") and hypr.wayland():
                subprocess.Popen(["wtype", "--", text], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            elif shutil.which("ydotool"):
                subprocess.Popen(["ydotool", "type", "--", text], stdout=subprocess.DEVNULL,
                                 stderr=subprocess.DEVNULL)
            else:
                from core.hotkeys import HotkeyManager
                clip.set_text(text)
                QTimer.singleShot(120, lambda: HotkeyManager.send("ctrl+v"))
        except Exception as e:
            self.set_status(f"⚠ {e}")
            return
        self.fire("done", inp)


@node("keys", "Press keys", "action", ins=["run"], outs=["done"], fields=[
    F("combo", "Keys", "hotkey", "")],
    help="Presses a key combination (Ctrl+S, Alt+Tab…) in the focused window.", width=330)
class Keys(Node):
    def receive(self, port, inp):
        combo = str(self.raw("combo") or "")
        if combo:
            from core.hotkeys import HotkeyManager
            HotkeyManager.send(combo)
        self.fire("done", inp)


@node("set_clipboard", "Copy to clipboard", "action", ins=["run"], outs=["done"], fields=[
    F("text", "Text", "multiline", "{input}", pin=True)])
class SetClipboard(Node):
    def receive(self, port, inp):
        clip.set_text(str(self.f("text", inp)))
        self.fire("done", inp)


class _Relay(QObject):
    """Hands a worker thread's result back to the Qt thread."""
    done = pyqtSignal(object)


_relay: _Relay | None = None


def _later(fn):
    """Run `fn` on the Qt thread (from any thread)."""
    global _relay
    if _relay is None:
        _relay = _Relay()
        _relay.done.connect(lambda f: f())
    _relay.done.emit(fn)


@node("http", "Web request", "action", ins=["run"], outs=["done", "failed"], values=["body", "status"], fields=[
    F("method", "Method", "choice", "GET", choices=[(m, m) for m in ("GET", "POST", "PUT", "PATCH", "DELETE")]),
    F("url", "URL", "text", "https://", pin=True),
    F("body", "Body (for POST…)", "multiline", "", pin=True, hint='{"content": "{input}"}'),
    F("headers", "Headers (Name: value, one per line)", "multiline", "", hint="Content-Type: application/json")],
    help="Calls a web address (a Discord webhook, an API…). Done carries the answer's text.", width=320)
class Http(Node):
    def start(self):
        self.body = ""
        self.code = ""

    def receive(self, port, inp):
        import urllib.request
        url = str(self.f("url", inp)).strip()
        method = str(self.raw("method") or "GET")
        data = str(self.f("body", inp))
        headers = {"User-Agent": "Luminescent"}
        for ln in str(self.raw("headers") or "").splitlines():
            k, _, v = ln.partition(":")
            if k.strip() and v.strip():
                headers[k.strip()] = self.eng.expand(v.strip(), inp)
        if data and method != "GET" and not any(k.lower() == "content-type" for k in headers):
            headers["Content-Type"] = "application/json" if data.lstrip().startswith(("{", "[")) else "text/plain"
        self.set_status(f"{method} …")

        def work():
            try:
                req = urllib.request.Request(url, data=data.encode() if data and method != "GET" else None,
                                             headers=headers, method=method)
                with urllib.request.urlopen(req, timeout=30) as r:
                    body = r.read(MAX_TEXT).decode("utf-8", "replace")
                    code = r.status
                _later(lambda: self._done(True, body, code))
            except Exception as e:
                code = getattr(e, "code", "")
                try:
                    body = e.read(MAX_TEXT).decode("utf-8", "replace") if hasattr(e, "read") else str(e)
                except Exception:
                    body = str(e)
                body = body or str(e)
                _later(lambda: self._done(False, body, code))
        threading.Thread(target=work, daemon=True).start()

    def _done(self, ok: bool, body: str, code):
        if not self.alive:
            return
        self.body, self.code = body, str(code)
        self.set_status(f"{code}" if ok else f"⚠ failed {code}")
        self.fire("done" if ok else "failed", body)

    def value(self, port, inp):
        return self.code if port == "status" else self.body


@node("download", "Download a file", "action", ins=["run"], outs=["done", "failed"], fields=[
    F("url", "URL", "text", "https://", pin=True),
    F("folder", "Save in", "folder", "~/Downloads"),
    F("name", "File name", "text", "", pin=True, hint="(from the URL)")],
    help="Done carries the saved file's path.")
class Download(Node):
    def receive(self, port, inp):
        import urllib.request
        import urllib.parse
        url = str(self.f("url", inp)).strip()
        folder = os.path.expanduser(str(self.raw("folder") or "~/Downloads"))
        name = str(self.f("name", inp)).strip() or os.path.basename(urllib.parse.urlparse(url).path) or "download"
        path = os.path.join(folder, name)
        self.set_status("downloading…")

        def work():
            try:
                os.makedirs(folder, exist_ok=True)
                req = urllib.request.Request(url, headers={"User-Agent": "Luminescent"})
                with urllib.request.urlopen(req, timeout=60) as r, open(path, "wb") as out:
                    shutil.copyfileobj(r, out)
                _later(lambda: (self.set_status("done"), self.fire("done", path)))
            except Exception as e:
                msg = str(e)
                _later(lambda: (self.set_status(f"⚠ {msg}"), self.fire("failed", msg)))
        threading.Thread(target=work, daemon=True).start()


@node("file_op", "Copy / move / delete", "action", ins=["run"], outs=["done", "failed"], fields=[
    F("op", "Do", "choice", "copy", choices=[("copy", "Copy"), ("move", "Move"), ("delete", "Delete"),
                                            ("mkdir", "Make a folder")]),
    F("src", "File or folder", "file", "", pin=True),
    F("dst", "To", "file", "", pin=True, hint="a folder, or a new path")],
    help="Done carries the new path. Delete moves nothing to the bin: it is gone.")
class FileOp(Node):
    def receive(self, port, inp):
        op = self.raw("op")
        src = os.path.expanduser(str(self.f("src", inp)).strip())
        dst = os.path.expanduser(str(self.f("dst", inp)).strip())
        try:
            if op == "mkdir":
                os.makedirs(src, exist_ok=True)
                out = src
            elif op == "delete":
                shutil.rmtree(src) if os.path.isdir(src) else os.remove(src)
                out = src
            else:
                target = os.path.join(dst, os.path.basename(src.rstrip("/\\"))) if os.path.isdir(dst) else dst
                if op == "move":
                    out = shutil.move(src, target)
                elif os.path.isdir(src):
                    out = shutil.copytree(src, target)
                else:
                    out = shutil.copy2(src, target)
            self.set_status("")
            self.fire("done", str(out))
        except Exception as e:
            self.set_status(f"⚠ {e}")
            self.fire("failed", str(e))


@node("write_file", "Write to a file", "action", ins=["run"], outs=["done", "failed"], fields=[
    F("path", "File", "file", "", pin=True),
    F("text", "Text", "multiline", "{input}", pin=True),
    F("mode", "", "choice", "append", choices=[("append", "Add to the end"), ("overwrite", "Replace the file")]),
    F("newline", "End with a new line", "bool", True)])
class WriteFile(Node):
    def receive(self, port, inp):
        path = os.path.expanduser(str(self.f("path", inp)).strip())
        text = str(self.f("text", inp)) + ("\n" if self.raw("newline") else "")
        try:
            os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
            with open(path, "a" if self.raw("mode") == "append" else "w", encoding="utf-8") as f:
                f.write(text)
            self.fire("done", path)
        except OSError as e:
            self.set_status(f"⚠ {e}")
            self.fire("failed", str(e))


@node("alert", "Warning on screen", "action", ins=["run"], outs=["ok"], fields=[
    F("title", "Title", "text", "Heads up", pin=True),
    F("text", "Message", "multiline", "{input}", pin=True),
    F("level", "Look", "choice", "warning", choices=[("warning", "Warning (amber)"),
                                                      ("danger", "Danger (red)"), ("info", "Info (theme colour)"),
                                                      ("ok", "Good news (green)")]),
    F("seconds", "Close by itself after (seconds, 0 = stay)", "number", 0),
    F("sound", "Play a sound", "choice", "alarm", choices=[("none", "No sound"), ("chime", "Chime"),
                                                           ("alarm", "Alarm")])],
    help="A big warning in the middle of the screen, above everything, that stays until you click Got it. "
         "ok fires when it closes. A new one with the same title replaces the old one.")
class AlertNode(Node):
    def receive(self, port, inp):
        from ui import alert
        if self.raw("sound") != "none":
            sounds.play(str(self.raw("sound")), "")
        alert.show(str(self.f("title", inp)), str(self.f("text", inp)), str(self.raw("level") or "warning"),
                   self.num("seconds", inp), on_close=lambda: self.fire("ok", inp))


@node("wallpaper", "Set wallpaper", "action", ins=["run"], outs=["done", "failed"], fields=[
    F("mode", "Use", "choice", "file", choices=[("file", "This picture, GIF or video"),
                                                ("next", "The next one in a folder"),
                                                ("random", "A random one from a folder")]),
    F("path", "Picture or folder", "file", "", pin=True, hint="a file, or a folder for next / random")],
    help="Changes the desktop wallpaper. Wire a timer (Every… 30 min) to it for a slideshow. "
         + ("Windows shows the first frame of a GIF." if IS_WIN else
            "GIFs play with swww, videos with mpvpaper.")
         + " Settings → Overlay & Interface → Wallpaper command lets your own tool do it instead.")
class WallpaperNode(Node):
    def receive(self, port, inp):
        from core import wallpaper
        path = str(self.f("path", inp)).strip()
        mode = self.raw("mode")
        if mode in ("next", "random"):
            folder = path if os.path.isdir(os.path.expanduser(path)) else os.path.dirname(path)
            state = dict(self.data.get("state") or {})
            path = wallpaper.pick_from_folder(folder, mode, state)
            if not path:
                self.set_status("⚠ no pictures in that folder")
                self.fire("failed", inp)
                return
            self.save_state(last=state.get("last", ""))
        if not path:
            self.set_status("⚠ pick a picture")
            self.fire("failed", inp)
            return
        self._err = ""
        wallpaper.apply(path, self.eng.settings.get("ui.wallpaper_engine_exe", ""),
                        on_error=lambda msg: setattr(self, "_err", msg))
        self.set_status(os.path.basename(path))
        QTimer.singleShot(1500, lambda: self._after(path, inp))

    def _after(self, path, inp):
        if not self.alive:
            return
        if self._err:
            self.set_status("⚠ " + self._err)
            self.fire("failed", self._err)
        else:
            self.fire("done", path)


@node("volume", "Volume", "action", ins=["run"], outs=["done"], values=["volume"], fields=[
    F("mode", "Do", "choice", "set", choices=[("set", "Set it to"), ("up", "Turn it up by"),
                                              ("down", "Turn it down by"), ("mute", "Mute"),
                                              ("unmute", "Unmute"), ("toggle_mute", "Mute / unmute")]),
    F("amount", "Percent", "number", 50, pin=True)],
    help="The volume of your default speakers or headphones. The volume port reads it as a number.")
class VolumeNode(Node):
    def receive(self, port, inp):
        err = system.set_volume(str(self.raw("mode")), self.num("amount", inp, 50))
        if err:
            self.set_status("⚠ " + err)
            return
        got = system.get_volume()
        self.set_status(f"{got[0]:.0f} %{' (muted)' if got[1] else ''}" if got else "")
        self.fire("done", inp)

    def value(self, port, inp):
        got = system.get_volume()
        return f"{got[0]:.0f}" if got else ""


@node("power", "Lock / sleep / shut down", "action", ins=["run"], outs=["done"], fields=[
    F("action", "Do", "choice", "lock", choices=system.POWER_ACTIONS),
    F("ask", "Ask me first", "bool", True)],
    help="Locks the screen, turns the screens off, sleeps, signs out, restarts or shuts down the PC.")
class PowerNode(Node):
    def receive(self, port, inp):
        action = str(self.raw("action"))
        label = dict(system.POWER_ACTIONS).get(action, action)
        if self.raw("ask") and action not in ("lock", "screen_off"):
            from core import widgets
            if not widgets.confirm(None, "Automation", f"{self.label()}: {label.lower()} now?", yes=label):
                return
        self.fire("done", inp)
        # after the pulse went out, so a done → notification still shows before the screen locks
        QTimer.singleShot(300, lambda: self._do(action))

    def _do(self, action):
        err = system.power(action)
        if err:
            self.set_status("⚠ " + err)


@node("keep_awake", "Keep awake", "action", ins=["run"], outs=["done"], values=["on"], fields=[
    F("mode", "Turn it", "choice", "on", choices=[("on", "on"), ("off", "off"), ("toggle", "the other way")])],
    help="Stops the screen going dark and the PC going to sleep (until it is turned off, or Luminescent "
         "quits). E.g. Program starts (a game, OBS) → on, Program stops → off. Also in the tray.")
class KeepAwakeNode(Node):
    def receive(self, port, inp):
        from core import keepawake
        ka = keepawake.get()
        mode = self.raw("mode")
        on = (not ka.on) if mode == "toggle" else mode == "on"
        err = ka.set(on, f"Automation: {self.label()}")
        self.set_status(("⚠ " + err) if err else ("keeping awake" if ka.on else "off"))
        if not err:
            self.fire("done", inp)

    def value(self, port, inp):
        from core import keepawake
        return "yes" if keepawake.get().on else "no"


def _notes(eng) -> list:
    m = eng.launcher.modules.get("notes")
    panel = getattr(m, "panel", None)
    if panel is None:
        return []
    return [(n.get("id", ""), n.get("name") or "Note") for n in panel.data["notes"]]


@node("open_note", "Open a note", "action", ins=["run"], outs=["done"], fields=[
    F("note", "Note", "choice", "", choices=_notes)],
    help="Shows the Notes panel on that note.")
class OpenNote(Node):
    def receive(self, port, inp):
        m = self.eng.launcher.modules.get("notes")
        if m is None or not m.running:
            self.set_status("⚠ the Notes module is off")
            return
        m.show_note_id(str(self.raw("note")))
        self.fire("done", inp)


@node("show", "Show", "action", ins=["in"], outs=["out"],
      help="Shows what comes through (handy to see what a wire carries) and passes it on.")
class Show(Node):
    def receive(self, port, inp):
        self.set_status(inp.replace("\n", " ⏎ ")[:120] or "(empty)")
        log.info("show [%s]: %s", self.label(), inp[:500])
        self.fire("out", inp)


# ── logic ─────────────────────────────────────────────────────────────────────
@node("delay", "Wait", "logic", ins=["in", "cancel"], outs=["out"], fields=[
    F("seconds", "Seconds", "number", 5, pin=True)],
    help="Passes the pulse on after a while.")
class Delay(Node):
    live = True

    def start(self):
        self.timers: list[tuple[QTimer, float, float]] = []

    def receive(self, port, inp):
        if port == "cancel":
            self.stop()
            return
        secs = max(0.0, self.num("seconds", inp, 5))
        t = QTimer()
        t.setSingleShot(True)
        entry = (t, time.monotonic(), secs)
        t.timeout.connect(lambda: (self.timers.remove(entry) if entry in self.timers else None,
                                   self.fire("out", inp)))
        self.timers.append(entry)
        t.start(int(secs * 1000))

    def stop(self):
        for t, _a, _b in self.timers:
            t.stop()
        self.timers = []

    def info(self):
        if not self.timers:
            return "", None
        _t, t0, secs = self.timers[0]
        left = max(0.0, secs - (time.monotonic() - t0))
        more = f" (+{len(self.timers) - 1} waiting)" if len(self.timers) > 1 else ""
        return f"{left:.1f} s{more}", 1 - left / max(0.01, secs)


_OPS = [("equals", "is"), ("not_equals", "is not"), ("contains", "contains"), ("starts", "starts with"),
        ("ends", "ends with"), ("regex", "matches (regex)"), ("empty", "is empty"), ("not_empty", "is not empty"),
        ("gt", ">"), ("lt", "<"), ("ge", "≥"), ("le", "≤")]


def compare(a: str, op: str, b: str) -> bool:
    if op in ("gt", "lt", "ge", "le"):
        try:
            x, y = float(a.strip().replace(",", ".")), float(b.strip().replace(",", "."))
        except ValueError:
            return False
        return {"gt": x > y, "lt": x < y, "ge": x >= y, "le": x <= y}[op]
    al, bl = a.strip().lower(), b.strip().lower()
    if op == "equals":
        return al == bl
    if op == "not_equals":
        return al != bl
    if op == "contains":
        return bl in al
    if op == "starts":
        return al.startswith(bl)
    if op == "ends":
        return al.endswith(bl)
    if op == "regex":
        try:
            return re.search(b, a) is not None
        except re.error:
            return False
    if op == "empty":
        return not a.strip()
    return bool(a.strip())


@node("if", "If", "logic", ins=["in"], outs=["yes", "no"], fields=[
    F("a", "This", "text", "{input}", pin=True),
    F("op", "", "choice", "contains", choices=_OPS),
    F("b", "That", "text", "", pin=True)],
    help="Sends the pulse out of yes or no. Text is compared ignoring upper/lower case.")
class If(Node):
    def receive(self, port, inp):
        ok = compare(str(self.f("a", inp)), str(self.raw("op")), str(self.f("b", inp)))
        self.set_status("yes" if ok else "no")
        self.fire("yes" if ok else "no", inp)


@node("gate", "Gate", "logic", ins=["in", "open", "close", "toggle"], outs=["out"], values=["open"], fields=[
    F("start_open", "Open from the start", "bool", True)],
    help="Lets pulses through only while it is open.")
class Gate(Node):
    def start(self):
        self.open = bool(self.raw("start_open"))
        self.set_status("OPEN" if self.open else "CLOSED")

    def receive(self, port, inp):
        if port == "in":
            if self.open:
                self.fire("out", inp)
            return
        self.open = {"open": True, "close": False}.get(port, not self.open)
        self.set_status("OPEN" if self.open else "CLOSED")

    def value(self, port, inp):
        return "yes" if self.open else "no"


@node("counter", "Count", "logic", ins=["in", "reset"], outs=["every", "out"], values=["count"], fields=[
    F("every", "Fire “every” each … times", "number", 3, pin=True)],
    help="Counts pulses. Out passes every pulse on with the count; every fires each Nth time.")
class Counter(Node):
    def start(self):
        self.count = int(self.data["state"].get("count", 0))
        self.set_status(str(self.count))

    def receive(self, port, inp):
        if port == "reset":
            self.count = 0
        else:
            self.count += 1
            n = max(1, int(self.num("every", inp, 3)))
            self.fire("out", str(self.count))
            if self.count % n == 0:
                self.fire("every", str(self.count))
        self.save_state(count=self.count)
        self.set_status(str(self.count))

    def value(self, port, inp):
        return str(self.count)


@node("once", "Only once", "logic", ins=["in", "reset"], outs=["out"],
      help="Lets the first pulse through, then nothing until reset.")
class Once(Node):
    def start(self):
        self.used = False

    def receive(self, port, inp):
        if port == "reset":
            self.used = False
            self.set_status("")
        elif not self.used:
            self.used = True
            self.set_status("used")
            self.fire("out", inp)


@node("cooldown", "Cooldown", "logic", ins=["in"], outs=["out"], fields=[
    F("seconds", "At most once every (seconds)", "number", 10)],
    help="Lets a pulse through, then ignores the next ones for a while.")
class Cooldown(Node):
    def start(self):
        self.last = -1e9

    def receive(self, port, inp):
        now = time.monotonic()
        if now - self.last >= self.num("seconds", default=10):
            self.last = now
            self.fire("out", inp)


@node("sequence", "Sequence", "logic", ins=["in"], outs=["first", "second", "third", "fourth"],
      help="Fires its outputs one after another, first to fourth.")
class Sequence(Node):
    def receive(self, port, inp):
        for p in self.spec.outs:
            self.fire(p, inp)


@node("turns", "Take turns", "logic", ins=["in", "reset"], outs=["first", "second", "third", "fourth"], fields=[
    F("count", "How many outputs take turns", "number", 2)],
    help="Each pulse goes out of the next output: first, second, first, second…")
class Turns(Node):
    def start(self):
        self.i = 0

    def receive(self, port, inp):
        if port == "reset":
            self.i = 0
            return
        n = max(1, min(4, int(self.num("count", default=2))))
        self.fire(self.spec.outs[self.i % n], inp)
        self.i += 1


@node("random", "Random chance", "logic", ins=["in"], outs=["yes", "no"], fields=[
    F("percent", "Chance of yes (%)", "number", 50, pin=True)])
class Chance(Node):
    def receive(self, port, inp):
        self.fire("yes" if random.random() * 100 < self.num("percent", inp, 50) else "no", inp)


@node("repeat", "Repeat", "logic", ins=["in", "stop"], outs=["each", "done"], fields=[
    F("times", "Times", "number", 3, pin=True),
    F("gap", "Seconds between", "number", 1, pin=True)],
    help="Fires each the number of times (its input counts 1, 2, 3…), then done.")
class Repeat(Node):
    live = True

    def start(self):
        self.timer = QTimer()
        self.timer.timeout.connect(self._step)
        self.left = 0
        self.n = 0
        self.inp = ""

    def receive(self, port, inp):
        if port == "stop":
            self.timer.stop()
            self.left = 0
            return
        self.n, self.left, self.inp = 0, max(0, int(self.num("times", inp, 3))), inp
        self.timer.start(int(max(0.0, self.num("gap", inp, 1)) * 1000))
        self._step()

    def _step(self):
        if self.left <= 0:
            self.timer.stop()
            self.fire("done", self.inp)
            return
        self.left -= 1
        self.n += 1
        self.fire("each", str(self.n))
        if self.left <= 0:
            self.timer.stop()
            self.fire("done", self.inp)

    def stop(self):
        self.timer.stop()

    def info(self):
        return (f"{self.left} to go", None) if self.timer.isActive() else ("", None)


# ── values & text ─────────────────────────────────────────────────────────────
@node("text", "Text", "value", values=["text"], fields=[F("text", "", "multiline", "", hint="Hello {input}")],
      help="A piece of text. {input} becomes what came in with the pulse that asked for it.")
class TextNode(Node):
    def value(self, port, inp):
        return self.f("text", inp)


@node("number", "Number", "value", values=["number"], fields=[F("number", "", "number", 0)])
class NumberNode(Node):
    def value(self, port, inp):
        return _fmt(self.num("number", inp))


def _fmt(x: float) -> str:
    return str(int(x)) if float(x).is_integer() else f"{x:.6g}"


@node("var_get", "Variable", "value", values=["value"], fields=[F("name", "Name", "text", "", hint="name")],
      help="Reads a variable (kept between restarts). {var:name} in any field does the same.")
class VarGet(Node):
    def value(self, port, inp):
        return self.eng.get_var(str(self.raw("name") or ""))

    def start(self):
        self.eng.vars_changed.connect(self._show)
        self._show()

    def stop(self):
        try:
            self.eng.vars_changed.disconnect(self._show)
        except TypeError:
            pass

    def _show(self):
        v = self.eng.get_var(str(self.raw("name") or ""))
        self.set_status(f"= {v[:60]}" if v else "(empty)")


@node("var_set", "Set variable", "action", ins=["run"], outs=["done"], fields=[
    F("name", "Name", "text", "", hint="name"),
    F("value", "Value", "text", "{input}", pin=True),
    F("mode", "", "choice", "set", choices=[("set", "Set it to the value"), ("append", "Add the value to its end"),
                                           ("add", "Add the value as a number"), ("clear", "Empty it")])],
    help="Keeps a value for later (also after a restart). Done carries the new value.")
class VarSet(Node):
    def receive(self, port, inp):
        name = str(self.raw("name") or "").strip()
        if not name:
            self.set_status("⚠ give it a name")
            return
        v, old, mode = str(self.f("value", inp)), self.eng.get_var(name), self.raw("mode")
        if mode == "append":
            v = old + v
        elif mode == "add":
            try:
                v = _fmt(float(old or 0) + float(v or 0))
            except ValueError:
                self.set_status("⚠ not a number")
                return
        elif mode == "clear":
            v = ""
        self.eng.set_var(name, v)
        self.set_status(f"{name} = {v[:50]}")
        self.fire("done", v)


@node("join", "Join text", "value", values=["text"], fields=[
    F("a", "A", "text", "", pin=True),
    F("b", "B", "text", "", pin=True),
    F("c", "C", "text", "", pin=True),
    F("sep", "Between them", "text", " ", hint="(nothing)")],
    help="A, B and C joined into one text (empty ones are left out). Wire it into a command, a message…")
class Join(Node):
    def value(self, port, inp):
        parts = [str(self.f(k, inp)) for k in ("a", "b", "c")]
        return str(self.raw("sep") or "").join(p for p in parts if p != "")


@node("math", "Maths", "value", values=["result"], fields=[
    F("a", "A", "text", "0", pin=True),
    F("op", "", "choice", "+", choices=[(o, o) for o in ("+", "-", "×", "÷", "mod", "min", "max", "^")]),
    F("b", "B", "text", "0", pin=True)])
class MathNode(Node):
    def value(self, port, inp):
        try:
            a, b = float(str(self.f("a", inp)).replace(",", ".")), float(str(self.f("b", inp)).replace(",", "."))
            op = self.raw("op")
            r = {"+": lambda: a + b, "-": lambda: a - b, "×": lambda: a * b, "÷": lambda: a / b,
                 "mod": lambda: a % b, "min": lambda: min(a, b), "max": lambda: max(a, b),
                 "^": lambda: a ** b}[op]()
            return _fmt(r)
        except (ValueError, ZeroDivisionError, KeyError, OverflowError):
            return ""


@node("now", "Date / time", "value", values=["text"], fields=[
    F("format", "Format", "text", "%Y-%m-%d %H:%M", hint="%H:%M · %A %d %B")],
    help="Now, written your way (%Y year, %m month, %d day, %H hour, %M minute, %A weekday, %B month name).")
class Now(Node):
    def value(self, port, inp):
        try:
            return datetime.now().strftime(str(self.raw("format") or "%Y-%m-%d %H:%M"))
        except ValueError:
            return datetime.now().strftime("%Y-%m-%d %H:%M")


@node("clip_get", "Clipboard text", "value", values=["text"])
class ClipGet(Node):
    def value(self, port, inp):
        return clipboard_text()


@node("read_file", "Read a file", "value", values=["text"], fields=[F("path", "File", "file", "", pin=True)])
class ReadFile(Node):
    def value(self, port, inp):
        try:
            with open(os.path.expanduser(str(self.f("path", inp)).strip()), encoding="utf-8",
                      errors="replace") as fh:
                return fh.read(MAX_TEXT)
        except OSError:
            return ""


@node("replace", "Replace text", "value", values=["text"], fields=[
    F("text", "In", "text", "{input}", pin=True),
    F("find", "Find", "text", ""),
    F("with", "Replace with", "text", "", pin=True),
    F("regex", "Find is a regex", "bool", False)])
class Replace(Node):
    def value(self, port, inp):
        text, find, rep = str(self.f("text", inp)), str(self.raw("find") or ""), str(self.f("with", inp))
        if not find:
            return text
        if self.raw("regex"):
            try:
                return re.sub(find, rep, text)
            except re.error:
                return text
        return text.replace(find, rep)


@node("change", "Change text", "value", values=["text"], fields=[
    F("text", "Text", "text", "{input}", pin=True),
    F("op", "Make it", "choice", "trim", choices=[("upper", "UPPER CASE"), ("lower", "lower case"),
                                                  ("title", "Title Case"), ("trim", "trimmed (no spaces around)"),
                                                  ("first", "its first line"), ("last", "its last line"),
                                                  ("length", "its length"), ("lines", "its number of lines")])])
class Change(Node):
    def value(self, port, inp):
        t = str(self.f("text", inp))
        op = self.raw("op")
        lines = t.strip().splitlines()
        return {"upper": t.upper, "lower": t.lower, "title": t.title, "trim": t.strip,
                "first": lambda: lines[0] if lines else "", "last": lambda: lines[-1] if lines else "",
                "length": lambda: str(len(t)), "lines": lambda: str(len(lines))}.get(op, lambda: t)()


@node("random_num", "Random number", "value", values=["number"], fields=[
    F("min", "From", "number", 1, pin=True),
    F("max", "To", "number", 6, pin=True)])
class RandomNum(Node):
    def value(self, port, inp):
        a, b = int(self.num("min", inp, 1)), int(self.num("max", inp, 6))
        return str(random.randint(min(a, b), max(a, b)))


@node("comment", "Note", "other", fields=[F("text", "", "multiline", "")],
      help="Writing for yourself. It does nothing.", width=280)
class Comment(Node):
    pass
