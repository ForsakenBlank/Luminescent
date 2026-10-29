"""
Keybinds: every keybind on the system in one searchable list.

Hyprland's own binds (from your config, with what they really do: "SUPER + G →
Open BROWSER: zen-browser") and Luminescent's hotkeys, with clashes marked.
Double-click a Hyprland bind to open its line in an editor, or a Luminescent
one to jump to its settings.  Hyprland binds can be re-keyed, removed (the line
is commented out) or added from here; every edit keeps a backup in
data/hypr-backups and is undone if Hyprland reports a config error.
The parsing and editing live in core/hyprbinds.py.

My keybinds: keys or mouse buttons (the side buttons of a Logitech mouse...) that Luminescent itself binds
while it runs, to open an app, run a command, use a Luminescent tool, change the volume or press other keys.
They are kept in settings keybinds.custom: [{"id", "keys", "does", "value", "name"}].
"""
import os
import uuid
import subprocess

from PyQt6.QtWidgets import (
    QLabel, QLineEdit, QTreeWidget, QTreeWidgetItem, QVBoxLayout, QHBoxLayout, QComboBox, QDialog,
    QFormLayout, QAbstractItemView, QHeaderView, QWidget, QStackedWidget, QFrame, QCompleter
)
from PyQt6.QtCore import Qt, QTimer
from PyQt6.QtGui import QColor, QBrush

from core import widgets, hyprbinds, hypr, theme, IS_WIN
from core import hotkeys as hk
from core.module import Module

FILTERS = [("all", "All keybinds"), ("mine", "My keybinds"), ("hyprland", "Hyprland (your config)"),
           ("luminescent", "Luminescent"), ("conflicts", "Clashes only")]
DOES = [("app", "Open an app"), ("command", "Run a command"), ("tool", "Use a Luminescent tool"),
        ("keys", "Press other keys"), ("volume", "Change the volume")]
VOLUME = [("up", "Volume up 5 %"), ("down", "Volume down 5 %"), ("mute", "Mute on / off")]
MINE = "mykeys"                      # the owner Luminescent registers your own keybinds under


def _hypr_keys(combo: str) -> str:
    """'super+shift+g' -> 'SUPER + SHIFT + g' (one key plus modifiers)."""
    keys = hk.to_hyprland(combo)
    return keys[0] if keys else ""


class MousePad(QFrame):
    """'Click here with the mouse button you want': side and extra buttons become mouse:275, mouse:276…"""
    picked = None

    _BUTTONS = {Qt.MouseButton.MiddleButton: 274, Qt.MouseButton.BackButton: 275, Qt.MouseButton.ForwardButton: 276,
                Qt.MouseButton.ExtraButton3: 277, Qt.MouseButton.ExtraButton4: 278, Qt.MouseButton.ExtraButton5: 279,
                Qt.MouseButton.ExtraButton6: 280}

    def __init__(self, on_pick):
        super().__init__()
        self.on_pick = on_pick
        self.setObjectName("keyBox")
        self.setFixedHeight(32)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        lay = QHBoxLayout(self)
        lay.setContentsMargins(10, 0, 10, 0)
        lay.addWidget(widgets.label("Click here with a side, middle or extra mouse button", 12, tone="muted"))

    def mousePressEvent(self, e):
        n = self._BUTTONS.get(e.button())
        if n is not None:
            self.on_pick(f"mouse:{n}")
        e.accept()


