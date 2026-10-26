"""
Dashboard: hack-client style "tab boxes" you can drag around, collapse, and
fill with items that open a script, program, file, folder, URL or command.

    data/dashboard.json:
    {"layout": 3, "active": board id, "locked": false,
     "boards": [{"id", "name",
                 "tabs": [{"id", "title", "x", "y", "w", "collapsed",
                           "items": [{"id", "kind", "name", "path", "args", "hotkey", "action",
                                      "cwd", "admin", "ask_args", "group"}]}]}]}

Boards are whole dashboards (Work, Gaming...) switched from the toolbar. The boxes float
on the whole screen (x/y are screen pixels). Layout 1 files (panel-relative boxes) and
layout 2 files (one board) are moved over once.

Items: `kind` is "item" (default), "group" (runs the items listed in `group`, in order),
"heading" or "separator" (they only tidy a box up). `action` items ("notes:toggle") run a
module action instead of a file; the built-in Tools tab is made of those. Item hotkeys are
registered with owner "dashboard" for every board, whether or not the overlay is open.

Rows can be dragged within a box and onto another box; files dragged in from the file
manager become items. The lock button stops all of that (and moving the boxes).
"""
import os
import uuid
import logging

from PyQt6.QtWidgets import (
    QWidget, QFrame, QLabel, QVBoxLayout, QHBoxLayout, QScrollArea, QDialog, QFormLayout, QLineEdit,
    QFileDialog, QSizePolicy, QPushButton, QCheckBox, QListWidget, QListWidgetItem, QAbstractItemView, QApplication
)
from PyQt6.QtCore import Qt, QObject, QTimer, QPoint, QMimeData, QSize, pyqtSignal
from PyQt6.QtGui import QDrag, QPixmap, QPainter, QColor, QIcon

from core import DATA_DIR, save_json, load_json, widgets, runner, desktop, usage, theme
from core import hotkeys as hk

log = logging.getLogger("dashboard")

PATH = os.path.join(DATA_DIR, "dashboard.json")
GRID = 8
BOX_W = 264
ROW_H = 32
HEAD_ROW_H = 26
SEP_ROW_H = 13
HEADER_H = 34
GAP = 32                    # between boxes when tidied
ORIGIN = (96, 136)          # where the first box sits: below the logo and the page switcher
LAYOUT = 3
TOOLS_TAB = "tools"
ITEM_MIME = "application/x-luminescent-item"
ITEM_KEYS = ("kind", "name", "path", "args", "hotkey", "action", "cwd")


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
    if not isinstance(it.get("group"), list):
        it["group"] = []


class DashboardStore(QObject):
    """The dashboard's data, item hotkeys and running items. Lives as long as the launcher, not the overlay."""
    changed = pyqtSignal()           # boards/tabs/items added, removed, renamed, moved between boxes
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
                t.setdefault("title", "Tab")
                t.setdefault("x", 16)
                t.setdefault("y", 16)
                t.setdefault("w", BOX_W)
                t.setdefault("collapsed", False)
                t["items"] = [i for i in t.get("items", []) if isinstance(i, dict)]
                for it in t["items"]:
                    _fill_item(it)
        if not any(b["id"] == d.get("active") for b in d["boards"]):
            d["active"] = d["boards"][0]["id"]
        d.setdefault("locked", False)
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
        return {"id": TOOLS_TAB, "title": "Tools", "x": x, "y": y, "w": BOX_W, "collapsed": False, "items": items}

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
            self.changed.emit()

    def add_board(self, name: str, copy_from: str = "") -> dict:
        b = {"id": _id(), "name": name.strip() or "Dashboard", "tabs": []}
        if copy_from:
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

    def delete_board(self, bid: str):
        if len(self.data["boards"]) < 2:
            return
        self.data["boards"] = [b for b in self.data["boards"] if b["id"] != bid]
        if self.data.get("active") == bid:
            self.data["active"] = self.data["boards"][0]["id"]
        self._structural()

    def move_tab_to_board(self, tid: str, bid: str):
        src = self.board_of_tab(tid)
        dst = self.board(bid)
        t = self.tab(tid)
        if src is None or dst is None or t is None or src is dst:
            return
        src["tabs"].remove(t)
        dst["tabs"].append(t)
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
    def add_tab(self, title: str, x: int, y: int) -> dict:
        t = {"id": _id(), "title": title, "x": x, "y": y, "w": BOX_W, "collapsed": False, "items": []}
        self.tabs().append(t)
        self._structural()
        return t

    def rename_tab(self, tid: str, title: str):
        t = self.tab(tid)
        if t and title.strip():
            t["title"] = title.strip()
            self._structural()

    def delete_tab(self, tid: str):
        for b in self.data["boards"]:
            b["tabs"] = [t for t in b["tabs"] if t["id"] != tid]
        self._structural()

    def restore_tools(self, x: int, y: int):
        if not any(t["id"] == TOOLS_TAB for t in self.tabs()):
            self.tabs().append(self.tools_tab(x, y))
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
        self.admin.setToolTip("Scripts run with sudo in their terminal; programs and commands ask through pkexec.")
        self.admin.setChecked(bool(self.item.get("admin")))
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
                              "cwd": self.cwd.text().strip(), "admin": self.admin.isChecked()})
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


