"""
Dashboard: hack-client style "tab boxes" you can drag around, collapse, and
fill with items that open a script, program, file, folder, URL or command.

    data/dashboard.json:
    {"layout": 3, "active": board id, "locked": false, "screens": {screen name: board id},
     "boards": [{"id", "name",
                 "tabs": [{"id", "kind", "title", "x", "y", "w", "h", "collapsed", "screen", "page", "state",
                           "items": [{"id", "kind", "name", "path", "args", "hotkey", "action",
                                      "cwd", "admin", "ask_args", "terminal", "group"}]}]}]}

Boards are whole dashboards (Work, Gaming...) switched from the toolbar. The boxes float
on the whole screen (x/y are pixels on the screen named by `screen`, "" = the screen the
overlay opens on). `screens` pins a board to another screen: that screen then always shows
it, whichever board the toolbar has switched to. Layout 1 files (panel-relative boxes) and
layout 2 files (one board) are moved over once.

A tab's `kind` is "items" (a box of items) or "page": a whole overlay page (Themes,
Settings, Clipboard...) in a box you can resize, `page` says which one and `state`
remembers where it was (the Settings section, the Clipboard folder...).

Items: `kind` is "item" (default), "group" (runs the items listed in `group`, in order),
"heading" or "separator" (they only tidy a box up). `action` items ("notes:toggle") run a
module action instead of a file; the built-in Tools tab is made of those. Item hotkeys are
registered with owner "dashboard" for every board, whether or not the overlay is open.

Rows can be dragged within a box and onto another box; files dragged in from the file
manager become items. The lock button stops all of that (and moving the boxes).
"""
import os
import sys
import json
import uuid
import logging

from PyQt6.QtWidgets import (
    QWidget, QFrame, QLabel, QVBoxLayout, QHBoxLayout, QScrollArea, QDialog, QFormLayout, QLineEdit,
    QFileDialog, QSizePolicy, QPushButton, QCheckBox, QListWidget, QListWidgetItem, QAbstractItemView
)
from PyQt6.QtCore import Qt, QObject, QTimer, QPoint, QRect, QMimeData, QSize, QEasingCurve, pyqtSignal
from PyQt6.QtGui import QDrag, QIcon

from core import DATA_DIR, save_json, load_json, widgets, runner, desktop, usage, theme, events, anim
from core.guard import alive
from core import hotkeys as hk
from core.settings import ALL_PAGES

log = logging.getLogger("dashboard")

PATH = os.path.join(DATA_DIR, "dashboard.json")
GRID = 8                    # the default snap; Settings → Dashboard → Snap tabs to a grid
BOX_W = 264
PAGE_W, PAGE_H = 720, 520   # a new page box (Settings, Themes...)
PAGE_MIN = (360, 220)
ROW_H = 32
HEAD_ROW_H = 26
SEP_ROW_H = 13
HEADER_H = 34
GAP = 32                    # between boxes when tidied
ORIGIN = (96, 136)          # where the first box sits: below the logo and the page switcher
LAYOUT = 3
TOOLS_TAB = "tools"
ITEM_MIME = "application/x-luminescent-item"
BOX_MIME = "application/x-luminescent-boxes"       # boxes dragged onto another screen
TIDY_STYLES = [("columns", "Columns (every box keeps its width)"), ("grid", "Grid (tabs the same width)"),
               ("rows", "Rows (left to right, then the next row)")]
TIDY_ORDERS = [("position", "Keep the order they are in"), ("name", "By name, A to Z"),
               ("size", "Biggest first")]
ITEM_KEYS = ("kind", "name", "path", "args", "hotkey", "action", "cwd")
FULL_PAGES = ("dashboard", "automation")     # whole-screen pages: never a box on the dashboard


def _id() -> str:
    return uuid.uuid4().hex[:10]


def runnable(it: dict) -> bool:
    return it.get("kind", "item") in ("item", "group")


def _fill_item(it: dict):
    it.setdefault("id", _id())
    for k in ITEM_KEYS:
        it.setdefault(k, "")
    it["kind"] = it["kind"] or "item"
    it.setdefault("admin", False)
    it.setdefault("ask_args", False)
    it.setdefault("terminal", False)          # a command that runs in a terminal window you can read
    if not isinstance(it.get("group"), list):
        it["group"] = []