class _KeysDialog(widgets.RoundedDialog):
    """Ask for a key combination or mouse button (and, for a new bind, what it does)."""

    def __init__(self, parent, launcher, title: str, combo: str = "", with_command: bool = False,
                 mine: dict | None = None, tools: list | None = None):
        super().__init__(parent, title)
        from settings_ui import HotkeyEdit
        self.launcher = launcher
        self.mine = mine is not None
        lay = self.body
        lay.setSpacing(10)
        form = QFormLayout()
        form.setSpacing(8)
        self.keys = HotkeyEdit()
        self.keys.set_value(combo)
        if launcher is not None:
            self.keys.recording_changed.connect(launcher.hotkeys.set_suspended)
        form.addRow("Keys", self.keys)
        if not IS_WIN:
            # a mouse button keeps the modifiers already typed: ctrl + side button
            form.addRow("", MousePad(self._mouse))
            pick = QComboBox()
            pick.addItem("…or pick a mouse button", "")
            for key, label in hk.MOUSE_BUTTONS:
                pick.addItem(label, key)
            pick.activated.connect(lambda i: self._mouse(pick.itemData(i)) if pick.itemData(i) else None)
            form.addRow("", pick)
        self.command = QLineEdit()
        self.command.setPlaceholderText("e.g. kitty, or uwsm app -- zen-browser")
        if with_command:
            form.addRow("Runs", self.command)
        self.name = QLineEdit()
        self.does = QComboBox()
        self.stack = QStackedWidget()
        self._values: dict[str, QWidget] = {}
        if self.mine:
            m = mine or {}
            self.name.setPlaceholderText("A name for the list (optional)")
            self.name.setText(str(m.get("name", "")))
            for key, label in DOES:
                if IS_WIN and key in ("keys", "volume"):
                    continue
                self.does.addItem(label, key)
                self._values[key] = w = self._value_widget(key, tools or [])
                self.stack.addWidget(w)
            self.does.currentIndexChanged.connect(lambda _i: self.stack.setCurrentWidget(
                self._values[self.does.currentData()]))
            i = self.does.findData(m.get("does", "app"))
            self.does.setCurrentIndex(max(0, i))
            self.stack.setCurrentWidget(self._values[self.does.currentData()])
            self._set_value(m.get("does", "app"), m.get("value", ""))
            form.addRow("Does", self.does)
            form.addRow("", self.stack)
            form.addRow("Name", self.name)
        lay.addLayout(form)
        hint = QLabel("Click the keys to type a combo (super+shift+g), press Record, or click the box above with a "
                      "mouse button. Record can not see combos Hyprland already uses, so type those.")
        hint.setObjectName("lumHint")
        hint.setWordWrap(True)
        lay.addWidget(hint)
        btns = QHBoxLayout()
        btns.addStretch()
        cancel = widgets.button("Cancel", "ghost")
        cancel.clicked.connect(self.reject)
        ok = widgets.button("Save", "primary")
        ok.clicked.connect(self.accept)
        btns.addWidget(cancel)
        btns.addWidget(ok)
        lay.addLayout(btns)
        self.resize(560, 0)

    def _mouse(self, button: str):
        mods = [p for p in hk.normalize(self.keys.value()).split("+") if p in hk.MODIFIERS]
        self.keys.set_value("+".join(mods + [button]))

    def _value_widget(self, key: str, tools: list) -> QWidget:
        if key == "app":
            box = QComboBox()
            box.setEditable(True)
            box.setInsertPolicy(QComboBox.InsertPolicy.NoInsert)
            from core import desktop
            for name, path in (desktop.apps() if not IS_WIN else []):
                box.addItem(name, path)
            box.completer().setFilterMode(Qt.MatchFlag.MatchContains)
            box.completer().setCompletionMode(QCompleter.CompletionMode.PopupCompletion)
            box.lineEdit().setPlaceholderText("Type to search your apps, or a program name")
            return box
        if key == "command":
            e = QLineEdit()
            e.setPlaceholderText("e.g. grim -g \"$(slurp)\" ~/shot.png")
            return e
        if key == "tool":
            box = QComboBox()
            box.setEditable(True)
            box.setInsertPolicy(QComboBox.InsertPolicy.NoInsert)
            for aid, label in tools:
                box.addItem(label, aid)
            box.completer().setFilterMode(Qt.MatchFlag.MatchContains)
            box.completer().setCompletionMode(QCompleter.CompletionMode.PopupCompletion)
            return box
        if key == "keys":
            from settings_ui import HotkeyEdit
            e = HotkeyEdit()
            if self.launcher is not None:
                e.recording_changed.connect(self.launcher.hotkeys.set_suspended)
            return e
        box = QComboBox()
        for v, label in VOLUME:
            box.addItem(label, v)
        return box

    def _set_value(self, does: str, value: str):
        w = self._values.get(does)
        if w is None or not value:
            return
        if isinstance(w, QLineEdit):
            w.setText(value)
        elif isinstance(w, QComboBox):
            i = w.findData(value)
            if i >= 0:
                w.setCurrentIndex(i)
            elif w.isEditable():
                w.setEditText(value)
        elif hasattr(w, "set_value"):
            w.set_value(value)

    def value(self) -> str:
        w = self._values.get(self.does.currentData())
        if isinstance(w, QLineEdit):
            return w.text().strip()
        if isinstance(w, QComboBox):
            # a typed program name that is not in the list is kept as typed
            if w.isEditable() and w.currentText() != w.itemText(w.currentIndex()):
                return w.currentText().strip()
            return str(w.currentData() or w.currentText()).strip()
        if hasattr(w, "value"):
            return w.value()
        return ""

    def entry(self) -> dict:
        """The keybind as it is stored in keybinds.custom."""
        does = self.does.currentData()
        name = self.name.text().strip()
        if not name:
            w = self._values.get(does)
            label = w.currentText() if isinstance(w, QComboBox) else self.value()
            name = {"app": "Open {}", "command": "Run {}", "tool": "{}", "keys": "Press {}",
                    "volume": "{}"}[does].format(hk.pretty(label) if does == "keys" else label)
        return {"keys": self.combo(), "does": does, "value": self.value(), "name": name}

    def combo(self) -> str:
        return self.keys.value()