class _WidthGrip(QWidget):
    def __init__(self, box: "TabBox"):
        super().__init__(box)
        self.box = box
        self.setCursor(Qt.CursorShape.SizeHorCursor)
        self._start = None

    def mousePressEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton and not self.box.page.store.locked:
            self._start = (e.globalPosition().x(), self.box.width())

    def mouseMoveEvent(self, e):
        if self._start:
            w = int(self._start[1] + e.globalPosition().x() - self._start[0])
            w = max(184, min(560, round(w / GRID) * GRID))
            w = min(w, self.box.parentWidget().width() - self.box.x())
            self.box.set_width(w)

    def mouseReleaseEvent(self, e):
        if self._start:
            self._start = None
            self.box.page.store.set_geometry(self.box.tab["id"], w=self.box.width())


class _DropLine(QFrame):
    def __init__(self, parent):
        super().__init__(parent)
        self.setFixedHeight(2)
        self.setStyleSheet(f"background: {theme.P.primary}; border: none;")
        self.hide()


class TabBox(QFrame):
    def __init__(self, page: "DashboardPage", tab: dict):
        super().__init__(page)
        self.page = page
        self.tab = tab
        self.setObjectName("tabBox")
        self.setAcceptDrops(True)
        self._press = None
        self._dragging = False
        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(0)

        self.header = QFrame()
        self.header.setObjectName("tabHeader")
        self.header.setFixedHeight(HEADER_H)
        self.header.setCursor(Qt.CursorShape.PointingHandCursor if page.store.locked else Qt.CursorShape.OpenHandCursor)
        hl = QHBoxLayout(self.header)
        hl.setContentsMargins(11, 0, 9, 0)
        hl.setSpacing(9)
        accent = QFrame()
        accent.setObjectName("tabAccent")
        accent.setFixedSize(3, 12)
        hl.addWidget(accent)
        title = widgets.label(tab["title"].upper(), 11, 600, mono=True, spacing=0.14)
        title.setObjectName("tabTitle")
        title.setSizePolicy(QSizePolicy.Policy.Ignored, QSizePolicy.Policy.Preferred)
        hl.addWidget(title, 1)
        count = widgets.label(f"{sum(1 for i in tab['items'] if runnable(i)):02d}", 10, 500, mono=True, tone="muted")
        hl.addWidget(count)
        self.chevron = widgets.label("", 10, tone="muted")
        self.chevron.setFixedWidth(14)
        self.chevron.setAlignment(Qt.AlignmentFlag.AlignCenter)
        hl.addWidget(self.chevron)
        for w in (accent, title, count, self.chevron):
            w.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        lay.addWidget(self.header)

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
        self._grip = _WidthGrip(self)
        self._drop_line = _DropLine(self.body)

        self.move(int(tab.get("x", ORIGIN[0])), int(tab.get("y", ORIGIN[1])))
        self.set_width(int(tab.get("w", BOX_W)))
        self.set_collapsed(bool(tab.get("collapsed")))

    def _body_height(self) -> int:
        if not self.tab["items"]:
            return ROW_H + 26
        return sum(r.height() for r in self.rows) + 8

    def fit(self):
        """Size to the content, but never past the bottom of the screen (the body scrolls instead)."""
        if self.tab.get("collapsed"):
            h = HEADER_H + 2
        else:
            room = max(HEADER_H + ROW_H + 12, self.parentWidget().height() - self.y() - GRID)
            h = min(HEADER_H + 2 + self._body_height(), room)
        self.setFixedHeight(h)
        self._grip.setGeometry(self.width() - 5, 0, 5, self.height())
        self._grip.raise_()

    def set_width(self, w: int):
        self.setFixedWidth(w)
        self.fit()

    def set_collapsed(self, on: bool):
        self.tab["collapsed"] = on
        self.scroll.setVisible(not on)
        self.chevron.setText("▸" if on else "▾")
        self.header.setProperty("collapsed", on)
        self.header.style().unpolish(self.header)
        self.header.style().polish(self.header)
        self.fit()

    def apply_filter(self, q: str) -> int:
        """Highlight the rows matching `q`, fade the rest. Returns how many matched."""
        n = 0
        for r in self.rows:
            hit = bool(q) and runnable(r.item) and q in r.search_text()
            n += hit
            for prop, val in (("match", hit), ("dim", bool(q) and not hit)):
                if r.property(prop) != val:
                    r.setProperty(prop, val)
                    r.style().unpolish(r)
                    r.style().polish(r)
                    for c in r.findChildren(QLabel):
                        c.style().unpolish(c)
                        c.style().polish(c)
        if q and n and self.tab.get("collapsed"):
            self.set_collapsed(False)
        return n

    # dragging by the header: a plain click (moved < 4 px) toggles collapse instead
    def mousePressEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton and e.position().y() <= HEADER_H:
            self._press = (e.globalPosition().toPoint(), self.pos())
            self._dragging = False
            self.raise_()
            self.page.raise_dock()
        else:
            super().mousePressEvent(e)

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
            self.style().unpolish(self)
            self.style().polish(self)
        canvas = self.parentWidget()
        x = round((origin.x() + d.x()) / GRID) * GRID
        y = round((origin.y() + d.y()) / GRID) * GRID
        x = max(0, min(x, canvas.width() - self.width()))
        y = max(0, min(y, canvas.height() - HEADER_H - 2))
        self.move(x, y)

    def mouseReleaseEvent(self, e):
        if self._press is None:
            return
        self._press = None
        if self._dragging:
            self._dragging = False
            self.header.setCursor(Qt.CursorShape.OpenHandCursor)
            self.setProperty("active", False)
            self.style().unpolish(self)
            self.style().polish(self)
            self.fit()
            self.page.store.set_geometry(self.tab["id"], x=self.x(), y=self.y())
        else:
            self.set_collapsed(not self.tab.get("collapsed"))
            self.page.store.set_geometry(self.tab["id"], collapsed=self.tab["collapsed"])

    def contextMenuEvent(self, e):
        if e.pos().y() <= HEADER_H:
            self.page.tab_menu(self, e.globalPos())

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
            self.style().unpolish(self)
            self.style().polish(self)

    def dragMoveEvent(self, e):
        if self._accepts(e.mimeData()):
            e.acceptProposedAction()
            self._show_line(self._drop_index(e.position().toPoint()))

    def dragLeaveEvent(self, e):
        self._drop_line.hide()
        self.setProperty("active", False)
        self.style().unpolish(self)
        self.style().polish(self)

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


