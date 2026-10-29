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
    QLineEdit, QCheckBox, QComboBox, QFontComboBox, QFileDialog, QPushButton, QSlider, QStackedWidget
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
            from core.colorpick import pick_colour
            colour = pick_colour(self, theme.P.primary, "The theme's accent colour")
            if not colour:
                return
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
        from core.colorpick import pick_colour
        self.store.add_folder(name, pick_colour(self, theme.P.primary, "Folder colour") or theme.P.primary)

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
        from core.colorpick import pick_colour
        c = pick_colour(self, f.get("colour", theme.P.primary), "Folder colour")
        if c:
            self.store.update_folder(f["id"], colour=c)


ROLE_GROUPS = [
    ("ACCENT", ["primary", "primary_hover", "primary_muted", "primary_text", "on_primary"]),
    ("BACKGROUNDS", ["bg_base", "bg_card", "bg_card_hover", "bg_frame", "bg_header", "border"]),
    ("TEXT", ["text_primary", "text_secondary", "text_muted"]),
    ("FOLDERS", ["folder_color", "folder_border"]),
]
ACCENT_SHADES = ("primary_hover", "primary_muted", "primary_text", "on_primary")


class _RoleRow(QFrame):
    """One colour of the theme in the list on the left: chip, name and code. Click to edit it."""

    def __init__(self, dlg: "ThemeDialog", role: str, label: str):
        super().__init__()
        self.dlg, self.role = dlg, role
        self.setObjectName("roleRow")
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.setFixedHeight(34)
        h = QHBoxLayout(self)
        h.setContentsMargins(8, 0, 10, 0)
        h.setSpacing(10)
        self.chip = QFrame()
        self.chip.setFixedSize(20, 20)
        h.addWidget(self.chip)
        self.name = widgets.label(label, 12.5)
        h.addWidget(self.name, 1)
        self.code = widgets.label("", 11, mono=True, tone="muted")
        h.addWidget(self.code)
        for w in (self.chip, self.name, self.code):
            w.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)

    def show_colour(self, c: str, selected: bool):
        # restyling is slow-ish and this runs while a colour is dragged: only what changed
        if c != getattr(self, "_c", None):
            self._c = c
            p = theme.P
            self.chip.setStyleSheet(f"background: {c}; border: 1px solid {theme.mix(p.border, p.text_muted, 0.3)};"
                                    f" border-radius: 5px;")
            self.code.setText(c.upper())
        if selected == getattr(self, "_sel", None):
            return
        self._sel = selected
        p = theme.P
        bg = p.primary_muted if selected else "transparent"
        edge = p.primary if selected else "transparent"
        self.setStyleSheet(f"#roleRow {{ background: {bg}; border: 1px solid {edge}; border-radius: {max(4, p.radius - 4)}px; }}"
                           f"#roleRow:hover {{ background: {p.primary_muted if selected else p.bg_card_hover}; }}")

    def mousePressEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton:
            self.dlg.select(self.role)