class KeybindsPanel(widgets.FloatingPanel):
    def __init__(self, m: "KeybindsModule"):
        self.m = m
        super().__init__("Keybinds", min_size=(640, 380), opacity=0.97, close_hides=True)
        self._rows: list[hyprbinds.Bind] = []
        lay = QVBoxLayout()
        lay.setContentsMargins(12, 10, 12, 12)
        lay.setSpacing(8)
        top = QHBoxLayout()
        self.search = QLineEdit()
        self.search.setPlaceholderText("🔍  Search keys, actions, commands…")
        self.search.setClearButtonEnabled(True)
        self.search.textChanged.connect(self._fill)
        top.addWidget(self.search, 1)
        self.filter = QComboBox()
        for key, label in FILTERS:
            self.filter.addItem(label, key)
        self.filter.currentIndexChanged.connect(self._fill)
        top.addWidget(self.filter)
        refresh = widgets.button("Refresh")
        refresh.clicked.connect(self.reload)
        top.addWidget(refresh)
        lay.addLayout(top)

        self.tree = QTreeWidget()
        self.tree.setColumnCount(5)
        self.tree.setHeaderLabels(["Keys", "Action", "Details", "Where", ""])
        self.tree.setRootIsDecorated(False)
        self.tree.setUniformRowHeights(True)
        self.tree.setSelectionMode(QAbstractItemView.SelectionMode.SingleSelection)
        self.tree.header().setSortIndicator(-1, Qt.SortOrder.AscendingOrder)   # config order until a header is clicked
        self.tree.setSortingEnabled(True)
        head = self.tree.header()
        head.setSectionResizeMode(0, QHeaderView.ResizeMode.ResizeToContents)
        head.setSectionResizeMode(1, QHeaderView.ResizeMode.ResizeToContents)
        head.setSectionResizeMode(2, QHeaderView.ResizeMode.Stretch)
        head.setSectionResizeMode(3, QHeaderView.ResizeMode.ResizeToContents)
        head.setSectionResizeMode(4, QHeaderView.ResizeMode.Fixed)
        head.resizeSection(4, 60)
        head.setStretchLastSection(False)
        self.tree.setStyleSheet(f"QTreeWidget {{ font-size: {theme.fs(12)}px; }}")
        self.tree.itemDoubleClicked.connect(lambda *_: self._open())
        lay.addWidget(self.tree, 1)

        self.count = QLabel()
        self.count.setObjectName("lumHint")
        lay.addWidget(self.count)
        btns = QHBoxLayout()
        editable = hypr.available()          # adding / changing binds edits your Hyprland config
        mine = widgets.button("＋  New keybind", "primary")
        mine.setToolTip("Keys or a mouse button that open an app, run a command, use a Luminescent tool…")
        mine.clicked.connect(self._add_mine)
        btns.addWidget(mine)
        for text, fn, variant in (("Add to Hyprland config…", self._add, ""), ("Change keys…", self._change, ""),
                                  ("Open in editor", self._open, ""), ("Remove…", self._delete, "danger")):
            if not editable and text not in ("Change keys…", "Remove…"):
                continue
            b = widgets.button(text, variant)
            b.clicked.connect(fn)
            btns.addWidget(b)
        btns.addStretch()
        lay.addLayout(btns)
        hint = QLabel("✎ changes a bind's keys, ✕ removes it. New keybind works while Luminescent runs and can use "
                      "mouse side buttons; Add to Hyprland config writes the bind into your config file. Red rows "
                      "share their keys with another bind. Double-click opens a bind's line in your config."
                      if editable else
                      "Every Luminescent hotkey. ✎ changes the keys, ✕ turns it off; red rows share their keys.")
        hint.setObjectName("lumHint")
        hint.setWordWrap(True)
        lay.addWidget(hint)
        self.body.addLayout(lay)
        self.resize(980, 620)
        self.center_on(widgets.cursor_screen_geometry())
        self.escape_pressed.connect(self.hide)

    # ── data ───────────────────────────────────────────────────────────────
    def reload(self):
        try:
            self._rows = hyprbinds.load(self.m.luminescent_binds())
        except Exception as e:
            self.m.log.exception("could not read the keybinds")
            self._rows = []
            self.count.setText(f"Could not read the keybinds: {e}")
        if not hypr.available():
            self.count.setText("Hyprland is not running: only Luminescent's hotkeys are listed.")
        self._fill()

    def _where(self, r: hyprbinds.Bind) -> str:
        if r.origin == "luminescent":
            return f"Luminescent · {self.m.owner_title(r.owner)}"
        if not r.file:
            return "Hyprland"
        rel = os.path.relpath(r.file, hyprbinds.CONFIG_DIR)
        return f"{rel}:{r.line}" + ("  (loop)" if r.call is not None and r.call.in_loop else "")

    def _fill(self, *_):
        q = self.search.text().strip().lower()
        mode = self.filter.currentData()
        self.tree.setSortingEnabled(False)
        self.tree.clear()
        red = QBrush(QColor("#E8645E"))
        shown = 0
        for i, r in enumerate(self._rows):
            if mode == "hyprland" and r.origin != "hyprland":
                continue
            if mode == "luminescent" and r.origin != "luminescent":
                continue
            if mode == "mine" and not (r.origin == "luminescent" and r.owner == MINE):
                continue
            if mode == "conflicts" and not r.conflict:
                continue
            if r.origin == "hyprland" or r.owner == MINE:
                action = r.action
            else:
                action = f"{self.m.owner_title(r.owner)}: {r.action}"
            detail = r.detail + (f"   [{', '.join(r.flags)}]" if r.flags else "")
            cols = [r.keys, action, detail, self._where(r)]
            if q and q not in " ".join(cols).lower():
                continue
            it = QTreeWidgetItem(cols)
            it.setData(0, Qt.ItemDataRole.UserRole, i)
            it.setToolTip(2, detail)
            if r.conflict:
                for c in range(4):
                    it.setForeground(c, red)
                it.setToolTip(0, "Another bind uses the same keys")
            self.tree.addTopLevelItem(it)
            self.tree.setItemWidget(it, 4, self._row_buttons(it))
            shown += 1
        self.tree.setSortingEnabled(True)
        n_conf = sum(1 for r in self._rows if r.conflict)
        self.count.setText(f"{shown} shown · {len(self._rows)} keybinds"
                           + (f" · {n_conf} clashing" if n_conf else ""))

    def _row_buttons(self, it: QTreeWidgetItem) -> QWidget:
        w = QWidget()
        lay = QHBoxLayout(w)
        lay.setContentsMargins(0, 0, 4, 0)
        lay.setSpacing(2)
        for glyph, tip, fn in (("✎", "Change the keys", self._change), ("✕", "Remove", self._delete)):
            b = widgets.icon_button(glyph, tip, 24, 11, ghost=True)
            b.clicked.connect(lambda _c=False, f=fn, item=it: (self.tree.setCurrentItem(item), f()))
            lay.addWidget(b)
        return w

    def _selected(self) -> hyprbinds.Bind | None:
        it = self.tree.currentItem()
        if it is None:
            return None
        return self._rows[it.data(0, Qt.ItemDataRole.UserRole)]

    # ── actions ────────────────────────────────────────────────────────────
    def _open(self):
        r = self._selected()
        if r is None:
            return
        if r.origin == "luminescent":
            self.hide()
            self.m.ctx.open_settings(self.m.owner_section(r.owner))
            return
        if not r.file:
            widgets.message(self, "Keybinds", "This bind was not found in your config files.")
            return
        err = hyprbinds.open_in_editor(r.file, r.line)
        if err:
            widgets.error(self, "Keybinds", err)

    def _clash(self, keys: str, release: bool = False, ignore: hyprbinds.Bind | None = None) -> str:
        parts = [p.strip() for p in keys.split("+")]
        cid = (tuple(sorted(hyprbinds._MOD_NAMES.get(p.upper(), p.upper()) for p in parts[:-1])),
               parts[-1].lower(), release)
        other = next((r for r in self._rows if r is not ignore and r.combo_id() == cid), None)
        if other is None:
            return ""
        return f"{keys} is already used by: {other.action} {other.detail}".strip()

    def _done(self, err: str, ok: str):
        if err:
            widgets.error(self, "Keybinds", err)
        else:
            self.m.ctx.notify("Keybinds", ok, 2500)
        QTimer.singleShot(300, self.reload)

    def _add(self):
        dlg = _KeysDialog(self, self.m.ctx.launcher, "Add a Hyprland keybind", with_command=True)
        if dlg.exec() != QDialog.DialogCode.Accepted:
            return
        keys, cmd = _hypr_keys(dlg.combo()), dlg.command.text().strip()
        if not keys or not cmd:
            widgets.error(self, "Keybinds", "Give it keys and a command.")
            return
        clash = self._clash(keys)
        if clash and not widgets.confirm(self, "Keybinds", clash + "\n\nAdd it anyway?"):
            return
        self._done(hyprbinds.add(keys, cmd), f"{keys} now runs {cmd}")

    def _add_mine(self):
        dlg = _KeysDialog(self, self.m.ctx.launcher, "New keybind", mine={}, tools=self.m.tools())
        while dlg.exec() == QDialog.DialogCode.Accepted:
            e = dlg.entry()
            err = self._check_mine(e)
            if not err:
                self.m.save_custom(None, e)
                self._done("", f"{hk.pretty(e['keys'])} now does: {e['name']}")
                return
            widgets.error(self, "Keybinds", err)

    def _check_mine(self, e: dict, ignore: dict | None = None) -> str:
        if not e["keys"]:
            return "Press Record, type the keys or click the box with a mouse button."
        err = hk.validate(e["keys"])
        if err:
            return err
        if not e["value"]:
            return "Choose what it does."
        owner = self.m.hotkeys.owner_of(e["keys"])
        if owner and not (ignore and hk.normalize(ignore.get("keys", "")) == hk.normalize(e["keys"])):
            return f"{hk.pretty(e['keys'])} is already a Luminescent hotkey ({self.m.owner_title(owner)})."
        return ""

    def _change(self):
        r = self._selected()
        if r is None:
            return
        if r.origin == "luminescent" and r.owner == MINE:
            entry = self.m.custom_for(r.combo)
            if entry is None:
                return
            dlg = _KeysDialog(self, self.m.ctx.launcher, "Edit keybind", entry.get("keys", ""), mine=entry,
                              tools=self.m.tools())
            while dlg.exec() == QDialog.DialogCode.Accepted:
                e = dlg.entry()
                err = self._check_mine(e, ignore=entry)
                if not err:
                    self.m.save_custom(entry["id"], e)
                    self._done("", f"Saved: {hk.pretty(e['keys'])} → {e['name']}")
                    return
                widgets.error(self, "Keybinds", err)
            return
        if r.origin == "luminescent":
            path = self.m.setting_path(r.owner, r.combo)
            if path is None:
                self._open()
                return
            dlg = _KeysDialog(self, self.m.ctx.launcher, f"Keys for: {self.m.owner_title(r.owner)}: {r.action}",
                              r.combo)
            if dlg.exec() != QDialog.DialogCode.Accepted:
                return
            new = dlg.combo()
            owner = self.m.hotkeys.owner_of(new) if new else ""
            if owner and hk.normalize(new) != hk.normalize(r.combo):
                widgets.error(self, "Keybinds", f"{hk.pretty(new)} is already used by {self.m.owner_title(owner)}.")
                return
            self.m.set_setting(path, new)
            self._done("", f"{r.action} is now on {hk.pretty(new)}" if new else f"{r.action} has no hotkey now")
            return
        if not r.editable:
            widgets.message(self, "Keybinds", "This bind is made in a loop (or was not found), so it can not be "
                                              "changed here. Opening the file instead.")
            self._open()
            return
        current = "+".join([{"SUPER": "windows"}.get(m, m.lower()) for m in r.mods] + [r.key.lower()])
        dlg = _KeysDialog(self, self.m.ctx.launcher, f"Keys for: {r.action} {r.detail}".strip(), current)
        if dlg.exec() != QDialog.DialogCode.Accepted:
            return
        keys = _hypr_keys(dlg.combo())
        if not keys:
            return
        clash = self._clash(keys, r.release, ignore=r)
        if clash and not widgets.confirm(self, "Keybinds", clash + "\n\nUse these keys anyway?"):
            return
        self._done(hyprbinds.change_keys(r, keys), f"{r.action} is now on {keys}")

    def _delete(self):
        r = self._selected()
        if r is None:
            return
        if r.origin == "luminescent" and r.owner == MINE:
            entry = self.m.custom_for(r.combo)
            if entry is not None and widgets.confirm(self, "Remove keybind", f"Remove {r.keys} ({entry['name']})?",
                                                     yes="Remove", danger=True):
                self.m.save_custom(entry["id"], None)
                self._done("", f"{r.keys} removed")
            return
        if r.origin == "luminescent":
            path = self.m.setting_path(r.owner, r.combo)
            if path is None:
                widgets.message(self, "Keybinds", "This hotkey is set somewhere else (a dashboard item...). "
                                                  "Opening its settings.")
                self._open()
                return
            if widgets.confirm(self, "Turn off hotkey", f"Turn off {r.keys} ({self.m.owner_title(r.owner)}: "
                                                        f"{r.action})? You can set it again later.",
                               yes="Turn off", danger=True):
                self.m.set_setting(path, "")
                self._done("", f"{r.keys} is off")
            return
        if not r.editable:
            widgets.message(self, "Keybinds", "This bind is made in a loop (or was not found); remove it in the "
                                              "file. Opening it.")
            self._open()
            return
        if not widgets.confirm(self, "Remove keybind", f"Remove {r.keys} ({r.action} {r.detail})?\n\nThe line is "
                                                       f"commented out in {os.path.basename(r.file)}, and a backup "
                                                       f"is kept.", yes="Remove", danger=True):
            return
        self._done(hyprbinds.delete(r), f"{r.keys} removed")


