"""
Caelestia & Glass: the Caelestia shell's own config files, editable from Luminescent, and see-through
("glass") windows per app.

Shell files: everything in ~/.config/caelestia. A .json file opens as a form (switches, numbers, text,
colours with a swatch), anything else as plain text. Save keeps a copy of the old file in
data/caelestia-backups first; Caelestia reloads its config by itself when the file changes. Colours shows
the scheme Caelestia is using now (~/.local/state/caelestia/scheme.json) and its wallpaper.

Glass: per app (its window class, e.g. firefox or kitty) how see-through it is when focused and when not,
and whether what is behind it gets blurred. These are Hyprland window rules Luminescent adds while it runs
(nothing is written to your config); they come back by themselves after a config reload. The blur
strength (size and passes) is Hyprland's own decoration setting, changed for this session only.

    settings caelestia.glass: [{"cls", "active", "inactive", "blur"}]
"""
import os
import json
import shutil
from datetime import datetime

from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QListWidget, QListWidgetItem, QStackedWidget, QPlainTextEdit,
    QLineEdit, QCheckBox, QSpinBox, QDoubleSpinBox, QPushButton, QScrollArea, QFrame, QLabel,
    QComboBox, QSlider, QGridLayout
)
from PyQt6.QtCore import Qt, QTimer, QFileSystemWatcher, pyqtSignal
from PyQt6.QtGui import QPixmap

from core import widgets, theme, hypr, IS_WIN, DATA_DIR
from core.module import Module

CONFIG_DIR = os.path.join(os.environ.get("XDG_CONFIG_HOME") or os.path.expanduser("~/.config"), "caelestia")
STATE_DIR = os.path.join(os.environ.get("XDG_STATE_HOME") or os.path.expanduser("~/.local/state"), "caelestia")
BACKUPS = os.path.join(DATA_DIR, "caelestia-backups")
TEXT_EXT = (".json", ".conf", ".txt", ".toml", ".ini", ".lua", ".qml", ".css", ".scss", ".sh", ".yaml", ".yml", "")


def config_files() -> list[str]:
    """Every text file in ~/.config/caelestia (and one folder down), shell.json first."""
    out = []
    if not os.path.isdir(CONFIG_DIR):
        return out
    for root, dirs, files in os.walk(CONFIG_DIR):
        depth = os.path.relpath(root, CONFIG_DIR).count(os.sep) + (root != CONFIG_DIR)
        if depth >= 2:
            dirs[:] = []
        dirs[:] = [d for d in dirs if not d.startswith(".")]
        for n in sorted(files):
            if n.startswith(".") or os.path.splitext(n)[1].lower() not in TEXT_EXT:
                continue
            p = os.path.join(root, n)
            if os.path.getsize(p) < 2_000_000:
                out.append(p)
    out.sort(key=lambda p: (os.path.basename(p) != "shell.json", p))
    return out


def backup(path: str) -> str:
    os.makedirs(BACKUPS, exist_ok=True)
    dst = os.path.join(BACKUPS, f"{datetime.now():%Y%m%d-%H%M%S}-{os.path.basename(path)}")
    if os.path.exists(path):
        shutil.copy2(path, dst)
    return dst


def is_colour(v) -> bool:
    if not isinstance(v, str) or not v.startswith("#") or len(v) not in (4, 7, 9):
        return False
    try:
        int(v[1:], 16)
        return True
    except ValueError:
        return False


def scheme() -> dict:
    """The colours of Caelestia's current scheme ({name: "#rrggbb"}), {} when there is none."""
    path = os.path.join(STATE_DIR, "scheme.json")
    try:
        with open(path, "r", encoding="utf-8") as f:
            d = json.load(f)
    except (OSError, ValueError):
        return {}
    cols = d.get("colours") or d.get("colors") or d
    out = {}
    for k, v in (cols.items() if isinstance(cols, dict) else []):
        v = str(v)
        v = v if v.startswith("#") else "#" + v
        if is_colour(v):
            out[k] = v
    return out


def wallpaper() -> str:
    """The wallpaper Caelestia shows, '' when it can not tell."""
    for name in ("wallpaper/path.txt", "wallpaper/last.txt"):
        try:
            with open(os.path.join(STATE_DIR, name), "r", encoding="utf-8") as f:
                p = f.read().strip()
            if p and os.path.isfile(p):
                return p
        except OSError:
            continue
    cur = os.path.join(STATE_DIR, "wallpaper", "current")
    return os.path.realpath(cur) if os.path.exists(cur) else ""


def nice(key: str) -> str:
    """'showOnHover' -> 'Show on hover', 'max_size' -> 'Max size'."""
    out = ""
    for i, c in enumerate(key.replace("_", " ")):
        out += (" " + c.lower()) if c.isupper() and i and key[i - 1].islower() else c
    return out[:1].upper() + out[1:]


