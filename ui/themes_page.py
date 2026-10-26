"""
Themes page: a grid of theme cards grouped in colour-coded folders.

Click a card to apply it. Right-click → Settings… (every colour on a full colour
wheel, font, corner radius, glow, background image + its opacity, icon,
wallpaper, settings profile, module accents), Rename, Duplicate, Export, Move to
folder, Delete. The toolbar imports a .lumtheme file or makes a whole theme from
one colour. Themes outside a folder are listed under BUILT-IN (Dark, Light) and MY THEMES.
"""
import os

from PyQt6.QtWidgets import (
    QWidget, QFrame, QLabel, QVBoxLayout, QHBoxLayout, QGridLayout, QScrollArea, QDialog, QFormLayout,
    QLineEdit, QSpinBox, QCheckBox, QComboBox, QFontComboBox, QColorDialog, QFileDialog, QPushButton, QSlider
)
from PyQt6.QtCore import Qt, QRectF, QTimer
from PyQt6.QtGui import QPainter, QPainterPath, QColor, QPixmap, QFont

from core import theme, widgets
from core.themes import ThemeStore, new_theme

COLS, GAP = 5, 16
PREV_W, PREV_H = 190, 112
IMAGE_FILTER = "Images (*.png *.jpg *.jpeg *.bmp *.gif *.webp);;All files (*)"
BUILTIN_GROUP, MINE_GROUP = "builtin", "mine"
MINE_COLOUR = "#3DBE96"


