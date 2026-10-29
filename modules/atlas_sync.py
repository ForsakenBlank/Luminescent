"""
Atlas Sync: swaps notes and timer time with the Atlas app on your phone (or another PC) over Wi-Fi.

Press "Let Atlas connect", then in Atlas open Sync and type the address and code shown here. Atlas compares both
sides and lists every change before anything is written. Notes go both ways (to-do lists as checklists), and
timer time adds up: what your timers ran here and what Atlas's timer trackers logged on the phone end up on both.
The protocol and what is remembered between syncs live in core/atlas_sync.py.
"""
import copy
import json
import os
import socket
import threading
from datetime import datetime

from PyQt6.QtWidgets import QHBoxLayout, QVBoxLayout
from PyQt6.QtCore import Qt, pyqtSignal, QTimer

from core import DATA_DIR, widgets, save_json, load_json, usage, theme
from core import atlas_sync
from core.module import Module

NOTES_PATH = os.path.join(DATA_DIR, "notes.json")


class AtlasSyncPanel(widgets.FloatingPanel):
    def __init__(self, m: "AtlasSyncModule"):
        self.m = m
        super().__init__("Atlas Sync", min_size=(340, 380), opacity=0.97, close_hides=True)
        lay = QVBoxLayout()
        lay.setContentsMargins(18, 14, 18, 18)
        lay.setSpacing(10)

        intro = widgets.label("Swap notes and timer time with Atlas on your phone. Both need to be on the same "
                              "Wi-Fi.", 12, tone="muted")
        intro.setWordWrap(True)
        lay.addWidget(intro)

        self.toggle = widgets.button("Let Atlas connect", "primary")
        self.toggle.clicked.connect(self.m.toggle_hosting)
        lay.addWidget(self.toggle)

        self.address_title = widgets.label("ADDRESS", 10, 600, tone="muted", spacing=1)
        self.address = widgets.label("", 18, 600)
        self.address.setTextInteractionFlags(Qt.TextInteractionFlag.TextSelectableByMouse)
        self.code_title = widgets.label("CODE", 10, 600, tone="muted", spacing=1)
        self.code = widgets.label("", 34, 700, mono=True, tone="accent")
        for w in (self.address_title, self.address, self.code_title, self.code):
            lay.addWidget(w)

        self.status = widgets.label("", 12)
        self.status.setWordWrap(True)
        lay.addWidget(self.status)
        lay.addStretch()

        self.last = widgets.label("", 11, tone="muted")
        self.last.setWordWrap(True)
        lay.addWidget(self.last)

        row = QHBoxLayout()
        row.addStretch()
        close = widgets.button("Close", "ghost")
        close.clicked.connect(self.hide)
        row.addWidget(close)
        lay.addLayout(row)

        self.body.addLayout(lay)
        self.resize(380, 440)
        self.center_on(widgets.cursor_screen_geometry())
        self.escape_pressed.connect(self.hide)
        self.refresh()

    def refresh(self):
        host = self.m.host
        on = host is not None and host.running
        self.toggle.setText("Stop" if on else "Let Atlas connect")
        for w in (self.address_title, self.address, self.code_title, self.code):
            w.setVisible(on)
        if on:
            addrs = atlas_sync.local_addresses()
            self.address.setText("\n".join(addrs) if addrs else "No network found")
            self.code.setText(f"{host.code[:3]} {host.code[3:]}")
        self.status.setText(self.m.last_status if on or self.m.last_status.startswith("Synced") else "")
        peers = self.m.store.state["peers"].values() if self.m.store else []
        if peers:
            from datetime import datetime
            newest = max(peers, key=lambda p: p.get("last", 0))
            when = datetime.fromtimestamp(newest.get("last", 0) / 1000).strftime("%d %b, %H:%M")
            self.last.setText(f"Last synced with {newest.get('name', 'Atlas')} on {when}")
        else:
            self.last.setText("Not synced yet")


