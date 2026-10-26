"""
Settings page (shown inside the Luminescent overlay). Pages are generated from
core.settings.SCHEMA, so adding a Field there is all it takes to get it into the UI.

Layout: a profile strip on top, a sidebar (search + sections with their field
counts), rows of "label | field" with a hairline between them, and a footer
(Reset page · unsaved-changes count · Undo changes · Save & Apply).
"""
import os
import copy
import logging

from PyQt6.QtWidgets import (
    QWidget, QLabel, QVBoxLayout, QHBoxLayout, QFrame, QStackedWidget, QButtonGroup,
    QScrollArea, QLineEdit, QSpinBox, QDoubleSpinBox, QCheckBox, QComboBox, QPushButton, QSlider,
    QColorDialog, QFileDialog
)
from PyQt6.QtCore import Qt, QTimer, QEvent, pyqtSignal
from PyQt6.QtGui import QColor, QKeySequence, QShortcut

from core import theme, widgets, backup, APP_NAME
from core import hotkeys as hk
from core.settings import (SCHEMA, Field, defaults, NAV_SECTIONS, ALL_PAGES, REQUIRED_PAGES, START_PANELS, TRAY_ITEMS,
                           SETTINGS_FILE)

log = logging.getLogger("settings_ui")

LABEL_W = 220


# ── field widgets ─────────────────────────────────────────────────────────────
_QT_KEYS = {
    Qt.Key.Key_Escape: "esc", Qt.Key.Key_Tab: "tab", Qt.Key.Key_Backspace: "backspace",
    Qt.Key.Key_Return: "enter", Qt.Key.Key_Enter: "enter", Qt.Key.Key_Insert: "insert",
    Qt.Key.Key_Delete: "delete", Qt.Key.Key_Home: "home", Qt.Key.Key_End: "end",
    Qt.Key.Key_PageUp: "page up", Qt.Key.Key_PageDown: "page down", Qt.Key.Key_Left: "left",
    Qt.Key.Key_Right: "right", Qt.Key.Key_Up: "up", Qt.Key.Key_Down: "down", Qt.Key.Key_Space: "space",
    Qt.Key.Key_Print: "print screen", Qt.Key.Key_ScrollLock: "scroll lock", Qt.Key.Key_Pause: "pause",
    Qt.Key.Key_CapsLock: "caps lock", Qt.Key.Key_NumLock: "num lock", Qt.Key.Key_Menu: "menu",
}
_MODIFIER_KEYS = {Qt.Key.Key_Control: "ctrl", Qt.Key.Key_Alt: "alt", Qt.Key.Key_Shift: "shift",
                  Qt.Key.Key_Meta: "windows", Qt.Key.Key_AltGr: "alt gr"}
_REC_RED = "#E8645E"