def _preview(t: dict, w: int = PREV_W, h: int = PREV_H) -> QPixmap:
    """The card picture: the theme's icon, else its background image, else a little mock-up in its colours."""
    px = QPixmap(w, h)
    px.fill(QColor(0, 0, 0, 0))
    p = QPainter(px)
    p.setRenderHint(QPainter.RenderHint.Antialiasing)
    c = t.get("colours", {})
    rad = max(0, int(t.get("radius", 10)) - 4)
    clip = QPainterPath()
    clip.addRoundedRect(QRectF(0, 0, w, h), rad, rad)
    for key in ("icon", "background_image"):
        path = t.get(key) or ""
        if path and os.path.isfile(path):
            img = QPixmap(path)
            if not img.isNull():
                img = img.scaled(w, h, Qt.AspectRatioMode.KeepAspectRatioByExpanding,
                                 Qt.TransformationMode.SmoothTransformation)
                p.setClipPath(clip)
                p.fillRect(0, 0, w, h, QColor(c.get("bg_base", "#000")))
                p.drawPixmap((w - img.width()) // 2, (h - img.height()) // 2, img)
                p.end()
                return px
    bg = QColor(c.get("bg_base", "#131317"))
    card = QColor(c.get("bg_card", "#1B1B21"))
    prim = QColor(c.get("primary", "#7B6CF6"))
    text = QColor(c.get("text_primary", "#ECECF1"))
    p.setPen(Qt.PenStyle.NoPen)
    p.setBrush(bg)
    p.drawPath(clip)
    s = h / 112.0                                   # the mock-up is drawn for a 112 px tall card
    pad = 10 * s
    # top row: accent dot, a title bar, then two pills (accent + card)
    cy = pad + 4 * s
    p.setBrush(prim)
    p.drawEllipse(QRectF(pad, cy - 3 * s, 6 * s, 6 * s))
    text.setAlphaF(0.5)
    p.setBrush(text)
    p.drawRoundedRect(QRectF(pad + 12 * s, cy - 2 * s, 34 * s, 4 * s), 2 * s, 2 * s)
    p.setBrush(card)
    p.drawRoundedRect(QRectF(w - pad - 16 * s, cy - 4 * s, 16 * s, 8 * s), 4 * s, 4 * s)
    p.setBrush(prim)
    p.drawRoundedRect(QRectF(w - pad - 16 * s - 6 * s - 22 * s, cy - 4 * s, 22 * s, 8 * s), 4 * s, 4 * s)
    # the accent tick under the header
    ly = pad + 8 * s + 8 * s
    p.drawRect(QRectF(pad, ly, 24 * s, max(1.0, 1 * s)))
    # three cards, the last one shorter
    top = ly + 1 * s + 8 * s
    avail_h = h - pad - top
    cw = (w - 2 * pad - 2 * 6 * s) / 3
    p.setBrush(card)
    for i in range(3):
        ch = avail_h * (0.6 if i == 2 else 1.0)
        p.drawRoundedRect(QRectF(pad + i * (cw + 6 * s), top, cw, ch), 4 * s, 4 * s)
    p.end()
    return px


class ThemeCard(QFrame):
    def __init__(self, page: "ThemesPage", t: dict, active: bool, width: int):
        super().__init__()
        self.page = page
        self.t = t
        self.setObjectName("themeCard")
        self.setProperty("active", active)
        self.setFixedWidth(width)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        lay = QVBoxLayout(self)
        b = 7 if active else 8                      # the active card's 2 px border eats a pixel
        lay.setContentsMargins(b, b, b, b)
        lay.setSpacing(10)
        img = QLabel()
        pw = width - 18
        img.setPixmap(_preview(t, pw, PREV_H))
        img.setFixedSize(pw, PREV_H)
        lay.addWidget(img)
        row = QHBoxLayout()
        row.setContentsMargins(4, 0, 4, 2)
        row.setSpacing(8)
        name = widgets.label(t["name"], 13, 500)
        row.addWidget(name, 1)
        check = QLabel("✓" if active else "")
        check.setFixedSize(18, 18)
        check.setAlignment(Qt.AlignmentFlag.AlignCenter)
        if active:
            check.setStyleSheet(f"background: {theme.P.primary}; color: {theme.P.on_primary}; border-radius: 9px;"
                                f" font-size: {theme.fs(10)}px;")
        row.addWidget(check)
        lay.addLayout(row)
        for w in (img, name, check):
            w.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        tip = "Built-in theme" if t.get("builtin") else ""
        if t.get("profile"):
            tip += f"\nSettings profile: {page.launcher.settings.profile_name(t['profile'])}"
        self.setToolTip((tip + "\nClick to use · right-click for settings").strip())

    def mouseReleaseEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton and self.rect().contains(e.position().toPoint()):
            self.page.apply(self.t["id"])

    def contextMenuEvent(self, e):
        self.page.card_menu(self.t, e.globalPos())


class AddCard(QFrame):
    """The dashed "＋ New theme" tile at the end of each group."""

    def __init__(self, page: "ThemesPage", folder_id: str, width: int, height: int):
        super().__init__()
        self.page = page
        self.folder_id = folder_id
        self.setObjectName("addCard")
        self.setFixedSize(width, height)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        lay = QVBoxLayout(self)
        lbl = widgets.label("＋ New theme", 13, 500, tone="muted")
        lbl.setAlignment(Qt.AlignmentFlag.AlignCenter)
        lbl.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        lay.addWidget(lbl)

    def mouseReleaseEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton and self.rect().contains(e.position().toPoint()):
            self.page.new_theme(self.folder_id)


class FolderHeader(QFrame):
    def __init__(self, page: "ThemesPage", key: str, name: str, colour: str, count: int, collapsed: bool,
                 folder: dict | None = None):
        super().__init__()
        self.page = page
        self.key = key
        self.folder = folder
        self.setObjectName("folderHeader")
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.setFixedHeight(36)
        lay = QHBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 8)
        lay.setSpacing(10)
        chev = widgets.label("▸" if collapsed else "▾", 9, tone="muted")
        chev.setFixedWidth(10)
        lay.addWidget(chev)
        chip = QFrame()
        chip.setFixedSize(8, 8)
        chip.setStyleSheet(f"background: {colour}; border-radius: 2px;")
        lay.addWidget(chip)
        lay.addWidget(widgets.label(name.upper(), 11, 600, mono=True, spacing=0.14))
        lay.addWidget(widgets.label(f"{count:02d}", 10, 500, mono=True, tone="muted"))
        lay.addStretch()

    def mouseReleaseEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton:
            self.page.toggle_group(self.key, self.folder)

    def contextMenuEvent(self, e):
        if self.folder:
            self.page.folder_menu(self.folder, e.globalPos())