# ── the .json form ────────────────────────────────────────────────────────────
class JsonForm(QWidget):
    """Every value in a JSON file as a control. `data` is changed in place; `changed` fires on every edit."""

    def __init__(self, data, on_change):
        super().__init__()
        self.data = data
        self.on_change = on_change
        self.rows: list[tuple[str, QWidget]] = []
        self.cards: list[tuple[QWidget, list]] = []      # each card with the rows (and cards) inside it
        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 8, 0)
        lay.setSpacing(10)
        if isinstance(data, dict):
            plain = {k: v for k, v in data.items() if not isinstance(v, dict)}
            if plain:
                lay.addWidget(self._card("General", data, list(plain), ""))
            for k, v in data.items():
                if isinstance(v, dict):
                    lay.addWidget(self._card(nice(k), v, list(v), k))
        else:
            lay.addWidget(widgets.label("This file is not a JSON object; edit it as text.", 12, tone="muted"))
        lay.addStretch()

    def _card(self, title: str, obj: dict, keys: list, path: str) -> QWidget:
        card = QFrame()
        card.setObjectName("lumCard")
        v = QVBoxLayout(card)
        v.setContentsMargins(14, 10, 14, 12)
        v.setSpacing(6)
        v.addWidget(widgets.label(title.upper(), 10.5, 600, mono=True, spacing=0.14, tone="accent"))
        inside = []
        self.cards.append((card, inside))
        for k in keys:
            val = obj[k]
            full = f"{path}.{k}" if path else k
            if isinstance(val, dict):
                sub = self._card(nice(k), val, list(val), full)
                v.addWidget(sub)
                inside.append(sub)
                continue
            row = QWidget()
            h = QHBoxLayout(row)
            h.setContentsMargins(0, 0, 0, 0)
            h.setSpacing(10)
            name = widgets.label(nice(k), 12.5)
            name.setToolTip(full)
            h.addWidget(name, 1)
            h.addWidget(self._editor(obj, k))
            v.addWidget(row)
            self.rows.append((full, row))
            inside.append(row)
        return card

    def _editor(self, obj: dict, k: str) -> QWidget:
        val = obj[k]

        def store(x):
            obj[k] = x
            self.on_change()
        if isinstance(val, bool):
            w = QCheckBox()
            w.setChecked(val)
            w.toggled.connect(store)
            return w
        if isinstance(val, int):
            w = QSpinBox()
            w.setRange(-1_000_000, 1_000_000)
            w.setValue(val)
            w.setFixedWidth(130)
            w.valueChanged.connect(store)
            return w
        if isinstance(val, float):
            w = QDoubleSpinBox()
            w.setRange(-1_000_000, 1_000_000)
            w.setDecimals(3)
            w.setSingleStep(0.05 if abs(val) <= 1 else 1)
            w.setValue(val)
            w.setFixedWidth(130)
            w.valueChanged.connect(store)
            return w
        if is_colour(val):
            box = QWidget()
            h = QHBoxLayout(box)
            h.setContentsMargins(0, 0, 0, 0)
            h.setSpacing(6)
            sw = QPushButton()
            sw.setFixedSize(28, 24)
            e = QLineEdit(val)
            e.setFixedWidth(100)

            def paint(c):
                sw.setStyleSheet(f"background: {c}; border: 1px solid {theme.P.border}; border-radius: 5px;")

            def typed(t):
                if is_colour(t):
                    paint(t)
                    store(t)

            def pick():
                from core.colorpick import pick_colour
                c = pick_colour(box, e.text(), nice(k))
                if c:
                    e.setText(c)
            paint(val)
            e.textChanged.connect(typed)
            sw.clicked.connect(pick)
            h.addWidget(sw)
            h.addWidget(e)
            return box
        if isinstance(val, str):
            w = QLineEdit(val)
            w.setMinimumWidth(240)
            w.textChanged.connect(store)
            return w
        # lists and null: as JSON text
        w = QLineEdit(json.dumps(val))
        w.setMinimumWidth(240)
        w.setProperty("mono", True)

        def typed_json(t):
            try:
                store(json.loads(t))
                w.setStyleSheet("")
            except ValueError:
                w.setStyleSheet(f"border-color: {theme.P.danger};")
        w.textChanged.connect(typed_json)
        return w

    def filter(self, q: str):
        q = q.strip().lower()
        for full, w in self.rows:
            w.setVisible(not q or q in full.lower() or q in nice(full.split(".")[-1]).lower())
        # inner cards come later in the list: settle them first, so their parents see the result
        for card, inside in reversed(self.cards):
            card.setVisible(not q or any(not w.isHidden() for w in inside))