class DashboardStore(QObject):
    """The dashboard's data, item hotkeys and running items. Lives as long as the launcher, not the overlay."""
    changed = pyqtSignal()           # boards/tabs/items added, removed, renamed, moved between boxes
    switched = pyqtSignal()          # another board is the active one
    running_changed = pyqtSignal()   # an item started or its process ended

    def __init__(self, launcher):
        super().__init__()
        self.launcher = launcher
        self.data = self._load()
        self._save_timer = QTimer(self)
        self._save_timer.setSingleShot(True)
        self._save_timer.setInterval(400)
        self._save_timer.timeout.connect(self.save_now)
        self._procs: dict[str, list] = {}          # item id -> [Popen] it started that may still run
        self._poll = QTimer(self)
        self._poll.setInterval(1500)
        self._poll.timeout.connect(self._check_running)

    # ── persistence ────────────────────────────────────────────────────────
    def _load(self) -> dict:
        d = load_json(PATH, None)
        if not isinstance(d, dict) or not (isinstance(d.get("tabs"), list) or isinstance(d.get("boards"), list)):
            d = self._first_run()
            try:
                save_json(PATH, d, indent=2)
            except OSError:
                log.exception("could not write dashboard.json")
        if isinstance(d.get("tabs"), list) and d.get("layout", 1) < 2:
            self._to_screen_layout(d)
        if not isinstance(d.get("boards"), list):
            d["boards"] = [{"id": "main", "name": "Main", "tabs": d.pop("tabs", [])}]
            d["active"] = "main"
        d.pop("tabs", None)
        d["layout"] = LAYOUT
        d["boards"] = [b for b in d["boards"] if isinstance(b, dict)] or [{"id": "main", "name": "Main", "tabs": []}]
        for b in d["boards"]:
            b.setdefault("id", _id())
            b.setdefault("name", "Dashboard")
            b["tabs"] = [t for t in b.get("tabs", []) if isinstance(t, dict)]
            for t in b["tabs"]:
                t.setdefault("id", _id())
                t.setdefault("kind", "items")
                t.setdefault("title", "Tab")
                t.setdefault("x", 16)
                t.setdefault("y", 16)
                t.setdefault("w", BOX_W)
                t.setdefault("h", PAGE_H)
                t.setdefault("collapsed", False)
                t.setdefault("screen", "")
                if t["kind"] == "page":
                    t.setdefault("page", "settings")
                    if not isinstance(t.get("state"), dict):
                        t["state"] = {}
                t["items"] = [i for i in t.get("items", []) if isinstance(i, dict)]
                for it in t["items"]:
                    _fill_item(it)
        if not any(b["id"] == d.get("active") for b in d["boards"]):
            d["active"] = d["boards"][0]["id"]
        d.setdefault("locked", False)
        ids = {b["id"] for b in d["boards"]}
        pins = d.get("screens") if isinstance(d.get("screens"), dict) else {}
        d["screens"] = {k: v for k, v in pins.items() if isinstance(k, str) and v in ids}
        return d

    def _to_screen_layout(self, d: dict):
        """Panel-relative boxes (layout 1) → screen positions; the Tools tab loses its emoji prefixes."""
        icons = {key: m.icon for key, m in self.launcher.modules.items()}
        for t in d["tabs"]:
            t["x"] = int(t.get("x", 16)) - 16 + ORIGIN[0]
            t["y"] = int(t.get("y", 16)) - 8 + ORIGIN[1]
            if int(t.get("w", BOX_W)) == 232:
                t["w"] = BOX_W
            for it in t.get("items", []):
                icon = icons.get((it.get("action") or "").split(":")[0])
                if icon and isinstance(it.get("name"), str) and it["name"].startswith(icon):
                    it["name"] = it["name"][len(icon):].strip()
        d["layout"] = 2

    def _first_run(self) -> dict:
        tabs = [self.tools_tab(*ORIGIN)]
        # the retired File Launcher's shortcuts become a "Shortcuts" tab
        raw = getattr(self.launcher.settings, "raw_saved", {}) or {}
        old = (raw.get("file_launcher") or {}).get("shortcuts") or []
        items = []
        for sc in old:
            path = (sc.get("path") or "").strip()
            if path:
                items.append({"id": _id(), "name": os.path.basename(path.rstrip("\\/")) or path, "path": path,
                              "args": "", "hotkey": sc.get("hotkey", ""), "action": ""})
        if items:
            tabs.append({"id": _id(), "title": "Shortcuts", "x": ORIGIN[0] + BOX_W + GAP, "y": ORIGIN[1], "w": BOX_W,
                         "collapsed": False, "items": items})
            log.info("moved %d File Launcher shortcut(s) into the dashboard", len(items))
        return {"layout": LAYOUT, "active": "main", "boards": [{"id": "main", "name": "Main", "tabs": tabs}]}

    def tools_tab(self, x: int, y: int) -> dict:
        items = []
        for key, m in self.launcher.modules.items():
            for aid, label, _fn in m.actions():
                items.append({"id": _id(), "kind": "item", "name": label, "path": "", "args": "", "hotkey": "",
                              "action": f"{key}:{aid}"})
        return {"id": TOOLS_TAB, "kind": "items", "title": "Tools", "x": x, "y": y, "w": BOX_W, "h": PAGE_H,
                "collapsed": False, "screen": "", "items": items}

    def schedule_save(self):
        self._save_timer.start()

    def save_now(self):
        self._save_timer.stop()
        try:
            save_json(PATH, self.data, indent=2)
        except OSError:
            log.exception("could not save dashboard.json")

    def _structural(self):
        self.schedule_save()
        self.register_hotkeys()
        self.changed.emit()

    # ── boards ─────────────────────────────────────────────────────────────
    def boards(self) -> list[dict]:
        return self.data["boards"]

    def board(self, bid: str) -> dict | None:
        return next((b for b in self.data["boards"] if b["id"] == bid), None)

    def active_board(self) -> dict:
        return self.board(self.data.get("active")) or self.data["boards"][0]

    def switch_board(self, bid: str):
        if self.board(bid) is not None and bid != self.data.get("active"):
            self.data["active"] = bid
            self.schedule_save()
            self.switched.emit()
            events.emit("dashboard.switched", self.board(bid)["name"])

    def add_board(self, name: str, copy_from: str = "", empty: bool = False) -> dict:
        """A new board: a copy of `copy_from`, completely empty, or with just the Tools tab."""
        b = {"id": _id(), "name": name.strip() or "Dashboard", "tabs": []}
        if empty:
            pass
        elif copy_from:
            import copy
            src = self.board(copy_from)
            if src:
                b["tabs"] = copy.deepcopy(src["tabs"])
                for t in b["tabs"]:
                    if t["id"] != TOOLS_TAB:
                        t["id"] = _id()
                    for it in t["items"]:
                        it["id"], it["hotkey"] = _id(), ""     # a hotkey can only run one item
        else:
            b["tabs"] = [self.tools_tab(*ORIGIN)]
            b["tabs"][0]["id"] = _id()
        self.data["boards"].append(b)
        self.data["active"] = b["id"]
        self._structural()
        return b

    def rename_board(self, bid: str, name: str):
        b = self.board(bid)
        if b and name.strip():
            b["name"] = name.strip()
            self._structural()

    def move_board(self, bid: str, delta: int):
        boards = self.data["boards"]
        b = self.board(bid)
        i = boards.index(b) if b else -1
        if 0 <= i + delta < len(boards) and i >= 0:
            boards.insert(i + delta, boards.pop(i))
            self._structural()

    def delete_board(self, bid: str):
        if len(self.data["boards"]) < 2:
            return
        self.data["boards"] = [b for b in self.data["boards"] if b["id"] != bid]
        self.data["screens"] = {k: v for k, v in self.data.get("screens", {}).items() if v != bid}
        if self.data.get("active") == bid:
            self.data["active"] = self.data["boards"][0]["id"]
        self._structural()

    # ── screens ────────────────────────────────────────────────────────────
    def screen_board(self, screen: str) -> dict | None:
        """The board pinned to `screen`, if any."""
        return self.board(self.data.get("screens", {}).get(screen, ""))

    def pinned_screens(self, bid: str) -> list[str]:
        return [k for k, v in self.data.get("screens", {}).items() if v == bid]

    def set_screen_board(self, screen: str, bid: str = ""):
        """Pin a board to a screen ("" = the screen shows the active board again)."""
        pins = self.data.setdefault("screens", {})
        if bid and self.board(bid) is not None:
            pins[screen] = bid
        else:
            pins.pop(screen, None)
        self._structural()

    def move_tab_to_board(self, tid: str, bid: str):
        self.move_tabs_to_board([tid], bid)

    def move_tabs_to_board(self, tids: list[str], bid: str):
        dst = self.board(bid)
        moved = False
        for tid in tids:
            src, t = self.board_of_tab(tid), self.tab(tid)
            if src is None or dst is None or t is None or src is dst:
                continue
            src["tabs"].remove(t)
            dst["tabs"].append(t)
            moved = True
        if moved:
            self._structural()

    # ── queries ────────────────────────────────────────────────────────────
    def tabs(self) -> list[dict]:
        """The active board's tabs."""
        return self.active_board()["tabs"]

    def all_tabs(self) -> list[tuple[dict, dict]]:
        return [(b, t) for b in self.data["boards"] for t in b["tabs"]]

    def tab(self, tid: str) -> dict | None:
        return next((t for _b, t in self.all_tabs() if t["id"] == tid), None)

    def board_of_tab(self, tid: str) -> dict | None:
        return next((b for b, t in self.all_tabs() if t["id"] == tid), None)

    def find_item(self, iid: str) -> tuple[dict | None, dict | None]:
        for _b, t in self.all_tabs():
            for it in t["items"]:
                if it["id"] == iid:
                    return t, it
        return None, None

    def hotkey_users(self, exclude_item: str = "") -> dict[str, str]:
        return {hk.normalize(it["hotkey"]): f"Dashboard → {t['title']} → {it['name']}"
                for _b, t in self.all_tabs() for it in t["items"] if it.get("hotkey") and it["id"] != exclude_item}

    @property
    def locked(self) -> bool:
        return bool(self.data.get("locked"))

    def set_locked(self, on: bool):
        self.data["locked"] = bool(on)
        self.schedule_save()

    # ── edits ──────────────────────────────────────────────────────────────
    def _tabs_of(self, bid: str = "") -> list[dict]:
        b = self.board(bid) if bid else None
        return (b or self.active_board())["tabs"]

    def add_tab(self, title: str, x: int, y: int, screen: str = "", board: str = "") -> dict:
        t = {"id": _id(), "kind": "items", "title": title, "x": x, "y": y, "w": BOX_W, "h": PAGE_H,
             "collapsed": False, "screen": screen, "items": []}
        self._tabs_of(board).append(t)
        self._structural()
        return t

    def add_page_tab(self, page: str, title: str, x: int, y: int, w: int, h: int, screen: str = "",
                     board: str = "") -> dict:
        """A whole overlay page (Themes, Settings...) as a box on a board (the active one by default)."""
        t = {"id": _id(), "kind": "page", "page": page, "title": title, "x": x, "y": y, "w": w, "h": h,
             "collapsed": False, "screen": screen, "state": {}, "items": []}
        self._tabs_of(board).append(t)
        self._structural()
        return t

    def rename_tab(self, tid: str, title: str):
        t = self.tab(tid)
        if t and title.strip():
            t["title"] = title.strip()
            self._structural()

    def delete_tab(self, tid: str):
        self.delete_tabs([tid])

    def delete_tabs(self, tids: list[str]):
        gone = set(tids)
        for b in self.data["boards"]:
            b["tabs"] = [t for t in b["tabs"] if t["id"] not in gone]
        self._structural()

    def restore_tools(self, x: int, y: int, board: str = "", screen: str = ""):
        tabs = self._tabs_of(board)
        if not any(t["id"] == TOOLS_TAB for t in tabs):
            t = self.tools_tab(x, y)
            t.update({"kind": "items", "h": PAGE_H, "screen": screen})
            tabs.append(t)
            self._structural()

    def add_item(self, tid: str, item: dict, index: int = -1):
        t = self.tab(tid)
        if t is not None:
            _fill_item(item)
            if 0 <= index <= len(t["items"]):
                t["items"].insert(index, item)
            else:
                t["items"].append(item)
            self._structural()

    def add_paths(self, tid: str, paths: list[str], index: int = -1):
        """Files or folders dropped from the file manager become items."""
        t = self.tab(tid)
        if t is None:
            return
        for p in paths:
            p = p.rstrip("/")
            if not p:
                continue
            name = os.path.splitext(os.path.basename(p))[0] if os.path.isfile(p) else os.path.basename(p) or p
            if p.endswith(".desktop") and os.path.isfile(p):
                name = desktop._read_entry(p).get("Name") or name
            item = {"id": _id(), "kind": "item", "name": name, "path": p}
            _fill_item(item)
            if 0 <= index <= len(t["items"]):
                t["items"].insert(index, item)
                index += 1
            else:
                t["items"].append(item)
        self._structural()

    def update_item(self, iid: str, **kw):
        _t, it = self.find_item(iid)
        if it is not None:
            it.update(kw)
            self._structural()

    def delete_item(self, iid: str):
        for _b, t in self.all_tabs():
            t["items"] = [i for i in t["items"] if i["id"] != iid]
            for i in t["items"]:
                if iid in i.get("group", []):
                    i["group"] = [g for g in i["group"] if g != iid]
        self._structural()

    def move_item(self, iid: str, to_tab: str = "", delta: int = 0, index: int = -1):
        """Move an item: to another tab (at `index`, else the end), or `delta` places up/down, or to `index`."""
        t, it = self.find_item(iid)
        if it is None:
            return
        dst = self.tab(to_tab) if to_tab else t
        if dst is None:
            return
        old = t["items"].index(it)
        if delta and dst is t:
            index = old + delta
            if not 0 <= index < len(t["items"]):
                return
            t["items"].pop(old)
            t["items"].insert(index, it)
        else:
            t["items"].pop(old)
            if dst is t and 0 <= index and old < index:
                index -= 1                    # the list just got shorter above the drop point
            if 0 <= index <= len(dst["items"]):
                dst["items"].insert(index, it)
            else:
                dst["items"].append(it)
        self._structural()

    def sort_tab(self, tid: str):
        t = self.tab(tid)
        if t:
            t["items"].sort(key=lambda i: i["name"].lower())
            self._structural()

    def set_geometry(self, tid: str, **kw):
        t = self.tab(tid)
        if t:
            t.update(kw)
            self.schedule_save()

    # ── running ────────────────────────────────────────────────────────────
    def run_item(self, iid: str, args: str | None = None):
        """Run an item. `args` replaces its arguments (Run with arguments…); "ask for arguments" items ask."""
        _t, it = self.find_item(iid)
        if it is None or not runnable(it):
            return
        usage.record("items", iid, it.get("name", ""))
        usage.activity("run", it.get("name") or it.get("path") or "item")
        events.emit("dashboard.item_run", it.get("name") or "")
        if it.get("kind") == "group":
            self._run_group(it)
            return
        if it.get("action"):
            self.launcher.run_action(it["action"])
            return
        if args is None and it.get("ask_args"):
            text, ok = widgets.ask_text(None, it.get("name") or "Arguments",
                                        f"Arguments for {it.get('name') or it.get('path')}:", it.get("args", ""))
            if not ok:
                return
            args = text
        s = self.launcher.settings
        cmd = it.get("path", "").strip()
        if it.get("terminal") and cmd and not os.path.exists(os.path.expanduser(cmd)) and "://" not in cmd:
            line = f"{cmd} {it.get('args', '') if args is None else args}".strip()
            err = runner.open_terminal(os.path.expanduser(it.get("cwd") or "~"), line, s.get("dashboard.terminal", ""))
            runner.log_command(line)
            if err:
                self.launcher.notify(it.get("name") or "Dashboard", err)
            return
        err, proc = runner.launch(it.get("path", ""), it.get("args", "") if args is None else args, it.get("name", ""),
                                  {"use_terminal": s.get("dashboard.use_terminal", True),
                                   "keep_open": s.get("dashboard.keep_open", True),
                                   "python_path": s.get("dashboard.python_path", ""),
                                   "terminal": s.get("dashboard.terminal", ""),
                                   "cwd": it.get("cwd", ""), "admin": bool(it.get("admin"))})
        if err:
            self.launcher.notify(it.get("name") or "Dashboard", err)
        if proc is not None:
            self._procs.setdefault(iid, []).append(proc)
            if not self._poll.isActive():
                self._poll.start()
            self.running_changed.emit()

    def _run_group(self, group: dict):
        ids = [i for i in group.get("group", []) if i != group["id"]]
        delay = max(0, int(self.launcher.settings.get("dashboard.group_delay_ms", 400)))
        for n, iid in enumerate(ids):
            QTimer.singleShot(n * delay, lambda i=iid: self.run_item(i))
        if not ids:
            self.launcher.notify(group.get("name") or "Group", "This group has no items yet (right-click → Edit…).")

    def is_running(self, iid: str) -> bool:
        return any(p.poll() is None for p in self._procs.get(iid, []))

    def stop_item(self, iid: str):
        stopped = sum(runner.stop(p) for p in self._procs.get(iid, []))
        _t, it = self.find_item(iid)
        if stopped and it:
            self.launcher.notify(it.get("name") or "Dashboard", "Stopped.", 1500)
        QTimer.singleShot(500, self._check_running)

    def _check_running(self):
        changed = False
        for iid in list(self._procs):
            alive = [p for p in self._procs[iid] if p.poll() is None]
            if len(alive) != len(self._procs[iid]):
                changed = True
            if alive:
                self._procs[iid] = alive
            else:
                del self._procs[iid]
        if not self._procs:
            self._poll.stop()
        if changed:
            self.running_changed.emit()

    def register_hotkeys(self):
        hotkeys = self.launcher.hotkeys
        hotkeys.unregister_all("dashboard")
        problems = []
        for _b, t in self.all_tabs():
            for it in t["items"]:
                combo = it.get("hotkey", "")
                if not combo or not runnable(it):
                    continue
                err = hotkeys.register("dashboard", combo, lambda i=it["id"]: self.run_item(i), f"run {it['name']}")
                if err:
                    problems.append(f"{it['name']}: {err}")
        if problems:
            self.launcher.notify("Dashboard hotkeys", "\n".join(problems))