class HotkeyEdit(QWidget):
    """The combo shown as key caps (Ctrl + Shift + Space). Click the caps to type one, or Record and press it."""
    recording_changed = pyqtSignal(bool)

    def __init__(self):
        super().__init__()
        lay = QHBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(8)
        self.box = QFrame()
        self.box.setObjectName("keyBox")
        self.box.setFixedHeight(32)
        self.box.setCursor(Qt.CursorShape.IBeamCursor)
        self.box.setToolTip("Click to type a combo, e.g. ctrl+shift+m")
        bl = QHBoxLayout(self.box)
        bl.setContentsMargins(8, 0, 8, 0)
        bl.setSpacing(4)
        self.caps = QWidget()
        self.caps_lay = QHBoxLayout(self.caps)
        self.caps_lay.setContentsMargins(0, 0, 0, 0)
        self.caps_lay.setSpacing(4)
        self.caps.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        bl.addWidget(self.caps)
        self.edit = QLineEdit()
        self.edit.setProperty("mono", True)
        self.edit.setPlaceholderText("e.g. ctrl+shift+m  (empty = off)")
        self.edit.setStyleSheet("QLineEdit { border: none; background: transparent; padding: 0; }")
        self.edit.setMinimumWidth(200)
        self.edit.hide()
        self.edit.textChanged.connect(self._validate)
        self.edit.installEventFilter(self)
        self.edit.returnPressed.connect(self._stop_typing)
        bl.addWidget(self.edit)
        lay.addWidget(self.box)

        self.record = QPushButton()
        self.record.setProperty("variant", "ghost")
        self.record.setCheckable(True)
        self.record.setFixedHeight(32)
        self.record.setCursor(Qt.CursorShape.PointingHandCursor)
        rl = QHBoxLayout(self.record)
        rl.setContentsMargins(12, 0, 12, 0)
        rl.setSpacing(7)
        dot = QFrame()
        dot.setFixedSize(7, 7)
        dot.setStyleSheet(f"background: {_REC_RED}; border-radius: 3px;")
        self._rec_text = widgets.label("Record", 13, 500, tone="secondary")
        for w in (dot, self._rec_text):
            w.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
            rl.addWidget(w)
        self.record.setFixedWidth(rl.sizeHint().width() + 8)
        self._rec_layout = rl
        self.record.toggled.connect(self._toggle_record)
        lay.addWidget(self.record)
        clear = widgets.icon_button("✕", "Clear (no hotkey)", 26, 11, ghost=True)
        clear.clicked.connect(lambda: self.set_value(""))
        lay.addWidget(clear)
        self.warning = widgets.label("", 12, tone="danger")
        self.warning.hide()
        lay.addWidget(self.warning)
        lay.addStretch()
        self._held: list[str] = []
        self._had_key = False
        self.conflict_fn = None          # combo -> "" or what else uses it (checked as you type / record)
        self._show_caps()

    def value(self) -> str:
        return hk.normalize(self.edit.text())

    def set_value(self, v: str):
        self.edit.setText(hk.normalize(v or ""))
        self._show_caps()

    def focus_edit(self):
        self.caps.hide()
        self.edit.show()
        self.edit.setFocus()
        self.edit.selectAll()

    def _stop_typing(self):
        if self.edit.isVisible() and not self.record.isChecked():
            self._show_caps()

    def _show_caps(self):
        widgets.clear_layout(self.caps_lay)
        text = self.edit.text().strip()
        if text:
            parts = [p for p in hk.pretty(text).split("+") if p] or [text]
            for i, part in enumerate(parts):
                if i:
                    self.caps_lay.addWidget(widgets.label("+", 11, tone="muted"))
                cap = widgets.label(part, 11, 500, mono=True, tone="")
                cap.setObjectName("keyCap")
                self.caps_lay.addWidget(cap)
        else:
            self.caps_lay.addWidget(widgets.label("Not set", 12, tone="muted"))
        self.edit.hide()
        self.caps.show()

    def mousePressEvent(self, e):
        if self.box.geometry().contains(e.position().toPoint()) and not self.record.isChecked():
            self.focus_edit()
            return
        super().mousePressEvent(e)

    def eventFilter(self, obj, e):
        if obj is self.edit and e.type() == QEvent.Type.FocusOut and not self.record.isChecked():
            QTimer.singleShot(0, self._stop_typing)
        return super().eventFilter(obj, e)

    def _validate(self):
        err = hk.validate(self.edit.text())
        clash = ""
        if not err and callable(self.conflict_fn):
            try:
                clash = self.conflict_fn(hk.normalize(self.edit.text())) or ""
            except Exception:
                log.exception("conflict check failed")
        self.edit.setToolTip(err or clash)
        self.box.setStyleSheet(f"#keyBox {{ border-color: {_REC_RED}; }}" if err or clash else "")
        self.warning.setText(("⚠ " + (err or clash)) if err or clash else "")
        self.warning.setVisible(bool(err or clash))
        if not self.edit.isVisible():
            self._show_caps()

    def recheck(self):
        self._validate()

    def _toggle_record(self, on: bool):
        self.recording_changed.emit(on)
        if on:
            self._rec_text.setText("Press keys…")
            self.record.setFixedWidth(self._rec_layout.sizeHint().width() + 8)
            self._held, self._had_key = [], False
            self.setFocus()
            self.grabKeyboard()
        else:
            self._rec_text.setText("Record")
            self.record.setFixedWidth(self._rec_layout.sizeHint().width() + 8)
            self.releaseKeyboard()
            self._show_caps()

    def keyPressEvent(self, e):
        if not self.record.isChecked():
            return super().keyPressEvent(e)
        if e.isAutoRepeat():
            return
        key = e.key()
        mods = []
        m = e.modifiers()
        if m & Qt.KeyboardModifier.ControlModifier:
            mods.append("ctrl")
        if m & Qt.KeyboardModifier.AltModifier:
            mods.append("alt")
        if m & Qt.KeyboardModifier.ShiftModifier:
            mods.append("shift")
        if m & Qt.KeyboardModifier.MetaModifier:
            mods.append("windows")
        if key in _MODIFIER_KEYS:
            name = _MODIFIER_KEYS[key]
            if name not in mods:
                mods.append(name)
            self._held = mods
            self.edit.setText("+".join(mods))     # tentative; committed on release
            self._show_caps()
            return
        name = self._key_name(key, e)
        if not name:
            return
        self._had_key = True
        self.edit.setText("+".join(mods + [name]))
        self.record.setChecked(False)

    def keyReleaseEvent(self, e):
        if not self.record.isChecked() or e.isAutoRepeat():
            return super().keyReleaseEvent(e)
        if e.key() in _MODIFIER_KEYS and not self._had_key:
            # modifier-only hotkey such as a bare "alt"
            self.record.setChecked(False)

    @staticmethod
    def _key_name(key, e) -> str:
        if key in _QT_KEYS:
            return _QT_KEYS[key]
        if Qt.Key.Key_F1 <= key <= Qt.Key.Key_F35:
            return f"f{key - Qt.Key.Key_F1 + 1}"
        if Qt.Key.Key_A <= key <= Qt.Key.Key_Z:
            return chr(key).lower()
        if Qt.Key.Key_0 <= key <= Qt.Key.Key_9:
            return chr(key)
        text = QKeySequence(key).toString().lower()
        if text and hk.validate(text) == "":
            return text
        return ""

    def hideEvent(self, e):
        if self.record.isChecked():
            self.record.setChecked(False)
        super().hideEvent(e)