class DashboardPage(QWidget):
    """The tab boxes, straight on the overlay's full-screen layer, with a toolbar pill at the bottom centre."""

    def __init__(self, launcher, host: QWidget | None = None):
        super().__init__(host)
        self.launcher = launcher
        self.store: DashboardStore = launcher.dashboard
        self.boxes: list[TabBox] = []
        if host is not None:
            self.setGeometry(host.rect())
        self.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.customContextMenuRequested.connect(self._canvas_menu)
        self._press_empty = False

        self.dock = QFrame(self)
        self.dock.setObjectName("lumDock")
        dl = QHBoxLayout(self.dock)
        dl.setContentsMargins(5, 5, 5, 5)
        dl.setSpacing(6)
        self.boards_box = QHBoxLayout()
        self.boards_box.setSpacing(2)
        dl.addLayout(self.boards_box)
        add = dock_button("New tab", "＋")
        add.clicked.connect(lambda: self.new_tab())
        dl.addWidget(add)
        tidy = dock_button("Tidy up", variant="ghost")
        tidy.setToolTip("Line every tab up in columns")
        tidy.clicked.connect(self.tidy)
        dl.addWidget(tidy)
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
                             "· drop files from your file manager onto a box")
        dl.addWidget(self.hint)

        self.empty = widgets.label("No tabs yet. Click ＋ New tab, name it (e.g. Scripts), then right-click its "
                                   "header → Add item.", 13, tone="muted")
        self.empty.setParent(self)
        self.store.changed.connect(self.refresh)
        self.store.running_changed.connect(self.refresh)
        self.refresh()

    def teardown(self):
        for sig in (self.store.changed, self.store.running_changed):
            try:
                sig.disconnect(self.refresh)
            except TypeError:
                pass

    @property
    def canvas(self) -> QWidget:
        return self

    def raise_dock(self):
        self.dock.raise_()

    # ── building ───────────────────────────────────────────────────────────
    def refresh(self):
        for b in self.boxes:
            b.setParent(None)
            b.deleteLater()
        self.boxes = [TabBox(self, t) for t in self.store.tabs()]
        for b in self.boxes:
            b.show()
        self.empty.setVisible(not self.boxes)
        self._build_boards()
        locked = self.store.locked
        self.lock_btn.setChecked(locked)
        self.lock_btn.setText("🔒 Locked" if locked else "🔓 Lock")
        self.lock_btn.setToolTip("Unlock to move boxes and items again" if locked else
                                 "Lock the layout: no moving boxes or dragging items")
        self.hint.setText("Layout locked" if locked else "Drag headers and items · drop files on a box")
        if self.search.text():
            self._filter(self.search.text())
        self._clamp()
        self.raise_dock()

    def _build_boards(self):
        widgets.clear_layout(self.boards_box)
        boards = self.store.boards()
        if len(boards) > 1:
            for b in boards:
                btn = QPushButton(b["name"])
                btn.setObjectName("boardTab")
                btn.setCheckable(True)
                btn.setChecked(b["id"] == self.store.active_board()["id"])
                btn.setCursor(Qt.CursorShape.PointingHandCursor)
                btn.setFixedHeight(32)
                btn.clicked.connect(lambda _, bid=b["id"]: self.store.switch_board(bid))
                btn.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
                btn.customContextMenuRequested.connect(
                    lambda pos, bid=b["id"], w=btn: self._board_menu(bid, w.mapToGlobal(pos)))
                self.boards_box.addWidget(btn)
        more = dock_button("▾" if len(boards) > 1 else "Dashboards ▾", variant="ghost")
        more.setToolTip("New dashboard, rename, delete…")
        more.clicked.connect(lambda: self._board_menu(self.store.active_board()["id"],
                                                      more.mapToGlobal(more.rect().topLeft()), above=True))
        self.boards_box.addWidget(more)
        sep = QFrame()
        sep.setObjectName("lumDockSep")
        sep.setFixedSize(1, 18)
        self.boards_box.addWidget(sep)

    def resizeEvent(self, e):
        super().resizeEvent(e)
        self._clamp()

    def _clamp(self):
        cw, ch = self.width(), self.height()
        if cw < 50 or ch < 50:
            return
        self.dock.adjustSize()
        self.dock.move((cw - self.dock.width()) // 2, ch - 28 - self.dock.height())
        self.empty.adjustSize()
        self.empty.move((cw - self.empty.width()) // 2, ch // 3)
        for b in self.boxes:
            x = max(0, min(b.x(), cw - b.width()))
            y = max(0, min(b.y(), ch - HEADER_H - 2))
            if (x, y) != (b.x(), b.y()):
                b.move(x, y)
            b.fit()

    def on_show(self):
        QTimer.singleShot(0, self._clamp)

    def on_hide(self):
        if self.search.text():
            self.search.clear()

    def _columns(self) -> int:
        return max(1, (self.width() - ORIGIN[0] * 2 + GAP) // (BOX_W + GAP))

    def _free_spot(self) -> QPoint:
        cols = self._columns()
        n = len(self.store.tabs())
        return QPoint(ORIGIN[0] + (n % cols) * (BOX_W + GAP), ORIGIN[1] + (n // cols) * 56)

    # a click on the open screen closes the overlay, like a click on the dimmed area; presses that bubble up from
    # a box, a row or the toolbar are swallowed here so they never reach the backdrop
    def mousePressEvent(self, e):
        self._press_empty = (e.button() == Qt.MouseButton.LeftButton
                             and self.childAt(e.position().toPoint()) is None)
        e.accept()

    def mouseReleaseEvent(self, e):
        if self._press_empty and self.rect().contains(e.position().toPoint()):
            self.launcher.overlay.outside_click()
        self._press_empty = False

    # ── search, lock ───────────────────────────────────────────────────────
    def _filter(self, text: str):
        q = text.strip().lower()
        total = sum(b.apply_filter(q) for b in self.boxes)
        if q:
            self.hint.setText(f"{total} match{'es' if total != 1 else ''} · Enter runs the first")
        else:
            self.hint.setText("Layout locked" if self.store.locked else "Drag headers and items · drop files on a box")
        self._clamp()

    def _run_first_match(self):
        q = self.search.text().strip().lower()
        if not q:
            return
        for b in self.boxes:
            for r in b.rows:
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

    def new_tab(self, at: QPoint | None = None):
        name, ok = widgets.ask_text(self, "New tab", "Name of the tab (e.g. Scripts, Games, Work):")
        if ok and name.strip():
            p = at or self._free_spot()
            self.store.add_tab(name.strip(), round(p.x() / GRID) * GRID, round(p.y() / GRID) * GRID)

    def add_item(self, tab: dict, kind: str = "item", index: int = -1):
        if kind in ("separator", "heading"):
            name = ""
            if kind == "heading":
                name, ok = widgets.ask_text(self, "Heading", "Heading text:")
                if not ok or not name.strip():
                    return
            self.store.add_item(tab["id"], {"kind": kind, "name": name.strip()}, index)
            return
        dlg = ItemDialog(self.launcher, None, self, kind=kind)
        if dlg.exec() == QDialog.DialogCode.Accepted:
            self.store.add_item(tab["id"], dlg.item, index)

    def add_files(self, tab: dict):
        paths, _ = QFileDialog.getOpenFileNames(self, "Add files", os.path.expanduser("~"))
        if paths:
            self.store.add_paths(tab["id"], paths)

    def edit_item(self, item: dict, focus: str = "name"):
        if item.get("kind") == "heading":
            self._rename_item(item)
            return
        dlg = ItemDialog(self.launcher, item, self, focus=focus)
        if dlg.exec() == QDialog.DialogCode.Accepted:
            self.store.update_item(item["id"], **{k: dlg.item.get(k, "") for k in
                                                  ("name", "path", "args", "hotkey", "cwd", "admin", "ask_args",
                                                   "group") if k in dlg.item})

    def run_with_args(self, item: dict):
        text, ok = widgets.ask_text(self, item.get("name") or "Run", "Arguments:", item.get("args", ""))
        if ok:
            self.run(item["id"], text)

    def tidy(self):
        if self.store.locked:
            return
        cols = self._columns()
        heights = [ORIGIN[1]] * cols
        for b in self.boxes:
            c = heights.index(min(heights))
            x, y = ORIGIN[0] + c * (BOX_W + GAP), heights[c]
            b.move(x, y)
            b.set_width(BOX_W)
            heights[c] = y + b.height() + GAP
            self.store.set_geometry(b.tab["id"], x=x, y=y, w=BOX_W)

    # ── menus ──────────────────────────────────────────────────────────────
    def item_menu(self, box: TabBox, item: dict, pos):
        m = widgets.menu(self)
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
        others = [t for t in self.store.tabs() if t["id"] != box.tab["id"]]
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

    def tab_menu(self, box: TabBox, pos):
        tab = box.tab
        m = widgets.menu(self)
        m.addAction("Add item…", lambda: self.add_item(tab))
        m.addAction("Add files…", lambda: self.add_files(tab))
        m.addAction("Add group (runs several items)…", lambda: self.add_item(tab, "group"))
        m.addAction("Add heading…", lambda: self.add_item(tab, "heading"))
        m.addAction("Add separator", lambda: self.add_item(tab, "separator"))
        m.addSeparator()
        m.addAction("Rename tab…", lambda: self._rename_tab(tab))
        m.addAction("Sort items A–Z", lambda: self.store.sort_tab(tab["id"]))
        others = [b for b in self.store.boards() if b["id"] != self.store.active_board()["id"]]
        if others:
            sub = m.addMenu("Move to dashboard")
            for b in others:
                sub.addAction(b["name"], lambda bid=b["id"]: self.store.move_tab_to_board(tab["id"], bid))
        m.addSeparator()
        m.addAction("Delete tab…", lambda: self._delete_tab(tab))
        m.exec(pos)

    def _canvas_menu(self, pos):
        m = widgets.menu(self)
        m.addAction("New tab here…", lambda: self.new_tab(pos))
        m.addAction("Tidy up", self.tidy)
        if not any(t["id"] == TOOLS_TAB for t in self.store.tabs()):
            m.addAction("Bring back the Tools tab", lambda: self.store.restore_tools(pos.x(), pos.y()))
        m.addSeparator()
        m.addAction("Unlock layout" if self.store.locked else "Lock layout", self._toggle_lock)
        m.exec(self.canvas.mapToGlobal(pos))

    def _board_menu(self, bid: str, pos, above: bool = False):
        b = self.store.board(bid)
        if b is None:
            return
        m = widgets.menu(self)
        if len(self.store.boards()) > 1:
            for other in self.store.boards():
                a = m.addAction(("●  " if other["id"] == self.store.active_board()["id"] else "     ") + other["name"],
                                lambda o=other["id"]: self.store.switch_board(o))
            m.addSeparator()
        m.addAction("New dashboard…", lambda: self._new_board(""))
        m.addAction(f"Duplicate “{b['name']}”…", lambda: self._new_board(bid))
        m.addAction(f"Rename “{b['name']}”…", lambda: self._rename_board(b))
        d = m.addAction(f"Delete “{b['name']}”…", lambda: self._delete_board(b))
        d.setEnabled(len(self.store.boards()) > 1)
        if above:
            pos = QPoint(pos.x(), pos.y() - m.sizeHint().height() - 4)
        m.exec(pos)

    def _new_board(self, copy_from: str):
        name, ok = widgets.ask_text(self, "New dashboard", "Name (e.g. Work, Gaming):")
        if ok and name.strip():
            self.store.add_board(name, copy_from)

    def _rename_board(self, b: dict):
        name, ok = widgets.ask_text(self, "Rename dashboard", "New name:", b["name"])
        if ok and name.strip():
            self.store.rename_board(b["id"], name)

    def _delete_board(self, b: dict):
        n = sum(len(t["items"]) for t in b["tabs"])
        if widgets.confirm(self, "Delete dashboard", f'Delete the dashboard "{b["name"]}" with its {len(b["tabs"])} '
                                                     f"tab(s) and {n} item(s)?", yes="Delete", danger=True):
            self.store.delete_board(b["id"])

    def _terminal_here(self, folder: str):
        self.launcher.overlay.hide()
        err = runner.open_terminal(folder, configured=self.launcher.settings.get("dashboard.terminal", ""))
        if err:
            self.launcher.notify("Terminal", err)

    def _rename_item(self, item):
        name, ok = widgets.ask_text(self, "Rename", "New name:", item.get("name", ""))
        if ok and name.strip():
            self.store.update_item(item["id"], name=name.strip())

    def _delete_item(self, item):
        if not runnable(item) or widgets.confirm(
                self, "Delete item", f'Delete "{item.get("name")}" from the dashboard?\n'
                                     f"(The file itself is not touched.)", yes="Delete", danger=True):
            self.store.delete_item(item["id"])

    def _rename_tab(self, tab):
        name, ok = widgets.ask_text(self, "Rename tab", "New name:", tab["title"])
        if ok and name.strip():
            self.store.rename_tab(tab["id"], name)

    def _delete_tab(self, tab):
        n = len(tab["items"])
        extra = f" and its {n} item(s)" if n else ""
        if widgets.confirm(self, "Delete tab", f'Delete the tab "{tab["title"]}"{extra}?', yes="Delete", danger=True):
            self.store.delete_tab(tab["id"])