# ── glass apps ────────────────────────────────────────────────────────────────
class GlassEditor(QWidget):
    """The list of see-through apps: per app how solid it is focused and unfocused, and blur on or off.
    get_value() / set_value() for the Settings page; `changed` fires on every edit."""
    changed = pyqtSignal()

    def __init__(self, open_classes=None):
        super().__init__()
        self._rules: list[dict] = []
        self._open_classes = open_classes or (lambda: [])
        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(6)
        self.list = QVBoxLayout()
        self.list.setSpacing(6)
        lay.addLayout(self.list)
        row = QHBoxLayout()
        row.setSpacing(8)
        self.pick = QComboBox()
        self.pick.setEditable(True)
        self.pick.setFixedHeight(32)
        self.pick.setMinimumWidth(260)
        self.pick.lineEdit().setPlaceholderText("Window class, e.g. kitty (or pick an open window)")
        self.pick.lineEdit().returnPressed.connect(self._add)
        row.addWidget(self.pick, 1)
        add = widgets.button("＋  Add app", "primary")
        add.setAutoDefault(False)
        add.clicked.connect(self._add)
        row.addWidget(add)
        lay.addLayout(row)
        self._fill()

    def get_value(self) -> list[dict]:
        return [dict(r) for r in self._rules]

    def set_value(self, rules):
        self._rules = [dict(r) for r in (rules or []) if isinstance(r, dict) and r.get("cls")]
        self._fill()

    def _fill(self):
        widgets.clear_layout(self.list)
        if not self._rules:
            self.list.addWidget(widgets.label("No apps yet. Pick one below and press Add app.", 12, tone="muted"))
        for i, r in enumerate(self._rules):
            self.list.addWidget(self._row(i, r))
        self.pick.clear()
        seen = {r["cls"] for r in self._rules}
        try:
            classes = self._open_classes()
        except Exception:
            classes = []
        for c in classes:
            if c not in seen:
                self.pick.addItem(c)
        self.pick.setEditText("")

    def _row(self, i: int, r: dict) -> QWidget:
        card = QFrame()
        card.setObjectName("lumCard")
        h = QHBoxLayout(card)
        h.setContentsMargins(12, 6, 8, 6)
        h.setSpacing(10)
        name = widgets.label(r["cls"], 13, 600, mono=True)
        name.setMinimumWidth(110)
        h.addWidget(name, 1)
        for key, label in (("active", "Focused"), ("inactive", "Not focused")):
            h.addWidget(widgets.label(label, 11.5, tone="secondary"))
            sl = QSlider(Qt.Orientation.Horizontal)
            sl.setRange(10, 100)
            sl.setFixedWidth(100)
            sl.setValue(int(round(float(r.get(key, 1.0)) * 100)))
            val = widgets.label(f"{sl.value()} %", 11.5, mono=True)
            val.setFixedWidth(44)
            sl.valueChanged.connect(lambda x, l=val: l.setText(f"{x} %"))
            sl.valueChanged.connect(lambda x, k=key: self._set(i, k, x / 100))
            h.addWidget(sl)
            h.addWidget(val)
        b = QCheckBox("Blur")
        b.setToolTip("Blur what is behind the window")
        b.setChecked(bool(r.get("blur", True)))
        b.toggled.connect(lambda on: self._set(i, "blur", on))
        h.addWidget(b)
        rm = widgets.icon_button("✕", "Remove", 24, 11, ghost=True)
        rm.setAutoDefault(False)
        rm.clicked.connect(lambda: self._remove(i))
        h.addWidget(rm)
        return card

    def _set(self, i: int, key: str, value):
        if 0 <= i < len(self._rules):
            self._rules[i][key] = value
            self.changed.emit()

    def _remove(self, i: int):
        if 0 <= i < len(self._rules):
            self._rules.pop(i)
            self._fill()
            self.changed.emit()

    def _add(self):
        cls = self.pick.currentText().strip()
        if not cls or any(r["cls"] == cls for r in self._rules):
            return
        self._rules.append({"cls": cls, "active": 0.92, "inactive": 0.78, "blur": True})
        self._fill()
        self.changed.emit()