# ── item dialog ───────────────────────────────────────────────────────────────
class ItemDialog(QDialog):
    def __init__(self, launcher, item: dict | None = None, parent=None, focus: str = "name", kind: str = "item"):
        super().__init__(parent)
        from settings_ui import HotkeyEdit
        self.launcher = launcher
        self.item = dict(item or {"kind": kind})
        self.kind = self.item.get("kind") or "item"
        self.setWindowTitle(("Edit " if item else "New ") + ("group" if self.kind == "group" else "item"))
        self.setWindowFlags(self.windowFlags() | Qt.WindowType.WindowStaysOnTopHint)
        self.setMinimumWidth(560)
        lay = QVBoxLayout(self)
        lay.setContentsMargins(18, 18, 18, 18)
        lay.setSpacing(12)
        form = QFormLayout()
        form.setSpacing(8)
        self.name = QLineEdit(self.item.get("name", ""))
        self.name.setPlaceholderText("Start my day" if self.kind == "group" else
                                     "What it shows in the tab, e.g. Backup photos")
        form.addRow("Name", self.name)

        self.is_action = bool(self.item.get("action"))
        self.path = QLineEdit(self.item.get("path", ""))
        self.path.setPlaceholderText("~/scripts/backup.py  ·  ~/Games  ·  https://…  ·  firefox  ·  an app's .desktop")
        self.args = QLineEdit(self.item.get("args", ""))
        self.ask = QCheckBox("Ask for arguments each time it runs")
        self.ask.setChecked(bool(self.item.get("ask_args")))
        self.cwd = QLineEdit(self.item.get("cwd", ""))
        self.cwd.setPlaceholderText("Default: the script's own folder (your home for commands)")
        self.admin = QCheckBox("Run as administrator")
        self.admin.setToolTip("Windows asks first (the administrator prompt)." if sys.platform == "win32" else
                              "Scripts run with sudo in their terminal; programs and commands ask through pkexec.")
        self.admin.setChecked(bool(self.item.get("admin")))
        self.term = QCheckBox("Commands run in a terminal window (to see their output)")
        self.term.setToolTip("For a command (not a file or folder): it runs in your terminal, which stays open.")
        self.term.setChecked(bool(self.item.get("terminal")))
        self.members: QListWidget | None = None
        if self.kind == "group":
            hint = QLabel("Tick the items it runs. Drag to change the order.")
            hint.setObjectName("lumHint")
            form.addRow("Runs", hint)
            self.members = QListWidget()
            self.members.setDragDropMode(QAbstractItemView.DragDropMode.InternalMove)
            self.members.setMinimumHeight(220)
            chosen = list(self.item.get("group", []))
            pool = [(it, t, b) for b, t in launcher.dashboard.all_tabs() for it in t["items"]
                    if runnable(it) and it.get("kind") != "group" and it["id"] != self.item.get("id")]
            by_id = {it["id"]: (it, t, b) for it, t, b in pool}
            order = [by_id[i] for i in chosen if i in by_id] + [x for x in pool if x[0]["id"] not in chosen]
            several = len(launcher.dashboard.boards()) > 1
            for it, t, b in order:
                where = f"{b['name']} › {t['title']}" if several else t["title"]
                li = QListWidgetItem(f"{it['name']}    ·  {where}")
                li.setData(Qt.ItemDataRole.UserRole, it["id"])
                li.setFlags(li.flags() | Qt.ItemFlag.ItemIsUserCheckable)
                li.setCheckState(Qt.CheckState.Checked if it["id"] in chosen else Qt.CheckState.Unchecked)
                self.members.addItem(li)
            form.addRow("", self.members)
        elif not self.is_action:
            row = QHBoxLayout()
            row.setSpacing(6)
            row.addWidget(self.path, 1)
            fb = widgets.button("File…")
            fb.clicked.connect(self._pick_file)
            db = widgets.button("Folder…")
            db.clicked.connect(self._pick_folder)
            row.addWidget(fb)
            row.addWidget(db)
            form.addRow("Opens", row)
            self.args.setPlaceholderText("Optional arguments for scripts and programs")
            form.addRow("Arguments", self.args)
            form.addRow("", self.ask)
            crow = QHBoxLayout()
            crow.setSpacing(6)
            crow.addWidget(self.cwd, 1)
            cb = widgets.button("Folder…")
            cb.clicked.connect(lambda: self._pick_into(self.cwd))
            crow.addWidget(cb)
            form.addRow("Working folder", crow)
            form.addRow("", self.admin)
            form.addRow("", self.term)
        else:
            lbl = QLabel(f"Runs a Luminescent action ({self.item['action']})")
            lbl.setObjectName("lumHint")
            form.addRow("Opens", lbl)
        self.hotkey = HotkeyEdit()
        self.hotkey.set_value(self.item.get("hotkey", ""))
        self.hotkey.recording_changed.connect(launcher.hotkeys.set_suspended)
        self.hotkey.conflict_fn = lambda combo: self._conflict(combo)
        form.addRow("Hotkey", self.hotkey)
        lay.addLayout(form)

        btns = QHBoxLayout()
        btns.addStretch()
        cancel = widgets.button("Cancel")
        cancel.clicked.connect(self.reject)
        ok = widgets.button("Save", "primary")
        ok.setDefault(True)
        ok.clicked.connect(self._ok)
        btns.addWidget(cancel)
        btns.addWidget(ok)
        lay.addLayout(btns)
        target = {"path": self.path.setFocus, "hotkey": self.hotkey.focus_edit}.get(focus, self.name.setFocus)
        QTimer.singleShot(0, lambda: (widgets.bring_to_front(self), target()))
        if focus == "file":
            QTimer.singleShot(50, self._pick_file)

    def _pick_file(self):
        p, _ = QFileDialog.getOpenFileName(self, "Choose a script, program or file",
                                           os.path.expanduser(self.path.text()) or os.path.expanduser("~"))
        if p:
            self.path.setText(p)
            if not self.name.text().strip():
                self.name.setText(os.path.splitext(os.path.basename(p))[0])

    def _pick_folder(self):
        p = QFileDialog.getExistingDirectory(self, "Choose a folder", os.path.expanduser(self.path.text()))
        if p:
            self.path.setText(p)
            if not self.name.text().strip():
                self.name.setText(os.path.basename(p.rstrip("/")) or p)

    def _pick_into(self, edit: QLineEdit):
        p = QFileDialog.getExistingDirectory(self, "Choose a folder", os.path.expanduser(edit.text()))
        if p:
            edit.setText(p)

    def _conflict(self, combo: str) -> str:
        if not combo:
            return ""
        owner = self.launcher.hotkeys.owner_of(combo)
        if owner and owner != "dashboard":
            return f"Already used by {owner}"
        other = self.launcher.dashboard.hotkey_users(self.item.get("id", "")).get(hk.normalize(combo))
        return f"Already used by {other}" if other else ""

    def _ok(self):
        combo = self.hotkey.value()
        if combo:
            err = hk.validate(combo) or self._conflict(combo)
            if err:
                widgets.error(self, "Hotkey", f"{hk.pretty(combo)}: {err}")
                return
        name = self.name.text().strip()
        path = self.path.text().strip()
        if self.kind == "group":
            self.item.update({"name": name or "Group", "hotkey": combo, "kind": "group",
                              "group": [self.members.item(i).data(Qt.ItemDataRole.UserRole)
                                        for i in range(self.members.count())
                                        if self.members.item(i).checkState() == Qt.CheckState.Checked]})
            self.accept()
            return
        if not self.is_action and not path:
            widgets.error(self, "Item", "Choose what this item opens.")
            return
        self.item.update({"name": name or os.path.basename(path.rstrip("/")) or path, "hotkey": combo})
        if not self.is_action:
            self.item.update({"path": path, "args": self.args.text().strip(), "ask_args": self.ask.isChecked(),
                              "cwd": self.cwd.text().strip(), "admin": self.admin.isChecked(),
                              "terminal": self.term.isChecked()})
        self.accept()

    def done(self, r):
        self.launcher.hotkeys.set_suspended(False)
        super().done(r)


# ── widgets ───────────────────────────────────────────────────────────────────
_BADGES = {"url": "URL", "folder": "DIR", "python": "PY", "file": "FILE", "command": "CMD", "none": "·"}
_icon_cache: dict[str, QIcon | None] = {}


def badge_for(item: dict) -> str:
    """The little type tag in front of a row: APP, DIR, URL, CMD, FILE, PY, GRP, or TOOL for Luminescent actions."""
    if item.get("kind") == "group":
        return "GRP"
    if item.get("action"):
        return "TOOL"
    path = item.get("path", "")
    kind = runner.kind_of(path)
    if kind == "program":
        ext = os.path.splitext(path.strip().strip('"'))[1].lower()
        return "SH" if ext in (".sh", ".bash", ".zsh", ".fish") else "APP"
    return _BADGES.get(kind, "·")


def icon_of(item: dict) -> QIcon | None:
    path = item.get("path", "")
    if not path or item.get("action") or item.get("kind") == "group":
        return None
    if path not in _icon_cache:
        try:
            _icon_cache[path] = desktop.icon_for(path, runner.kind_of(path))
        except Exception:
            log.debug("icon lookup failed for %s", path, exc_info=True)
            _icon_cache[path] = None
    return _icon_cache[path]


