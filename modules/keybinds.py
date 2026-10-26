"""
Keybinds: every keybind on the system in one searchable list.

Hyprland's own binds (from your config, with what they really do: "SUPER + G →
Open BROWSER: zen-browser") and Luminescent's hotkeys, with clashes marked.
Double-click a Hyprland bind to open its line in an editor, or a Luminescent
one to jump to its settings.  Hyprland binds can be re-keyed, removed (the line
is commented out) or added from here; every edit keeps a backup in
data/hypr-backups and is undone if Hyprland reports a config error.
The parsing and editing live in core/hyprbinds.py.
"""
import os

from PyQt6.QtWidgets import (
    QLabel, QLineEdit, QTreeWidget, QTreeWidgetItem, QVBoxLayout, QHBoxLayout, QComboBox, QDialog,
    QFormLayout, QAbstractItemView, QHeaderView
)
from PyQt6.QtCore import Qt, QTimer
from PyQt6.QtGui import QColor, QBrush

from core import widgets, hyprbinds, hypr
from core import hotkeys as hk
from core.module import Module

FILTERS = [("all", "All keybinds"), ("hyprland", "Hyprland (your config)"), ("luminescent", "Luminescent"),
           ("conflicts", "Clashes only")]


def _hypr_keys(combo: str) -> str:
    """'super+shift+g' -> 'SUPER + SHIFT + g' (one key plus modifiers)."""
    keys = hk.to_hyprland(combo)
    return keys[0] if keys else ""


class _KeysDialog(QDialog):
    """Ask for a key combination (and, for a new bind, the command it runs)."""

    def __init__(self, parent, launcher, title: str, combo: str = "", with_command: bool = False):
        super().__init__(parent)
        from settings_ui import HotkeyEdit
        self.setWindowTitle(title)
        self.setWindowFlags(self.windowFlags() | Qt.WindowType.WindowStaysOnTopHint)
        lay = QVBoxLayout(self)
        form = QFormLayout()
        self.keys = HotkeyEdit()
        self.keys.set_value(combo)
        if launcher is not None:
            self.keys.recording_changed.connect(launcher.hotkeys.set_suspended)
        form.addRow("Keys", self.keys)
        self.command = QLineEdit()
        self.command.setPlaceholderText("e.g. kitty, or uwsm app -- zen-browser")
        if with_command:
            form.addRow("Runs", self.command)
        lay.addLayout(form)
        hint = QLabel("Click the keys to type a combo (super+shift+g). Record can not see combos Hyprland "
                      "already uses, so type those.")
        hint.setObjectName("lumHint")
        hint.setWordWrap(True)
        lay.addWidget(hint)
        btns = QHBoxLayout()
        btns.addStretch()
        cancel = widgets.button("Cancel")
        cancel.clicked.connect(self.reject)
        ok = widgets.button("Save", "primary")
        ok.clicked.connect(self.accept)
        btns.addWidget(cancel)
        btns.addWidget(ok)
        lay.addLayout(btns)
        self.resize(520, 0)

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
        self.tree.setColumnCount(4)
        self.tree.setHeaderLabels(["Keys", "Action", "Details", "Where"])
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
        self.tree.setStyleSheet("QTreeWidget { font-size: 12px; }")
        self.tree.itemDoubleClicked.connect(lambda *_: self._open())
        lay.addWidget(self.tree, 1)

        self.count = QLabel()
        self.count.setObjectName("lumHint")
        lay.addWidget(self.count)
        btns = QHBoxLayout()
        editable = hypr.available()          # adding / changing binds edits your Hyprland config
        for text, fn, variant in (("Add bind…", self._add, "primary"), ("Change keys…", self._change, ""),
                                  ("Open in editor", self._open, ""), ("Remove…", self._delete, "danger")):
            if not editable and text != "Open in editor":
                continue
            b = widgets.button("Open its settings" if not editable else text, variant)
            b.clicked.connect(fn)
            btns.addWidget(b)
        btns.addStretch()
        lay.addLayout(btns)
        hint = QLabel("Double-click opens the line in your config (or a Luminescent hotkey's settings). Red rows "
                      "share their keys with another bind. Binds made in a loop are edited in the file."
                      if editable else
                      "Every Luminescent hotkey. Double-click one to change it in Settings; red rows share their keys.")
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
            if mode == "conflicts" and not r.conflict:
                continue
            action = r.action if r.origin == "hyprland" else f"{self.m.owner_title(r.owner)}: {r.action}"
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
            shown += 1
        self.tree.setSortingEnabled(True)
        n_conf = sum(1 for r in self._rows if r.conflict)
        self.count.setText(f"{shown} shown · {len(self._rows)} keybinds"
                           + (f" · {n_conf} clashing" if n_conf else ""))

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

    def _change(self):
        r = self._selected()
        if r is None:
            return
        if r.origin == "luminescent":
            self._open()
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
        if r.origin == "luminescent":
            widgets.message(self, "Keybinds", "Luminescent's hotkeys are turned off in its settings (clear the "
                                              "hotkey there).")
            self._open()
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

    def stop(self):
        if self.panel:
            self.panel.close()
            self.panel.deleteLater()
            self.panel = None

    def apply_settings(self):
        self.hotkeys.unregister_all(self.key)
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
        if owner in ("launcher", "dashboard"):
            return {"launcher": "Luminescent", "dashboard": "Dashboard"}[owner]
        m = launcher.modules.get(owner) if launcher is not None else None
        return m.title if m is not None else owner

    @staticmethod
    def owner_section(owner: str) -> str:
        return {"launcher": "general", "dashboard": "dashboard"}.get(owner, owner)

    def toggle(self):
        if self.panel is None:
            with self.themed():
                self.panel = KeybindsPanel(self)
        if self.panel.isVisible():
            self.panel.hide()
        else:
            self.panel.reload()
            self.panel.show_and_raise()
            QTimer.singleShot(0, self.panel.search.setFocus)