class SyncWidget(QWidget):
    """Settings → Caelestia: what the shell's colours are, and buttons to apply or restore them."""

    def __init__(self, launcher):
        super().__init__()
        self.launcher = launcher
        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(8)
        self.swatches = QHBoxLayout()
        self.swatches.setSpacing(8)
        lay.addLayout(self.swatches)
        row = QHBoxLayout()
        row.setSpacing(8)
        for text, variant, fn in (("Apply the theme to the shell now", "primary", self._apply),
                                  ("Restore Caelestia's own colours", "", self._restore),
                                  ("Open Caelestia && Glass", "ghost", self._open)):
            b = widgets.button(text, variant)
            b.setAutoDefault(False)
            b.clicked.connect(fn)
            row.addWidget(b)
        row.addStretch()
        lay.addLayout(row)
        self.status = widgets.label("", 12, tone="muted")
        self.status.setWordWrap(True)
        lay.addWidget(self.status)
        self.refresh()

    def _module(self):
        return self.launcher.modules.get("caelestia") if self.launcher else None

    def refresh(self):
        from core import caelestia_sync
        widgets.clear_layout(self.swatches)
        t = self.launcher.themes.active() if self.launcher else {}
        pri, sec = caelestia_sync.shell_colours(t)
        for c, name in ((pri, "Primary"), (sec, "Secondary")):
            chip = QFrame()
            chip.setFixedSize(22, 22)
            chip.setStyleSheet(f"background: {c}; border: 1px solid {theme.P.border}; border-radius: 6px;")
            self.swatches.addWidget(chip)
            self.swatches.addWidget(widgets.label(f"{name} {c}", 12, mono=True, tone="secondary"))
            self.swatches.addSpacing(10)
        self.swatches.addStretch()
        if IS_WIN:
            self.status.setText("Caelestia is a Linux shell.")
        elif not caelestia_sync.installed():
            self.status.setText("Caelestia was not found (~/.local/state/caelestia and ~/.config/caelestia are "
                                "missing), so there is no shell to recolour.")
        else:
            cur = caelestia_sync.read_scheme()
            self.status.setText(f"From the theme \"{t.get('name', '')}\". Caelestia's scheme now: "
                                f"{cur.get('name', '?')} {cur.get('flavour', '')} ({cur.get('mode', '?')}).")

    def _apply(self):
        m = self._module()
        err = m.sync_colours(force=True) if m else "Caelestia & Glass is turned off."
        self.refresh()
        if err:
            self.status.setText(f"⚠ {err}")

    def _restore(self):
        from core import caelestia_sync
        err = caelestia_sync.restore()
        self.refresh()
        self.status.setText(f"⚠ {err}" if err else "Caelestia's own colours are back. Turn off \"The shell follows "
                                                    "the theme\" or the next theme you pick recolours it again.")

    def _open(self):
        m = self._module()
        if m and m.running:
            m.toggle()