class ThemesPage(QWidget):
    def __init__(self, launcher):
        super().__init__()
        self.launcher = launcher
        self.store: ThemeStore = launcher.themes
        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(0)
        bar = QHBoxLayout()
        bar.setContentsMargins(24, 0, 24, 0)
        bar.setSpacing(12)
        head = QWidget()
        head.setFixedHeight(52)
        head.setLayout(bar)
        bar.addWidget(widgets.label("Themes", 15, 600))
        self.active_lbl = widgets.label("", 10, 500, mono=True, spacing=0.06, tone="muted")
        bar.addWidget(self.active_lbl)
        bar.addStretch()
        imp = widgets.button("Import…", "ghost", "Add a theme someone shared (.lumtheme)")
        imp.setFixedHeight(30)
        imp.clicked.connect(self.import_theme)
        gen = widgets.button("From a colour…", "ghost", "Pick one colour; every other colour is chosen to match it")
        gen.setFixedHeight(30)
        gen.clicked.connect(self.theme_from_colour)
        bar.addWidget(imp)
        bar.addWidget(gen)
        fold = widgets.button("New folder", "ghost")
        fold.setFixedHeight(30)
        fold.clicked.connect(self.new_folder)
        add = widgets.button("＋ New theme", "primary")
        add.setFixedHeight(30)
        add.setToolTip("A copy of the active theme. Right-click any card → Settings… to change it.")
        add.clicked.connect(lambda: self.new_theme(""))
        bar.addWidget(fold)
        bar.addWidget(add)
        lay.addWidget(head)
        self.scroll = QScrollArea()
        self.scroll.setWidgetResizable(True)
        self.scroll.setFrameShape(QFrame.Shape.NoFrame)
        self.scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        lay.addWidget(self.scroll, 1)
        self.store.changed.connect(self._later_refresh)
        self.refresh()

    def teardown(self):
        try:
            self.store.changed.disconnect(self._later_refresh)
        except TypeError:
            pass

    def _later_refresh(self):
        QTimer.singleShot(0, self.refresh)

    def _groups(self):
        """[(key, name, colour, folder or None, themes)]: BUILT-IN, MY THEMES, then the user's folders."""
        folders = {f["id"]: f for f in self.store.folders()}
        loose = [t for t in self.store.themes() if not t.get("folder") or t["folder"] not in folders]
        out = [(BUILTIN_GROUP, "Built-in", theme.PRESETS["dark"]["primary"], None,
                [t for t in loose if t.get("builtin")])]
        mine = [t for t in loose if not t.get("builtin")]
        if mine or not folders:
            out.append((MINE_GROUP, "My themes", MINE_COLOUR, None, mine))
        for f in self.store.folders():
            out.append((f["id"], f["name"], f.get("colour", theme.P.primary), f,
                        [t for t in self.store.themes() if t.get("folder") == f["id"]]))
        return out

    def refresh(self):
        holder = QWidget()
        v = QVBoxLayout(holder)
        v.setContentsMargins(24, 4, 24, 24)
        v.setSpacing(22)
        avail = (self.width() if self.width() > 300 else 1178) - 48 - 10
        card_w = max(150, (avail - GAP * (COLS - 1)) // COLS)
        active = self.store.data["active"]
        act = self.store.get(active)
        self.active_lbl.setText(f"ACTIVE: {act['name'].upper()}" if act else "")
        closed = set(self.store.data.get("collapsed_groups") or [])
        for key, name, colour, folder, themes in self._groups():
            collapsed = bool(folder.get("collapsed")) if folder else key in closed
            group = QVBoxLayout()
            group.setSpacing(14)
            group.addWidget(FolderHeader(self, key, name, colour, len(themes), collapsed, folder))
            if not collapsed:
                grid = QGridLayout()
                grid.setHorizontalSpacing(GAP)
                grid.setVerticalSpacing(GAP)
                grid.setAlignment(Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignTop)
                cards = [ThemeCard(self, t, t["id"] == active, card_w) for t in themes]
                for i, c in enumerate(cards):
                    grid.addWidget(c, i // COLS, i % COLS)
                h = max((c.sizeHint().height() for c in cards), default=160)
                n = len(cards)
                grid.addWidget(AddCard(self, folder["id"] if folder else "", card_w, h), n // COLS, n % COLS)
                group.addLayout(grid)
            v.addLayout(group)
        v.addStretch()
        old = self.scroll.takeWidget()
        self.scroll.setWidget(holder)
        if old is not None:
            old.deleteLater()

    def resizeEvent(self, e):
        super().resizeEvent(e)
        if abs(e.size().width() - e.oldSize().width()) > 40:
            self.refresh()

    # ── actions ────────────────────────────────────────────────────────────
    def toggle_group(self, key: str, folder: dict | None):
        if folder:
            self.store.update_folder(folder["id"], collapsed=not folder.get("collapsed"))
            return
        closed = set(self.store.data.get("collapsed_groups") or [])
        closed ^= {key}
        self.store.data["collapsed_groups"] = sorted(closed)
        self.store.save()
        self.refresh()

    def apply(self, tid: str):
        if tid != self.store.data["active"]:
            QTimer.singleShot(0, lambda: self.store.apply(tid))

    def new_theme(self, folder_id: str = ""):
        name, ok = widgets.ask_text(self, "New theme", "Name:")
        if ok and name.strip():
            t = new_theme(name.strip(), self.store.active())
            t["profile"] = ""
            t["folder"] = folder_id
            self.store.add(t)
            self.edit(t)

    def import_theme(self):
        path, _ = QFileDialog.getOpenFileName(self, "Import a theme", os.path.expanduser("~"),
                                              "Luminescent themes (*.lumtheme);;All files (*)")
        if not path:
            return
        try:
            t = self.store.import_file(path)
        except (ValueError, OSError) as e:
            widgets.error(self, "Import theme", str(e))
            return
        if widgets.confirm(self, "Import theme", f'Added "{t["name"]}". Use it now?', yes="Use it"):
            self.apply(t["id"])

    def export_theme(self, t: dict):
        safe = "".join(c for c in t["name"] if c not in '\\/:*?"<>|').strip() or "theme"
        path, _ = QFileDialog.getSaveFileName(self, "Export theme", os.path.join(os.path.expanduser("~"),
                                              safe + ".lumtheme"), "Luminescent themes (*.lumtheme)")
        if not path:
            return
        if not path.endswith(".lumtheme"):
            path += ".lumtheme"
        try:
            self.store.export(t["id"], path)
        except (ValueError, OSError) as e:
            widgets.error(self, "Export theme", str(e))
            return
        self.launcher.notify("Themes", f"Saved {path}")

    def theme_from_colour(self, colour: str = ""):
        if not colour:
            c = QColorDialog.getColor(QColor(theme.P.primary), self, "The theme's accent colour")
            if not c.isValid():
                return
            colour = c.name()
        mode = widgets.choose(self, "Theme from a colour", f"A dark or a light theme around {colour.upper()}?",
                              ["Dark", "Light"])
        if mode is None:
            return
        t = self.store.from_accent(colour, mode == "Light")
        self.apply(t["id"])

    def new_folder(self):
        name, ok = widgets.ask_text(self, "New folder", "Folder name:")
        if not ok or not name.strip():
            return
        c = QColorDialog.getColor(QColor(theme.P.primary), self, "Folder colour")
        self.store.add_folder(name, c.name() if c.isValid() else theme.P.primary)

    def edit(self, t: dict):
        dlg = ThemeDialog(self.launcher, t, self)
        if dlg.exec() == QDialog.DialogCode.Accepted:
            self.store.update_theme(dlg.result_theme())

    def card_menu(self, t: dict, pos):
        m = widgets.menu(self)
        m.addAction("Use this theme", lambda: self.apply(t["id"]))
        m.addAction("Settings…", lambda: self.edit(t))
        m.addAction("Rename…", lambda: self._rename(t))
        m.addAction("Duplicate", lambda: self.store.duplicate(t["id"]))
        m.addAction("Export to a file…", lambda: self.export_theme(t))
        sub = m.addMenu("Move to folder")
        sub.addAction("(no folder)", lambda: self.store.move_to_folder(t["id"], ""))
        for f in self.store.folders():
            sub.addAction(f["name"], lambda fid=f["id"]: self.store.move_to_folder(t["id"], fid))
        if t.get("builtin"):
            m.addAction("Reset to the original colours", lambda: self.store.reset_builtin(t["id"]))
        m.addSeparator()
        d = m.addAction("Delete…", lambda: self._delete(t))
        d.setEnabled(self.store.can_delete(t["id"]))
        if t.get("builtin"):
            d.setText("Delete… (built-in themes stay)")
        m.exec(pos)

    def folder_menu(self, f: dict, pos):
        m = widgets.menu(self)
        m.addAction("Rename…", lambda: self._rename_folder(f))
        m.addAction("Colour…", lambda: self._folder_colour(f))
        m.addSeparator()
        m.addAction("Delete folder (keeps its themes)", lambda: self.store.delete_folder(f["id"]))
        m.exec(pos)

    def _rename(self, t):
        name, ok = widgets.ask_text(self, "Rename theme", "New name:", t["name"])
        if ok:
            self.store.rename(t["id"], name)

    def _delete(self, t):
        if widgets.confirm(self, "Delete theme", f'Delete the theme "{t["name"]}"?', yes="Delete", danger=True):
            self.store.delete(t["id"])

    def _rename_folder(self, f):
        name, ok = widgets.ask_text(self, "Rename folder", "New name:", f["name"])
        if ok and name.strip():
            self.store.update_folder(f["id"], name=name.strip())

    def _folder_colour(self, f):
        c = QColorDialog.getColor(QColor(f.get("colour", theme.P.primary)), self, "Folder colour")
        if c.isValid():
            self.store.update_folder(f["id"], colour=c.name())


class _Swatch(QPushButton):
    def __init__(self, colour: str, on_change):
        super().__init__()
        self._on_change = on_change
        self.setFixedSize(96, 26)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.clicked.connect(self._pick)
        self.set(colour)

    def set(self, colour: str):
        self.colour = colour
        txt = "#000" if QColor(colour).lightness() > 128 else "#fff"
        self.setText(colour.upper())
        self.setStyleSheet(f"QPushButton {{ background: {colour}; color: {txt}; border: 1px solid {theme.P.border};"
                           f" font-family: {theme.MONO_CSS}; padding: 0; }}")

    def _pick(self):
        dlg = QColorDialog(QColor(self.colour), self)
        dlg.setWindowTitle("Pick a colour")
        dlg.setOption(QColorDialog.ColorDialogOption.ShowAlphaChannel, False)
        before = self.colour
        dlg.currentColorChanged.connect(lambda c: (self.set(c.name()), self._on_change()))
        if dlg.exec() != QDialog.DialogCode.Accepted:
            self.set(before)
        self._on_change()


class ThemeDialog(QDialog):
    def __init__(self, launcher, t: dict, parent=None):
        super().__init__(parent)
        self.launcher = launcher
        self.t = {**t, "colours": dict(t.get("colours", {}))}
        self.setWindowTitle(f"Theme settings — {t['name']}")
        self.setWindowFlags(self.windowFlags() | Qt.WindowType.WindowStaysOnTopHint)
        self.resize(760, 640)
        root = QHBoxLayout(self)
        root.setContentsMargins(16, 16, 16, 16)
        root.setSpacing(16)

        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.Shape.NoFrame)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        host = QWidget()
        form = QFormLayout(host)
        form.setSpacing(7)
        scroll.setWidget(host)
        root.addWidget(scroll, 1)

        self.name = QLineEdit(t["name"])
        form.addRow("Name", self.name)
        head = widgets.label("COLOURS", 11, 600, mono=True, spacing=0.14)
        gen = widgets.button("Match everything to the accent", "ghost",
                             "Picks backgrounds, text and borders that go with the accent colour below")
        gen.clicked.connect(self._generate)
        form.addRow(head, gen)
        self.swatches = {}
        for role, label in theme.COLOUR_ROLES:
            sw = _Swatch(self.t["colours"].get(role, theme.PRESETS["dark"][role]), self._update_preview)
            self.swatches[role] = sw
            form.addRow(label, sw)

        head2 = widgets.label("LOOK", 11, 600, mono=True, spacing=0.14)
        form.addRow(head2)
        self.font = QFontComboBox()
        self.font.setCurrentFont(QFont(t.get("font_family", "Geist")))
        form.addRow("Font", self.font)
        self.radius = QSpinBox()
        self.radius.setRange(0, 30)
        self.radius.setValue(int(t.get("radius", 10)))
        form.addRow("Corner radius", self.radius)
        self.glow = QCheckBox("Accent glow around the overlay")
        self.glow.setChecked(bool(t.get("glow", True)))
        form.addRow("", self.glow)
        self.bg_image = self._path_row(form, "Background image", t.get("background_image", ""), IMAGE_FILTER,
                                       "Shown behind the overlay content.")
        op = QWidget()
        ol = QHBoxLayout(op)
        ol.setContentsMargins(0, 0, 0, 0)
        self.opacity = QSlider(Qt.Orientation.Horizontal)
        self.opacity.setRange(0, 100)
        self.opacity.setValue(int(float(t.get("image_opacity", 0.35)) * 100))
        olbl = QLabel()
        olbl.setFixedWidth(40)
        self.opacity.valueChanged.connect(lambda v: olbl.setText(f"{v}%"))
        olbl.setText(f"{self.opacity.value()}%")
        ol.addWidget(self.opacity, 1)
        ol.addWidget(olbl)
        form.addRow("Image opacity", op)
        self.icon = self._path_row(form, "Card icon", t.get("icon", ""), IMAGE_FILTER,
                                   "Shown on the theme card. Empty: the background image or a preview.")

        head_m = widgets.label("MODULE ACCENTS", 11, 600, mono=True, spacing=0.14)
        form.addRow(head_m)
        hint = QLabel("Give a module its own accent colour (e.g. Notes in green). Empty: the theme's accent.")
        hint.setObjectName("lumHint")
        hint.setWordWrap(True)
        form.addRow(hint)
        self.module_swatches = {}
        mods = dict(self.t.get("modules") or {})
        for key, m in launcher.modules.items():
            if key in ("doodle", "colorpicker"):
                continue                      # no windows of their own to colour
            row = QHBoxLayout()
            row.setSpacing(6)
            sw = _Swatch(mods.get(key) or theme.P.primary, self._update_preview)
            sw.setProperty("unset", key not in mods)
            if key not in mods:
                sw.setText("THEME")
            sw.clicked.connect(lambda _=False, s=sw: self._module_picked(s))
            clear = widgets.icon_button("✕", "Use the theme's accent", 26, 11)
            clear.clicked.connect(lambda _=False, s=sw: (s.setProperty("unset", True), s.set(theme.P.primary),
                                                          s.setText("THEME")))
            row.addWidget(sw)
            row.addWidget(clear)
            row.addStretch()
            self.module_swatches[key] = sw
            form.addRow(f"{m.icon}  {m.title}", row)

        head3 = widgets.label("WHEN THIS THEME IS APPLIED", 11, 600, mono=True, spacing=0.14)
        form.addRow(head3)
        self.wallpaper = self._path_row(
            form, "Desktop wallpaper", t.get("wallpaper", ""),
            "Pictures or Wallpaper Engine projects (*.jpg *.jpeg *.png *.webp *.bmp project.json *.json *.pkg *.mp4);;"
            "All files (*)",
            "A picture becomes the desktop wallpaper (through Noctalia, swww, hyprpaper or swaybg). A Wallpaper "
            "Engine project.json (or a file in its folder) is played with linux-wallpaperengine. Empty: the "
            "wallpaper is left alone.")
        self.profile = QComboBox()
        self.profile.addItem("Default settings", "")
        for p in launcher.settings.profiles():
            if p["id"] != "default":
                self.profile.addItem(f"Profile: {p['name']}", p["id"])
        self.profile.setCurrentIndex(max(0, self.profile.findData(t.get("profile", ""))))
        form.addRow("Settings profile", self.profile)
        pl = QLabel("Profiles are made in Settings (top of the page). Each holds its own hotkeys, sizes, sounds…")
        pl.setObjectName("lumHint")
        pl.setWordWrap(True)
        form.addRow("", pl)

        side = QVBoxLayout()
        side.setSpacing(8)
        side.addWidget(widgets.label("PREVIEW", 11, 600, mono=True, spacing=0.14))
        self.preview = QLabel()
        self.preview.setFixedSize(PREV_W * 2, PREV_H * 2)
        side.addWidget(self.preview)
        side.addStretch()
        btns = QHBoxLayout()
        cancel = widgets.button("Cancel")
        cancel.clicked.connect(self.reject)
        ok = widgets.button("Save", "primary")
        ok.setDefault(True)
        ok.clicked.connect(self.accept)
        btns.addWidget(cancel)
        btns.addWidget(ok)
        side.addLayout(btns)
        root.addLayout(side)
        self._update_preview()
        QTimer.singleShot(0, lambda: widgets.bring_to_front(self))

    def _path_row(self, form, label, value, flt, help_text) -> QLineEdit:
        w = QWidget()
        lay = QVBoxLayout(w)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(2)
        row = QHBoxLayout()
        edit = QLineEdit(value)
        edit.textChanged.connect(self._update_preview)
        b = widgets.button("Browse…")
        b.clicked.connect(lambda: self._browse(edit, flt))
        c = widgets.icon_button("✕", "Clear", 26, 11)
        c.clicked.connect(lambda: edit.setText(""))
        row.addWidget(edit, 1)
        row.addWidget(b)
        row.addWidget(c)
        lay.addLayout(row)
        h = QLabel(help_text)
        h.setObjectName("lumHint")
        h.setWordWrap(True)
        lay.addWidget(h)
        form.addRow(label, w)
        return edit

    def _browse(self, edit: QLineEdit, flt: str):
        p, _ = QFileDialog.getOpenFileName(self, "Choose a file", edit.text(), flt)
        if p:
            edit.setText(p)

    def result_theme(self) -> dict:
        t = dict(self.t)
        t["name"] = self.name.text().strip() or t["name"]
        t["colours"] = {role: sw.colour for role, sw in self.swatches.items()}
        t.update({"font_family": self.font.currentFont().family(), "radius": self.radius.value(),
                  "glow": self.glow.isChecked(), "background_image": self.bg_image.text().strip(),
                  "image_opacity": self.opacity.value() / 100.0, "icon": self.icon.text().strip(),
                  "wallpaper": self.wallpaper.text().strip(), "profile": self.profile.currentData() or ""})
        if hasattr(self, "module_swatches"):
            t["modules"] = {k: sw.colour for k, sw in self.module_swatches.items() if not sw.property("unset")}
        return t

    @staticmethod
    def _module_picked(sw: _Swatch):
        # runs after the colour dialog: a cancelled pick leaves the module on the theme's accent
        if sw.property("unset") and sw.colour.upper() == theme.P.primary.upper():
            sw.setText("THEME")
        else:
            sw.setProperty("unset", False)

    def _generate(self):
        accent = self.swatches["primary"].colour
        light = theme.luminance(self.swatches["bg_base"].colour) > 140
        for role, value in theme.generate(accent, light).items():
            if role in self.swatches:
                self.swatches[role].set(value)
        self._update_preview()

    def _update_preview(self, *_):
        if not hasattr(self, "preview"):
            return
        self.preview.setPixmap(_preview(self.result_theme(), PREV_W * 2, PREV_H * 2))