class ColorButton(QPushButton):
    """A framed swatch with its hex code."""

    def __init__(self):
        super().__init__()
        self._value = "#000000"
        self.setFixedHeight(32)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.setStyleSheet(f"QPushButton {{ background: {theme.P.bg_frame}; padding: 0; }}"
                           f"QPushButton:hover {{ border-color: {theme.P.text_muted}; }}")
        lay = QHBoxLayout(self)
        lay.setContentsMargins(6, 0, 10, 0)
        lay.setSpacing(8)
        self._chip = QFrame()
        self._chip.setFixedSize(20, 20)
        self._hex = widgets.label("", 12, mono=True)
        for w in (self._chip, self._hex):
            w.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
            lay.addWidget(w)
        self.setFixedWidth(130)
        self.clicked.connect(self._pick)

    def value(self) -> str:
        return self._value

    def set_value(self, v: str):
        self._value = v or "#000000"
        self._chip.setStyleSheet(f"background: {self._value}; border-radius: 4px;")
        self._hex.setText(self._value.upper())

    def _pick(self):
        c = QColorDialog.getColor(QColor(self._value), self, "Pick a colour")
        if c.isValid():
            self.set_value(c.name())


class PathEdit(QWidget):
    def __init__(self, kind: str, file_filter: str = ""):
        super().__init__()
        self.kind, self.filter = kind, file_filter
        self.setMaximumWidth(560)
        lay = QHBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(8)
        self.edit = QLineEdit()
        self.edit.setProperty("mono", True)
        self.edit.setFixedHeight(32)
        b = QPushButton("Browse…")
        b.setFixedHeight(32)
        b.setCursor(Qt.CursorShape.PointingHandCursor)
        b.clicked.connect(self._browse)
        lay.addWidget(self.edit, 1)
        lay.addWidget(b)

    def value(self) -> str:
        return self.edit.text().strip()

    def set_value(self, v):
        self.edit.setText(str(v or ""))

    def _browse(self):
        if self.kind == "dir":
            p = QFileDialog.getExistingDirectory(self, "Choose a folder", self.edit.text())
        else:
            p, _ = QFileDialog.getOpenFileName(self, "Choose a file", self.edit.text(), self.filter or "All files (*)")
        if p:
            self.edit.setText(p)


class OpacitySlider(QWidget):
    def __init__(self):
        super().__init__()
        self.setMaximumWidth(420)
        lay = QHBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(14)
        self.slider = QSlider(Qt.Orientation.Horizontal)
        self.slider.setRange(0, 100)
        self.label = widgets.label("0%", 12, mono=True)
        self.label.setFixedWidth(40)
        self.label.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
        self.slider.valueChanged.connect(lambda v: self.label.setText(f"{v}%"))
        lay.addWidget(self.slider, 1)
        lay.addWidget(self.label)

    def value(self) -> float:
        return self.slider.value() / 100.0

    def set_value(self, v):
        self.slider.setValue(int(round(float(v or 0) * 100)))


class NavOrderEditor(QWidget):
    """The overlay's page switcher: tick the pages you want, put them in order with ↑ ↓.
    Dashboard and Settings are always there."""

    def __init__(self):
        super().__init__()
        self.setMaximumWidth(360)
        self._keys: list[str] = []
        self._on: set[str] = set()
        self.lay = QVBoxLayout(self)
        self.lay.setContentsMargins(0, 0, 0, 0)
        self.lay.setSpacing(4)

    def _rebuild(self):
        widgets.clear_layout(self.lay)
        names = dict(ALL_PAGES)
        n = 0
        for i, k in enumerate(self._keys):
            on = k in self._on
            row = QFrame()
            row.setObjectName("navRow")
            row.setFixedHeight(32)
            rl = QHBoxLayout(row)
            rl.setContentsMargins(10, 0, 4, 0)
            rl.setSpacing(10)
            cb = QCheckBox()
            cb.setChecked(on)
            cb.setEnabled(k not in REQUIRED_PAGES)
            cb.setToolTip("Always shown" if k in REQUIRED_PAGES else "Show this page in the bar")
            cb.toggled.connect(lambda v, key=k: self._toggle(key, v))
            rl.addWidget(cb)
            if on:
                n += 1
            rl.addWidget(widgets.label(f"{n:02d}" if on else "··", 10, mono=True, tone="muted"))
            rl.addWidget(widgets.label(names[k], 13, tone="primary" if on else "muted"), 1)
            if on and n <= 9:
                rl.addWidget(widgets.label(f"Ctrl+{n}", 10, mono=True, tone="muted"))
            for txt, d, ok in (("↑", -1, i > 0), ("↓", 1, i < len(self._keys) - 1)):
                b = widgets.icon_button(txt, "Move up" if d < 0 else "Move down", 22, 10, ghost=True)
                b.setEnabled(ok)
                b.clicked.connect(lambda _, j=i, dd=d: self._move(j, dd))
                rl.addWidget(b)
            self.lay.addWidget(row)

    def _toggle(self, key: str, on: bool):
        (self._on.add if on else self._on.discard)(key)
        QTimer.singleShot(0, self._rebuild)

    def _move(self, i: int, d: int):
        j = i + d
        if 0 <= j < len(self._keys):
            self._keys[i], self._keys[j] = self._keys[j], self._keys[i]
            self._rebuild()

    def set_value(self, order):
        names = dict(ALL_PAGES)
        keys = list(dict.fromkeys(k for k in (order or []) if k in names))
        self._on = set(keys) | set(REQUIRED_PAGES)
        self._keys = keys + [k for k, _ in ALL_PAGES if k not in keys]
        self._rebuild()

    def get_value(self):
        return [k for k in self._keys if k in self._on]


