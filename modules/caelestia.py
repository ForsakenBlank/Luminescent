"""
Caelestia & Glass: the Caelestia shell's own config files, editable from Luminescent, and see-through
("glass") windows per app.

Shell files: everything in ~/.config/caelestia. A .json file opens as a form (switches, numbers, text,
colours with a swatch), anything else as plain text. Save keeps a copy of the old file in
data/caelestia-backups first; Caelestia reloads its config by itself when the file changes.

Colours: every colour of the scheme Caelestia is using now (~/.local/state/caelestia/scheme.json), each one
editable. "Apply to Caelestia" writes them into scheme.json (a backup first); the shell watches that file and
recolours itself. Changed colours can also be kept:
  * for this Caelestia scheme (name / flavour / mode): they are put back whenever Caelestia switches to it again
    or rewrites it (a new wallpaper with a dynamic scheme), while "Put my colours back…" is on;
  * for a Luminescent theme: written into Caelestia's scheme whenever that theme is applied (the theme
    editor's Caelestia colours page edits them too); a theme without any puts the old colours back.
Caelestia draws every colour flat (one value per colour role), so gradients can not go into its scheme.

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
from PyQt6.QtCore import Qt, QTimer, QFileSystemWatcher
from PyQt6.QtGui import QPixmap

from core import widgets, theme, hypr, colourpick, IS_WIN, DATA_DIR
from core import wallpaper as wallpaper_tool
from core.module import Module
from core.guard import alive
from modules import caelestia_look as look

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
    # every theme switch can write a file, so only the newest copies of each are kept
    name = os.path.basename(path)
    old = sorted(n for n in os.listdir(BACKUPS) if n.endswith("-" + name))[:-40]
    for n in old:
        try:
            os.remove(os.path.join(BACKUPS, n))
        except OSError:
            pass
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


SCHEME_PATH = os.path.join(STATE_DIR, "scheme.json")


def scheme_file() -> dict | None:
    """scheme.json as it is (None when there is none or it can not be read)."""
    try:
        with open(SCHEME_PATH, "r", encoding="utf-8") as f:
            d = json.load(f)
        return d if isinstance(d, dict) else None
    except (OSError, ValueError):
        return None


def scheme_id(d: dict | None) -> str:
    """"name / flavour / mode" of a scheme.json, the key colours are saved under for that scheme."""
    if not d:
        return ""
    parts = [str(d.get(k) or "") for k in ("name", "flavour", "mode")]
    return " / ".join(p for p in parts if p) or "scheme"


def write_colours(changes: dict) -> str:
    """Put `changes` ({name: "#rrggbb"}) into scheme.json the way it already writes colours (with or without
    the #). A copy of the old file goes to data/caelestia-backups first. Returns '' or what went wrong."""
    d = scheme_file()
    if d is None:
        return "Caelestia's scheme.json was not found (is Caelestia installed and running?)."
    cols = d.get("colours") if isinstance(d.get("colours"), dict) else (
        d.get("colors") if isinstance(d.get("colors"), dict) else d)
    hashed = any(str(v).startswith("#") for v in cols.values())
    for k, v in changes.items():
        if k in cols and is_colour(v):
            cols[k] = v if hashed else v.lstrip("#").lower()
    try:
        backup(SCHEME_PATH)
        tmp = SCHEME_PATH + ".lum-tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(d, f)
        os.replace(tmp, SCHEME_PATH)            # in one step, so the shell never reads half a file
    except OSError as e:
        return f"Could not write scheme.json: {e}"
    return ""


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
                c = colourpick.pick_colour(box, e.text() if is_colour(e.text()) else "#000000", k)
                if c:
                    e.setText(c.lower() if e.text()[1:].islower() else c)
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
                w.setStyleSheet(f"border-color: {theme.DANGER};")
        w.textChanged.connect(typed_json)
        return w

    def filter(self, q: str):
        q = q.strip().lower()
        for full, w in self.rows:
            w.setVisible(not q or q in full.lower() or q in nice(full.split(".")[-1]).lower())
        # inner cards come later in the list: settle them first, so their parents see the result
        for card, inside in reversed(self.cards):
            card.setVisible(not q or any(not w.isHidden() for w in inside))


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

        every = QFrame()
        every.setObjectName("lumCard")
        eg = QGridLayout(every)
        eg.setContentsMargins(14, 10, 14, 10)
        eg.setHorizontalSpacing(12)
        eg.addWidget(widgets.label("EVERY OTHER APP", 10.5, 600, mono=True, spacing=0.14, tone="accent"), 0, 0, 1, 3)
        self.every_on = QCheckBox("Change how see-through every app without its own row below is")
        a, i = float(self.m.s("all_active", 0) or 0), float(self.m.s("all_inactive", 0) or 0)
        self.every_on.setChecked(bool(a or i))
        eg.addWidget(self.every_on, 1, 0, 1, 3)
        self.every_active = self._slider(20, 100, int(round((a or 1.0) * 100)), self._save_every)
        self.every_inactive = self._slider(20, 100, int(round((i or 0.9) * 100)), self._save_every)
        for row, (label, s) in enumerate((("Focused (the mouse is on it)", self.every_active),
                                          ("Not focused", self.every_inactive)), 2):
            eg.addWidget(widgets.label(label, 12.5), row, 0)
            eg.addWidget(s[0], row, 1)
            eg.addWidget(s[1], row, 2)
            s[0].setEnabled(self.every_on.isChecked())
        self.every_on.toggled.connect(lambda on: (self.every_active[0].setEnabled(on),
                                                  self.every_inactive[0].setEnabled(on), self._save_every()))
        self.theme_note = widgets.label("", 11, tone="muted")
        self.theme_note.setWordWrap(True)
        eg.addWidget(self.theme_note, 4, 0, 1, 3)
        v.addWidget(every)
        self.glass_list = QVBoxLayout()
        self.glass_list.setSpacing(6)
        box = QWidget()
        box.setLayout(self.glass_list)
        sc = QScrollArea()
        sc.setWidgetResizable(True)
        sc.setFrameShape(QFrame.Shape.NoFrame)
        sc.setWidget(box)
        v.addWidget(sc, 1)
        row = QHBoxLayout()
        self.app_pick = QComboBox()
        self.app_pick.setEditable(True)
        self.app_pick.lineEdit().setPlaceholderText("Window class, e.g. kitty (or pick an open window)")
        row.addWidget(self.app_pick, 1)
        add = widgets.button("＋  Add app", "primary")
        add.clicked.connect(self._add_glass)
        row.addWidget(add)
        self.pick_btn = widgets.button("Add the app I click next", "",
                                       "Click any window within 5 seconds and it is added to the list")
        self.pick_btn.clicked.connect(self._pick_focused)
        row.addWidget(self.pick_btn)
        apply = widgets.button("Apply now")
        apply.clicked.connect(self._apply_glass)
        row.addWidget(apply)
        v.addLayout(row)
        self.glass_status = widgets.label("", 11, tone="muted")
        self.glass_status.setWordWrap(True)
        v.addWidget(self.glass_status)
        self._fill_glass()
        return page

    def _slider(self, lo: int, hi: int, val: int, on_release=None):
        s = QSlider(Qt.Orientation.Horizontal)
        s.setRange(lo, hi)
        s.setValue(val)
        lbl = widgets.label(str(val), 12, mono=True)
        lbl.setFixedWidth(30)
        s.valueChanged.connect(lambda x: lbl.setText(str(x)))
        s.sliderReleased.connect(on_release or self._save_glass)
        return s, lbl

    def _save_every(self):
        on = self.every_on.isChecked()
        self.m.settings.set("caelestia.all_active", self.every_active[0].value() / 100 if on else 0)
        self.m.settings.set("caelestia.all_inactive", self.every_inactive[0].value() / 100 if on else 0)
        self.m.settings.save(emit=False)
        self._apply_glass()

    def _pick_focused(self):
        """Wait for the next window to get focus (5 s at most) and add its app."""
        start = (hypr.query("activewindow") or {}).get("address")
        self._pick_left = 10
        self.pick_btn.setEnabled(False)

        def tick():
            if not alive(self):
                return
            w = hypr.query("activewindow") or {}
            cls = str(w.get("class") or "")
            if w.get("address") != start and cls and cls != hypr.APP_ID:
                self.pick_btn.setEnabled(True)
                self.pick_btn.setText("Add the app I click next")
                self.app_pick.setEditText(cls)
                self._add_glass()
                self.glass_status.setText(f"Added {cls}.")
                return
            self._pick_left -= 1
            if self._pick_left <= 0:
                self.pick_btn.setEnabled(True)
                self.pick_btn.setText("Add the app I click next")
                self.glass_status.setText("No other window was clicked.")
                return
            self.pick_btn.setText(f"Click a window… {self._pick_left // 2 + 1}")
            QTimer.singleShot(500, tick)
        tick()

    def _fill_glass(self):
        g = self.m.theme_glass()
        launcher = self.m.ctx.launcher
        name = launcher.themes.active().get("name", "") if launcher is not None else ""
        self.theme_note.setText(
            f"The theme {name} has its own every-app and blur settings (theme editor → Glass), so those are used "
            "while it is on." if g else "A theme can have its own every-app and blur settings: theme editor → Glass.")
        widgets.clear_layout(self.glass_list)
        rules = self.m.glass()
        if not rules:
            self.glass_list.addWidget(widgets.label("No apps yet. Pick one below and press Add app.", 12,
                                                    tone="muted"))
        for i, r in enumerate(rules):
            self.glass_list.addWidget(self._glass_row(i, r))
        self.glass_list.addStretch()
        self.app_pick.clear()
        seen = {r["cls"] for r in rules}
        for c in self.m.open_classes():
            if c not in seen:
                self.app_pick.addItem(c)
        self.app_pick.setEditText("")

    def _glass_row(self, i: int, r: dict) -> QWidget:
        card = QFrame()
        card.setObjectName("lumCard")
        h = QHBoxLayout(card)
        h.setContentsMargins(12, 6, 8, 6)
        h.setSpacing(10)
        h.addWidget(widgets.label(r["cls"], 13, 600, mono=True), 1)
        for key, label in (("active", "Focused"), ("inactive", "Not focused")):
            h.addWidget(widgets.label(label, 11.5, tone="secondary"))
            s = QSlider(Qt.Orientation.Horizontal)
            s.setRange(20, 100)
            s.setFixedWidth(110)
            s.setValue(int(round(float(r.get(key, 1.0)) * 100)))
            val = widgets.label(f"{s.value()} %", 11.5, mono=True)
            val.setFixedWidth(44)
            s.valueChanged.connect(lambda x, l=val: l.setText(f"{x} %"))
            s.sliderReleased.connect(lambda s=s, k=key: self._set_glass(i, k, s.value() / 100))
            h.addWidget(s)
            h.addWidget(val)
        b = QCheckBox("Blur")
        b.setChecked(bool(r.get("blur", True)))
        b.toggled.connect(lambda on: self._set_glass(i, "blur", on))
        h.addWidget(b)
        rm = widgets.icon_button("✕", "Remove", 24, 11, ghost=True)
        rm.clicked.connect(lambda: self._remove_glass(i))
        h.addWidget(rm)
        return card

    def _set_glass(self, i: int, key: str, value):
        rules = self.m.glass()
        if 0 <= i < len(rules):
            rules[i][key] = value
            self.m.set_glass(rules)
            self._apply_glass()

    def _remove_glass(self, i: int):
        rules = self.m.glass()
        if 0 <= i < len(rules):
            rules.pop(i)
            self.m.set_glass(rules)
            self._fill_glass()
            self._apply_glass()

    def _add_glass(self):
        cls = self.app_pick.currentText().strip()
        if not cls:
            return
        rules = self.m.glass()
        if any(r["cls"] == cls for r in rules):
            return
        rules.append({"cls": cls, "active": 0.92, "inactive": 0.78, "blur": True})
        self.m.set_glass(rules)
        self._fill_glass()
        self._apply_glass()

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
        self._edits: dict[str, str] = {}
        top = QHBoxLayout()
        top.setSpacing(14)
        self.wall = QLabel()
        self.wall.setFixedSize(240, 135)
        self.wall.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.wall.setStyleSheet(f"background: {theme.P.bg_card}; border-radius: 10px;")
        top.addWidget(self.wall)
        side = QVBoxLayout()
        side.setSpacing(6)
        self.scheme_name = widgets.label("", 15, 600)
        side.addWidget(self.scheme_name)
        self.scheme_info = widgets.label("", 11.5, tone="muted")
        self.scheme_info.setWordWrap(True)
        side.addWidget(self.scheme_info)
        hint = widgets.label("Click a colour to change it. Caelestia draws its colours flat, so they are plain colours "
                             "(no gradients).", 11, tone="muted")
        hint.setWordWrap(True)
        side.addWidget(hint)
        side.addStretch()
        top.addLayout(side, 1)
        v.addLayout(top)

        bar = QHBoxLayout()
        bar.setSpacing(6)
        self.colour_search = QLineEdit()
        self.colour_search.setPlaceholderText("🔍  Find a colour…")
        self.colour_search.setClearButtonEnabled(True)
        self.colour_search.textChanged.connect(lambda _t: self._fill_colours(reload=False))
        bar.addWidget(self.colour_search, 1)
        self.apply_btn = widgets.button("Apply to Caelestia", "primary", "Write the changed colours into its scheme now")
        self.apply_btn.clicked.connect(self._apply_colours)
        bar.addWidget(self.apply_btn)
        self.undo_btn = widgets.button("Undo changes", "ghost")
        self.undo_btn.clicked.connect(lambda: (self._edits.clear(), self._fill_colours(reload=False)))
        bar.addWidget(self.undo_btn)
        keep = widgets.button("Save for…", "", "Keep these colours for this Caelestia scheme or a Luminescent theme")
        keep.clicked.connect(lambda: self._save_menu(keep))
        bar.addWidget(keep)
        make = widgets.button("Luminescent theme from it", "ghost")
        make.setToolTip("Make a Luminescent theme in these colours and switch to it")
        make.clicked.connect(lambda: self.m.theme_from_scheme())
        bar.addWidget(make)
        v.addLayout(bar)
        self.colour_status = widgets.label("", 11, tone="muted")
        self.colour_status.setWordWrap(True)
        v.addWidget(self.colour_status)

        self.swatches = QGridLayout()
        self.swatches.setHorizontalSpacing(10)
        self.swatches.setVerticalSpacing(6)
        box = QWidget()
        outer = QVBoxLayout(box)
        outer.setContentsMargins(0, 0, 8, 0)
        outer.addLayout(self.swatches)
        outer.addStretch()
        sc = QScrollArea()
        sc.setWidgetResizable(True)
        sc.setFrameShape(QFrame.Shape.NoFrame)
        sc.setWidget(box)
        v.addWidget(sc, 1)
        return page

    def _fill_colours(self, reload: bool = True):
        widgets.clear_layout(self.swatches)
        if reload:
            self._scheme = scheme()
            d = scheme_file()
            self._scheme_id = scheme_id(d)
            wp = wallpaper()
            if wp:
                px = QPixmap(wp)
                if not px.isNull():
                    self.wall.setPixmap(px.scaled(self.wall.size(), Qt.AspectRatioMode.KeepAspectRatioByExpanding,
                                                  Qt.TransformationMode.SmoothTransformation))
            else:
                self.wall.setText("No wallpaper found")
        cols = self._scheme
        saved = self.m.saved_for_scheme(self._scheme_id)
        self.scheme_name.setText(self._scheme_id.title() if cols else "No Caelestia scheme")
        self.scheme_info.setText(
            f"{len(cols)} colours · {SCHEME_PATH}" + (f" · {len(saved)} kept for this scheme" if saved else "")
            if cols else "Caelestia's colour scheme was not found (is Caelestia installed and running?).")
        q = self.colour_search.text().strip().lower()
        shown = [(k, c) for k, c in sorted(cols.items()) if not q or q in k.lower() or q in nice(k).lower()]
        for i, (k, c) in enumerate(shown):
            now = self._edits.get(k, c)
            sw = colourpick.ColourSwatch(now, nice(k), width=118)
            sw.changed.connect(lambda val, key=k: self._edit_colour(key, val))
            name = widgets.label(nice(k), 11.5, tone="primary" if k in self._edits else "secondary")
            if k in self._edits:
                name.setText(f"{nice(k)}  ●")
                name.setToolTip(f"Changed from {c}")
            self.swatches.addWidget(sw, i // 2, (i % 2) * 2)
            self.swatches.addWidget(name, i // 2, (i % 2) * 2 + 1)
        self.swatches.setColumnStretch(1, 1)
        self.swatches.setColumnStretch(3, 1)
        n = len(self._edits)
        self.apply_btn.setEnabled(bool(n))
        self.undo_btn.setEnabled(bool(n))
        self.colour_status.setText(f"{n} colour{'s' if n != 1 else ''} changed, not applied yet." if n else "")

    def _edit_colour(self, key: str, val):
        if not isinstance(val, str):
            return
        if val.upper() == self._scheme.get(key, "").upper():
            self._edits.pop(key, None)
        else:
            self._edits[key] = val.upper()
        self._fill_colours(reload=False)

    def _apply_colours(self):
        err = write_colours(self._edits)
        if err:
            widgets.error(self, "Caelestia", err)
            return
        n = len(self._edits)
        self._edits.clear()
        self._fill_colours()
        self.colour_status.setText(f"Applied {n} colour{'s' if n != 1 else ''}. The old scheme is in "
                                   "data/caelestia-backups.")

    def _save_menu(self, anchor: QWidget):
        m = widgets.menu(self)
        changed = {**self.m.saved_for_scheme(self._scheme_id), **self._edits}
        a = m.addAction(f"This Caelestia scheme ({self._scheme_id or 'none'})",
                        lambda: self._keep_for_scheme(changed))
        a.setEnabled(bool(self._scheme_id and changed))
        launcher = self.m.ctx.launcher
        if launcher is not None:
            sub = m.addMenu("A Luminescent theme")
            for t in launcher.themes.themes():
                sub.addAction(t["name"], lambda tid=t["id"]: self._keep_for_theme(tid))
        m.addSeparator()
        f = m.addAction("Forget the colours kept for this scheme", self._forget_scheme)
        f.setEnabled(bool(self.m.saved_for_scheme(self._scheme_id)))
        m.exec(anchor.mapToGlobal(anchor.rect().bottomLeft()))

    def _keep_for_scheme(self, changed: dict):
        self.m.save_for_scheme(self._scheme_id, changed)
        if self._edits:
            self._apply_colours()
        self.colour_status.setText(f"Kept {len(changed)} colours for {self._scheme_id}. They come back whenever "
                                   "Caelestia uses this scheme again.")
        self._fill_colours()

    def _forget_scheme(self):
        self.m.save_for_scheme(self._scheme_id, {})
        self._fill_colours()

    def _keep_for_theme(self, tid: str):
        launcher = self.m.ctx.launcher
        t = launcher.themes.get(tid) if launcher is not None else None
        if t is None:
            return
        import copy
        t = copy.deepcopy(t)
        t["caelestia"] = {"colours": {**self._scheme, **self._edits}}
        launcher.themes.update_theme(t)
        self.colour_status.setText(f"Kept for the theme {t['name']}: Caelestia gets these colours whenever it is "
                                   "applied.")


class CaelestiaModule(Module):
    key = "caelestia"
    title = "Caelestia & Glass"
    icon = "◐"

    def __init__(self, ctx):
        super().__init__(ctx)
        self.panel: CaelestiaPanel | None = None
        self._hooked = False
        self._watch: QFileSystemWatcher | None = None
        self._recheck = QTimer(self)
        self._recheck.setSingleShot(True)
        self._recheck.setInterval(600)
        self._recheck.timeout.connect(self._scheme_changed)

    def start(self):
        self.hotkey("hotkey", self.toggle, "Caelestia & Glass")
        self.hotkey("lock_hotkey", self.lock_screen, "Lock the screen")
        if look.available():
            launcher = self.ctx.launcher
            look.write_state(((launcher.themes.active().get("caelestia") or {}).get("extras"))
                             if launcher is not None else {})
            if self.s("extras", False) and look.mirror_outdated():
                # caelestia was updated: link its new files in and patch them again
                ok, msg = look.build_mirror()
                self.log.info("caelestia extras rebuilt after an update: %s", msg or "ok") if ok else \
                    self.log.warning("caelestia extras: %s", msg)
        sync = hypr.sync()
        if sync is not None and not self._hooked:
            sync.reloaded.connect(lambda: QTimer.singleShot(500, self.apply_glass))   # a reload drops the rules
            self._hooked = True
        QTimer.singleShot(1500, self.apply_glass)
        if not IS_WIN and os.path.isdir(STATE_DIR) and self._watch is None:
            self._watch = QFileSystemWatcher(self)
            self._watch.addPath(STATE_DIR)
            if os.path.isfile(SCHEME_PATH):
                self._watch.addPath(SCHEME_PATH)
            self._watch.fileChanged.connect(lambda _p: self._recheck.start())
            self._watch.directoryChanged.connect(lambda _p: self._recheck.start())

    def stop(self):
        if self._watch is not None:
            self._watch.deleteLater()
            self._watch = None
        self.apply_glass(off=True)
        if self.panel:
            self.panel.close()
            self.panel.deleteLater()
            self.panel = None

    def retheme(self):
        if self.panel and not self.panel.isVisible():
            self.stop()
            self.start()

    def actions(self):
        return [("open", "Caelestia shell settings and glass", self.toggle),
                ("lock", "Lock the screen (Caelestia's lock screen)", self.lock_screen)]

    def tray_actions(self):
        return [("Caelestia & Glass", self.toggle), ("Lock the screen", self.lock_screen)]

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

    def theme_glass(self) -> dict:
        """The active theme's own glass (every app's focused / unfocused opacity and the blur), {} for none."""
        launcher = self.ctx.launcher
        t = launcher.themes.active() if launcher is not None else {}
        g = t.get("glass") if isinstance(t.get("glass"), dict) else {}
        return {k: v for k, v in g.items() if k in ("active", "inactive", "blur_size", "blur_passes")
                and isinstance(v, (int, float)) and not isinstance(v, bool)}

    def every_app(self) -> tuple[float, float]:
        """Focused and unfocused opacity for every app without a rule of its own; 0 = Hyprland's own."""
        g = self.theme_glass()
        if "active" in g or "inactive" in g:
            return float(g.get("active", 1.0)), float(g.get("inactive", 1.0))
        return float(self.s("all_active", 0) or 0), float(self.s("all_inactive", 0) or 0)

    def _hypr_option(self, name: str):
        out = hypr.request(f"j/getoption {name}")
        try:
            d = json.loads(out)
        except ValueError:
            return None
        return d.get("float", d.get("int"))

    def _set_decoration(self, values: dict) -> str:
        """Change Hyprland decoration options for this session, remembering what they were so they can go
        back. values: {option: number, or None for Hyprland's own}."""
        if not hasattr(self, "_orig"):
            self._orig = {}
        out = {}
        for name, v in values.items():
            if v is None:
                if name in self._orig:
                    out[name] = self._orig.pop(name)
                continue
            if name not in self._orig:
                was = self._hypr_option(f"decoration:{name.replace('.', ':')}")
                if was is None:
                    continue
                self._orig[name] = was
            out[name] = v
        if not out:
            return ""
        groups: dict = {}
        for name, v in out.items():
            *path, leaf = name.split(".")
            d = groups
            for p in path:
                d = d.setdefault(p, {})
            d[leaf] = v

        def lua_table(d):
            return "{ " + ", ".join(f"{k} = {lua_table(v) if isinstance(v, dict) else v}" for k, v in d.items()) + " }"
        return hypr.lua(f"hl.config({{ decoration = {lua_table(groups)} }})")

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
        g = self.theme_glass()
        size = int(g.get("blur_size", self.s("blur_size", 0)) or 0)
        passes = int(g.get("blur_passes", self.s("blur_passes", 0)) or 0)
        active, inactive = (0.0, 0.0) if off else self.every_app()
        err = self._set_decoration({
            "blur.size": None if off else (size or None), "blur.passes": None if off else (passes or None),
            "active_opacity": round(active, 2) or None, "inactive_opacity": round(inactive, 2) or None})
        if err:
            return f"The window rules are on, but Hyprland did not take the blur or every app's opacity: {err}"
        return ""

    # ── colours ────────────────────────────────────────────────────────────
    def saved_for_scheme(self, sid: str) -> dict:
        d = self.s("scheme_colours", {}) or {}
        return dict(d.get(sid) or {}) if sid and isinstance(d, dict) else {}

    def save_for_scheme(self, sid: str, colours: dict):
        d = dict(self.s("scheme_colours", {}) or {})
        if colours:
            d[sid] = {k: v for k, v in colours.items() if is_colour(v)}
        else:
            d.pop(sid, None)
        self.settings.set("caelestia.scheme_colours", d)
        self.settings.save(emit=False)

    def _scheme_changed(self):
        """scheme.json changed (Caelestia switched schemes or made one from a wallpaper): put kept colours back."""
        if self._watch is not None and os.path.isfile(SCHEME_PATH) and SCHEME_PATH not in self._watch.files():
            self._watch.addPath(SCHEME_PATH)          # a file replaced in one step drops out of the watch
        d = scheme_file()
        now = scheme()
        kept = self.saved_for_scheme(scheme_id(d)) if self.s("keep_scheme_colours", True) else {}
        # the active theme's own colours win over the ones kept for the scheme, and both go in one write so
        # they can never take turns overwriting each other
        launcher = self.ctx.launcher
        active = launcher.themes.active() if launcher is not None else {}
        mine = {k: v for k, v in (((active.get("caelestia") or {}).get("colours")) or {}).items() if is_colour(v)}
        todo = {k: v for k, v in {**kept, **mine}.items() if k in now and now[k].upper() != v.upper()}
        if mine:
            # caelestia made new colours under the theme's, those are what a theme without its own puts back
            before = dict(self.s("before_theme", {}) or {})
            before.update({k: now[k] for k in mine if k in todo})
            self.settings.set("caelestia.before_theme", before)
            self.settings.save(emit=False)
        if todo:
            err = write_colours(todo)
            if err:
                self.log.warning("could not put the kept Caelestia colours back: %s", err)
            else:
                self.log.info("put %d colours back into Caelestia's %s scheme", len(todo), scheme_id(d))
                launcher = self.ctx.launcher
                self.refresh_apps(getattr(launcher, "_push_terminal_colours", None))
        elif self.ctx.launcher is not None and hasattr(self.ctx.launcher, "_push_terminal_colours"):
            # caelestia gave the terminals its colours again: the theme's go back on top (when that is on)
            QTimer.singleShot(500, self.ctx.launcher._push_terminal_colours)
        if self.panel is not None and self.panel.isVisible() and self.panel.pages.currentIndex() == 2 \
                and not self.panel._edits:
            self.panel._fill_colours()

    def apply_look(self, t: dict) -> str:
        """The theme's Caelestia shell settings and extras (see caelestia_look). '' or an error."""
        if not look.available():
            return ""
        cael = t.get("caelestia") or {}
        changes, before = look.plan_shell(look.clean(cael.get("shell")), dict(self.s("before_shell", {}) or {}))
        err = look.write_shell(changes, backup)
        if not err:
            self.settings.set("caelestia.before_shell", before)
            self.settings.save(emit=False)
            if changes:
                self.log.info("gave Caelestia %d shell settings for the theme %s", len(changes), t.get("name", ""))
        return err or look.write_state(cael.get("extras"))

    def set_extras(self, on: bool) -> str:
        """Turn the Caelestia extras (pictures for the lyrics and the lock screen) on or off. Caelestia restarts
        once to pick it up. '' or an error."""
        if on:
            launcher = self.ctx.launcher
            look.write_state(((launcher.themes.active().get("caelestia") or {}).get("extras"))
                             if launcher is not None else {})
            ok, msg = look.build_mirror()
            if not ok:
                return msg
            if msg:
                self.ctx.notify(self.title, msg)
        else:
            err = look.remove_mirror()
            if err:
                return err
        self.settings.set("caelestia.extras", on)
        self.settings.save(emit=False)
        return look.restart_shell()

    def lock_screen(self):
        err = look.lock_now()
        if err:
            self.ctx.notify(self.title, f"Could not lock: {err}")

    def refresh_apps(self, then=None):
        """Have Caelestia re-colour the apps it themes (terminal, btop, GTK, Qt...) from scheme.json, the way it
        does when it switches scheme itself; Luminescent only writes scheme.json, which only the shell watches.
        `then()` runs once it is done."""
        exe = shutil.which("caelestia")
        if not exe or not self.s("theme_apps", True) or os.environ.get("LUMINESCENT_HOME"):
            if then:
                then()
            return
        try:
            with open(exe, "r", encoding="utf-8", errors="replace") as f:
                first = f.readline().strip()
        except OSError:
            first = ""
        py = first[2:].strip().split()[0] if first.startswith("#!") else "python3"
        if py.endswith("/env"):
            py = "python3"
        code = ("import json, sys\n"
                "from caelestia.utils.theme import apply_colours\n"
                "d = json.load(open(sys.argv[1], encoding='utf-8'))\n"
                "c = d.get('colours') or d.get('colors') or {}\n"
                "apply_colours({k: str(v).lstrip('#') for k, v in c.items()}, d.get('mode') or 'dark')\n")
        from PyQt6.QtCore import QProcess
        proc = QProcess(self)

        def done(*_a):
            if proc.exitCode() != 0:
                self.log.warning("Caelestia could not re-colour its apps: %s",
                                 bytes(proc.readAllStandardError()).decode(errors="replace")[-300:])
            proc.deleteLater()
            if then:
                then()
        proc.finished.connect(done)
        proc.start(py, ["-c", code, SCHEME_PATH])

    def draws_wallpaper(self) -> bool:
        return self.running and not IS_WIN and bool(shutil.which("caelestia")) and wallpaper_tool.caelestia_draws()

    def set_wallpaper(self, path: str, then=None):
        """Caelestia's own wallpaper (it may make a new scheme from it). `then()` runs once it is on, so a theme's
        colours go on after the wallpaper's, never under them."""
        from PyQt6.QtCore import QProcess
        old = scheme()
        proc = QProcess(self)

        def done(*_a):
            if proc.exitCode() != 0:
                self.log.warning("Caelestia could not switch the wallpaper: %s",
                                 bytes(proc.readAllStandardError()).decode(errors="replace")[-300:])
            else:
                self.log.info("wallpaper set to %s (Caelestia)", path)
                wallpaper_tool.stop_swaybg()
                if scheme() != old:
                    # caelestia made new colours from the picture: those are now what a theme without colours of
                    # its own shows, not the ones from the old wallpaper
                    self.settings.set("caelestia.before_theme", {})
                    self.settings.save(emit=False)
            proc.deleteLater()
            if then:
                then()
        proc.finished.connect(done)
        proc.start("caelestia", ["wallpaper", "-f", path])

    def theme_applied(self, t: dict, then=None):
        """A Luminescent theme was applied: give Caelestia that theme's colours, shell settings and extras, and
        put back what an earlier theme changed that this one leaves alone. `then()` runs after Caelestia's apps
        have their new colours (at once when nothing changed)."""
        if IS_WIN:
            return
        err = self.apply_look(t)
        if err:
            self.ctx.notify(self.title, err)
        self.apply_glass()
        cols = {k: v for k, v in (((t.get("caelestia") or {}).get("colours")) or {}).items() if is_colour(v)}
        now = scheme()
        if not now:
            if then:
                then()
            return
        # what caelestia had before a theme changed it, so a theme without colours of its own can put it back
        before = {k: v for k, v in (self.s("before_theme", {}) or {}).items() if is_colour(v)}
        todo = {k: v for k, v in before.items() if k not in cols}
        before = {k: v for k, v in before.items() if k in cols}
        for k, v in cols.items():
            if k in now:
                before.setdefault(k, now[k])
                todo[k] = v
        todo = {k: v for k, v in todo.items() if k in now and now[k].upper() != v.upper()}
        self.settings.set("caelestia.before_theme", before)
        self.settings.save(emit=False)
        if todo:
            err = write_colours(todo)
            if err:
                self.ctx.notify(self.title, err)
            else:
                self.log.info("gave Caelestia %d colours for the theme %s", len(todo), t.get("name", ""))
                self.refresh_apps(then)
                return
        if then:
            then()

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