class AtlasSyncModule(Module):
    key = "atlas_sync"
    title = "Atlas Sync"
    icon = "⇄"

    status_changed = pyqtSignal()
    calendar_changed = pyqtSignal()
    _run_on_gui = pyqtSignal(object)

    def __init__(self, ctx):
        super().__init__(ctx)
        self.panel: AtlasSyncPanel | None = None
        self.host: atlas_sync.Host | None = None
        self.store: atlas_sync.Store | None = None
        self.last_status = ""
        # the host listens on its own thread, anything touching notes or windows hops over to the qt thread
        self._run_on_gui.connect(self._gui_call, Qt.ConnectionType.QueuedConnection)
        self.status_changed.connect(self._refresh, Qt.ConnectionType.QueuedConnection)

    def start(self):
        self.store = atlas_sync.Store(os.path.join(DATA_DIR, "atlas_sync.json"))
        self.hotkey("hotkey", self.show_panel, "Atlas Sync")
        if not hasattr(self, "_cal_timer"):
            self._cal_timer = QTimer(self)
            self._cal_timer.setInterval(30_000)
            self._cal_timer.timeout.connect(self.check_calendar)
        self._cal_timer.start()
        QTimer.singleShot(5000, self.check_calendar)

    def stop(self):
        if hasattr(self, "_cal_timer"):
            self._cal_timer.stop()
        self.stop_hosting()
        if self.panel:
            self.panel.close()
            self.panel.deleteLater()
            self.panel = None

    def retheme(self):
        if self.panel and not self.panel.isVisible():
            self.panel.close()
            self.panel.deleteLater()
            self.panel = None

    def actions(self):
        return [("open", "Sync with Atlas…", self.show_panel),
                ("host", "Let Atlas connect", self.start_hosting)]

    # ── panel ──
    def show_panel(self):
        if self.panel is None:
            with self.themed():
                self.panel = AtlasSyncPanel(self)
        self.panel.refresh()
        self.panel.show_and_raise()

    def _refresh(self):
        if self.panel:
            self.panel.refresh()

    # ── hosting ──
    def toggle_hosting(self):
        if self.host and self.host.running:
            self.stop_hosting()
        else:
            self.start_hosting()

    def start_hosting(self):
        if self.host and self.host.running:
            self.show_panel()
            return
        self.host = atlas_sync.Host(self.store, socket.gethostname() or "PC", self._local, self._write, self._status)
        try:
            self.host.start()
        except OSError as e:
            self.host = None
            self.last_status = ""
            widgets.error(None, "Atlas Sync", f"Couldn't start listening on port {atlas_sync.PORT}: {e}")
            return
        self.show_panel()

    def stop_hosting(self):
        if self.host:
            self.host.stop()
            self.host = None
        self.last_status = ""
        self._refresh()

    def _status(self, text: str, frac=None):
        self.last_status = text
        self.status_changed.emit()
        if text.startswith("Synced") and "nothing new" not in text:
            self.ctx.notify("Atlas Sync", text)

    # ── reaching the notes and timers, always on the qt thread ──
    def _gui_call(self, job):
        fn, box, done = job
        try:
            box["result"] = fn()
        except Exception as e:
            box["error"] = e
        done.set()

    def _on_gui(self, fn):
        box, done = {}, threading.Event()
        self._run_on_gui.emit((fn, box, done))
        if not done.wait(60):
            raise atlas_sync.SyncError("Luminescent is busy, try again")
        if "error" in box:
            raise box["error"]
        return box.get("result")

    def _notes_module(self):
        launcher = self.ctx.launcher
        m = launcher.modules.get("notes") if launcher else None
        return m if m is not None and m.running and m.panel is not None else None

    @staticmethod
    def _refresh_notes(nm):
        """Notes changed underneath the open windows, so they all show the new state."""
        p = nm.panel
        ids = {n.get("id") for n in p.data["notes"]}
        for nid, st in list(nm.stickies.items()):
            if nid in ids:
                st.refresh()
            else:
                nm.unstick(nid)
        if p.current is not None and p.current < len(p.data["notes"]):
            p._show_note(p.current)
        else:
            p.show_home()
        for nid in ids:
            nm.note_changed.emit(nid, None)

    def _local(self):
        def read():
            nm = self._notes_module()
            if nm:
                nm.panel.flush()
                notes = copy.deepcopy(nm.panel.data["notes"])
            else:
                saved = load_json(NOTES_PATH, {}) or {}
                notes = [n for n in saved.get("notes", []) if isinstance(n, dict)]
            usage.flush()
            activity = []
            try:
                with open(usage.ACTIVITY_PATH, "r", encoding="utf-8", errors="replace") as f:
                    for ln in f:
                        try:
                            activity.append(json.loads(ln))
                        except ValueError:
                            pass
            except OSError:
                pass
            return notes, activity, self._theme_fields()
        return self._on_gui(read)

    def _theme_fields(self) -> dict | None:
        """The theme Luminescent is using, for Atlas (Settings → Atlas Sync → Send my theme)."""
        launcher = self.ctx.launcher
        if launcher is None or not self.s("share_theme", True):
            return None
        t = launcher.themes.active()
        cols = t.get("colours", {})
        return atlas_sync.atlas_theme(t.get("name", "Theme"), cols, theme.luminance(cols.get("bg_base", "#000")) <= 140)

    def _import_themes(self):
        """Atlas themes that came with a sync become Luminescent themes ("Atlas: name")."""
        launcher = self.ctx.launcher
        if launcher is None or not self.s("take_themes", True):
            return
        for rec in self.store.incoming_themes:
            name = str(rec["fields"].get("name") or "Theme")
            try:
                launcher.themes.from_colours(f"Atlas: {name}", atlas_sync.theme_roles(rec["fields"]))
            except Exception:
                self.log.exception("could not make a theme from Atlas theme %s", name)
        self.store.incoming_themes = []

    # ── Atlas's calendar on this PC ──
    def calendar(self) -> tuple[list, list]:
        return atlas_sync.calendar(self.store) if self.store else ([], [])

    def check_calendar(self):
        """A desktop notification for Atlas events about to start and tasks due today, like the phone shows."""
        if not self.store or not self.s("notify_calendar", True):
            return
        now = datetime.now()
        now_ms = int(now.timestamp() * 1000)
        notified = self.store.state["notified"]
        before = len(notified)
        events, tasks = self.calendar()
        lead_default = int(self.s("lead_minutes", 15) or 15)
        for e in events:
            mins = e.get("reminderMinutes")
            mins = lead_default if mins is None else int(mins)
            if mins < 0:
                continue
            for start in atlas_sync.occurrences(e, now_ms - 60_000, now_ms + (mins + 1) * 60_000):
                key = f"{e['uid']}@{start}"
                if key in notified or start - mins * 60_000 > now_ms:
                    continue
                notified.append(key)
                when = datetime.fromtimestamp(start / 1000)
                text = "now" if start <= now_ms + 60_000 else f"at {when:%H:%M}"
                if e.get("allDay"):
                    text = "today"
                self.ctx.toast(f"📅 {e.get('title') or 'Event'}",
                               f"Starts {text}" + (f" · {e['location']}" if e.get("location") else ""))
        hour = int(self.s("task_hour", 9) or 9)
        for t in tasks:
            if t.get("done") or not t.get("remind", True) or t.get("due") is None:
                continue
            due = datetime.fromtimestamp(int(t["due"]) / 1000)
            key = f"{t['uid']}@{due:%Y-%m-%d}"
            if due.date() == now.date() and now.hour >= hour and key not in notified:
                notified.append(key)
                self.ctx.toast(f"✓ {t.get('title') or 'Task'}", "Due today (from Atlas)")
        if len(notified) != before:
            del notified[:-500]
            self.store.save()

    def _write(self, change):
        def write():
            nm = self._notes_module()
            if nm:
                nm.panel.flush()
                lines = change(nm.panel.data["notes"])
                nm.panel._save_now()
                self._refresh_notes(nm)
            else:
                saved = load_json(NOTES_PATH, {}) or {}
                saved.setdefault("notes", [])
                lines = change(saved["notes"])
                save_json(NOTES_PATH, saved, indent=2)
            if lines:
                usage.flush()
                with open(usage.ACTIVITY_PATH, "a", encoding="utf-8") as f:
                    f.write("".join(json.dumps(ln, ensure_ascii=False) + "\n" for ln in lines))
            self._import_themes()
            QTimer.singleShot(0, self.calendar_changed.emit)
        self._on_gui(write)