class CheckListEditor(QWidget):
    """A column of check boxes for a list setting (General → Show on start, Tray menu)."""

    def __init__(self, choices: list[tuple[str, str]], columns: int = 1):
        super().__init__()
        from PyQt6.QtWidgets import QGridLayout
        self._boxes: dict[str, QCheckBox] = {}
        grid = QGridLayout(self)
        grid.setContentsMargins(0, 4, 0, 0)
        grid.setHorizontalSpacing(24)
        grid.setVerticalSpacing(8)
        rows = (len(choices) + columns - 1) // columns
        for i, (key, label) in enumerate(choices):
            cb = QCheckBox(label)
            self._boxes[key] = cb
            grid.addWidget(cb, i % rows, i // rows)
        grid.setColumnStretch(columns, 1)

    def set_value(self, v):
        chosen = set(v or [])
        for k, cb in self._boxes.items():
            cb.setChecked(k in chosen)

    def get_value(self):
        return [k for k, cb in self._boxes.items() if cb.isChecked()]


class BackupWidget(QWidget):
    def __init__(self, launcher):
        super().__init__()
        self.launcher = launcher
        lay = QHBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(8)
        ex = widgets.button("Export backup…")
        ex.clicked.connect(self._export)
        im = widgets.button("Import backup…")
        im.clicked.connect(self._import)
        lay.addWidget(ex)
        lay.addWidget(im)
        hint = widgets.label("Settings, notes, timers, reminders, hearing history, dashboard and themes in one zip. "
                             "The vault is never included.", 12, tone="muted")
        hint.setWordWrap(True)
        lay.addWidget(hint, 1)

    def _export(self):
        from datetime import date
        default = os.path.join(os.path.expanduser("~"), "Documents", f"{APP_NAME}-backup-{date.today()}.zip")
        path, _ = QFileDialog.getSaveFileName(self, "Export backup", default, "Zip (*.zip)")
        if not path:
            return
        try:
            n = backup.export(path)
            widgets.message(self, "Backup", f"Saved {n} files to\n{path}")
        except Exception as e:
            widgets.error(self, "Backup", f"Could not write the backup:\n{e}")

    def _import(self):
        path, _ = QFileDialog.getOpenFileName(self, "Import backup", os.path.expanduser("~"), "Zip (*.zip)")
        if not path:
            return
        try:
            names = backup.check(path)
        except Exception as e:
            widgets.error(self, "Backup", str(e))
            return
        if not widgets.confirm(self, "Import backup",
                               f"Replace your current setup with the {len(names)} files in this backup?\n\n"
                               f"Your current files are zipped to data/backups first. Luminescent restarts afterwards.",
                               yes="Import and restart", danger=True):
            return
        try:
            auto = backup.restore(path)
        except Exception as e:
            widgets.error(self, "Backup", f"Import failed:\n{e}")
            return
        log.info("backup imported from %s (previous files in %s)", path, auto)
        self.launcher.restart()


class QuitWidget(QWidget):
    def __init__(self, launcher):
        super().__init__()
        lay = QHBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(8)
        q = widgets.button("Quit everything", "danger")
        q.clicked.connect(launcher.confirm_quit)
        r = widgets.button("Restart")
        r.clicked.connect(launcher.restart)
        lay.addWidget(q)
        lay.addWidget(r)
        hint = widgets.label("The ✕ on the overlay only hides it. This closes the background process and every "
                             "module.", 12, tone="muted")
        hint.setWordWrap(True)
        lay.addWidget(hint, 1)


def _short_path(path: str) -> str:
    home = os.path.expanduser("~")
    if path == home or path.startswith(home + os.sep):
        return "~" + path[len(home):]
    return path


class ProfileBar(QFrame):
    """Pick / create / rename / delete settings profiles."""

    def __init__(self, page: "SettingsPage"):
        super().__init__()
        self.page = page
        self.settings = page.settings
        self.setObjectName("setProfileBar")
        self.setFixedHeight(46)
        lay = QHBoxLayout(self)
        lay.setContentsMargins(20, 0, 20, 0)
        lay.setSpacing(10)
        lay.addWidget(widgets.label("PROFILE", 10, 500, mono=True, spacing=0.12, tone="muted"))
        self.combo = QComboBox()
        self.combo.setMinimumWidth(150)
        self.combo.setFixedHeight(28)
        self.combo.setToolTip("Themes can pick a profile (Themes → right-click → Settings…). General, Overlay and "
                              "Dashboard settings are shared by every profile.")
        self.combo.activated.connect(self._switch)
        lay.addWidget(self.combo)
        new = widgets.button("New profile", "ghost")
        new.clicked.connect(self._new)
        ren = widgets.button("Rename", "ghost")
        ren.clicked.connect(self._rename)
        self.delete = widgets.button("Delete", "ghost")
        self.delete.clicked.connect(self._delete)
        for b in (new, ren, self.delete):
            b.setFixedHeight(28)
            lay.addWidget(b)
        lay.addStretch()
        self.path = widgets.label("", 10.5, mono=True, tone="muted")
        lay.addWidget(self.path)
        self.refresh()

    def refresh(self):
        self.combo.blockSignals(True)
        self.combo.clear()
        for p in self.settings.profiles():
            self.combo.addItem(p["name"], p["id"])
        self.combo.setCurrentIndex(max(0, self.combo.findData(self.settings.active_profile())))
        self.combo.blockSignals(False)
        self.delete.setEnabled(self.settings.active_profile() != "default")
        full = _short_path(getattr(self.settings, "path", "") or SETTINGS_FILE)
        self.path.setToolTip(full)
        self.path.setText(self.path.fontMetrics().elidedText(full, Qt.TextElideMode.ElideMiddle, 380))

    def _unsaved_ok(self) -> bool:
        if not self.page.is_dirty():
            return True
        return widgets.confirm(self, "Unsaved changes", "Discard the changes you have not saved yet?", yes="Discard")

    def _switch(self, _idx):
        pid = self.combo.currentData()
        if pid == self.settings.active_profile():
            return
        if not self._unsaved_ok():
            self.refresh()
            return
        self.settings.switch_profile(pid)
        self.page.load_values()
        self.refresh()

    def _new(self):
        name, ok = widgets.ask_text(self, "New settings profile",
                                    "Name (it starts as a copy of the current settings):")
        if ok and name.strip():
            if not self._unsaved_ok():
                return
            pid = self.settings.create_profile(name)
            self.settings.switch_profile(pid)
            self.page.load_values()
            self.refresh()

    def _rename(self):
        pid = self.settings.active_profile()
        name, ok = widgets.ask_text(self, "Rename profile", "New name:", self.settings.profile_name(pid))
        if ok and name.strip():
            self.settings.rename_profile(pid, name)
            self.refresh()

    def _delete(self):
        pid = self.settings.active_profile()
        if pid == "default":
            return
        if widgets.confirm(self, "Delete profile", f'Delete the settings profile "{self.settings.profile_name(pid)}"?'
                                                   f"\nYou go back to the Default profile.", yes="Delete", danger=True):
            self.settings.delete_profile(pid)
            self.page.load_values()
            self.refresh()


class _SideButton(QPushButton):
    """A sidebar section: name on the left, its field count in mono on the right."""

    def __init__(self, title: str, count: int):
        super().__init__()
        self.setProperty("side", True)
        self.setCheckable(True)
        self.setFixedHeight(32)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        lay = QHBoxLayout(self)
        lay.setContentsMargins(12, 0, 12, 0)
        self.name = widgets.label(title, 13, 500, tone="")
        n = widgets.label(str(count), 10, mono=True, tone="muted")
        for w in (self.name, n):
            w.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        lay.addWidget(self.name, 1)
        lay.addWidget(n)
        self.toggled.connect(self._tone)
        self._tone(False)

    def _tone(self, on: bool):
        self.name.setProperty("tone", "primary" if on else "secondary")
        self.name.style().unpolish(self.name)
        self.name.style().polish(self.name)


# ── the page ──────────────────────────────────────────────────────────────────
class SettingsPage(QWidget):
    """Edits a copy of the settings; nothing is written until Save & Apply. Lives inside the overlay."""

    def __init__(self, settings, launcher):
        super().__init__()
        self.settings = settings
        self.launcher = launcher
        self._widgets: dict[str, tuple[Field, object]] = {}   # "section.key" -> (field, widget)
        self._pages: dict[str, int] = {}
        self._loaded: dict = {}
        self._stale = False
        self._note = ""
        settings.changed.connect(self._mark_stale)
        settings.quietly_changed.connect(self._quiet_change)
        settings.profile_changed.connect(lambda _pid: self._mark_stale())

        root = QVBoxLayout(self)
        root.setContentsMargins(0, 0, 0, 0)
        root.setSpacing(0)
        self.profiles = ProfileBar(self)
        root.addWidget(self.profiles)

        main = QHBoxLayout()
        main.setSpacing(0)
        side = QFrame()
        side.setObjectName("setSidebar")
        side.setFixedWidth(232)
        sl = QVBoxLayout(side)
        sl.setContentsMargins(10, 14, 10, 14)
        sl.setSpacing(2)
        search_box = QFrame()
        sbl = QHBoxLayout(search_box)
        sbl.setContentsMargins(0, 0, 0, 10)
        self.search = QLineEdit()
        self.search.setPlaceholderText("Search settings")
        self.search.setFixedHeight(32)
        self.search.setStyleSheet(f"QLineEdit {{ background: {theme.P.bg_base}; padding-right: 30px; }}")
        self.search.textChanged.connect(self._filter)
        sbl.addWidget(self.search)
        slash = widgets.label("/", 10, mono=True, tone="muted")
        slash.setObjectName("keyCap")
        slash.setParent(self.search)
        slash.adjustSize()
        self._slash = slash
        sl.addWidget(search_box)
        QShortcut(QKeySequence("/"), self, activated=lambda: (self.search.setFocus(), self.search.selectAll()),
                  context=Qt.ShortcutContext.WidgetWithChildrenShortcut)

        self.side_group = QButtonGroup(self)
        self.side_group.setExclusive(True)
        self.side_buttons: list[_SideButton] = []
        side_scroll = QScrollArea()
        side_scroll.setWidgetResizable(True)
        side_scroll.setFrameShape(QFrame.Shape.NoFrame)
        side_scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        side_holder = QWidget()
        shl = QVBoxLayout(side_holder)
        shl.setContentsMargins(0, 0, 0, 0)
        shl.setSpacing(2)
        side_scroll.setWidget(side_holder)
        sl.addWidget(side_scroll, 1)
        main.addWidget(side)

        self.stack = QStackedWidget()
        main.addWidget(self.stack, 1)
        root.addLayout(main, 1)

        for i, section in enumerate(SCHEMA):
            b = _SideButton(section.title, sum(1 for f in section.fields if f.type != "widget"))
            b.clicked.connect(lambda _, k=i: self._select(k))
            self.side_group.addButton(b)
            shl.addWidget(b)
            self.side_buttons.append(b)
            self._pages[section.key] = self.stack.count()
            self.stack.addWidget(self._build_page(section))
        shl.addStretch()

        bar = QFrame()
        bar.setObjectName("setFooter")
        bar.setFixedHeight(56)
        bl = QHBoxLayout(bar)
        bl.setContentsMargins(20, 0, 20, 0)
        bl.setSpacing(10)
        reset = widgets.button("Reset page", "danger")
        reset.setFixedHeight(32)
        reset.setToolTip("Put this page back to its defaults (not saved until Save & Apply)")
        reset.clicked.connect(self._reset_page)
        bl.addWidget(reset)
        bl.addStretch()
        self._dot = QFrame()
        self._dot.setFixedSize(6, 6)
        self._dot.setStyleSheet(f"background: {theme.P.primary}; border-radius: 3px;")
        bl.addWidget(self._dot)
        self.status = widgets.label("", 11, mono=True, spacing=0.04, tone="muted")
        bl.addWidget(self.status)
        bl.addStretch()
        revert = widgets.button("Undo changes", "outline")
        revert.setFixedHeight(32)
        revert.clicked.connect(lambda: (self.load_values(), self._set_note("CHANGES UNDONE")))
        save = widgets.button("Save && Apply", "primary")
        save.setFixedHeight(32)
        save.clicked.connect(self.save)
        bl.addWidget(revert)
        bl.addWidget(save)
        root.addWidget(bar)

        # the unsaved-changes count: polled only while the page is on screen
        self._dirty_timer = QTimer(self)
        self._dirty_timer.setInterval(600)
        self._dirty_timer.timeout.connect(self._update_status)

        self.load_values()
        self._select(0)

    def resizeEvent(self, e):
        super().resizeEvent(e)
        s = self.search
        self._slash.move(s.width() - self._slash.width() - 8, (s.height() - self._slash.height()) // 2)

    def teardown(self):
        self._dirty_timer.stop()
        self.launcher.hotkeys.set_suspended(False)
        for sig, slot in ((self.settings.changed, self._mark_stale), (self.settings.quietly_changed, self._quiet_change)):
            try:
                sig.disconnect(slot)
            except TypeError:
                pass

    def _mark_stale(self):
        self._stale = True

    def _quiet_change(self):
        """A module stored a value itself. Take it over now if nothing is being edited, else on the next show."""
        if self.isVisible() and not self.is_dirty():
            self.load_values()
        else:
            self._stale = True

    def _select(self, i: int):
        if 0 <= i < len(self.side_buttons):
            self.side_buttons[i].setChecked(True)
            self.stack.setCurrentIndex(i)

    # ── page building ──────────────────────────────────────────────────────
    def _build_page(self, section) -> QWidget:
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.Shape.NoFrame)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        page = QWidget()
        lay = QVBoxLayout(page)
        lay.setContentsMargins(36, 26, 36, 26)
        lay.setSpacing(0)

        head = QHBoxLayout()
        head.setSpacing(12)
        title = widgets.label(section.title, 20, 600)
        head.addWidget(title, 0, Qt.AlignmentFlag.AlignBottom)
        if section.description:
            d = widgets.label(section.description, 13, tone="muted")
            d.setWordWrap(True)
            head.addWidget(d, 1, Qt.AlignmentFlag.AlignBottom)
        else:
            head.addStretch()
        lay.addLayout(head)
        lay.addSpacing(14)
        if self.launcher.module_errors.get(section.key):
            e = widgets.label(f"⚠ This module could not be loaded: {self.launcher.module_errors[section.key]}",
                              13, tone="danger")
            e.setWordWrap(True)
            lay.addWidget(e)
            lay.addSpacing(8)

        rows = []
        for f in section.fields:
            w = self._make_widget(section.key, f)
            if f.type != "widget":
                self._widgets[f"{section.key}.{f.key}"] = (f, w)
            rows.append(self._row(f, w))
        for i, row in enumerate(rows):
            if i == len(rows) - 1:
                row.setObjectName("")       # no hairline under the last row
            lay.addWidget(row)
        lay.addStretch()
        scroll.setWidget(page)
        return scroll

    def _row(self, f: Field, w) -> QFrame:
        row = QFrame()
        row.setObjectName("setRow")
        tall = f.type in ("custom",) or (f.type == "bool" and f.help)
        rl = QHBoxLayout(row)
        rl.setContentsMargins(0, 14 if tall else 10, 0, 14 if tall else 10)
        rl.setSpacing(0)
        row.setMinimumHeight(52)
        lbl = widgets.label(f.label + ("  ⟳" if f.restart else ""), 13, tone="secondary")
        if f.restart:
            lbl.setToolTip("Needs a restart to fully apply")
        lbl.setFixedWidth(LABEL_W)
        lbl.setWordWrap(True)
        lbl.setContentsMargins(0, 0, 16, 0)
        top = f.type == "custom" or bool(f.help and f.type != "bool")
        rl.addWidget(lbl, 0, Qt.AlignmentFlag.AlignLeft |
                     (Qt.AlignmentFlag.AlignTop if top else Qt.AlignmentFlag.AlignVCenter))
        if top:
            lbl.setContentsMargins(0, 8, 16, 0)
        col = QVBoxLayout()
        col.setSpacing(5)
        if f.type == "bool":
            line = QHBoxLayout()
            line.setSpacing(9)
            line.addWidget(w, 0, Qt.AlignmentFlag.AlignTop)
            if f.help:
                h = widgets.label(f.help, 12, tone="muted")
                h.setWordWrap(True)
                h.mousePressEvent = lambda e, cb=w: cb.toggle()
                h.setCursor(Qt.CursorShape.PointingHandCursor)
                line.addWidget(h, 1)
            else:
                line.addStretch()
            col.addLayout(line)
        else:
            line = QHBoxLayout()           # fields keep their own (max) width; the rest stays empty
            line.setSpacing(0)
            line.addWidget(w, 1)
            line.addStretch()
            col.addLayout(line)
            if f.help:
                col.addWidget(self._help(f.help))
        rl.addLayout(col, 1)
        return row

    def _help(self, text: str) -> QLabel:
        h = widgets.label(text, 12, tone="muted")
        h.setWordWrap(True)
        return h

    def _make_widget(self, section_key: str, f: Field):
        t = f.type
        if t == "hotkey":
            w = HotkeyEdit()
            w.recording_changed.connect(self.launcher.hotkeys.set_suspended)
            w.conflict_fn = lambda combo, full=f"{section_key}.{f.key}": self._live_conflict(full, combo)
            return w
        if t == "text":
            w = QLineEdit()
            w.setFixedHeight(32)
            w.setMaximumWidth(520)
            return w
        if t in ("dir", "file"):
            return PathEdit(t, f.file_filter)
        if t == "int":
            w = QSpinBox()
            w.setRange(int(f.min), int(f.max))
            w.setSingleStep(int(f.step or 1))
            w.setFixedSize(120, 32)
            return w
        if t == "float":
            w = QDoubleSpinBox()
            w.setRange(float(f.min), float(f.max))
            w.setSingleStep(float(f.step or 0.1))
            w.setFixedSize(120, 32)
            return w
        if t == "opacity":
            return OpacitySlider()
        if t == "bool":
            return QCheckBox()
        if t == "color":
            return ColorButton()
        if t == "choice":
            w = QComboBox()
            for v, label in f.choices:
                w.addItem(label, v)
            w.setMinimumWidth(220)
            w.setFixedHeight(32)
            return w
        full = f"{section_key}.{f.key}"
        if t == "widget":
            if full == "general.backup":
                return BackupWidget(self.launcher)
            if full == "general.quit":
                return QuitWidget(self.launcher)
        if t == "custom":
            if full == "ui.nav_order":
                return NavOrderEditor()
            if full == "general.start_panels":
                return CheckListEditor(START_PANELS, 2)
            if full == "general.tray_items":
                return CheckListEditor(TRAY_ITEMS, 2)
            if full == "reminders.items":
                try:
                    from modules.reminders import RemindersEditor
                    return RemindersEditor(preview_fn=self.launcher.preview_reminder,
                                           themes=lambda: [(t["id"], t["name"]) for t in self.launcher.themes.themes()])
                except Exception as e:
                    log.exception("reminders editor unavailable")
                    return QLabel(f"Reminders editor unavailable: {e}")
        return QLabel(f"(unsupported field type {t})")

    def _filter(self, text: str):
        q = text.strip().lower()
        first = -1
        for i, section in enumerate(SCHEMA):
            hay = " ".join([section.title, section.description] + [f.label + " " + f.help for f in section.fields]).lower()
            hide = bool(q) and q not in hay
            self.side_buttons[i].setHidden(hide)
            if not hide and first < 0:
                first = i
        cur = self.stack.currentIndex()
        if q and (cur < 0 or self.side_buttons[cur].isHidden()) and first >= 0:
            self._select(first)

    # ── values ─────────────────────────────────────────────────────────────
    @staticmethod
    def _get(f: Field, w):
        t = f.type
        if t == "hotkey":
            return w.value()
        if t == "text":
            return w.text().strip()
        if t in ("dir", "file", "opacity", "color"):
            return w.value()
        if t in ("int", "float"):
            return w.value()
        if t == "bool":
            return w.isChecked()
        if t == "choice":
            return w.currentData()
        if t == "custom" and hasattr(w, "get_value"):
            return w.get_value()
        return f.default

    @staticmethod
    def _set(f: Field, w, v):
        t = f.type
        if t in ("hotkey", "dir", "file", "opacity", "color"):
            w.set_value(v)
        elif t == "text":
            w.setText(str(v or ""))
        elif t == "int":
            w.setValue(int(v))
        elif t == "float":
            w.setValue(float(v))
        elif t == "bool":
            w.setChecked(bool(v))
        elif t == "choice":
            i = w.findData(v)
            w.setCurrentIndex(i if i >= 0 else 0)
        elif t == "custom" and hasattr(w, "set_value"):
            w.set_value(copy.deepcopy(v))

    def load_values(self):
        data = self.settings.all()
        for full, (f, w) in self._widgets.items():
            sec, key = full.split(".", 1)
            try:
                self._set(f, w, data.get(sec, {}).get(key, f.default))
            except Exception:
                log.exception("could not load %s", full)
                self._set(f, w, f.default)
        for f, w in self._widgets.values():
            if f.type == "hotkey":
                w.recheck()
        self._loaded = self.collect()
        self._stale = False
        self.profiles.refresh()
        self._update_status()

    def collect(self) -> dict:
        data = self.settings.all()
        for full, (f, w) in self._widgets.items():
            sec, key = full.split(".", 1)
            data.setdefault(sec, {})[key] = self._get(f, w)
        return data

    def changed_count(self) -> int:
        now, was = self.collect(), self._loaded
        return sum(1 for full in self._widgets
                   for sec, key in [full.split(".", 1)]
                   if now.get(sec, {}).get(key) != was.get(sec, {}).get(key))

    def is_dirty(self) -> bool:
        return self.collect() != self._loaded

    def _set_note(self, text: str):
        self._note = text
        self._update_status()

    def _update_status(self):
        n = self.changed_count() if self._loaded else 0
        if n:
            self._note = ""
            self.status.setText(f"{n} UNSAVED CHANGE{'S' if n != 1 else ''}")
        else:
            self.status.setText(self._note or "ALL SAVED")
        self._dot.setVisible(bool(n))

    def _reset_page(self):
        idx = self.stack.currentIndex()
        section = SCHEMA[idx]
        d = defaults()[section.key]
        for f in section.fields:
            full = f"{section.key}.{f.key}"
            if full in self._widgets and f.type != "custom":
                self._set(f, self._widgets[full][1], d[f.key])
        self._update_status()

    def _live_conflict(self, full: str, combo: str) -> str:
        """What else uses `combo` right now (other fields on these pages, dashboard items, your Hyprland binds)."""
        if not combo:
            return ""
        sec_key = full.split(".", 1)[0]
        for other, (f, w) in self._widgets.items():
            if other == full or f.type != "hotkey":
                continue
            osec = other.split(".", 1)[0]
            en = self._widgets.get(f"{osec}.enabled")
            if en is not None and not en[1].isChecked():
                continue                     # a module that is switched off
            if osec == sec_key and sec_key == "doodle" and "esc" in combo:
                continue
            if hk.normalize(w.value()) == combo:
                from core.settings import SECTIONS
                return f"Also used by {SECTIONS[osec].title} → {f.label}"
        other = self.launcher.dashboard.hotkey_users().get(combo)
        if other:
            return f"Also used by {other}"
        conflict = getattr(self.launcher.hotkeys, "_conflict", None)
        if conflict is None or hk.validate(combo) != "":
            return ""                            # windows has no hyprland binds to clash with
        for keys in hk.to_hyprland(combo):
            if conflict(keys):
                return "Already a Hyprland bind in your config"
        return ""

    def _conflicts(self, data: dict) -> list[str]:
        """Find hotkeys used more than once (ignoring disabled modules)."""
        seen: dict[str, str] = {}
        problems = []
        for section in SCHEMA:
            sec = data.get(section.key, {})
            if "enabled" in sec and not sec["enabled"]:
                continue
            for f in section.fields:
                if f.type == "hotkey":
                    combo = hk.normalize(sec.get(f.key, ""))
                    if not combo:
                        continue
                    err = hk.validate(combo)
                    if err:
                        problems.append(f"{section.title} → {f.label}: '{combo}' is not a valid hotkey ({err})")
                        continue
                    label = f"{section.title} → {f.label}"
                    if combo in seen:
                        problems.append(f"{hk.pretty(combo)} is used by both {seen[combo]} and {label}")
                    seen[combo] = label
        for combo, label in self.launcher.dashboard.hotkey_users().items():
            if combo in seen:
                problems.append(f"{hk.pretty(combo)} is used by both {seen[combo]} and {label}")
            seen[combo] = label
        return problems

    def save(self):
        data = self.collect()
        problems = self._conflicts(data)
        if problems:
            widgets.error(self, "Hotkey problems", "Fix these before saving:\n\n• " + "\n• ".join(problems))
            return
        old = self.settings.all()
        restart_needed = any(
            old.get(s.key, {}).get(f.key) != data.get(s.key, {}).get(f.key)
            for s in SCHEMA for f in s.fields if f.restart
        )
        self.settings.replace(data)
        self._loaded = self.collect()
        self._stale = False
        self._set_note("SAVED AND APPLIED")
        if restart_needed and widgets.confirm(self, "Restart needed", "Some changes only fully apply after a "
                                              "restart.\nRestart Luminescent now?", yes="Restart", no="Later"):
            self.launcher.restart()

    # ── overlay hooks ──────────────────────────────────────────────────────
    def state(self) -> dict:
        """Which section is open (a dashboard box remembers it)."""
        i = self.stack.currentIndex()
        return {"section": next((k for k, v in self._pages.items() if v == i), "")}

    def restore_state(self, st: dict):
        self.open_section(st.get("section", ""))

    def open_section(self, key: str = ""):
        if key in self._pages:
            self.search.clear()
            self._select(self._pages[key])

    def on_show(self):
        if self._stale and not self.is_dirty():
            self.load_values()
        self._update_status()
        self._dirty_timer.start()

    def on_hide(self):
        self._dirty_timer.stop()
        self.launcher.hotkeys.set_suspended(False)

    def hideEvent(self, e):
        self._dirty_timer.stop()
        super().hideEvent(e)

    def showEvent(self, e):
        super().showEvent(e)
        self._dirty_timer.start()