# ── the panel ─────────────────────────────────────────────────────────────────
class CaelestiaPanel(widgets.FloatingPanel):
    def __init__(self, m: "CaelestiaModule"):
        self.m = m
        super().__init__("Caelestia & Glass", min_size=(720, 460), opacity=0.97, close_hides=True)
        self._path = ""
        self._data = None
        self._dirty = False
        lay = QVBoxLayout()
        lay.setContentsMargins(12, 10, 12, 12)
        lay.setSpacing(10)
        tabs = QHBoxLayout()
        tabs.setSpacing(4)
        self.pages = QStackedWidget()
        self._tabs = []
        for i, (name, build) in enumerate((("Shell files", self._files_page), ("Glass", self._glass_page),
                                           ("Colours", self._colours_page))):
            b = QPushButton(name)
            b.setObjectName("boardTab")
            b.setCheckable(True)
            b.setCursor(Qt.CursorShape.PointingHandCursor)
            b.clicked.connect(lambda _c=False, i=i: self.show_tab(i))
            tabs.addWidget(b)
            self._tabs.append(b)
            self.pages.addWidget(build())
        tabs.addStretch()
        lay.addLayout(tabs)
        lay.addWidget(self.pages, 1)
        self.body.addLayout(lay)
        self.resize(980, 660)
        self.center_on(widgets.cursor_screen_geometry())
        self.escape_pressed.connect(self.hide)
        self.show_tab(0)

    def show_tab(self, i: int):
        for j, b in enumerate(self._tabs):
            b.setChecked(i == j)
        self.pages.setCurrentIndex(i)
        if i == 0:
            self.reload_files()
        elif i == 1:
            self.glass_editor.set_value(self.m.glass())
        elif i == 2:
            self._fill_colours()

    # ── shell files ────────────────────────────────────────────────────────
    def _files_page(self) -> QWidget:
        page = QWidget()
        h = QHBoxLayout(page)
        h.setContentsMargins(0, 0, 0, 0)
        h.setSpacing(10)
        left = QVBoxLayout()
        self.files = QListWidget()
        self.files.setFixedWidth(220)
        self.files.currentItemChanged.connect(self._file_picked)
        left.addWidget(self.files, 1)
        new = widgets.button("New shell.json", "ghost")
        new.setToolTip("Caelestia uses its defaults until you have one")
        new.clicked.connect(self._new_shell_json)
        left.addWidget(new)
        folder = widgets.button("Open the folder", "ghost")
        folder.clicked.connect(lambda: self.m.open_folder())
        left.addWidget(folder)
        h.addLayout(left)
        right = QVBoxLayout()
        right.setSpacing(8)
        top = QHBoxLayout()
        self.search = QLineEdit()
        self.search.setPlaceholderText("🔍  Find a setting…")
        self.search.setClearButtonEnabled(True)
        self.search.textChanged.connect(lambda t: self._form.filter(t) if self._form else None)
        top.addWidget(self.search, 1)
        self.as_text = widgets.button("Edit as text", "ghost")
        self.as_text.setCheckable(True)
        self.as_text.toggled.connect(lambda _on: self._show_file())
        top.addWidget(self.as_text)
        self.save_btn = widgets.button("Save", "primary")
        self.save_btn.clicked.connect(self.save)
        top.addWidget(self.save_btn)
        right.addLayout(top)
        self.view = QStackedWidget()
        self.scroll = QScrollArea()
        self.scroll.setWidgetResizable(True)
        self.scroll.setFrameShape(QFrame.Shape.NoFrame)
        self.text = QPlainTextEdit()
        self.text.setProperty("mono", True)
        self.text.textChanged.connect(self._text_changed)
        self.empty = widgets.label("", 13, tone="muted")
        self.empty.setWordWrap(True)
        self.empty.setAlignment(Qt.AlignmentFlag.AlignCenter)
        for w in (self.scroll, self.text, self.empty):
            self.view.addWidget(w)
        right.addWidget(self.view, 1)
        self.status = widgets.label("", 11, tone="muted")
        right.addWidget(self.status)
        h.addLayout(right, 1)
        self._form: JsonForm | None = None
        return page

    def reload_files(self):
        keep = self._path
        self.files.blockSignals(True)
        self.files.clear()
        for p in config_files():
            it = QListWidgetItem(os.path.relpath(p, CONFIG_DIR))
            it.setData(Qt.ItemDataRole.UserRole, p)
            self.files.addItem(it)
        self.files.blockSignals(False)
        if not self.files.count():
            self._path = ""
            self.empty.setText(f"No Caelestia config found in {CONFIG_DIR}.\n\nCaelestia uses its built-in defaults "
                               "until there is a shell.json. Press New shell.json to start one.")
            self.view.setCurrentWidget(self.empty)
            self.save_btn.setEnabled(False)
            return
        row = next((i for i in range(self.files.count())
                    if self.files.item(i).data(Qt.ItemDataRole.UserRole) == keep), 0)
        self.files.setCurrentRow(row)

    def _file_picked(self, it, _prev=None):
        if it is None:
            return
        if self._dirty and self._path and not widgets.confirm(self, "Caelestia", "Throw away your unsaved changes?"):
            return
        self._path = it.data(Qt.ItemDataRole.UserRole)
        self._load()

    def _load(self):
        try:
            with open(self._path, "r", encoding="utf-8") as f:
                self._raw = f.read()
        except OSError as e:
            self._raw = ""
            self.status.setText(f"Could not read it: {e}")
        self._data = None
        if self._path.endswith(".json"):
            try:
                self._data = json.loads(self._raw or "{}")
            except ValueError as e:
                self.status.setText(f"Not valid JSON ({e}); showing it as text.")
        self._dirty = False
        self._show_file()

    def _show_file(self):
        self.save_btn.setEnabled(bool(self._path))
        form = self._data is not None and not self.as_text.isChecked()
        self.search.setEnabled(form)
        if form:
            self._form = JsonForm(self._data, self._changed)
            self.scroll.setWidget(self._form)
            self._form.filter(self.search.text())
            self.view.setCurrentWidget(self.scroll)
        else:
            self._form = None
            self.text.blockSignals(True)
            self.text.setPlainText(json.dumps(self._data, indent=4) if self._data is not None else self._raw)
            self.text.blockSignals(False)
            self.view.setCurrentWidget(self.text)
        self._update_status()

    def _changed(self):
        self._dirty = True
        self._update_status()

    def _text_changed(self):
        self._dirty = True
        if self._data is not None:
            try:
                self._data = json.loads(self.text.toPlainText())
            except ValueError:
                pass
        self._update_status()

    def _update_status(self):
        name = os.path.basename(self._path) if self._path else ""
        self.status.setText(f"{name} · unsaved changes" if self._dirty else
                            f"{name} · saved. Caelestia picks up changes by itself." if name else "")

    def save(self):
        if not self._path:
            return
        if self.view.currentWidget() is self.text:
            text = self.text.toPlainText()
            if self._path.endswith(".json"):
                try:
                    self._data = json.loads(text)
                except ValueError as e:
                    widgets.error(self, "Caelestia", f"That is not valid JSON, so it was not saved:\n{e}")
                    return
        else:
            text = json.dumps(self._data, indent=4) + "\n"
        try:
            backup(self._path)
            with open(self._path, "w", encoding="utf-8") as f:
                f.write(text)
        except OSError as e:
            widgets.error(self, "Caelestia", f"Could not save it:\n{e}")
            return
        self._dirty = False
        self._update_status()
        self.m.ctx.notify("Caelestia", f"Saved {os.path.basename(self._path)} (the old one is in "
                                       "data/caelestia-backups)", 3000)

    def _new_shell_json(self):
        path = os.path.join(CONFIG_DIR, "shell.json")
        if not os.path.exists(path):
            os.makedirs(CONFIG_DIR, exist_ok=True)
            with open(path, "w", encoding="utf-8") as f:
                f.write("{}\n")
        self._path = path
        self.reload_files()

    # ── glass ──────────────────────────────────────────────────────────────
    def _glass_page(self) -> QWidget:
        page = QWidget()
        v = QVBoxLayout(page)
        v.setContentsMargins(0, 0, 0, 0)
        v.setSpacing(10)
        intro = widgets.label("See-through windows per app: how clear each app is when it has focus and when it "
                              "does not, and whether what is behind it is blurred. Luminescent adds these as "
                              "Hyprland window rules while it runs; your config file is not changed.", 12,
                              tone="muted")
        intro.setWordWrap(True)
        v.addWidget(intro)
        blur = QFrame()
        blur.setObjectName("lumCard")
        g = QGridLayout(blur)
        g.setContentsMargins(14, 10, 14, 10)
        g.setHorizontalSpacing(12)
        g.addWidget(widgets.label("BLUR STRENGTH", 10.5, 600, mono=True, spacing=0.14, tone="accent"), 0, 0, 1, 4)
        self.blur_size = self._slider(0, 30, int(self.m.s("blur_size", 0)))
        self.blur_passes = self._slider(0, 6, int(self.m.s("blur_passes", 0)))
        for row, (label, s) in enumerate((("Size (0 = keep Hyprland's)", self.blur_size),
                                          ("Passes (0 = keep Hyprland's)", self.blur_passes)), 1):
            g.addWidget(widgets.label(label, 12.5), row, 0)
            g.addWidget(s[0], row, 1)
            g.addWidget(s[1], row, 2)
        v.addWidget(blur)
        self.glass_editor = GlassEditor(self.m.open_classes)
        self.glass_editor.set_value(self.m.glass())
        self.glass_editor.changed.connect(self._glass_changed)
        sc = QScrollArea()
        sc.setWidgetResizable(True)
        sc.setFrameShape(QFrame.Shape.NoFrame)
        box = QWidget()
        bl = QVBoxLayout(box)
        bl.setContentsMargins(0, 0, 6, 0)
        bl.addWidget(self.glass_editor)
        bl.addStretch()
        sc.setWidget(box)
        v.addWidget(sc, 1)
        self.glass_status = widgets.label("", 11, tone="muted")
        self.glass_status.setWordWrap(True)
        v.addWidget(self.glass_status)
        self._glass_timer = QTimer(self)
        self._glass_timer.setSingleShot(True)
        self._glass_timer.setInterval(250)
        self._glass_timer.timeout.connect(self._apply_glass)
        return page

    def _slider(self, lo: int, hi: int, val: int):
        s = QSlider(Qt.Orientation.Horizontal)
        s.setRange(lo, hi)
        s.setValue(val)
        lbl = widgets.label(str(val), 12, mono=True)
        lbl.setFixedWidth(30)
        s.valueChanged.connect(lambda x: lbl.setText(str(x)))
        s.sliderReleased.connect(self._save_glass)
        return s, lbl

    def _glass_changed(self):
        self.m.set_glass(self.glass_editor.get_value())
        self._glass_timer.start()          # sliders fire a lot: apply once they rest

    def _save_glass(self):
        self.m.settings.set("caelestia.blur_size", self.blur_size[0].value())
        self.m.settings.set("caelestia.blur_passes", self.blur_passes[0].value())
        self.m.settings.save(emit=False)
        self._apply_glass()

    def _apply_glass(self):
        err = self.m.apply_glass()
        self.glass_status.setText(f"⚠ {err}" if err else "Applied. Focus another window to see the unfocused look.")

    # ── colours ────────────────────────────────────────────────────────────
    def _colours_page(self) -> QWidget:
        page = QWidget()
        v = QVBoxLayout(page)
        v.setContentsMargins(0, 0, 0, 0)
        v.setSpacing(10)
        top = QHBoxLayout()
        self.wall = QLabel()
        self.wall.setFixedSize(320, 180)
        self.wall.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.wall.setStyleSheet(f"background: {theme.P.bg_card}; border-radius: 10px;")
        top.addWidget(self.wall)
        side = QVBoxLayout()
        self.scheme_info = widgets.label("", 12, tone="muted")
        self.scheme_info.setWordWrap(True)
        side.addWidget(self.scheme_info)
        make = widgets.button("Make a Luminescent theme from it", "primary")
        make.clicked.connect(lambda: self.m.theme_from_scheme())
        side.addWidget(make, 0, Qt.AlignmentFlag.AlignLeft)
        side.addStretch()
        top.addLayout(side, 1)
        v.addLayout(top)
        self.swatches = QGridLayout()
        self.swatches.setSpacing(6)
        box = QWidget()
        outer = QVBoxLayout(box)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.addLayout(self.swatches)
        outer.addStretch()
        sc = QScrollArea()
        sc.setWidgetResizable(True)
        sc.setFrameShape(QFrame.Shape.NoFrame)
        sc.setWidget(box)
        v.addWidget(sc, 1)
        return page

    def _fill_colours(self):
        widgets.clear_layout(self.swatches)
        cols = scheme()
        wp = wallpaper()
        if wp:
            px = QPixmap(wp)
            if not px.isNull():
                self.wall.setPixmap(px.scaled(self.wall.size(), Qt.AspectRatioMode.KeepAspectRatioByExpanding,
                                              Qt.TransformationMode.SmoothTransformation))
        else:
            self.wall.setText("No wallpaper found")
        self.scheme_info.setText(f"{len(cols)} colours in Caelestia's scheme ({os.path.join(STATE_DIR, 'scheme.json')})."
                                 if cols else "Caelestia's colour scheme was not found (is Caelestia installed and "
                                              "running?).")
        for i, (k, c) in enumerate(sorted(cols.items())):
            sw = QFrame()
            sw.setFixedSize(26, 26)
            sw.setStyleSheet(f"background: {c}; border: 1px solid {theme.P.border}; border-radius: 6px;")
            lbl = widgets.label(f"{nice(k)}  {c}", 11.5, mono=True, tone="secondary")
            self.swatches.addWidget(sw, i // 3, (i % 3) * 2)
            self.swatches.addWidget(lbl, i // 3, (i % 3) * 2 + 1)


class CaelestiaModule(Module):
    key = "caelestia"
    title = "Caelestia & Glass"
    icon = "◐"

    def __init__(self, ctx):
        super().__init__(ctx)
        self.panel: CaelestiaPanel | None = None
        self._hooked = False
        self._themes_hooked = False
        self._watch: QFileSystemWatcher | None = None
        self._keep_timer = QTimer()
        self._keep_timer.setSingleShot(True)
        self._keep_timer.setInterval(1500)
        self._keep_timer.timeout.connect(self._keep_check)

    def start(self):
        self.hotkey("hotkey", self.toggle, "Caelestia & Glass")
        sync = hypr.sync()
        if sync is not None and not self._hooked:
            sync.reloaded.connect(lambda: QTimer.singleShot(500, self.apply_glass))   # a reload drops the rules
            self._hooked = True
        launcher = self.ctx.launcher
        if launcher is not None and not self._themes_hooked:
            launcher.themes.applied.connect(self._theme_applied)
            self._themes_hooked = True
        QTimer.singleShot(1500, self.apply_glass)
        QTimer.singleShot(800, self.sync_colours)
        QTimer.singleShot(900, self.sync_appearance)
        self._watch_scheme()

    def stop(self):
        self.apply_glass(off=True)
        if self._themes_hooked and self.ctx.launcher is not None:
            try:
                self.ctx.launcher.themes.applied.disconnect(self._theme_applied)
            except TypeError:
                pass
            self._themes_hooked = False
        if self._watch is not None:
            self._watch.deleteLater()
            self._watch = None
        self._keep_timer.stop()
        if self.panel:
            self.panel.close()
            self.panel.deleteLater()
            self.panel = None

    # ── the shell follows the theme ────────────────────────────────────────
    def _theme_applied(self, t: dict):
        if self.running and self.s("follow_theme", True):
            self.sync_colours(t, force=True)

    def sync_colours(self, t: dict | None = None, force: bool = False) -> str:
        """Give Caelestia the theme's colours. Without `force` only when "The shell follows the theme" is on and
        the shell has other colours; with it always (the Apply button, a theme being applied)."""
        from core import caelestia_sync
        if IS_WIN or not (force or self.s("follow_theme", True)):
            return ""
        if not caelestia_sync.installed() or os.environ.get("LUMINESCENT_HOME"):
            # no stray scheme.json on a PC without Caelestia, and none from a test copy
            return "" if not force else "Caelestia was not found (no ~/.config/caelestia or ~/.local/state/caelestia)."
        launcher = self.ctx.launcher
        if t is None and launcher is not None:
            t = launcher.themes.active()
        if not t:
            return "No theme."
        _mode, cols = caelestia_sync.palette(t)
        if not force and (caelestia_sync.read_scheme().get("colours") or {}) == cols:
            caelestia_sync._last_written.update(cols)
            return ""
        err = caelestia_sync.write_scheme(t)
        if err:
            self.ctx.notify(self.title, f"Could not change Caelestia's colours: {err}")
            return err
        if self.s("apply_apps", False):
            caelestia_sync.apply_to_apps(t)
        return ""

    def _watch_scheme(self):
        from core import caelestia_sync
        if IS_WIN or self._watch is not None or not os.path.isdir(caelestia_sync.STATE_DIR):
            return
        # the folder, not the file: scheme.json is replaced, not written in place
        self._watch = QFileSystemWatcher([caelestia_sync.STATE_DIR])
        self._watch.directoryChanged.connect(lambda _p: self._keep_timer.start())

    def _keep_check(self):
        from core import caelestia_sync
        if self.running and self.s("follow_theme", True) and self.s("keep_colours", False) \
                and caelestia_sync.changed_by_someone_else():
            self.sync_colours(force=True)

    def sync_appearance(self) -> str:
        from core import caelestia_sync
        if IS_WIN or not self.s("shell_appearance", False) or os.environ.get("LUMINESCENT_HOME"):
            return ""
        err = caelestia_sync.set_appearance({key: self.s(key) for key, _path, _kind in caelestia_sync.APPEARANCE
                                             if self.s(key) is not None})
        if err:
            self.ctx.notify(self.title, f"Could not change shell.json: {err}")
        return err

    def retheme(self):
        # the panel is rebuilt in the new colours the next time it opens
        if self.panel and not self.panel.isVisible():
            self.panel.close()
            self.panel.deleteLater()
            self.panel = None

    def apply_settings(self):
        """Settings saved: no restart (that would close the panel with unsaved edits), just apply them."""
        self.hotkeys.unregister_all(self.key)
        self.hotkey("hotkey", self.toggle, "Caelestia & Glass")
        self.apply_glass()
        self.sync_colours()
        self.sync_appearance()
        self._watch_scheme()
        if self.panel and self.panel.isVisible():
            self.panel.glass_editor.set_value(self.glass())

    def actions(self):
        return [("open", "Caelestia shell settings and glass", self.toggle)]

    def tray_actions(self):
        return [("Caelestia & Glass", self.toggle)]

    def toggle(self):
        if self.panel is None:
            with self.themed():
                self.panel = CaelestiaPanel(self)
        if self.panel.isVisible() and not self.panel.property("fading_out"):
            self.panel.fade_hide()
        else:
            self.panel.show_and_raise()

    def open_folder(self):
        from core import runner
        os.makedirs(CONFIG_DIR, exist_ok=True)
        runner.open_path(CONFIG_DIR)

    # ── glass ──────────────────────────────────────────────────────────────
    def glass(self) -> list[dict]:
        return [dict(r) for r in self.s("glass", []) or [] if isinstance(r, dict) and r.get("cls")]

    def set_glass(self, rules: list[dict]):
        self.settings.set("caelestia.glass", rules)
        self.settings.save(emit=False)

    @staticmethod
    def open_classes() -> list[str]:
        out = []
        for c in hypr.query("clients") or []:
            cls = str(c.get("class") or "")
            if cls and cls != hypr.APP_ID and cls not in out:
                out.append(cls)
        return sorted(out, key=str.lower)

    def apply_glass(self, off: bool = False) -> str:
        """Replace Luminescent's glass window rules with the ones in settings. Returns '' or an error."""
        if IS_WIN or not hypr.available():
            return "Glass needs Hyprland."
        parts = ["LUMI = LUMI or {} for _, r in pairs(LUMI.glass or {}) do pcall(function() r:set_enabled(false) end) "
                 "end LUMI.glass = {}"]
        for r in [] if off else self.glass():
            a = max(0.05, min(1.0, float(r.get("active", 1.0))))
            i = max(0.05, min(1.0, float(r.get("inactive", 1.0))))
            cls = "^(" + str(r["cls"]).replace("\\", "\\\\") + ")$"
            blur = "false" if r.get("blur", True) else "true"
            parts.append(f"LUMI.glass[#LUMI.glass + 1] = hl.window_rule({{ match = {{ class = {hypr.lstr(cls)} }}, "
                         f"opacity = {hypr.lstr(f'{a:.2f} {i:.2f}')}, no_blur = {blur} }})")
        err = hypr.lua(" ".join(parts))
        if err:
            return f"Hyprland did not take the window rules: {err}"
        size, passes = int(self.s("blur_size", 0) or 0), int(self.s("blur_passes", 0) or 0)
        if not off and (size or passes):
            blur = ", ".join(x for x in (f"size = {size}" if size else "", f"passes = {passes}" if passes else "") if x)
            err = hypr.lua(f"hl.config({{ decoration = {{ blur = {{ {blur} }} }} }})")
            if err:
                return f"The window rules are on, but Hyprland did not take the blur strength: {err}"
        return ""

    # ── colours ────────────────────────────────────────────────────────────
    def theme_from_scheme(self):
        cols = scheme()
        launcher = self.ctx.launcher
        if not cols or launcher is None:
            self.ctx.notify(self.title, "Caelestia's colour scheme was not found.")
            return
        pick = lambda *names: next((cols[n] for n in names if n in cols), "")
        tid = launcher.themes.from_colours("Caelestia", {
            "primary": pick("primary", "accent", "blue"), "bg": pick("surface", "background", "base"),
            "card": pick("surfaceContainer", "surface_container", "mantle", "surfaceContainerLow"),
            "text": pick("onSurface", "on_surface", "text"), "muted": pick("onSurfaceVariant", "outline", "subtext0"),
            "border": pick("outlineVariant", "outline_variant", "surface2", "outline")})
        if tid:
            launcher.apply_theme(tid)
            self.ctx.notify(self.title, "Made a Caelestia theme from the shell's colours and switched to it.")