class ThemeDialog(QDialog):
    """Edit a theme: its colours (picked right here, with a live preview), look, desktop and module accents."""

    def __init__(self, launcher, t: dict, parent=None):
        super().__init__(parent)
        from core.colorpick import ColourPicker, ColourField
        self.launcher = launcher
        self.t = {**t, "colours": {**theme.PRESETS["dark"], **dict(t.get("colours", {}))}}
        self.colours = dict(self.t["colours"])
        self.setWindowTitle(f"Theme settings · {t['name']}")
        widgets.card_dialog(self)
        self.resize(1120, 720)
        self._role = "primary"
        self._sample_timer = QTimer(self)
        self._sample_timer.setSingleShot(True)
        self._sample_timer.setInterval(120)
        self._sample_timer.timeout.connect(self._update_sample)

        root = QVBoxLayout(self)
        root.setContentsMargins(18, 12, 18, 16)
        root.setSpacing(12)

        # name + tabs
        top = QHBoxLayout()
        top.setSpacing(10)
        self.name = QLineEdit(t["name"])
        self.name.setFixedHeight(34)
        self.name.setFixedWidth(260)
        self.name.setPlaceholderText("Theme name")
        top.addWidget(self.name)
        top.addSpacing(10)
        self.pages = QStackedWidget()
        self._tabs = []
        for i, title in enumerate(("Colours", "Look", "Desktop && Caelestia", "Module accents")):
            b = QPushButton(title)
            b.setObjectName("boardTab")
            b.setCheckable(True)
            b.setAutoDefault(False)
            b.setCursor(Qt.CursorShape.PointingHandCursor)
            b.clicked.connect(lambda _c=False, i=i: self._tab(i))
            top.addWidget(b)
            self._tabs.append(b)
        top.addStretch()
        root.addLayout(top)

        body = QHBoxLayout()
        body.setSpacing(18)
        body.addWidget(self.pages, 1)

        # the preview column, on every tab
        side = QVBoxLayout()
        side.setSpacing(10)
        side.addWidget(widgets.label("PREVIEW", 10.5, 600, mono=True, spacing=0.14, tone="muted"))
        self.preview = QLabel()
        self.preview.setFixedSize(int(PREV_W * 1.7), int(PREV_H * 1.7))
        side.addWidget(self.preview)
        self.sample = QFrame()
        self.sample.setObjectName("sampleBox")
        self.sample.setFixedWidth(int(PREV_W * 1.7))
        sl = QVBoxLayout(self.sample)
        sl.setContentsMargins(14, 12, 14, 14)
        sl.setSpacing(8)
        self.s_title = QLabel("Sample panel")
        self.s_text = QLabel("Secondary text reads like this.")
        self.s_text.setWordWrap(True)
        self.s_muted = QLabel("muted · hints · counts")
        sl.addWidget(self.s_title)
        sl.addWidget(self.s_text)
        sl.addWidget(self.s_muted)
        self.s_input = QLineEdit("An input box")
        sl.addWidget(self.s_input)
        row = QHBoxLayout()
        row.setSpacing(6)
        self.s_btn = QPushButton("Button")
        self.s_btn.setAutoDefault(False)
        self.s_acc = QPushButton("Accent")
        self.s_acc.setAutoDefault(False)
        self.s_check = QCheckBox("Ticked")
        self.s_check.setChecked(True)
        row.addWidget(self.s_btn)
        row.addWidget(self.s_acc)
        row.addWidget(self.s_check)
        row.addStretch()
        sl.addLayout(row)
        side.addWidget(self.sample)
        self.contrast = widgets.label("", 11.5, tone="muted")
        self.contrast.setWordWrap(True)
        self.contrast.setFixedWidth(int(PREV_W * 1.7))
        side.addWidget(self.contrast)
        side.addStretch()
        body.addLayout(side)
        root.addLayout(body, 1)

        # ── colours tab ──
        cpage = QWidget()
        ch = QHBoxLayout(cpage)
        ch.setContentsMargins(0, 0, 0, 0)
        ch.setSpacing(16)
        lst = QWidget()
        lv = QVBoxLayout(lst)
        lv.setContentsMargins(0, 0, 6, 0)
        lv.setSpacing(4)
        labels = dict(theme.COLOUR_ROLES)
        self.rows: dict[str, _RoleRow] = {}
        for title, roles in ROLE_GROUPS:
            lv.addSpacing(6)
            lv.addWidget(widgets.label(title, 10, 600, mono=True, spacing=0.14, tone="accent"))
            for role in roles:
                r = _RoleRow(self, role, labels.get(role, role))
                self.rows[role] = r
                lv.addWidget(r)
        lv.addStretch()
        sc = QScrollArea()
        sc.setWidgetResizable(True)
        sc.setFrameShape(QFrame.Shape.NoFrame)
        sc.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        sc.setWidget(lst)
        sc.setFixedWidth(290)
        ch.addWidget(sc)
        pv = QVBoxLayout()
        pv.setSpacing(10)
        self.role_title = widgets.label("", 15, 600)
        pv.addWidget(self.role_title)
        self.picker = ColourPicker(self.colours["primary"])
        self.picker.changed.connect(self._picked)
        pv.addWidget(self.picker, 1)
        self.follow = QCheckBox("The other accent colours follow the accent (hover, muted, text, text on accent)")
        self.follow.setChecked(True)
        pv.addWidget(self.follow)
        quick = QHBoxLayout()
        quick.setSpacing(8)
        gen = widgets.button("Match everything to the accent", "", "Picks backgrounds, text and borders that go "
                                                                   "with the accent colour")
        gen.setAutoDefault(False)
        gen.clicked.connect(self._generate)
        quick.addWidget(gen)
        flip = widgets.button("Swap dark / light", "ghost", "The same accent on a light background (or a dark one)")
        flip.setAutoDefault(False)
        flip.clicked.connect(self._flip)
        quick.addWidget(flip)
        undo = widgets.button("Undo all changes", "ghost")
        undo.setAutoDefault(False)
        undo.clicked.connect(self._undo_all)
        quick.addWidget(undo)
        quick.addStretch()
        pv.addLayout(quick)
        ch.addLayout(pv, 1)
        self.pages.addWidget(cpage)

        # ── look tab ──
        lpage, form = self._form_page()
        self.font = QFontComboBox()
        self.font.setCurrentFont(QFont(t.get("font_family", "Geist")))
        self.font.setFixedHeight(32)
        self.font.currentFontChanged.connect(self._changed)
        form.addRow(self._lbl("Font"), self.font)
        rad = QWidget()
        rl = QHBoxLayout(rad)
        rl.setContentsMargins(0, 0, 0, 0)
        self.radius = QSlider(Qt.Orientation.Horizontal)
        self.radius.setRange(0, 30)
        self.radius.setValue(int(t.get("radius", 10)))
        rlbl = widgets.label("", 12, mono=True)
        rlbl.setFixedWidth(46)
        self.radius.valueChanged.connect(lambda v: (rlbl.setText(f"{v} px"), self._changed()))
        rlbl.setText(f"{self.radius.value()} px")
        rl.addWidget(self.radius, 1)
        rl.addWidget(rlbl)
        form.addRow(self._lbl("Corner radius"), rad)
        self.glow = QCheckBox("Accent glow around the overlay")
        self.glow.setChecked(bool(t.get("glow", True)))
        form.addRow(self._lbl(""), self.glow)
        self.bg_image = self._path_row(form, "Background image", t.get("background_image", ""), IMAGE_FILTER,
                                       "Shown behind the overlay content.")
        op = QWidget()
        ol = QHBoxLayout(op)
        ol.setContentsMargins(0, 0, 0, 0)
        self.opacity = QSlider(Qt.Orientation.Horizontal)
        self.opacity.setRange(0, 100)
        self.opacity.setValue(int(float(t.get("image_opacity", 0.35)) * 100))
        olbl = widgets.label("", 12, mono=True)
        olbl.setFixedWidth(46)
        self.opacity.valueChanged.connect(lambda v: (olbl.setText(f"{v}%"), self._changed()))
        olbl.setText(f"{self.opacity.value()}%")
        ol.addWidget(self.opacity, 1)
        ol.addWidget(olbl)
        form.addRow(self._lbl("Image opacity"), op)
        self.icon = self._path_row(form, "Card icon", t.get("icon", ""), IMAGE_FILTER,
                                   "Shown on the theme card. Empty: the background image or a preview.")
        self.pages.addWidget(lpage)

        # ── desktop tab ──
        dpage, form = self._form_page()
        cae = dict(t.get("caelestia") or {})
        form.addRow(self._heading("CAELESTIA SHELL"))
        hint = widgets.label("With Settings → Caelestia → The shell follows the theme on, applying this theme recolours "
                             "the Caelestia bar, launcher, dashboard and notifications. Its two main colours:", 12,
                             tone="muted")
        hint.setWordWrap(True)
        form.addRow(hint)
        self.cae_fields = {}
        for key, label, auto in (("primary", "Primary", "Use the accent"), ("secondary", "Secondary", "Matched to the primary"),
                                 ("tertiary", "Third colour", "Matched to the primary")):
            line = QHBoxLayout()
            line.setSpacing(10)
            field = ColourField(cae.get(key) or "#7B6CF6", f"Caelestia {label.lower()}")
            box = QCheckBox(auto)
            box.setChecked(not cae.get(key))
            field.setEnabled(bool(cae.get(key)))
            box.toggled.connect(lambda on, f=field: (f.setEnabled(not on), self._update_shell()))
            field.changed.connect(lambda _c: self._update_shell())
            line.addWidget(field)
            line.addWidget(box)
            line.addStretch()
            self.cae_fields[key] = (field, box)
            form.addRow(self._lbl(label), line)
        self.shell_preview = QLabel()
        self.shell_preview.setFixedHeight(46)
        form.addRow(self._lbl("Shell colours"), self.shell_preview)
        form.addRow(self._heading("WHEN THIS THEME IS APPLIED"))
        self.wallpaper = self._path_row(
            form, "Desktop wallpaper", t.get("wallpaper", ""),
            "Pictures or Wallpaper Engine projects (*.jpg *.jpeg *.png *.webp *.bmp project.json *.json *.pkg *.mp4);;"
            "All files (*)",
            "A picture becomes the desktop wallpaper (through Noctalia, swww, hyprpaper or swaybg, or your own "
            "command in Settings). A Wallpaper Engine project.json is played with linux-wallpaperengine. Empty: "
            "the wallpaper is left alone.")
        self.profile = QComboBox()
        self.profile.setFixedHeight(32)
        self.profile.addItem("Default settings", "")
        for p in launcher.settings.profiles():
            if p["id"] != "default":
                self.profile.addItem(f"Profile: {p['name']}", p["id"])
        self.profile.setCurrentIndex(max(0, self.profile.findData(t.get("profile", ""))))
        form.addRow(self._lbl("Settings profile"), self.profile)
        pl = widgets.label("Profiles are made in Settings (top of the page). Each holds its own hotkeys, sizes, "
                           "sounds…", 12, tone="muted")
        pl.setWordWrap(True)
        form.addRow(self._lbl(""), pl)
        self.pages.addWidget(dpage)

        # ── module accents tab ──
        mpage, form = self._form_page()
        hint = widgets.label("Give a module its own accent colour (e.g. Notes in green). Untouched: the theme's "
                             "accent.", 12, tone="muted")
        hint.setWordWrap(True)
        form.addRow(hint)
        self.module_fields = {}
        mods = dict(self.t.get("modules") or {})
        for key, m in launcher.modules.items():
            if key in ("doodle", "colorpicker"):
                continue                      # no windows of their own to colour
            line = QHBoxLayout()
            line.setSpacing(10)
            field = ColourField(mods.get(key) or self.colours["primary"], f"{m.title} accent")
            box = QCheckBox("The theme's accent")
            box.setChecked(key not in mods)
            field.setEnabled(key in mods)
            box.toggled.connect(lambda on, f=field: f.setEnabled(not on))
            line.addWidget(field)
            line.addWidget(box)
            line.addStretch()
            self.module_fields[key] = (field, box)
            form.addRow(self._lbl(f"{m.icon}  {m.title}"), line)
        self.pages.addWidget(mpage)

        # buttons
        btns = QHBoxLayout()
        btns.setSpacing(8)
        btns.addStretch()
        cancel = widgets.button("Cancel", "ghost")
        cancel.setAutoDefault(False)
        cancel.clicked.connect(self.reject)
        ok = widgets.button("Save", "primary")
        ok.setAutoDefault(False)
        ok.setDefault(True)
        ok.clicked.connect(self.accept)
        btns.addWidget(cancel)
        btns.addWidget(ok)
        root.addLayout(btns)

        self._start = dict(self.colours)
        self._tab(0)
        self.select("primary")
        self._changed()
        QTimer.singleShot(0, lambda: widgets.bring_to_front(self))

    # ── building bits ──────────────────────────────────────────────────────
    def _form_page(self):
        page = QWidget()
        form = QFormLayout(page)
        form.setContentsMargins(0, 4, 8, 0)
        form.setHorizontalSpacing(16)
        form.setVerticalSpacing(10)
        form.setLabelAlignment(Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter)
        sc = QScrollArea()
        sc.setWidgetResizable(True)
        sc.setFrameShape(QFrame.Shape.NoFrame)
        sc.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        sc.setWidget(page)
        return sc, form

    @staticmethod
    def _lbl(text: str) -> QLabel:
        lbl = widgets.label(text, 13, tone="secondary")
        lbl.setFixedWidth(150)
        return lbl

    @staticmethod
    def _heading(text: str) -> QLabel:
        h = widgets.label(text, 10.5, 600, mono=True, spacing=0.14, tone="accent")
        h.setContentsMargins(0, 8, 0, 0)
        return h

    def _path_row(self, form, label, value, flt, help_text) -> QLineEdit:
        w = QWidget()
        lay = QVBoxLayout(w)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(4)
        row = QHBoxLayout()
        row.setSpacing(6)
        edit = QLineEdit(value)
        edit.setFixedHeight(32)
        edit.textChanged.connect(self._changed)
        b = widgets.button("Browse…")
        b.setAutoDefault(False)
        b.clicked.connect(lambda: self._browse(edit, flt))
        c = widgets.icon_button("✕", "Clear", 28, 11, ghost=True)
        c.setAutoDefault(False)
        c.clicked.connect(lambda: edit.setText(""))
        row.addWidget(edit, 1)
        row.addWidget(b)
        row.addWidget(c)
        lay.addLayout(row)
        h = widgets.label(help_text, 12, tone="muted")
        h.setWordWrap(True)
        lay.addWidget(h)
        form.addRow(self._lbl(label), w)
        return edit

    def _browse(self, edit: QLineEdit, flt: str):
        p, _ = QFileDialog.getOpenFileName(self, "Choose a file", edit.text(), flt)
        if p:
            edit.setText(p)

    def _tab(self, i: int):
        for j, b in enumerate(self._tabs):
            b.setChecked(i == j)
        self.pages.setCurrentIndex(i)

    # ── colours ────────────────────────────────────────────────────────────
    def select(self, role: str):
        self._role = role
        self.role_title.setText(dict(theme.COLOUR_ROLES).get(role, role))
        self.picker.set_colour(self.colours[role])
        self.picker.set_original(self._start.get(role, self.colours[role]) if hasattr(self, "_start")
                                 else self.colours[role])
        self.follow.setVisible(role == "primary")
        self._paint_rows()

    def _paint_rows(self):
        for role, row in self.rows.items():
            row.show_colour(self.colours[role], role == self._role)

    def _picked(self, c: str):
        self.colours[self._role] = c
        if self._role == "primary" and self.follow.isChecked():
            self.colours.update(theme.accent_roles(c, self.colours["bg_base"]))
            self.colours["folder_border"] = self.colours["primary_text"]
        self._paint_rows()
        self._changed()

    def _generate(self):
        light = theme.luminance(self.colours["bg_base"]) > 140
        self.colours.update(theme.generate(self.colours["primary"], light))
        self.select(self._role)
        self._changed()

    def _flip(self):
        light = theme.luminance(self.colours["bg_base"]) > 140
        self.colours.update(theme.generate(self.colours["primary"], not light))
        self.select(self._role)
        self._changed()

    def _undo_all(self):
        self.colours = dict(self._start)
        self.select(self._role)
        self._changed()

    # ── preview ────────────────────────────────────────────────────────────
    def _changed(self, *_):
        if not hasattr(self, "preview"):
            return
        self.preview.setPixmap(_preview(self.result_theme(), self.preview.width(), self.preview.height()))
        self._sample_timer.start()
        self._update_shell()
        c = self.colours
        lines = []
        for fg, bg, what in ((c["text_primary"], c["bg_base"], "Text on the background"),
                             (c["text_muted"], c["bg_card"], "Muted text on cards"),
                             (c["on_primary"], c["primary"], "Text on the accent")):
            r = theme.contrast(fg, bg)
            mark = "✓" if r >= 4.5 else ("≈" if r >= 3 else "⚠ hard to read")
            lines.append(f"{what}: {r:.1f} : 1  {mark}")
        self.contrast.setText("\n".join(lines))

    def _update_sample(self):
        """The sample panel in the theme being edited, with the real stylesheet."""
        old = theme.P
        try:
            theme.use(ThemeStore.palette_values(self.result_theme()))
            css = theme.stylesheet()
            p = theme.P
        finally:
            theme.P = old
        self.sample.setStyleSheet(css + f"#sampleBox {{ background: {p.bg_base}; border: 1px solid {p.border};"
                                        f" border-radius: {p.radius}px; }}")
        for w, px, weight, colour in ((self.s_title, 14, 600, p.text_primary), (self.s_text, 12.5, 400, p.text_secondary),
                                      (self.s_muted, 11, 400, p.text_muted)):
            w.setFont(theme.qfont(px, weight, family=p.font_family))
            w.setStyleSheet(f"color: {colour}; background: transparent;")
        self.s_acc.setProperty("variant", "primary")
        for w in (self.s_acc, self.s_btn, self.s_check, self.s_input):
            w.style().unpolish(w)
            w.style().polish(w)

    def _update_shell(self):
        if not hasattr(self, "shell_preview"):
            return
        from core import caelestia_sync
        pri, sec = caelestia_sync.shell_colours(self.result_theme())
        ter = self.result_theme().get("caelestia", {}).get("tertiary") or caelestia_sync.auto_tertiary(pri)
        for key, c in (("primary", pri), ("secondary", sec), ("tertiary", ter)):
            field, box = self.cae_fields[key]
            if box.isChecked():
                field.set_value(c)          # an automatic colour shows what it will be
        w, h = 300, 40
        px = QPixmap(w, h)
        px.fill(QColor(0, 0, 0, 0))
        p = QPainter(px)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor(self.colours["bg_base"]))
        p.drawRoundedRect(QRectF(0.5, 0.5, w - 1, h - 1), 10, 10)
        for i, (c, name) in enumerate(((pri, "PRIMARY"), (sec, "SECONDARY"), (ter, "THIRD"))):
            x = 8 + i * 97
            p.setBrush(QColor(c))
            p.drawRoundedRect(QRectF(x, 8, 24, 24), 7, 7)
            p.setPen(QColor(self.colours["text_secondary"]))
            p.setFont(theme.qfont(9.5, 600, mono=True))
            p.drawText(QRectF(x + 30, 8, 64, 24), Qt.AlignmentFlag.AlignVCenter, name)
            p.setPen(Qt.PenStyle.NoPen)
        p.end()
        self.shell_preview.setPixmap(px)

    # ── result ─────────────────────────────────────────────────────────────
    def result_theme(self) -> dict:
        t = dict(self.t)
        t["name"] = self.name.text().strip() or t["name"]
        t["colours"] = dict(self.colours)
        if hasattr(self, "font"):
            t.update({"font_family": self.font.currentFont().family(), "radius": self.radius.value(),
                      "glow": self.glow.isChecked(), "background_image": self.bg_image.text().strip(),
                      "image_opacity": self.opacity.value() / 100.0, "icon": self.icon.text().strip()})
        if hasattr(self, "wallpaper"):
            t["wallpaper"] = self.wallpaper.text().strip()
            t["profile"] = self.profile.currentData() or ""
        if hasattr(self, "cae_fields"):
            t["caelestia"] = {k: f.value() for k, (f, box) in self.cae_fields.items() if not box.isChecked()}
        if hasattr(self, "module_fields"):
            t["modules"] = {k: f.value() for k, (f, box) in self.module_fields.items() if not box.isChecked()}
        return t