class ItemRow(QFrame):
    def __init__(self, box: "TabBox", item: dict):
        super().__init__()
        self.box = box
        self.item = item
        self.kind = item.get("kind", "item")
        self._press = None
        self.setObjectName("tabRow")
        lay = QHBoxLayout(self)
        if self.kind == "separator":
            self.setFixedHeight(SEP_ROW_H)
            lay.setContentsMargins(12, 0, 12, 0)
            lay.addWidget(widgets.rule())
            self.setToolTip("Separator · drag to move · right-click to delete")
            return
        if self.kind == "heading":
            self.setFixedHeight(HEAD_ROW_H)
            lay.setContentsMargins(12, 6, 12, 0)
            h = widgets.label((item.get("name") or "").upper(), 9.5, 600, mono=True, spacing=0.14, tone="")
            h.setObjectName("tabHeading")
            lay.addWidget(h, 1)
            self.setToolTip("Heading · drag to move · right-click to rename")
            return
        self.setFixedHeight(ROW_H)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        lay.setContentsMargins(12, 0, 12, 0)
        lay.setSpacing(10)
        icon = icon_of(item) if box.page.launcher.settings.get("dashboard.show_icons", True) else None
        if icon is not None:
            pic = QLabel()
            pic.setPixmap(icon.pixmap(QSize(18, 18)))
            pic.setFixedSize(30, 18)
            pic.setAlignment(Qt.AlignmentFlag.AlignCenter)
            lay.addWidget(pic)
        else:
            tag = widgets.label(badge_for(item), 8.5, 500, mono=True, spacing=0.06, tone="")
            tag.setObjectName("typeBadge")
            tag.setAlignment(Qt.AlignmentFlag.AlignCenter)
            tag.setFixedSize(30, 17)
            lay.addWidget(tag)
        name = widgets.label(item.get("name") or "(unnamed)", 13)
        name.setObjectName("tabRowText")
        name.setSizePolicy(QSizePolicy.Policy.Ignored, QSizePolicy.Policy.Preferred)
        lay.addWidget(name, 1)
        if item.get("admin"):
            lay.addWidget(widgets.label("ROOT", 8.5, 600, mono=True, tone="danger"))
        if box.page.store.is_running(item["id"]):
            dot = QFrame()
            dot.setFixedSize(7, 7)
            dot.setStyleSheet("background: #3FBF74; border-radius: 3px;")
            dot.setToolTip("Running · right-click → Stop")
            lay.addWidget(dot)
        if item.get("hotkey"):
            lay.addWidget(widgets.label(hk.pretty(item["hotkey"]), 11, mono=True, tone="muted"))
        for w in self.findChildren(QWidget):
            w.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        if self.kind == "group":
            members = [box.page.store.find_item(i)[1] for i in item.get("group", [])]
            tip = "Runs, in order:\n" + "\n".join(f"  {n + 1}. {m['name']}" for n, m in enumerate(m for m in members if m))
        else:
            tip = item.get("action") or item.get("path", "")
            if item.get("args"):
                tip += "  " + item["args"]
            if item.get("cwd"):
                tip += f"\nin {item['cwd']}"
            if item.get("ask_args"):
                tip += "\nAsks for arguments"
        self.setToolTip(tip + "\nDouble-click to run · drag to move · right-click for options")

    def search_text(self) -> str:
        return f"{self.item.get('name', '')} {self.item.get('path', '')} {self.item.get('action', '')}".lower()

    def mousePressEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton:
            self._press = e.position().toPoint()
        super().mousePressEvent(e)

    def mouseMoveEvent(self, e):
        if self._press is None or not (e.buttons() & Qt.MouseButton.LeftButton) or self.box.page.store.locked:
            return
        if (e.position().toPoint() - self._press).manhattanLength() < 8:
            return
        self._press = None
        drag = QDrag(self)
        md = QMimeData()
        md.setData(ITEM_MIME, self.item["id"].encode())
        md.setText(self.item.get("name", ""))
        drag.setMimeData(md)
        px = self.grab()
        drag.setPixmap(px)
        drag.setHotSpot(QPoint(px.width() // 6, px.height() // 2))
        drag.exec(Qt.DropAction.MoveAction)

    def mouseReleaseEvent(self, e):
        self._press = None
        super().mouseReleaseEvent(e)

    def mouseDoubleClickEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton and runnable(self.item):
            self.box.page.run(self.item["id"])

    def contextMenuEvent(self, e):
        self.box.page.item_menu(self.box, self.item, e.globalPos())


def snap_to(v: float, g: int) -> int:
    """Round to the snap grid; g of 0 or 1 = no grid (pixel by pixel)."""
    return int(round(v / g) * g) if g > 1 else int(round(v))


def _repolish(w: QWidget):
    w.style().unpolish(w)
    w.style().polish(w)


class _Grip(QWidget):
    """Resize handle on a box's right edge, bottom edge or bottom-right corner."""
    CURSORS = {"r": Qt.CursorShape.SizeHorCursor, "b": Qt.CursorShape.SizeVerCursor,
               "rb": Qt.CursorShape.SizeFDiagCursor}

    def __init__(self, box: "_Box", edge: str):
        super().__init__(box)
        self.box = box
        self.edge = edge
        self.setCursor(self.CURSORS[edge])
        self._start = None

    def place(self):
        w, h, t = self.box.width(), self.box.height(), 6
        if self.edge == "r":
            self.setGeometry(w - t, HEADER_H, t, max(0, h - HEADER_H - 2 * t))
        elif self.edge == "b":
            self.setGeometry(0, h - t, max(0, w - 2 * t), t)
        else:
            self.setGeometry(w - 2 * t, h - 2 * t, 2 * t, 2 * t)
        self.setVisible(self.edge == "r" or not self.box.tab.get("collapsed"))
        self.raise_()

    def mousePressEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton and not self.box.page.store.locked:
            self._start = (e.globalPosition().toPoint(), self.box.width(), self.box.body_height())

    def mouseMoveEvent(self, e):
        if self._start is None:
            return
        p0, w0, h0 = self._start
        d = e.globalPosition().toPoint() - p0
        g = self.box.page.snap()
        w = snap_to(w0 + d.x(), g) if "r" in self.edge else w0
        h = snap_to(h0 + d.y(), g) if "b" in self.edge else h0
        self.box.resize_to(w, h)

    def mouseReleaseEvent(self, e):
        if self._start is not None:
            self._start = None
            self.box.resize_done()


class _DropLine(QFrame):
    def __init__(self, parent):
        super().__init__(parent)
        self.setFixedHeight(2)
        self.setStyleSheet(f"background: {theme.P.primary}; border: none;")
        self.hide()


class _Box(QFrame):
    """What every box on the dashboard shares: dragging by the header (on the snap grid, with the rest of
    the selection, onto another screen), being selected, and the screen (canvas) it lives on."""

    def __init__(self, page: "DashboardPage", tab: dict, canvas: "Canvas"):
        super().__init__(canvas)
        self.page = page
        self.tab = tab
        self.canvas = canvas
        self.sig = None
        self.setObjectName("tabBox")
        self._press = None
        self._dragging = False
        self._group: list[tuple["_Box", QPoint]] = []
        self._grab = QPoint()

    def set_selected(self, on: bool):
        if bool(self.property("selected")) != on:
            self.setProperty("selected", on)
            _repolish(self)

    def body_height(self) -> int:
        return self.height() - HEADER_H

    def resize_to(self, w: int, h: int):
        pass

    def resize_done(self):
        pass

    def clicked(self):
        """The header was clicked without moving: fold or unfold."""
        self.set_collapsed(not self.tab.get("collapsed"))
        self.page.store.set_geometry(self.tab["id"], collapsed=self.tab["collapsed"])

    def set_collapsed(self, on: bool):
        pass

    def fit(self):
        pass

    def mousePressEvent(self, e):
        if e.button() != Qt.MouseButton.LeftButton or e.position().y() > HEADER_H:
            super().mousePressEvent(e)
            return
        if e.modifiers() & (Qt.KeyboardModifier.ControlModifier | Qt.KeyboardModifier.ShiftModifier):
            self.page.toggle_selected(self)
            self._press = None
            return
        if self not in self.page.selected:
            self.page.clear_selection()
        anim.stop(self, "pos")
        self._press = (e.globalPosition().toPoint(), self.pos())
        self._grab = e.position().toPoint()
        self._group = [(b, b.pos()) for b in self.page.selected if b is not self and b.canvas is self.canvas]
        self._dragging = False
        for b, _p in self._group:
            b.raise_()
        self.raise_()
        self.page.raise_dock()

    def mouseMoveEvent(self, e):
        if self._press is None or self.page.store.locked:
            return
        start, origin = self._press
        d = e.globalPosition().toPoint() - start
        if not self._dragging and d.manhattanLength() < 4:
            return
        if not self._dragging:
            self._dragging = True
            self.header.setCursor(Qt.CursorShape.ClosedHandCursor)
            self.setProperty("active", True)
            _repolish(self)
        gp = e.globalPosition().toPoint()
        other = self.page.canvas_at(gp)
        if other is not None and other is not self.canvas:
            self._hand_off(origin)               # the mouse is on another screen: carry the box(es) over
            return
        g = self.page.snap()
        x, y = self.page.clamp(self.canvas, snap_to(origin.x() + d.x(), g), snap_to(origin.y() + d.y(), g),
                               self.width())
        self.move(x, y)
        dx, dy = x - origin.x(), y - origin.y()
        for b, o in self._group:
            b.move(*self.page.clamp(b.canvas, o.x() + dx, o.y() + dy, b.width()))

    def _hand_off(self, origin: QPoint):
        """Each screen is a window of its own and a widget can not leave its window, so a box dragged onto
        another screen continues as a drag and drop: the picture follows the mouse there and the screen it is
        dropped on takes the box(es)."""
        group = [(b, o) for b, o in self._group if alive(b)]
        boxes = [self] + [b for b, _o in group]
        homes = [(self, origin)] + group
        self._press = None
        self._dragging = False
        self._group = []
        self.header.setCursor(Qt.CursorShape.OpenHandCursor)
        self.setProperty("active", False)
        _repolish(self)
        for b, o in homes:
            b.move(o)                              # where they go back to if the drop is cancelled
        md = QMimeData()
        lead = self.pos()
        md.setData(BOX_MIME, json.dumps({"lead": self.tab["id"], "grab": [self._grab.x(), self._grab.y()],
                                         "boxes": [[b.tab["id"], b.x() - lead.x(), b.y() - lead.y()]
                                                   for b in boxes]}).encode())
        drag = QDrag(self)
        drag.setMimeData(md)
        px = self.grab()
        if px.height() > 360:                       # a tall page box: the top part is enough to see
            px = px.copy(0, 0, px.width(), 360)
        drag.setPixmap(px)
        drag.setHotSpot(self._grab)
        for b in boxes:
            b.hide()
        self.page.dragging_boxes = True
        try:
            done = drag.exec(Qt.DropAction.MoveAction) == Qt.DropAction.MoveAction
        finally:
            self.page.dragging_boxes = False
        if not done:
            for b in boxes:
                if alive(b):
                    b.show()

    def mouseReleaseEvent(self, e):
        if self._press is None:
            return
        self._press = None
        if not self._dragging:
            self.clicked()
            return
        self._dragging = False
        self.header.setCursor(Qt.CursorShape.OpenHandCursor)
        self.setProperty("active", False)
        _repolish(self)
        target = self.page.canvas_at(widgets.global_cursor(e))
        if target is not None and target is not self.canvas and not self._group:
            at = widgets.global_cursor(e)
            grab = QPoint(self._grab)
            QTimer.singleShot(0, lambda: self.page.move_box_to_canvas(self, target, target.mapFromGlobal(at) - grab)
                              if alive(self) and alive(target) else None)
            return
        for b in [self] + [b for b, _p in self._group]:
            b.fit()
            self.page.store.set_geometry(b.tab["id"], x=b.x(), y=b.y())
        self._group = []

    def contextMenuEvent(self, e):
        if e.pos().y() <= HEADER_H:
            self.page.tab_menu(self, e.globalPos())

    def _header(self, title: str, extra: list[QWidget] | None = None) -> QFrame:
        self.header = QFrame()
        self.header.setObjectName("tabHeader")
        self.header.setFixedHeight(HEADER_H)
        self.header.setCursor(Qt.CursorShape.PointingHandCursor if self.page.store.locked
                              else Qt.CursorShape.OpenHandCursor)
        hl = QHBoxLayout(self.header)
        hl.setContentsMargins(11, 0, 9, 0)
        hl.setSpacing(9)
        accent = QFrame()
        accent.setObjectName("tabAccent")
        accent.setFixedSize(3, 12)
        hl.addWidget(accent)
        self.title_lbl = widgets.label(title.upper(), 11, 600, mono=True, spacing=0.14)
        self.title_lbl.setObjectName("tabTitle")
        self.title_lbl.setSizePolicy(QSizePolicy.Policy.Ignored, QSizePolicy.Policy.Preferred)
        hl.addWidget(self.title_lbl, 1)
        for w in (accent, self.title_lbl):
            w.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        for w in extra or []:
            hl.addWidget(w)
        self.chevron = widgets.label("", 10, tone="muted")
        self.chevron.setFixedWidth(14)
        self.chevron.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.chevron.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        hl.addWidget(self.chevron)
        return self.header


class TabBox(_Box):
    """A box of items."""

    def __init__(self, page: "DashboardPage", tab: dict, canvas: "Canvas"):
        super().__init__(page, tab, canvas)
        self.setAcceptDrops(True)
        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(0)
        count = widgets.label(f"{sum(1 for i in tab['items'] if runnable(i)):02d}", 10, 500, mono=True, tone="muted")
        count.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        lay.addWidget(self._header(tab["title"], [count]))

        self.scroll = QScrollArea()
        self.scroll.setWidgetResizable(True)
        self.scroll.setFrameShape(QFrame.Shape.NoFrame)
        self.scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.body = QWidget()
        bl = QVBoxLayout(self.body)
        bl.setContentsMargins(0, 4, 0, 4)
        bl.setSpacing(0)
        self.rows: list[ItemRow] = []
        for it in tab["items"]:
            r = ItemRow(self, it)
            self.rows.append(r)
            bl.addWidget(r)
        if not tab["items"]:
            empty = widgets.label("Right-click the header → Add item,\nor drop files here", 12, tone="muted")
            empty.setAlignment(Qt.AlignmentFlag.AlignCenter)
            empty.setFixedHeight(ROW_H + 18)
            bl.addWidget(empty)
        bl.addStretch()
        self.scroll.setWidget(self.body)
        lay.addWidget(self.scroll)
        self._grip = _Grip(self, "r")
        self._drop_line = _DropLine(self.body)

        self.move(int(tab.get("x", ORIGIN[0])), int(tab.get("y", ORIGIN[1])))
        self.set_width(int(tab.get("w", BOX_W)))
        self.set_collapsed(bool(tab.get("collapsed")))

    def _body_height(self) -> int:
        if not self.tab["items"]:
            return ROW_H + 26
        return sum(r.height() for r in self.rows) + 8

    def target_height(self) -> int:
        if self.tab.get("collapsed"):
            return HEADER_H + 2
        room = max(HEADER_H + ROW_H + 12, self.page.bottom_limit(self) - self.y())
        return min(HEADER_H + 2 + self._body_height(), room)

    def fit(self):
        """Size to the content, but never past the bottom of the screen or the toolbar (the body scrolls)."""
        if anim.busy(self) and self.property("folding"):
            return                                   # a fold is gliding there already
        self.setFixedHeight(self.target_height())
        self._grip.place()

    def set_width(self, w: int):
        self.setFixedWidth(w)
        self.fit()

    def resize_to(self, w: int, h: int):
        w = max(184, min(720, w))
        self.set_width(min(w, self.canvas.width() - self.x()))

    def resize_done(self):
        self.page.store.set_geometry(self.tab["id"], w=self.width())

    def set_collapsed(self, on: bool):
        self.tab["collapsed"] = on
        self.chevron.setText("▸" if on else "▾")
        self.header.setProperty("collapsed", on)
        _repolish(self.header)
        _fold(self, self.scroll, on)

    def apply_filter(self, q: str) -> int:
        """Highlight the rows matching `q`, fade the rest. Returns how many matched."""
        n = 0
        for r in self.rows:
            hit = bool(q) and runnable(r.item) and q in r.search_text()
            n += hit
            for prop, val in (("match", hit), ("dim", bool(q) and not hit)):
                if r.property(prop) != val:
                    r.setProperty(prop, val)
                    _repolish(r)
                    for c in r.findChildren(QLabel):
                        _repolish(c)
        if q and n and self.tab.get("collapsed"):
            self.set_collapsed(False)
        return n

    # ── drops: rows from any box, files from the file manager ──────────────
    def _accepts(self, md: QMimeData) -> bool:
        if self.page.store.locked:
            return False
        return md.hasFormat(ITEM_MIME) or (md.hasUrls() and any(u.isLocalFile() for u in md.urls()))

    def _drop_index(self, pos_in_box: QPoint) -> int:
        y = self.body.mapFrom(self, pos_in_box).y()
        for i, r in enumerate(self.rows):
            if y < r.y() + r.height() / 2:
                return i
        return len(self.rows)

    def _show_line(self, index: int):
        if self.tab.get("collapsed"):
            self._drop_line.hide()
            return
        if index < len(self.rows):
            y = self.rows[index].y() - 1
        elif self.rows:
            y = self.rows[-1].y() + self.rows[-1].height() - 1
        else:
            y = 6
        self._drop_line.setGeometry(8, max(0, y), self.body.width() - 16, 2)
        self._drop_line.show()
        self._drop_line.raise_()

    def dragEnterEvent(self, e):
        if self._accepts(e.mimeData()):
            e.acceptProposedAction()
            self.setProperty("active", True)
            _repolish(self)

    def dragMoveEvent(self, e):
        if self._accepts(e.mimeData()):
            e.acceptProposedAction()
            self._show_line(self._drop_index(e.position().toPoint()))

    def dragLeaveEvent(self, e):
        self._drop_line.hide()
        self.setProperty("active", False)
        _repolish(self)

    def dropEvent(self, e):
        self.dragLeaveEvent(e)
        md = e.mimeData()
        index = self._drop_index(e.position().toPoint()) if not self.tab.get("collapsed") else -1
        if md.hasFormat(ITEM_MIME):
            iid = bytes(md.data(ITEM_MIME)).decode()
            e.acceptProposedAction()
            QTimer.singleShot(0, lambda: self.page.store.move_item(iid, to_tab=self.tab["id"], index=index))
            return
        paths = [u.toLocalFile() for u in md.urls() if u.isLocalFile()]
        if paths:
            e.acceptProposedAction()
            QTimer.singleShot(0, lambda: self.page.store.add_paths(self.tab["id"], paths, index))


class PageBox(_Box):
    """A whole overlay page (Themes, Settings, Clipboard, Notes...) in a box you can move and resize.
    The page is built a moment after the box (Settings takes a while) and remembers where it was."""

    def __init__(self, page: "DashboardPage", tab: dict, canvas: "Canvas"):
        super().__init__(page, tab, canvas)
        self.inner: QWidget | None = None
        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(0)
        full = widgets.icon_button("⤢", "Open it as the full page", 22, 12, ghost=True)
        full.clicked.connect(lambda: self.page.launcher.overlay.show_page(self.tab["page"]))
        close = widgets.icon_button("✕", "Take it off the dashboard", 22, 11, ghost=True)
        close.clicked.connect(lambda: self.page.remove_boxes([self]))
        lay.addWidget(self._header(tab.get("title") or page_name(tab["page"]), [full, close]))
        self.body = QFrame()
        self.body.setObjectName("pageBody")
        self.bl = QVBoxLayout(self.body)
        self.bl.setContentsMargins(0, 0, 0, 0)
        wait = widgets.label("Loading…", 12, tone="muted")
        wait.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.bl.addWidget(wait)
        lay.addWidget(self.body, 1)
        self._grips = [_Grip(self, e) for e in ("r", "b", "rb")]
        self.move(int(tab.get("x", ORIGIN[0])), int(tab.get("y", ORIGIN[1])))
        self.setFixedWidth(max(PAGE_MIN[0], int(tab.get("w", PAGE_W))))
        self.set_collapsed(bool(tab.get("collapsed")))

    def build(self):
        if self.inner is not None:
            return
        self.inner = self.page.launcher.overlay._make_page(self.tab["page"])
        widgets.clear_layout(self.bl)
        self.bl.addWidget(self.inner)
        restore = getattr(self.inner, "restore_state", None)
        if callable(restore) and self.tab.get("state"):
            try:
                restore(self.tab["state"])
            except Exception:
                log.exception("could not restore the %s box", self.tab["page"])
        if self.isVisible():
            self.shown()

    def safe_mode(self):
        widgets.clear_layout(self.bl)
        lbl = widgets.label("Safe mode: page boxes are not loaded.\nRestart normally from the tray.", 12,
                            tone="muted")
        lbl.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.bl.addWidget(lbl)

    def shown(self):
        fn = getattr(self.inner, "on_show", None)
        if callable(fn):
            fn()

    def hidden(self):
        self.save_state()
        fn = getattr(self.inner, "on_hide", None)
        if callable(fn):
            fn()

    def save_state(self):
        fn = getattr(self.inner, "state", None)
        if callable(fn):
            try:
                st = fn() or {}
            except Exception:
                log.exception("could not read the %s box's state", self.tab["page"])
                return
            if st != self.tab.get("state"):
                self.tab["state"] = st
                self.page.store.schedule_save()

    def teardown(self):
        self.save_state()
        fn = getattr(self.inner, "teardown", None)
        if callable(fn):
            try:
                fn()
            except Exception:
                log.exception("page teardown failed")

    def body_height(self) -> int:
        return int(self.tab.get("h", PAGE_H))

    def target_height(self) -> int:
        if self.tab.get("collapsed"):
            return HEADER_H + 2
        room = max(HEADER_H + PAGE_MIN[1], self.page.bottom_limit(self) - self.y())
        return min(HEADER_H + 2 + self.body_height(), room)

    def fit(self):
        if anim.busy(self) and self.property("folding"):
            return
        self.setFixedHeight(self.target_height())
        for g in self._grips:
            g.place()

    def resize_to(self, w: int, h: int):
        w = max(PAGE_MIN[0], min(w, self.canvas.width() - self.x()))
        self.tab["h"] = max(PAGE_MIN[1], min(h, self.canvas.height() - self.y() - HEADER_H))
        self.setFixedWidth(w)
        self.fit()

    def resize_done(self):
        self.page.store.set_geometry(self.tab["id"], w=self.width(), h=self.tab["h"])

    def set_collapsed(self, on: bool):
        self.tab["collapsed"] = on
        self.chevron.setText("▸" if on else "▾")
        self.header.setProperty("collapsed", on)
        _repolish(self.header)
        _fold(self, self.body, on)


def _fold(box: "_Box", body: QWidget, on: bool):
    """Fold or unfold a box: its height glides, the body is hidden once it is folded away."""
    if not box.isVisible() or not anim.on("boxes"):
        anim.stop(box, "height")
        box.setProperty("folding", False)
        body.setVisible(not on)
        box.fit()
        return
    start = box.height()
    if not on:
        body.setVisible(True)
    box.setProperty("folding", True)
    end = box.target_height()

    def done():
        box.setProperty("folding", False)
        body.setVisible(not on)
        box.fit()
    anim.value(start, end, lambda v: box.setFixedHeight(int(v)), "boxes", 200, done, box,
               QEasingCurve.Type.OutCubic, "height")


class Canvas(QWidget):
    """One screen's worth of dashboard: the boxes on it, the selection box and the right-click menu.
    A click on the open space never closes the overlay; a drag draws a selection box, like a desktop."""

    def __init__(self, page: "DashboardPage", parent: QWidget, screen_name: str):
        super().__init__(parent)
        self.page = page
        self.screen_name = screen_name
        self.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.customContextMenuRequested.connect(lambda pos: page.canvas_menu(self, pos))
        self._from = None
        self._base: list[_Box] = []
        self._band = QFrame(self)
        self._band.setObjectName("lumBand")
        self._band.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        self._band.hide()
        self._ghost = QFrame(self)                  # where dropped boxes will land
        self._ghost.setObjectName("lumBand")
        self._ghost.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        self._ghost.hide()
        self.setAcceptDrops(True)

    @staticmethod
    def _boxes_of(md) -> dict | None:
        if not md.hasFormat(BOX_MIME):
            return None
        try:
            return json.loads(bytes(md.data(BOX_MIME)).decode())
        except ValueError:
            return None

    def dragEnterEvent(self, e):
        if self._boxes_of(e.mimeData()) is not None:
            e.acceptProposedAction()

    def dragMoveEvent(self, e):
        d = self._boxes_of(e.mimeData())
        if d is None:
            return
        e.acceptProposedAction()
        lead = self.page.store.tab(d.get("lead", ""))
        w = int(lead.get("w", BOX_W)) if lead else BOX_W
        h = HEADER_H + 2 if not lead or lead.get("collapsed") else min(240, int(lead.get("h", 160)))
        at = e.position().toPoint() - QPoint(*d.get("grab", [0, 0]))
        x, y = self.page.clamp(self, at.x(), at.y(), w)
        self._ghost.setGeometry(x, y, w, h)
        self._ghost.show()
        self._ghost.raise_()

    def dragLeaveEvent(self, e):
        self._ghost.hide()

    def dropEvent(self, e):
        self._ghost.hide()
        d = self._boxes_of(e.mimeData())
        if d is None:
            return
        e.acceptProposedAction()
        at = e.position().toPoint() - QPoint(*d.get("grab", [0, 0]))
        # after the drag has finished (the source box is still inside QDrag.exec now)
        QTimer.singleShot(0, lambda: self.page.drop_boxes(d.get("boxes", []), self, at) if alive(self) else None)

    def mousePressEvent(self, e):
        e.accept()
        pos = e.position().toPoint()
        if e.button() != Qt.MouseButton.LeftButton or self.childAt(pos) is not None:
            return
        add = bool(e.modifiers() & (Qt.KeyboardModifier.ControlModifier | Qt.KeyboardModifier.ShiftModifier))
        if not add:
            self.page.clear_selection()
        self._from = pos
        self._base = list(self.page.selected)

    def mouseMoveEvent(self, e):
        if self._from is None:
            return
        r = QRect(self._from, e.position().toPoint()).normalized()
        if not self._band.isVisible() and r.width() + r.height() < 6:
            return
        self._band.setGeometry(r)
        self._band.show()
        self._band.raise_()
        hits = [b for b in self.page.boxes if b.canvas is self and b.geometry().intersects(r)]
        self.page.set_selection(self._base + [b for b in hits if b not in self._base])

    def mouseReleaseEvent(self, e):
        self._from = None
        self._band.hide()


def dock_button(text: str, accent_prefix: str = "", variant: str = "") -> QPushButton:
    """A toolbar pill. `accent_prefix` (e.g. ＋) is drawn in the accent text colour before the label."""
    b = QPushButton("" if accent_prefix else text)
    b.setCursor(Qt.CursorShape.PointingHandCursor)
    if variant:
        b.setProperty("variant", variant)
    b.setFixedHeight(32)
    if accent_prefix:
        lay = QHBoxLayout(b)
        lay.setContentsMargins(14, 0, 14, 0)
        lay.setSpacing(8)
        for part, tone in ((accent_prefix, "accent"), (text, "primary")):
            lbl = widgets.label(part, 13, 500, tone=tone)
            lbl.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
            lay.addWidget(lbl)
        b.setMinimumWidth(lay.sizeHint().width())
    return b


def page_name(key: str) -> str:
    return dict(ALL_PAGES).get(key, key.capitalize())


def boxable_pages() -> list[tuple[str, str]]:
    """The overlay pages that can sit on the dashboard as a box."""
    return [(k, n) for k, n in ALL_PAGES if k not in FULL_PAGES]


class DashboardPage(QWidget):
    """The boxes, straight on the overlay's full-screen layer of every screen, with a toolbar pill at the
    bottom centre of the main one. Each board's boxes are built once and kept: switching boards only hides
    one set and shows the other, and a change rebuilds only the boxes whose tab changed. A screen with a
    board pinned to it shows that board instead of the active one."""

    def __init__(self, launcher, host: QWidget | None = None, main_screen: str = "",
                 extra: dict[str, QWidget] | None = None):
        super().__init__(host)
        self.launcher = launcher
        self.store: DashboardStore = launcher.dashboard
        self.selected: list[_Box] = []
        self._boxes: dict[str, dict[str, _Box]] = {}          # board id -> tab id -> box
        self._queue: list[PageBox] = []
        self._ctx: QWidget | None = None                       # the screen last clicked: menus and dialogs go there
        self._undo: list[tuple] = []                           # where the boxes were before the last tidy up
        self._shown_once = False
        self.dragging_boxes = False
        self._pump_timer = QTimer(self)
        self._pump_timer.setSingleShot(True)
        self._pump_timer.timeout.connect(self._pump)
        if host is not None:
            self.setGeometry(host.rect())
        self.main = Canvas(self, self, main_screen)
        self.main.setGeometry(self.rect())
        self.canvases: dict[str, Canvas] = {main_screen: self.main}
        for name, surf in (extra or {}).items():
            c = Canvas(self, surf, name)
            c.setGeometry(surf.rect())
            c.show()
            self.canvases[name] = c

        self.dock = QFrame(self)
        self.dock.setObjectName("lumDock")
        self.dock.mousePressEvent = lambda e: e.accept()
        dl = QHBoxLayout(self.dock)
        dl.setContentsMargins(5, 5, 5, 5)
        dl.setSpacing(6)
        self.boards_box = QHBoxLayout()
        self.boards_box.setSpacing(2)
        dl.addLayout(self.boards_box)
        add = dock_button("Add", "＋")
        add.setToolTip("A new tab of items, or a page (Themes, Settings, Clipboard…) as a box")
        add.clicked.connect(lambda: self._add_menu(add.mapToGlobal(add.rect().topLeft())))
        dl.addWidget(add)
        tidy = dock_button("Tidy up", variant="ghost")
        tidy.setToolTip("Line the boxes up (only the selected ones if any are selected). The arrow next to it "
                        "picks how; Ctrl+Z puts them back.")
        tidy.clicked.connect(self.tidy)
        dl.addWidget(tidy)
        tidy_opts = dock_button("▾", variant="ghost")
        tidy_opts.setToolTip("How Tidy up arranges the boxes")
        tidy_opts.setStyleSheet("QPushButton { padding: 0px; }")
        tidy_opts.setFixedWidth(28)
        tidy_opts.clicked.connect(lambda: self._tidy_menu(tidy_opts.mapToGlobal(tidy_opts.rect().topLeft())))
        dl.addWidget(tidy_opts)
        self.undo_btn = dock_button("↶ Undo", variant="ghost")
        self.undo_btn.setToolTip("Put the boxes back where they were before Tidy up (Ctrl+Z)")
        self.undo_btn.clicked.connect(self.undo_tidy)
        self.undo_btn.hide()
        dl.addWidget(self.undo_btn)
        self.lock_btn = dock_button("", variant="ghost")
        self.lock_btn.setCheckable(True)
        self.lock_btn.clicked.connect(self._toggle_lock)
        dl.addWidget(self.lock_btn)
        sep = QFrame()
        sep.setObjectName("lumDockSep")
        sep.setFixedSize(1, 18)
        dl.addWidget(sep)
        self.search = QLineEdit()
        self.search.setObjectName("lumSearch")
        self.search.setPlaceholderText("Find an item…")
        self.search.setClearButtonEnabled(True)
        self.search.setFixedSize(190, 32)
        self.search.textChanged.connect(self._filter)
        self.search.returnPressed.connect(self._run_first_match)
        dl.addWidget(self.search)
        self.hint = widgets.label("", 12, tone="muted")
        self.hint.setContentsMargins(8, 0, 10, 0)
        self.hint.setToolTip("Double-click an item to run it · right-click for options · drag items between boxes "
                             "· drop files from your file manager onto a box · drag on the open space to select")
        dl.addWidget(self.hint)

        self.empty = widgets.label("This dashboard is empty. Click ＋ Add for a tab of items or a page, or "
                                   "right-click anywhere.", 13, tone="muted")
        self.empty.setParent(self.main)
        self.empty.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        self.store.changed.connect(self.refresh)
        self.store.switched.connect(self._switched)
        self.store.running_changed.connect(self.refresh)
        self.refresh()

    def teardown(self):
        for sig, slot in ((self.store.changed, self.refresh), (self.store.running_changed, self.refresh),
                          (self.store.switched, self._switched)):
            try:
                sig.disconnect(slot)
            except TypeError:
                pass
        self._pump_timer.stop()
        for boxes in self._boxes.values():
            for b in boxes.values():
                if isinstance(b, PageBox):
                    b.teardown()
        self.store.schedule_save()

    @property
    def canvas(self) -> QWidget:
        return self.main

    @property
    def boxes(self) -> list[_Box]:
        """The boxes on screen: the active board's, and those of boards pinned to the other screens."""
        out = []
        for b in self.shown_boards():
            out += [x for x in self._boxes.get(b["id"], {}).values() if alive(x)]
        return out

    # ── which board goes where ─────────────────────────────────────────────
    def pins(self) -> dict[str, dict]:
        """Screen name -> the board pinned to it, for the other screens that are here now."""
        out = {}
        for name in self.canvases:
            if name == self.main.screen_name:
                continue                              # the main screen always shows the active board
            b = self.store.screen_board(name)
            if b is not None:
                out[name] = b
        return out

    def shown_boards(self) -> list[dict]:
        out = [self.store.active_board()]
        for b in self.pins().values():
            if b not in out:
                out.append(b)
        return out

    def board_of_canvas(self, canvas: "Canvas") -> dict:
        return self.pins().get(canvas.screen_name) or self.store.active_board()

    def _parent(self) -> QWidget:
        """Menus and dialogs open on the screen you were working on (a menu belongs to a window, and its
        window decides which screen it shows on)."""
        return self._ctx if alive(self._ctx) else self

    def raise_dock(self):
        self.dock.raise_()

    def snap(self) -> int:
        return max(0, int(self.launcher.settings.get("dashboard.snap", GRID) or 0))

    # ── building ───────────────────────────────────────────────────────────
    def canvas_for(self, tab: dict, board: dict | None = None) -> "Canvas":
        pins = self.pins()
        if board is not None:
            for name, b in pins.items():
                if b is board:
                    return self.canvases[name]
        c = self.canvases.get(tab.get("screen") or "")
        if c is None or c.screen_name in pins:
            return self.main                          # that screen is gone, or shows a board of its own now
        return c

    def _sig(self, tab: dict) -> str:
        return json.dumps([tab.get("title"), tab.get("items"), [self.store.is_running(i["id"]) for i in tab["items"]],
                           self.store.locked], sort_keys=True, default=str)

    def _drop_box(self, b: _Box):
        if b in self.selected:
            self.selected.remove(b)
        if isinstance(b, PageBox):
            if b in self._queue:
                self._queue.remove(b)
            b.teardown()
        b.hide()
        b.setParent(None)
        b.deleteLater()

    def _sync_board(self, board: dict, force: bool = False):
        """Make the cached boxes match the board: new tabs get a box, changed ones a new box, gone ones none."""
        cache = self._boxes.setdefault(board["id"], {})
        want = {t["id"]: t for t in board["tabs"]}
        for tid in list(cache):
            b = cache[tid]
            if want.get(tid) is not b.tab or isinstance(b, PageBox) != (b.tab.get("kind") == "page"):
                self._drop_box(cache.pop(tid))
        for t in board["tabs"]:
            b = cache.get(t["id"])
            canvas = self.canvas_for(t, board)
            if t.get("kind") == "page":
                if b is None:
                    b = cache[t["id"]] = PageBox(self, t, canvas)
                    b.fresh = True
                    self._queue.append(b)
                else:
                    b.title_lbl.setText((t.get("title") or page_name(t["page"])).upper())
                    if b.canvas is not canvas:
                        self._reparent(b, canvas, b.pos())
                continue
            sig = self._sig(t)
            if b is not None and (force or b.sig != sig or b.canvas is not canvas):
                self._drop_box(cache.pop(t["id"]))
                b = None
            if b is None:
                b = cache[t["id"]] = TabBox(self, t, canvas)
                b.sig = sig
                b.fresh = True
        if self._queue and not self._pump_timer.isActive():
            self._pump_timer.start(30)

    def _pump(self):
        """Build the page boxes' pages one per event-loop pass, the active board's first."""
        if not self._queue:
            return
        active = {id(b) for b in self.boxes}
        self._queue.sort(key=lambda b: id(b) not in active)
        b = self._queue.pop(0)
        if getattr(self.launcher, "safe_mode", False):
            b.safe_mode()
            if self._queue:
                self._pump_timer.start(0)
            return
        try:
            b.build()
        except Exception:
            log.exception("could not build the %s box", b.tab.get("page"))
        if self._queue:
            self._pump_timer.start(40 if id(self._queue[0]) in active else 250)

    def refresh(self, force: bool = False):
        boards = {b["id"] for b in self.store.boards()}
        for bid in list(self._boxes):
            if bid not in boards:
                for b in self._boxes.pop(bid).values():
                    self._drop_box(b)
        if force:
            for bid in list(self._boxes):
                if bid != self.store.active_board()["id"]:
                    for b in self._boxes.pop(bid).values():
                        self._drop_box(b)
        self._show_active(force)
        self._build_boards()
        locked = self.store.locked
        self.lock_btn.setChecked(locked)
        self.lock_btn.setText("🔒 Locked" if locked else "🔓 Lock")
        self.lock_btn.setToolTip("Unlock to move boxes and items again" if locked else
                                 "Lock the layout: no moving boxes or dragging items")
        self._set_hint()
        if self.search.text():
            self._filter(self.search.text())
        self._clamp()
        self.raise_dock()

    def _switched(self):
        self.clear_selection()
        self._show_active()
        self._build_boards()
        if self.search.text():
            self._filter(self.search.text())
        self._clamp()
        self.raise_dock()

    def _show_active(self, force: bool = False):
        active = self.store.active_board()
        shown = self.shown_boards()
        shown_ids = {b["id"] for b in shown}
        appear = []
        self.setUpdatesEnabled(False)
        try:
            for bid, boxes in self._boxes.items():
                if bid in shown_ids:
                    continue
                for b in boxes.values():
                    if b.isVisible():
                        if isinstance(b, PageBox):
                            b.hidden()
                        b.hide()
            for board in shown:
                self._sync_board(board, force)
                for t in board["tabs"]:
                    b = self._boxes[board["id"]].get(t["id"])
                    if b is not None and not b.isVisible():
                        b.show()
                        b.raise_()
                        if isinstance(b, PageBox) and b.inner is not None:
                            b.shown()
                        if getattr(b, "fresh", False) and self._shown_once and self.isVisible():
                            appear.append(b)
                    if b is not None:
                        b.fresh = False
            self.selected = [b for b in self.selected if alive(b) and b.isVisible()]
            on_main = any(b.canvas is self.main for b in self.boxes)
            pinned_away = self.store.pinned_screens(active["id"])
            self.empty.setText(f"“{active['name']}” is shown on {', '.join(pinned_away)}. Switch to another "
                               f"dashboard for this screen." if pinned_away and not on_main else
                               "This dashboard is empty. Click ＋ Add for a tab of items or a page, or "
                               "right-click anywhere.")
            self.empty.setVisible(not on_main)
        finally:
            self.setUpdatesEnabled(True)
        if self.isVisible():
            self._shown_once = True
        for b in appear:
            anim.fade(b, 0.0, 1.0, "boxes", 220)

    def _build_boards(self, compact: bool | None = None):
        widgets.clear_layout(self.boards_box)
        boards = self.store.boards()
        active = self.store.active_board()
        if compact is None:
            compact = len(boards) > 1 and self._boards_too_wide()
        if len(boards) > 1 and not compact:
            for b in boards:
                btn = QPushButton(b["name"])
                btn.setObjectName("boardTab")
                btn.setCheckable(True)
                btn.setChecked(b["id"] == active["id"])
                btn.setCursor(Qt.CursorShape.PointingHandCursor)
                btn.setFixedHeight(32)
                btn.clicked.connect(lambda _, bid=b["id"]: self.store.switch_board(bid))
                btn.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
                btn.customContextMenuRequested.connect(
                    lambda pos, bid=b["id"], w=btn: self._board_menu(bid, w.mapToGlobal(pos)))
                self.boards_box.addWidget(btn)
        if compact:
            more = QPushButton(f"{active['name']}  ▾")
            more.setObjectName("boardTab")
            more.setCheckable(True)
            more.setChecked(True)
            more.setCursor(Qt.CursorShape.PointingHandCursor)
            more.setFixedHeight(32)
        else:
            more = dock_button("▾" if len(boards) > 1 else "Dashboards ▾", variant="ghost")
        more.setToolTip("Switch dashboard, new dashboard, rename, delete…")
        more.clicked.connect(lambda: self._board_menu(active["id"], more.mapToGlobal(more.rect().topLeft()),
                                                      above=True))
        self.boards_box.addWidget(more)
        sep = QFrame()
        sep.setObjectName("lumDockSep")
        sep.setFixedSize(1, 18)
        self.boards_box.addWidget(sep)
        for i in range(self.boards_box.count()):
            w = self.boards_box.itemAt(i).widget()
            if w is not None:
                w.show()            # now, not on the next pass, so the toolbar is measured with them

    def _boards_too_wide(self) -> bool:
        """Would one pill per board make the toolbar wider than the screen leaves room for?"""
        fm = self.fontMetrics()
        pills = sum(fm.horizontalAdvance(b["name"]) + 30 for b in self.store.boards())
        return pills + 560 > self.width() - 120

    def resizeEvent(self, e):
        super().resizeEvent(e)
        self.main.setGeometry(self.rect())
        self._clamp()

    def _set_hint(self):
        self.hint.setText("Layout locked" if self.store.locked else "Drag headers and items · drop files on a box")

    def _clamp(self):
        cw, ch = self.width(), self.height()
        if cw < 50 or ch < 50:
            return
        self.hint.show()
        self.dock.layout().activate()
        self.dock.adjustSize()
        if self.dock.width() > cw - 48:
            self.hint.hide()                    # many boards: the hint goes first
            self.dock.layout().activate()
            self.dock.adjustSize()
        self.dock.move((cw - self.dock.width()) // 2, ch - 28 - self.dock.height())
        self.empty.adjustSize()
        self.empty.move((cw - self.empty.width()) // 2, ch // 3)
        for b in self.boxes:
            x, y = self.clamp(b.canvas, b.x(), b.y(), b.width())
            if (x, y) != (b.x(), b.y()):
                b.move(x, y)
            b.fit()

    def clamp(self, canvas: QWidget, x: int, y: int, w: int) -> tuple[int, int]:
        return (max(0, min(int(x), canvas.width() - w)), max(0, min(int(y), canvas.height() - HEADER_H - 2)))

    def bottom_limit(self, box: _Box) -> int:
        """How far down a box may reach: above the toolbar if it would run into it, else the screen's edge."""
        bottom = box.canvas.height() - GRID
        if box.canvas is self.main and self.dock.isVisible():
            d = self.dock.geometry()
            if box.x() < d.right() and box.x() + box.width() > d.left() and box.y() < d.top():
                bottom = d.top() - 12
        return bottom

    def on_show(self):
        self._shown_once = True
        QTimer.singleShot(0, self._clamp)
        for b in self.boxes:
            if isinstance(b, PageBox) and b.inner is not None and b.isVisible():
                b.shown()
        if self._queue and not self._pump_timer.isActive():
            self._pump_timer.start(30)

    def on_hide(self):
        if self.search.text():
            self.search.clear()
        for b in self.boxes:
            if isinstance(b, PageBox) and b.inner is not None:
                b.hidden()

    def origin(self, canvas: QWidget) -> tuple[int, int]:
        """Where the first box goes: on the main screen below the clock and the status chips."""
        if canvas is not self.main:
            return ORIGIN[0], 64
        below = self.launcher.overlay.left_chrome_bottom()
        return ORIGIN[0], max(ORIGIN[1], snap_to(below + 28, GRID))

    def _columns(self, canvas: QWidget) -> int:
        return max(1, (canvas.width() - ORIGIN[0] * 2 + GAP) // (BOX_W + GAP))

    def _free_spot(self, canvas: QWidget | None = None) -> QPoint:
        canvas = canvas or self.main
        cols = self._columns(canvas)
        n = sum(1 for b in self.boxes if b.canvas is canvas)
        x0, y0 = self.origin(canvas)
        return QPoint(x0 + (n % cols) * (BOX_W + GAP), y0 + (n // cols) * 56)

    # ── selection ──────────────────────────────────────────────────────────
    def set_selection(self, boxes: list[_Box]):
        for b in self.selected:
            if b not in boxes:
                b.set_selected(False)
        for b in boxes:
            b.set_selected(True)
        self.selected = list(boxes)

    def clear_selection(self):
        self.set_selection([])

    def toggle_selected(self, box: _Box):
        self.set_selection([b for b in self.selected if b is not box] if box in self.selected
                           else self.selected + [box])

    def select_all(self):
        self.set_selection(self.boxes)

    def overlay_key(self, e) -> bool:
        """Keys while the overlay shows the dashboard (and no text box has the focus)."""
        k = e.key()
        ctrl = bool(e.modifiers() & Qt.KeyboardModifier.ControlModifier)
        if k == Qt.Key.Key_Delete and self.selected:
            self.remove_boxes(self.selected)
            return True
        if ctrl and k == Qt.Key.Key_A:
            self.select_all()
            return True
        if ctrl and k == Qt.Key.Key_Z and self._undo:
            self.undo_tidy()
            return True
        if k == Qt.Key.Key_Escape and self.selected:
            self.clear_selection()
            return True
        return False

    def remove_boxes(self, boxes: list[_Box]):
        boxes = list(boxes)
        if not boxes or self.store.locked and not widgets.confirm(
                self._parent(), "Layout locked", "The layout is locked. Remove them anyway?", yes="Remove"):
            return
        n_items = sum(len(b.tab["items"]) for b in boxes)
        if len(boxes) == 1 and isinstance(boxes[0], PageBox):
            text = f'Take the {boxes[0].tab.get("title") or "page"} box off the dashboard?'
        else:
            text = f"Delete {len(boxes)} box(es)" + (f" with {n_items} item(s)" if n_items else "") + "?"
        if widgets.confirm(self._parent(), "Remove", text, yes="Remove", danger=True):
            self.store.delete_tabs([b.tab["id"] for b in boxes])

    # ── screens ────────────────────────────────────────────────────────────
    def canvas_at(self, gp: QPoint) -> "Canvas | None":
        for c in self.canvases.values():
            if c.isVisible() and QRect(c.mapToGlobal(QPoint(0, 0)), c.size()).contains(gp):
                return c
        return None

    def _reparent(self, b: _Box, canvas: "Canvas", at: QPoint):
        b.setParent(canvas)
        b.canvas = canvas
        b.move(*self.clamp(canvas, at.x(), at.y(), b.width()))
        b.fit()
        b.show()
        b.raise_()

    def move_box_to_canvas(self, b: _Box, canvas: "Canvas", at: QPoint):
        if not alive(b) or not alive(canvas):
            return
        g = self.snap()
        x, y = self.clamp(canvas, snap_to(at.x(), g), snap_to(at.y(), g), b.width())
        target = self.board_of_canvas(canvas)
        src = self.store.board_of_tab(b.tab["id"])
        screen = canvas.screen_name                     # put on a screen by hand: it stays on that screen
        if src is not None and target is not src:
            # that screen shows another dashboard: the box joins it (and is built again there)
            self.store.set_geometry(b.tab["id"], x=x, y=y, screen=screen)
            b.tab["screen"] = screen
            self.store.move_tabs_to_board([b.tab["id"]], target["id"])
            nb = self._boxes.get(target["id"], {}).get(b.tab["id"])
            if alive(nb):
                anim.fade(nb, 0.0, 1.0, "boxes", 220)
            return
        self._reparent(b, canvas, QPoint(x, y))
        b.tab["screen"] = screen
        self.store.set_geometry(b.tab["id"], x=b.x(), y=b.y(), screen=screen)
        if isinstance(b, TabBox):
            b.sig = self._sig(b.tab)
        anim.fade(b, 0.0, 1.0, "boxes", 220)

    def drop_boxes(self, entries: list, canvas: "Canvas", at: QPoint):
        """Boxes dragged here from another screen: [[tab id, dx, dy], ...] relative to the first one."""
        by_id = {b.tab["id"]: b for b in self.boxes}
        for entry in entries:
            try:
                tid, dx, dy = entry[0], int(entry[1]), int(entry[2])
            except (IndexError, TypeError, ValueError):
                continue
            b = by_id.get(tid)
            if b is not None:
                self.move_box_to_canvas(b, canvas, at + QPoint(dx, dy))
                if alive(b) and not b.isVisible() and b.canvas is canvas:
                    b.show()

    def _screen_label(self, c: "Canvas") -> str:
        name = c.screen_name or "Main"
        pinned = self.pins().get(c.screen_name)
        if c is self.main:
            return f"{name}  (this screen)"
        return f"{name}  ({pinned['name']})" if pinned else name

    def _screen_menu(self, m, boxes: list[_Box]):
        others = [c for c in self.canvases.values() if c.isVisible()]
        if len(others) < 2:
            return
        sub = m.addMenu("Move to screen")
        for c in self.canvases.values():
            a = sub.addAction(("●  " if all(b.canvas is c for b in boxes) else "     ") + self._screen_label(c),
                              lambda c=c: [self.move_box_to_canvas(b, c, b.pos()) for b in boxes])
            a.setEnabled(not all(b.canvas is c for b in boxes))

    def _pin_menu(self, m, canvas: "Canvas"):
        """Right-click on another screen: which dashboard that screen shows."""
        if canvas is self.main or len(self.canvases) < 2:
            return
        sub = m.addMenu("This screen shows")
        cur = self.pins().get(canvas.screen_name)
        a = sub.addAction(("●  " if cur is None else "     ") + "The active dashboard (follows the toolbar)",
                          lambda: self.store.set_screen_board(canvas.screen_name, ""))
        a.setToolTip("Boxes you put on this screen belong to whichever dashboard is active")
        sub.addSeparator()
        for b in self.store.boards():
            sub.addAction(("●  " if cur is b else "     ") + f"Always “{b['name']}”",
                          lambda bid=b["id"]: self.store.set_screen_board(canvas.screen_name, bid))

    # ── search, lock ───────────────────────────────────────────────────────
    def _filter(self, text: str):
        q = text.strip().lower()
        total = sum(b.apply_filter(q) for b in self.boxes if isinstance(b, TabBox))
        if q:
            self.hint.setText(f"{total} match{'es' if total != 1 else ''} · Enter runs the first")
        else:
            self._set_hint()
        self._clamp()

    def _run_first_match(self):
        q = self.search.text().strip().lower()
        if not q:
            return
        for b in self.boxes:
            for r in getattr(b, "rows", []):
                if runnable(r.item) and q in r.search_text():
                    self.run(r.item["id"])
                    return

    def _toggle_lock(self):
        self.store.set_locked(not self.store.locked)
        self.refresh()

    # ── actions ────────────────────────────────────────────────────────────
    def run(self, iid: str, args: str | None = None):
        self.launcher.overlay.hide()
        QTimer.singleShot(150, lambda: self.store.run_item(iid, args))

    def new_tab(self, at: QPoint | None = None, canvas: QWidget | None = None):
        name, ok = widgets.ask_text(self._parent(), "New tab", "Name of the tab (e.g. Scripts, Games, Work):")
        if ok and name.strip():
            canvas = canvas or self.main
            p = at or self._free_spot(canvas)
            g = self.snap()
            self.store.add_tab(name.strip(), snap_to(p.x(), g), snap_to(p.y(), g),
                               "" if canvas is self.main else canvas.screen_name, self.board_of_canvas(canvas)["id"])

    def add_page(self, key: str, at: QPoint | None = None, canvas: QWidget | None = None):
        canvas = canvas or self.main
        p = at or self._free_spot(canvas)
        w = min(PAGE_W, max(PAGE_MIN[0], canvas.width() - p.x() - 24))
        h = min(PAGE_H, max(PAGE_MIN[1], canvas.height() - p.y() - HEADER_H - 96))
        g = self.snap()
        self.store.add_page_tab(key, page_name(key), snap_to(p.x(), g), snap_to(p.y(), g), w, h,
                                "" if canvas is self.main else canvas.screen_name, self.board_of_canvas(canvas)["id"])

    def add_item(self, tab: dict, kind: str = "item", index: int = -1):
        if kind in ("separator", "heading"):
            name = ""
            if kind == "heading":
                name, ok = widgets.ask_text(self._parent(), "Heading", "Heading text:")
                if not ok or not name.strip():
                    return
            self.store.add_item(tab["id"], {"kind": kind, "name": name.strip()}, index)
            return
        dlg = ItemDialog(self.launcher, None, self._parent(), kind=kind)
        if dlg.exec() == QDialog.DialogCode.Accepted:
            self.store.add_item(tab["id"], dlg.item, index)

    def add_files(self, tab: dict):
        paths, _ = QFileDialog.getOpenFileNames(self._parent(), "Add files", os.path.expanduser("~"))
        if paths:
            self.store.add_paths(tab["id"], paths)

    def edit_item(self, item: dict, focus: str = "name"):
        if item.get("kind") == "heading":
            self._rename_item(item)
            return
        dlg = ItemDialog(self.launcher, item, self._parent(), focus=focus)
        if dlg.exec() == QDialog.DialogCode.Accepted:
            self.store.update_item(item["id"], **{k: dlg.item.get(k, "") for k in
                                                  ("name", "path", "args", "hotkey", "cwd", "admin", "ask_args",
                                                   "terminal", "group") if k in dlg.item})

    def run_with_args(self, item: dict):
        text, ok = widgets.ask_text(self._parent(), item.get("name") or "Run", "Arguments:", item.get("args", ""))
        if ok:
            self.run(item["id"], text)

    # ── tidy up ────────────────────────────────────────────────────────────
    def tidy(self, only: "Canvas | None" = None):
        """Line the boxes up, screen by screen: only the selected ones if any are selected (they stay where the
        selection starts), else every box. How is up to Settings → Dashboard (or the arrow next to the button);
        the boxes glide there and Ctrl+Z puts them back."""
        if self.store.locked:
            self.launcher.notify("Dashboard", "The layout is locked. Unlock it to tidy up.", 2500)
            return
        style = self.launcher.settings.get("dashboard.tidy_style", "columns")
        order = self.launcher.settings.get("dashboard.tidy_order", "position")
        picked = [b for b in self.selected if alive(b) and b.isVisible()]
        undo = []
        for canvas in self.canvases.values():
            if only is not None and canvas is not only:
                continue
            boxes = [b for b in (picked or self.boxes) if b.canvas is canvas]
            if not boxes:
                continue
            undo += [(b.tab["id"], b.x(), b.y(), b.width()) for b in boxes]
            if picked:
                x0, y0 = min(b.x() for b in boxes), min(b.y() for b in boxes)
            else:
                x0, y0 = self.origin(canvas)
            right = canvas.width() - ORIGIN[0] // 2
            for b, (x, y, w) in self._arrange(boxes, x0, y0, right, style, order):
                x, y = self.clamp(canvas, x, y, w)          # a screen that is too full: the headers stay on it
                if isinstance(b, TabBox) and w != b.width():
                    b.set_width(w)
                anim.move(b, QPoint(x, y), "boxes", 260)
                self.store.set_geometry(b.tab["id"], x=x, y=y, w=b.width())
        if undo:
            self._undo = undo
            self.undo_btn.show()
            self._clamp_later()
            QTimer.singleShot(12000, self._forget_undo)
        for b in self.boxes:
            QTimer.singleShot(anim.ms(280), b.fit) if anim.on("boxes") else b.fit()

    def _arrange(self, boxes: list[_Box], x0: int, y0: int, right: int, style: str, order: str):
        """Where each box goes: [(box, (x, y, width))]."""
        if order == "name":
            boxes = sorted(boxes, key=lambda b: (b.tab.get("title") or "").lower())
        elif order == "size":
            boxes = sorted(boxes, key=lambda b: -(b.width() * b.height()))
        else:                                    # reading order: row bands from the top, then left to right
            boxes = sorted(boxes, key=lambda b: (b.y() // 48, b.x()))
        out = []
        placed: list[QRect] = []
        col = BOX_W + GAP
        if style == "rows":
            x, y, row_h = x0, y0, 0
            for b in boxes:
                w, h = b.width(), b.height()
                if x > x0 and x + w > right:
                    x, y, row_h = x0, y + row_h + GAP, 0
                out.append((b, (x, y, w)))
                x += w + GAP
                row_h = max(row_h, h)
            return out
        for b in boxes:
            w = BOX_W if style == "grid" and isinstance(b, TabBox) else b.width()
            h = b.height()
            if style == "grid":
                span = max(1, -(-(w + GAP) // col))
                w_slot = span * col - GAP
                xs = [x0 + i * col for i in range(max(1, (right - x0 + GAP) // col))]
                xs = [x for x in xs if x + w_slot <= right] or [x0]
            else:
                # the left edge, the column grid and the right edge of every box placed so far: neat lines
                xs = {x0} | {x0 + i * col for i in range(max(1, (right - x0) // col))} | \
                     {r.right() + 1 + GAP for r in placed}
                xs = sorted(x for x in xs if x + w <= right) or [x0]
            best = None
            for x in xs:
                y = max([r.bottom() + 1 + GAP for r in placed if r.left() < x + w and r.right() + 1 > x],
                        default=y0)
                if best is None or (y, x) < best:
                    best = (y, x)
            y, x = best
            placed.append(QRect(x, y, w, h))
            out.append((b, (x, y, w)))
        return out

    def undo_tidy(self):
        if not self._undo:
            return
        by_id = {b.tab["id"]: b for b in self.boxes}
        for tid, x, y, w in self._undo:
            b = by_id.get(tid)
            if b is None:
                continue
            if isinstance(b, TabBox) and w != b.width():
                b.set_width(w)
            anim.move(b, QPoint(x, y), "boxes", 260)
            self.store.set_geometry(tid, x=x, y=y, w=w)
        self._forget_undo()
        for b in self.boxes:
            QTimer.singleShot(anim.ms(280), b.fit) if anim.on("boxes") else b.fit()

    def _forget_undo(self):
        self._undo = []
        if alive(self.undo_btn):
            self.undo_btn.hide()
            self._clamp_later()

    def _clamp_later(self):
        QTimer.singleShot(0, self._clamp)

    def _tidy_menu(self, pos: QPoint):
        self._ctx = self.main
        m = widgets.menu(self)
        s = self.launcher.settings
        head = m.addAction("ARRANGE")
        head.setEnabled(False)
        for key, label in TIDY_STYLES:
            a = m.addAction(label, lambda k=key: self._set_tidy("dashboard.tidy_style", k))
            a.setCheckable(True)
            a.setChecked(s.get("dashboard.tidy_style", "columns") == key)
        m.addSeparator()
        head = m.addAction("ORDER")
        head.setEnabled(False)
        for key, label in TIDY_ORDERS:
            a = m.addAction(label, lambda k=key: self._set_tidy("dashboard.tidy_order", k))
            a.setCheckable(True)
            a.setChecked(s.get("dashboard.tidy_order", "position") == key)
        m.addSeparator()
        m.addAction("Tidy up now", self.tidy)
        boxes = self.boxes
        m.addAction("Fold every box", lambda: self._fold_all(boxes, True))
        m.addAction("Unfold every box", lambda: self._fold_all(boxes, False))
        if self._undo:
            m.addAction("Undo the last tidy up   (Ctrl+Z)", self.undo_tidy)
        m.exec(QPoint(pos.x(), pos.y() - m.sizeHint().height() - 4))

    def _set_tidy(self, key: str, value: str):
        s = self.launcher.settings
        s.set(key, value)
        s.save_quiet()
        self.tidy()

    def _fold_all(self, boxes: list[_Box], on: bool):
        for b in boxes:
            if alive(b) and bool(b.tab.get("collapsed")) != on:
                b.set_collapsed(on)
                self.store.set_geometry(b.tab["id"], collapsed=on)

    # ── menus ──────────────────────────────────────────────────────────────
    def item_menu(self, box: TabBox, item: dict, pos):
        self._ctx = box.canvas
        m = widgets.menu(box.canvas)
        kind = item.get("kind", "item")
        idx = box.tab["items"].index(item)
        if runnable(item):
            m.addAction("▶  Run", lambda: self.run(item["id"]))
            if kind == "item" and not item.get("action"):
                m.addAction("Run with arguments…", lambda: self.run_with_args(item))
            if self.store.is_running(item["id"]):
                m.addAction("■  Stop", lambda: self.store.stop_item(item["id"]))
            m.addSeparator()
            m.addAction("Edit…", lambda: self.edit_item(item))
            m.addAction("Rename…", lambda: self._rename_item(item))
            if kind == "item" and not item.get("action"):
                m.addAction("Change file…", lambda: self.edit_item(item, focus="file"))
            m.addAction("Set hotkey…", lambda: self.edit_item(item, focus="hotkey"))
        elif kind == "heading":
            m.addAction("Rename…", lambda: self._rename_item(item))
        others = [t for t in self.store.tabs() if t["id"] != box.tab["id"] and t.get("kind") != "page"]
        if others:
            sub = m.addMenu("Move to tab")
            for t in others:
                sub.addAction(t["title"], lambda tid=t["id"]: self.store.move_item(item["id"], to_tab=tid))
        if idx > 0:
            m.addAction("Move up", lambda: self.store.move_item(item["id"], delta=-1))
        if idx < len(box.tab["items"]) - 1:
            m.addAction("Move down", lambda: self.store.move_item(item["id"], delta=1))
        path = os.path.expandvars(os.path.expanduser(item.get("path") or ""))
        if kind == "item" and path and os.path.exists(path):
            m.addSeparator()
            folder = path if os.path.isdir(path) else os.path.dirname(path)
            m.addAction("Open terminal here", lambda: self._terminal_here(item.get("cwd") or folder))
            m.addAction("Show in file manager", lambda: desktop.reveal(path))
        m.addSeparator()
        ins = m.addMenu("Insert below")
        ins.addAction("Item…", lambda: self.add_item(box.tab, "item", idx + 1))
        ins.addAction("Group…", lambda: self.add_item(box.tab, "group", idx + 1))
        ins.addAction("Heading…", lambda: self.add_item(box.tab, "heading", idx + 1))
        ins.addAction("Separator", lambda: self.add_item(box.tab, "separator", idx + 1))
        m.addSeparator()
        m.addAction("Delete…" if runnable(item) else "Delete", lambda: self._delete_item(item))
        m.exec(pos)

    def tab_menu(self, box: _Box, pos):
        tab = box.tab
        several = box in self.selected and len(self.selected) > 1
        group = list(self.selected) if several else [box]
        self._ctx = box.canvas
        m = widgets.menu(box.canvas)
        if isinstance(box, PageBox):
            m.addAction(f"Open {page_name(tab['page'])} as the full page",
                        lambda: self.launcher.overlay.show_page(tab["page"]))
            m.addAction("Rename box…", lambda: self._rename_tab(tab))
        else:
            m.addAction("Add item…", lambda: self.add_item(tab))
            m.addAction("Add files…", lambda: self.add_files(tab))
            m.addAction("Add group (runs several items)…", lambda: self.add_item(tab, "group"))
            m.addAction("Add heading…", lambda: self.add_item(tab, "heading"))
            m.addAction("Add separator", lambda: self.add_item(tab, "separator"))
            m.addSeparator()
            m.addAction("Rename tab…", lambda: self._rename_tab(tab))
            m.addAction("Sort items A–Z", lambda: self.store.sort_tab(tab["id"]))
        m.addSeparator()
        here = self.store.board_of_tab(tab["id"]) or self.store.active_board()
        others = [b for b in self.store.boards() if b["id"] != here["id"]]
        if others:
            sub = m.addMenu(f"Move {len(group)} boxes to dashboard" if several else "Move to dashboard")
            for b in others:
                sub.addAction(b["name"], lambda bid=b["id"]: self.store.move_tabs_to_board(
                    [x.tab["id"] for x in group], bid))
        self._screen_menu(m, group)
        m.addSeparator()
        if several:
            m.addAction(f"Remove the {len(group)} selected boxes…", lambda: self.remove_boxes(group))
        elif isinstance(box, PageBox):
            m.addAction("Take it off the dashboard", lambda: self.remove_boxes([box]))
        else:
            m.addAction("Delete tab…", lambda: self.remove_boxes([box]))
        m.exec(pos)

    def _add_menu(self, pos: QPoint, canvas: QWidget | None = None, at: QPoint | None = None, into=None):
        if into is None:
            self._ctx = self.main
        m = into or widgets.menu(self)
        m.addAction("Tab of items…", lambda: self.new_tab(at, canvas))
        sub = m.addMenu("Page") if into is not None else m
        if into is None:
            m.addSeparator()
            head = m.addAction("PAGES  (resizable boxes)")
            head.setEnabled(False)
        for key, name in boxable_pages():
            sub.addAction(name, lambda k=key: self.add_page(k, at, canvas))
        if into is None:
            m.exec(QPoint(pos.x(), pos.y() - m.sizeHint().height() - 4))

    def canvas_menu(self, canvas: "Canvas", pos: QPoint):
        self._ctx = canvas
        m = widgets.menu(canvas)
        sub = m.addMenu("＋  Add here")
        self._add_menu(pos, canvas, pos, into=sub)
        self._pin_menu(m, canvas)
        if self.selected:
            m.addSeparator()
            head = m.addAction(f"{len(self.selected)} SELECTED")
            head.setEnabled(False)
            sel = list(self.selected)
            m.addAction("Fold them", lambda: [b.set_collapsed(True) or self.store.set_geometry(
                b.tab["id"], collapsed=True) for b in sel])
            m.addAction("Unfold them", lambda: [b.set_collapsed(False) or self.store.set_geometry(
                b.tab["id"], collapsed=False) for b in sel])
            others = [b for b in self.store.boards() if b["id"] != self.store.active_board()["id"]]
            if others:
                mv = m.addMenu("Move to dashboard")
                for b in others:
                    mv.addAction(b["name"], lambda bid=b["id"]: self.store.move_tabs_to_board(
                        [x.tab["id"] for x in sel], bid))
            self._screen_menu(m, sel)
            m.addAction("Remove them…", lambda: self.remove_boxes(sel))
        m.addSeparator()
        m.addAction("Select all   (Ctrl+A)", self.select_all)
        m.addAction("Tidy up this screen", lambda: self.tidy(canvas))
        if self._undo:
            m.addAction("Undo tidy up   (Ctrl+Z)", self.undo_tidy)
        snap = m.addMenu("Snap to a grid")
        cur = self.snap()
        for v, text in ((0, "Off (smooth, pixel by pixel)"), (4, "4 px"), (8, "8 px"), (16, "16 px"),
                        (32, "32 px")):
            a = snap.addAction(text, lambda v=v: self._set_snap(v))
            a.setCheckable(True)
            a.setChecked(cur == v or (v == 0 and cur <= 1))
        board = self.board_of_canvas(canvas)
        if not any(t["id"] == TOOLS_TAB for t in board["tabs"]):
            m.addAction("Bring back the Tools tab", lambda: self.store.restore_tools(
                pos.x(), pos.y(), board["id"], "" if canvas is self.main else canvas.screen_name))
        m.addSeparator()
        m.addAction("Unlock layout" if self.store.locked else "Lock layout", self._toggle_lock)
        m.exec(canvas.mapToGlobal(pos))

    def _set_snap(self, v: int):
        s = self.launcher.settings
        s.set("dashboard.snap", v)
        s.save_quiet()

    def _board_menu(self, bid: str, pos, above: bool = False):
        b = self.store.board(bid)
        if b is None:
            return
        self._ctx = self.main
        m = widgets.menu(self)
        boards = self.store.boards()
        if len(boards) > 1:
            for other in boards:
                m.addAction(("●  " if other["id"] == self.store.active_board()["id"] else "     ") + other["name"],
                            lambda o=other["id"]: self.store.switch_board(o))
            m.addSeparator()
        m.addAction("New dashboard…", self._new_board)
        m.addAction(f"Rename “{b['name']}”…", lambda: self._rename_board(b))
        if len(boards) > 1:
            i = boards.index(b)
            if i > 0:
                m.addAction(f"Move “{b['name']}” left", lambda: self.store.move_board(bid, -1))
            if i < len(boards) - 1:
                m.addAction(f"Move “{b['name']}” right", lambda: self.store.move_board(bid, 1))
        screens = [c for c in self.canvases.values() if c is not self.main]
        if screens:
            sub = m.addMenu(f"Show “{b['name']}” on screen")
            pinned = set(self.store.pinned_screens(bid))
            for c in screens:
                on = c.screen_name in pinned
                sub.addAction(("●  " if on else "     ") + c.screen_name,
                              lambda n=c.screen_name, on=on: self.store.set_screen_board(n, "" if on else bid))
        d = m.addAction(f"Delete “{b['name']}”…", lambda: self._delete_board(b))
        d.setEnabled(len(boards) > 1)
        if above:
            pos = QPoint(pos.x(), pos.y() - m.sizeHint().height() - 4)
        m.exec(pos)

    def _new_board(self):
        cur = self.store.active_board()
        how = widgets.choose(self._parent(), "New dashboard", "How should the new dashboard start?",
                             ["Empty", "With the Tools tab", f"A copy of “{cur['name']}”"])
        if how is None:
            return
        name, ok = widgets.ask_text(self._parent(), "New dashboard", "Name (e.g. Work, Gaming):")
        if ok and name.strip():
            self.store.add_board(name, cur["id"] if how.startswith("A copy") else "", empty=how == "Empty")

    def _rename_board(self, b: dict):
        name, ok = widgets.ask_text(self._parent(), "Rename dashboard", "New name:", b["name"])
        if ok and name.strip():
            self.store.rename_board(b["id"], name)

    def _delete_board(self, b: dict):
        n = sum(len(t["items"]) for t in b["tabs"])
        if widgets.confirm(self._parent(), "Delete dashboard",
                           f'Delete the dashboard "{b["name"]}" with its {len(b["tabs"])} box(es) and {n} item(s)?',
                           yes="Delete", danger=True):
            self.store.delete_board(b["id"])

    def _terminal_here(self, folder: str):
        self.launcher.overlay.hide()
        err = runner.open_terminal(folder, configured=self.launcher.settings.get("dashboard.terminal", ""))
        if err:
            self.launcher.notify("Terminal", err)

    def _rename_item(self, item):
        name, ok = widgets.ask_text(self._parent(), "Rename", "New name:", item.get("name", ""))
        if ok and name.strip():
            self.store.update_item(item["id"], name=name.strip())

    def _delete_item(self, item):
        if not runnable(item) or widgets.confirm(
                self._parent(), "Delete item", f'Delete "{item.get("name")}" from the dashboard?\n'
                                     f"(The file itself is not touched.)", yes="Delete", danger=True):
            self.store.delete_item(item["id"])

    def _rename_tab(self, tab):
        name, ok = widgets.ask_text(self._parent(), "Rename", "New name:", tab["title"])
        if ok and name.strip():
            self.store.rename_tab(tab["id"], name)