class KeybindsModule(Module):
    key = "keybinds"
    title = "Keybinds"
    icon = "⌨"

    def __init__(self, ctx):
        super().__init__(ctx)
        self.panel: KeybindsPanel | None = None

    def start(self):
        self.hotkey("hotkey", self.toggle, "keybinds")
        for e in self.custom():
            err = self.hotkeys.register(MINE, e.get("keys", ""), lambda e=e: self.run_custom(e), e.get("name", ""))
            if err and e.get("keys"):
                self.ctx.notify(self.title, f"{hk.pretty(e['keys'])} ({e.get('name', '')}) not bound: {err}")

    # ── your own keybinds ──────────────────────────────────────────────────
    def custom(self) -> list[dict]:
        return [e for e in self.s("custom", []) or [] if isinstance(e, dict)]

    def custom_for(self, combo: str) -> dict | None:
        combo = hk.normalize(combo)
        return next((e for e in self.custom() if hk.normalize(e.get("keys", "")) == combo), None)

    def save_custom(self, eid: str | None, entry: dict | None):
        """Add (eid None), replace or remove (entry None) one of your keybinds, then bind them again."""
        items = self.custom()
        if eid is None and entry is not None:
            items.append(dict(entry, id=uuid.uuid4().hex[:8]))
        else:
            items = [dict(entry, id=eid) if e.get("id") == eid else e for e in items if entry is not None
                     or e.get("id") != eid]
        self.set_setting("keybinds.custom", items)

    def set_setting(self, path: str, value):
        self.settings.set(path, value)
        self.settings.save()                   # every module binds its hotkeys again

    def run_custom(self, e: dict):
        does, value = e.get("does"), str(e.get("value", ""))
        launcher = self.ctx.launcher
        try:
            if does == "tool":
                launcher.run_action(value)
            elif does == "app":
                from core import runner
                err = runner.run(value)
                if err:
                    self.ctx.notify(self.title, err)
            elif does == "command":
                shell = ["cmd", "/c", value] if IS_WIN else ["sh", "-c", value]
                subprocess.Popen(shell, start_new_session=not IS_WIN, stdout=subprocess.DEVNULL,
                                 stderr=subprocess.DEVNULL)
            elif does == "keys":
                mods, key = hk._split(value)
                hypr.send_shortcut(" ".join(hk.MODIFIERS[m] for m in mods), hk.keysym(key))
            elif does == "volume":
                arg = {"up": ["set-volume", "-l", "1.5", "@DEFAULT_AUDIO_SINK@", "5%+"],
                       "down": ["set-volume", "@DEFAULT_AUDIO_SINK@", "5%-"],
                       "mute": ["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"]}.get(value)
                if arg:
                    subprocess.Popen(["wpctl", *arg], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except Exception as ex:
            self.log.exception("keybind %s failed", e.get("keys"))
            self.ctx.notify(self.title, f"{e.get('name', '')}: {ex}")

    def tools(self) -> list[tuple[str, str]]:
        launcher = self.ctx.launcher
        out = []
        for key, m in (launcher.modules.items() if launcher is not None else []):
            for aid, label, _fn in m.actions():
                out.append((f"{key}:{aid}", f"{m.title}: {label}"))
        return out

    def setting_path(self, owner: str, combo: str) -> str | None:
        """Which setting holds this Luminescent hotkey ("notes.hotkey"), or None (a dashboard item...)."""
        from core.settings import SCHEMA
        combo = hk.normalize(combo)
        sections = ["general", "ui"] if owner == "launcher" else [self.owner_section(owner)]
        if owner == MINE:
            return None
        for sec in SCHEMA:
            if sec.key not in sections:
                continue
            for f in sec.fields:
                if f.type == "hotkey" and hk.normalize(self.settings.get(f"{sec.key}.{f.key}", "") or "") == combo:
                    return f"{sec.key}.{f.key}"
        return None

    def stop(self):
        self.hotkeys.unregister_all(MINE)
        if self.panel:
            self.panel.close()
            self.panel.deleteLater()
            self.panel = None

    def apply_settings(self):
        self.hotkeys.unregister_all(self.key)
        self.hotkeys.unregister_all(MINE)
        self.start()

    def retheme(self):
        if self.panel and not self.panel.isVisible():
            self.stop()

    def actions(self):
        return [("open", "Keybinds (every hotkey on the system)", self.toggle)]

    def tray_actions(self):
        return [("Keybinds", self.toggle)]

    # ── Luminescent's own hotkeys ──────────────────────────────────────────
    def luminescent_binds(self) -> list[tuple[str, str, str]]:
        return self.hotkeys.listing()

    def owner_title(self, owner: str) -> str:
        launcher = self.ctx.launcher
        if owner in ("launcher", "dashboard", MINE):
            return {"launcher": "Luminescent", "dashboard": "Dashboard", MINE: "My keybinds"}[owner]
        m = launcher.modules.get(owner) if launcher is not None else None
        return m.title if m is not None else owner

    @staticmethod
    def owner_section(owner: str) -> str:
        return {"launcher": "general", "dashboard": "dashboard"}.get(owner, owner)

    def toggle(self):
        if self.panel is None:
            with self.themed():
                self.panel = KeybindsPanel(self)
        if self.panel.isVisible() and not self.panel.property("fading_out"):
            self.panel.fade_hide()
        else:
            self.panel.reload()
            self.panel.show_and_raise()
            QTimer.singleShot(0, self.panel.search.setFocus)
