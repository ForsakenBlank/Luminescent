"""
Themes page: a grid of theme cards grouped in colour-coded folders.

Click a card to apply it. Right-click → Settings… opens the theme editor: every colour
(each one can be a gradient), font, corner radius, glow, background image + its opacity,
icon, wallpaper, settings profile, and a whole look of their own (colours, gradients,
corner radius) for each module, the notifications and the on-screen warnings. Also
Rename, Duplicate, Export, Move to folder, Delete. The toolbar imports a .lumtheme file or makes a whole theme from
one colour. Themes outside a folder are listed under BUILT-IN (Dark, Light) and MY THEMES.
"""
import os
import copy
import json

from PyQt6.QtWidgets import (
    QWidget, QFrame, QLabel, QVBoxLayout, QHBoxLayout, QGridLayout, QScrollArea, QDialog, QFormLayout,
    QLineEdit, QCheckBox, QComboBox, QFontComboBox, QFileDialog, QPushButton, QSlider, QSplitter,
    QStackedWidget, QButtonGroup, QSizePolicy
)
from PyQt6.QtCore import Qt, QRectF, QPointF, QTimer
from PyQt6.QtGui import QPainter, QPainterPath, QColor, QPixmap, QFont, QPen

from core import theme, widgets, colourpick, IS_WIN
from core.themes import ThemeStore, new_theme

COLS, GAP = 5, 16
PREV_W, PREV_H = 190, 112
IMAGE_FILTER = "Images (*.png *.jpg *.jpeg *.bmp *.gif *.webp);;All files (*)"
BUILTIN_GROUP, MINE_GROUP = "builtin", "mine"
MINE_COLOUR = "#3DBE96"


def _pal(t: dict) -> theme.Palette:
    """The palette a theme would give, without switching to it."""
    return theme.palette_from(ThemeStore.palette_values(t))


def _text(qp: QPainter, r: QRectF, text: str, colour: str, px: float, weight: int = 400, mono: bool = False,
          align=Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter, spacing: float = 0.0):
    qp.setPen(QColor(colour))
    qp.setFont(theme.qfont(px, weight, mono, spacing, scaled=False))
    qp.drawText(r, align, text)


def paint_overlay(qp: QPainter, r: QRectF, p: theme.Palette, s: float = 1.0):
    """A small mock-up of the overlay in palette `p`: page switcher, a title, three cards."""
    rad = max(0.0, p.radius - 4) * s
    clip = QPainterPath()
    clip.addRoundedRect(r, rad, rad)
    qp.save()
    qp.setClipPath(clip)
    qp.setPen(Qt.PenStyle.NoPen)
    qp.fillRect(r, theme.qbrush("bg_base", r, 1.0, p))
    pad = 10 * s
    w = r.width()
    # page switcher pill hanging from the top
    nav = QRectF(r.left() + w * 0.28, r.top(), w * 0.44, 15 * s)
    qp.setBrush(theme.qbrush("bg_header", nav, 1.0, p))
    qp.drawRoundedRect(nav.adjusted(0, -8 * s, 0, 0), 8 * s, 8 * s)
    pill = QRectF(nav.left() + 4 * s, nav.top() + 3 * s, nav.width() * 0.3, 9 * s)
    qp.setBrush(theme.qbrush("primary", pill, 1.0, p))
    qp.drawRoundedRect(pill, 4.5 * s, 4.5 * s)
    line = QRectF(nav.left(), nav.top(), nav.width(), max(1.0, 1.5 * s))
    qp.fillRect(line, theme.qbrush("primary", line, 1.0, p))
    for i in range(2):
        t = QColor(p.text_secondary)
        t.setAlphaF(0.6)
        qp.setBrush(t)
        qp.drawRoundedRect(QRectF(pill.right() + (8 + i * 26) * s, pill.top() + 3 * s, 18 * s, 3 * s), 1.5, 1.5)
    # a title, an accent tick
    top = nav.bottom() + 9 * s
    tc = QColor(p.text_primary)
    qp.setBrush(tc)
    qp.drawRoundedRect(QRectF(r.left() + pad, top, 46 * s, 5 * s), 2.5 * s, 2.5 * s)
    tick = QRectF(r.left() + pad, top + 10 * s, 22 * s, max(1.0, 1.5 * s))
    qp.fillRect(tick, theme.qbrush("primary", tick, 1.0, p))
    # three cards, the last one shorter
    ctop = tick.bottom() + 7 * s
    avail = r.bottom() - pad - ctop
    cw = (w - 2 * pad - 2 * 6 * s) / 3
    for i in range(3):
        c = QRectF(r.left() + pad + i * (cw + 6 * s), ctop, cw, avail * (0.62 if i == 2 else 1.0))
        qp.setBrush(theme.qbrush("bg_card", c, 1.0, p))
        qp.setPen(QPen(theme.qbrush("border", c, 1.0, p), 1))
        qp.drawRoundedRect(c, 4 * s, 4 * s)
        qp.setPen(Qt.PenStyle.NoPen)
        dot = QRectF(c.left() + 6 * s, c.top() + 6 * s, 5 * s, 5 * s)
        qp.setBrush(theme.qbrush("primary", dot, 1.0, p))
        qp.drawEllipse(dot)
        for j in range(3):
            lc = QColor(p.text_secondary if j else p.text_primary)
            lc.setAlphaF(0.7)
            qp.setBrush(lc)
            qp.drawRoundedRect(QRectF(c.left() + 6 * s, c.top() + (16 + j * 8) * s, c.width() * (0.7 - j * 0.15),
                                      3 * s), 1.5, 1.5)
    qp.restore()


def paint_panel(qp: QPainter, r: QRectF, p: theme.Palette, title: str = "PANEL"):
    """A floating panel in palette `p`: header, accent tick, a card, a field, a button, a switch, a slider."""
    rad = float(p.radius)
    qp.save()
    qp.setPen(QPen(theme.qbrush("border", r, 1.0, p), 1))
    qp.setBrush(theme.qbrush("bg_base", r, 1.0, p))
    qp.drawRoundedRect(r, rad, rad)
    head = QRectF(r.left() + 1, r.top() + 1, r.width() - 2, 34)
    path = QPainterPath()
    path.addRoundedRect(head.adjusted(0, 0, 0, rad), max(0.0, rad - 1), max(0.0, rad - 1))
    clip = QPainterPath()
    clip.addRect(head)
    qp.setPen(Qt.PenStyle.NoPen)
    qp.setBrush(theme.qbrush("bg_header", head, 1.0, p))
    qp.drawPath(path.intersected(clip))
    qp.setPen(QPen(QColor(p.border), 1))
    qp.drawLine(QPointF(head.left(), head.bottom()), QPointF(head.right(), head.bottom()))
    tick = QRectF(head.left(), head.bottom() - 1, 36, 2)
    qp.fillRect(tick, theme.qbrush("primary", tick, 1.0, p))
    _text(qp, head.adjusted(14, 0, -30, 0), title.upper(), p.text_primary, 10, 600, True, spacing=0.16)
    _text(qp, head.adjusted(0, 0, -12, 0), "✕", p.text_secondary, 11,
          align=Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
    x, y, w = r.left() + 14, head.bottom() + 12, r.width() - 28
    card = QRectF(x, y, w, 58)
    qp.setPen(QPen(theme.qbrush("border", card, 1.0, p), 1))
    qp.setBrush(theme.qbrush("bg_card", card, 1.0, p))
    qp.drawRoundedRect(card, max(0, rad - 2), max(0, rad - 2))
    _text(qp, card.adjusted(12, 8, -8, -32), "Heading text", p.text_primary, 12.5, 600)
    _text(qp, card.adjusted(12, 26, -8, -16), "Secondary text", p.text_secondary, 11)
    _text(qp, card.adjusted(12, 40, -8, -2), "Muted · hint", p.text_muted, 10.5)
    badge = QRectF(card.right() - 62, card.top() + 9, 52, 18)
    qp.setPen(Qt.PenStyle.NoPen)
    qp.setBrush(theme.qbrush("primary_muted", badge, 1.0, p))
    qp.drawRoundedRect(badge, 9, 9)
    _text(qp, badge, "TAG", p.primary_text, 9, 600, True, Qt.AlignmentFlag.AlignCenter, 0.1)
    y = card.bottom() + 10
    field = QRectF(x, y, w * 0.62, 28)
    qp.setPen(QPen(QColor(p.primary), 1))
    qp.setBrush(theme.qbrush("bg_frame", field, 1.0, p))
    qp.drawRoundedRect(field, max(0, rad - 4), max(0, rad - 4))
    _text(qp, field.adjusted(10, 0, 0, 0), "A field", p.text_primary, 11.5)
    btn = QRectF(field.right() + 8, y, w - field.width() - 8, 28)
    qp.setPen(Qt.PenStyle.NoPen)
    qp.setBrush(theme.qbrush("primary", btn, 1.0, p))
    qp.drawRoundedRect(btn, max(0, rad - 4), max(0, rad - 4))
    _text(qp, btn, "Button", p.on_primary, 11.5, 600, align=Qt.AlignmentFlag.AlignCenter)
    y = btn.bottom() + 12
    box = QRectF(x, y + 2, 14, 14)
    qp.setBrush(theme.qbrush("primary", box, 1.0, p))
    qp.drawRoundedRect(box, 4, 4)
    _text(qp, box, "✓", p.on_primary, 9, 700, align=Qt.AlignmentFlag.AlignCenter)
    _text(qp, QRectF(x + 22, y, w, 18), "A switch that is on", p.text_primary, 11.5)
    y += 28
    track = QRectF(x, y, w, 4)
    qp.setBrush(QColor(p.bg_frame))
    qp.setPen(QPen(QColor(p.border), 1))
    qp.drawRoundedRect(track, 2, 2)
    done = QRectF(x, y, w * 0.58, 4)
    qp.setPen(Qt.PenStyle.NoPen)
    qp.setBrush(theme.qbrush("primary", done, 1.0, p))
    qp.drawRoundedRect(done, 2, 2)
    qp.setBrush(QColor(p.text_primary))
    qp.setPen(QPen(QColor(p.primary), 2))
    qp.drawEllipse(QPointF(done.right(), y + 2), 6, 6)
    qp.restore()


def paint_toast(qp: QPainter, r: QRectF, p: theme.Palette, title: str, text: str, style: str = "glow"):
    """A notification card the way ui/toast.py draws it."""
    from ui.toast import paint_card
    paint_card(qp, r, p, style)
    _text(qp, QRectF(r.left() + 40, r.top() + 10, r.width() - 52, 18), title, p.text_primary, 12.5, 600)
    _text(qp, QRectF(r.left() + 40, r.top() + 28, r.width() - 52, r.height() - 34), text, p.text_secondary, 11,
          align=Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignTop)
    qp.drawPixmap(int(r.left() + 16), int(r.top() + 13), theme.sparkle_pixmap(13, p.primary))


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
    paint_overlay(p, QRectF(0, 0, w, h), _pal(t), h / 112.0)
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
        if e.button() == Qt.MouseButton.LeftButton and self.rect().contains(e.position().toPoint()):
            self.page.toggle_group(self.key, self.folder)

    def contextMenuEvent(self, e):
        self.page.folder_menu(self.key, self.folder, e.globalPos())


class ThemesPage(QWidget):
    def __init__(self, launcher):
        super().__init__()
        self._order: list[str] = []
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
        """[(key, name, colour, folder or None, themes)]: BUILT-IN, MY THEMES, then the user's folders, in the
        order they were dragged into."""
        out = self._default_groups()
        order = self.store.data.get("group_order") or []
        rank = {k: i for i, k in enumerate(order)}
        # a group that is not in the saved order yet (a new folder) goes where it would be by default
        return sorted(out, key=lambda g: (rank.get(g[0], len(order) + out.index(g))))

    def _default_groups(self):
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
        self._order = []
        self._reorder = widgets.DragReorder(v, holder)
        self._reorder.moved.connect(self._group_moved)
        for key, name, colour, folder, themes in self._groups():
            collapsed = bool(folder.get("collapsed")) if folder else key in closed
            box = QWidget()
            group = QVBoxLayout(box)
            group.setContentsMargins(0, 0, 0, 0)
            group.setSpacing(14)
            header = FolderHeader(self, key, name, colour, len(themes), collapsed, folder)
            header.setToolTip("Click to fold · drag to move the folder · right-click for options")
            group.addWidget(header)
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
            v.addWidget(box)
            self._reorder.add(box, header)
            header.setCursor(Qt.CursorShape.PointingHandCursor)    # still a click to fold, so keep the hand
            self._order.append(key)
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
            colour = colourpick.pick_colour(self, theme.P.primary, "The theme's accent colour")
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
        c = colourpick.pick_colour(self, theme.P.primary, "Folder colour")
        self.store.add_folder(name, c or theme.P.primary)

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
        group = next((g[4] for g in self._groups() if any(x["id"] == t["id"] for x in g[4])), [])
        ids = [x["id"] for x in group]
        i = ids.index(t["id"]) if t["id"] in ids else -1
        earlier = m.addAction("Move earlier", lambda: self.store.swap_themes(t["id"], ids[i - 1]))
        earlier.setEnabled(i > 0)
        later = m.addAction("Move later", lambda: self.store.swap_themes(t["id"], ids[i + 1]))
        later.setEnabled(0 <= i < len(ids) - 1)
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

    def folder_menu(self, key: str, f: dict | None, pos):
        m = widgets.menu(self)
        if f:
            m.addAction("Rename…", lambda: self._rename_folder(f))
            m.addAction("Colour…", lambda: self._folder_colour(f))
            m.addSeparator()
        i = self._order.index(key) if key in self._order else -1
        up = m.addAction("Move up", lambda: self._group_moved(i, i - 1))
        up.setEnabled(i > 0)
        down = m.addAction("Move down", lambda: self._group_moved(i, i + 1))
        down.setEnabled(0 <= i < len(self._order) - 1)
        top = m.addAction("Move to the top", lambda: self._group_moved(i, 0))
        top.setEnabled(i > 0)
        bottom = m.addAction("Move to the bottom", lambda: self._group_moved(i, len(self._order) - 1))
        bottom.setEnabled(0 <= i < len(self._order) - 1)
        if f:
            m.addSeparator()
            m.addAction("Delete folder (keeps its themes)", lambda: self.store.delete_folder(f["id"]))
        m.exec(pos)

    def _group_moved(self, old: int, new: int):
        order = list(self._order)
        if not (0 <= old < len(order)) or not (0 <= new < len(order)) or old == new:
            return
        order.insert(new, order.pop(old))
        # keep groups that are hidden right now (an empty "My themes") where they were
        saved = [k for k in (self.store.data.get("group_order") or []) if k not in order]
        self.store.set_group_order(order + saved)

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
        c = colourpick.pick_colour(self, f.get("colour", theme.P.primary), "Folder colour")
        if c:
            self.store.update_folder(f["id"], colour=c)


# ── the theme editor ──────────────────────────────────────────────────────────
# sections of a theme that can have their own look (besides the modules): key -> (title, icon, what it covers)
EXTRA_SECTIONS = [("toasts", "Notifications", "◔", "The notification cards in the corner of the screen."),
                  ("alerts", "Warnings on screen", "⚠", "The big warnings in the middle of the screen (Automation).")]
NO_WINDOWS = ("doodle", "colorpicker", "snapper", "ruler")      # modules without windows of their own to colour
ROLE_HELP = {
    "primary": "Buttons, switches, sliders, the page switcher and accent lines.",
    "primary_muted": "Behind selected rows, tags and ticked buttons.",
    "primary_text": "Links, counts and other accent-coloured text.",
    "on_primary": "Text on accent-coloured buttons.",
    "bg_base": "Panels, pages, menus and dialogs.",
    "bg_card": "Cards, buttons and list boxes.",
    "bg_frame": "Text fields, the settings sidebar and other insets.",
    "bg_header": "Panel headers and footers.",
}


def _clear(layout):
    widgets.clear_layout(layout)


class _Splitter(QSplitter):
    """A splitter whose handles are a thin line you can grab (accent on hover)."""

    def __init__(self):
        super().__init__(Qt.Orientation.Horizontal)
        self.setHandleWidth(9)
        self.setChildrenCollapsible(False)
        p = theme.P

        def line(c):
            return (f"qlineargradient(x1:0, y1:0, x2:1, y2:0, stop:0 transparent, stop:0.44 transparent, "
                    f"stop:0.45 {c}, stop:0.55 {c}, stop:0.56 transparent, stop:1 transparent)")
        self.setStyleSheet(f"QSplitter::handle {{ background: {line(p.border)}; }}"
                           f"QSplitter::handle:hover {{ background: {line(p.primary)}; }}")


class SectionPreview(QWidget):
    """The right-hand column: what the section being edited looks like, drawn live."""

    def __init__(self, dlg: "ThemeDialog"):
        super().__init__()
        self.dlg = dlg
        self.setMinimumWidth(260)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

    def paintEvent(self, e):
        qp = QPainter(self)
        qp.setRenderHint(QPainter.RenderHint.Antialiasing)
        dlg = self.dlg
        key = dlg.current
        base = dlg.base_palette()
        w = self.width()
        y = 4.0
        if key in ("general", "colours", "applied"):
            h = min(w * 0.6, 260)
            paint_overlay(qp, QRectF(0, y, w, h), base, h / 112.0)
            y += h + 16
            paint_panel(qp, QRectF(0, y, w, 250), base, "A PANEL")
            return
        p = dlg.section_palette(key)
        title = dlg.section_title(key)
        if key == "toasts":
            from ui.toast import card_style
            style = card_style(dlg.launcher.settings)
            paint_toast(qp, QRectF(0, y, w, 64), p, "Luminescent", "Notes copied to the clipboard.", style)
            paint_toast(qp, QRectF(0, y + 76, w, 92), p, "Hearing Guard",
                        "You have used 80 % of today's safe listening allowance. A longer message wraps "
                        "onto more lines and the card grows with it.", style)
            return
        if key == "alerts":
            r = QRectF(0, y, w, 170)
            qp.setPen(QPen(QColor("#F2B705"), 2))
            qp.setBrush(theme.qbrush("bg_base", r, 1.0, p))
            qp.drawRoundedRect(r.adjusted(1, 1, -1, -1), p.radius + 4, p.radius + 4)
            _text(qp, QRectF(20, y + 16, 40, 40), "⚠", "#F2B705", 26, 600)
            _text(qp, QRectF(64, y + 18, w - 80, 36), "Battery low", p.text_primary, 18, 700)
            _text(qp, QRectF(22, y + 62, w - 44, 50), "Plug the charger in before it switches off.",
                  p.text_secondary, 12.5, align=Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignTop)
            btn = QRectF(w - 116, y + 120, 96, 32)
            qp.setPen(Qt.PenStyle.NoPen)
            qp.setBrush(QColor("#F2B705"))
            qp.drawRoundedRect(btn, max(0, p.radius - 4), max(0, p.radius - 4))
            _text(qp, btn, "Got it", "#111111", 12, 600, align=Qt.AlignmentFlag.AlignCenter)
            return
        paint_panel(qp, QRectF(0, y, w, 260), p, title)
        y += 276
        _text(qp, QRectF(0, y, w, 16), "THE REST OF THE APP", base.text_muted, 9.5, 600, True, spacing=0.12)
        h = min(w * 0.45, 170)
        paint_overlay(qp, QRectF(0, y + 22, w, h), base, h / 112.0)


class RoleRow(QWidget):
    """One colour role: its name, what it colours, and a swatch (colour or gradient)."""

    def __init__(self, role: str, label: str, value, on_change, unset_text: str = ""):
        super().__init__()
        self.role = role
        lay = QHBoxLayout(self)
        lay.setContentsMargins(0, 3, 0, 3)
        lay.setSpacing(10)
        col = QVBoxLayout()
        col.setSpacing(1)
        col.addWidget(widgets.label(label, 12.5))
        if ROLE_HELP.get(role):
            h = widgets.label(ROLE_HELP[role], 10.5, tone="muted")
            h.setWordWrap(True)
            col.addWidget(h)
        lay.addLayout(col, 1)
        self.swatch = colourpick.ColourSwatch(value, label, gradient=True, unset_text=unset_text, width=170,
                                              live=True)
        self.swatch.changed.connect(lambda v: on_change(role, v))
        lay.addWidget(self.swatch)
        self.clear = widgets.icon_button("✕", "Back to the theme's colour", 26, 11, ghost=True)
        self.clear.setVisible(bool(unset_text))
        self.clear.clicked.connect(lambda: (self.swatch.set_value(None), on_change(role, None)))
        lay.addWidget(self.clear)

    def set_value(self, v):
        self.swatch.set_value(v)


def _card(title: str, hint: str = "") -> tuple[QFrame, QVBoxLayout]:
    card = QFrame()
    card.setObjectName("lumCard")
    v = QVBoxLayout(card)
    v.setContentsMargins(16, 12, 16, 14)
    v.setSpacing(6)
    v.addWidget(widgets.label(title, 10.5, 600, mono=True, spacing=0.14, tone="accent"))
    if hint:
        h = widgets.label(hint, 11.5, tone="muted")
        h.setWordWrap(True)
        v.addWidget(h)
        v.addSpacing(2)
    return card, v


class ThemeDialog(widgets.RoundedDialog):
    """Everything about one theme. Left: the parts of it (General, Colours, every module, Notifications…); middle:
    that part's settings; right: a live preview. Drag the lines between the columns to resize them."""

    def __init__(self, launcher, t: dict, parent=None):
        super().__init__(parent, f"Theme settings · {t['name']}", resizable=True)
        self.launcher = launcher
        self.t = copy.deepcopy(t)
        self.t.setdefault("colours", {})
        self.t["gradients"] = theme.clean_gradients(self.t.get("gradients"))
        self.t["modules"] = {k: v for k, v in ((k, theme.clean_section(v))
                                               for k, v in (self.t.get("modules") or {}).items()) if v}
        self._saved = ""
        self.current = "general"
        self._pages: dict[str, QWidget] = {}
        self._role_rows: dict[str, dict[str, RoleRow]] = {}
        self._redraw = QTimer(self)
        self._redraw.setSingleShot(True)
        self._redraw.setInterval(15)
        self._redraw.timeout.connect(lambda: self.preview.update())
        self.setMinimumSize(820, 540)

        self.body.setContentsMargins(0, 0, 0, 0)
        self.body.setSpacing(0)
        split = _Splitter()
        self.split = split

        # sections
        side = QFrame()
        side.setObjectName("setSidebar")
        sl = QVBoxLayout(side)
        sl.setContentsMargins(10, 12, 10, 12)
        sl.setSpacing(2)
        self.side_group = QButtonGroup(self)
        self.side_buttons: dict[str, QPushButton] = {}

        def heading(text):
            lbl = widgets.label(text, 9.5, 600, mono=True, spacing=0.14, tone="muted")
            lbl.setContentsMargins(10, 10, 0, 4)
            sl.addWidget(lbl)

        def add(key, text):
            b = QPushButton(text)
            b.setProperty("side", True)
            b.setCheckable(True)
            b.setFixedHeight(30)
            b.setCursor(Qt.CursorShape.PointingHandCursor)
            b.clicked.connect(lambda _c=False, k=key: self.show_section(k))
            self.side_group.addButton(b)
            self.side_buttons[key] = b
            sl.addWidget(b)

        heading("THEME")
        for key, text in (("general", "General"), ("colours", "Colours"), ("applied", "When it is applied")):
            add(key, text)
        heading("PARTS")
        for key, title, icon, _d in EXTRA_SECTIONS:
            add(key, f"{icon}  {title}")
        heading("MODULES")
        for key, m in launcher.modules.items():
            if key not in NO_WINDOWS:
                add(key, f"{m.icon}  {m.title}")
        sl.addStretch()
        side_scroll = QScrollArea()
        side_scroll.setWidgetResizable(True)
        side_scroll.setFrameShape(QFrame.Shape.NoFrame)
        side_scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        side_scroll.setWidget(side)
        side_scroll.setMinimumWidth(170)
        split.addWidget(side_scroll)

        self.stack = QStackedWidget()
        self.stack.setMinimumWidth(380)
        split.addWidget(self.stack)

        right = QWidget()
        rl = QVBoxLayout(right)
        rl.setContentsMargins(14, 14, 16, 14)
        rl.setSpacing(8)
        rl.addWidget(widgets.label("PREVIEW", 10.5, 600, mono=True, spacing=0.14, tone="muted"))
        self.preview = SectionPreview(self)
        rl.addWidget(self.preview, 1)
        split.addWidget(right)
        split.setStretchFactor(0, 0)
        split.setStretchFactor(1, 1)
        split.setStretchFactor(2, 0)
        self.body.addWidget(split, 1)

        foot = QFrame()
        foot.setObjectName("setFooter")
        foot.setFixedHeight(54)
        fl = QHBoxLayout(foot)
        fl.setContentsMargins(16, 0, 16, 0)
        fl.setSpacing(8)
        self.reset_btn = widgets.button("Reset this part", "danger",
                                        "Everything on this page back to the theme's own look")
        self.reset_btn.clicked.connect(self._reset_section)
        fl.addWidget(self.reset_btn)
        fl.addStretch()
        self.status = widgets.label("", 10.5, mono=True, spacing=0.04, tone="muted")
        fl.addWidget(self.status)
        fl.addSpacing(8)
        cancel = widgets.button("Cancel", "ghost")
        cancel.clicked.connect(self.reject)
        ok = widgets.button("Save", "primary")
        ok.setDefault(True)
        ok.clicked.connect(self.accept)
        fl.addWidget(cancel)
        fl.addWidget(ok)
        self.body.addWidget(foot)

        self.fit_to_screen(1240, 820, parent)
        split.setSizes([210, max(420, self.width() - 210 - 380), 380])
        self.show_section("general")
        self._saved = json.dumps(self.result_theme(), sort_keys=True)

    # ── palettes for the preview ───────────────────────────────────────────
    def base_palette(self) -> theme.Palette:
        return _pal(self.result_theme())

    def section_palette(self, key: str) -> theme.Palette:
        return theme.section_palette(self.base_palette(), theme.clean_section(self.t["modules"].get(key) or {}))

    def section_title(self, key: str) -> str:
        for k, title, _i, _d in EXTRA_SECTIONS:
            if k == key:
                return title
        m = self.launcher.modules.get(key)
        return m.title if m else key

    def changed(self):
        self._redraw.start()
        dirty = json.dumps(self.result_theme(), sort_keys=True) != self._saved
        self.status.setText("UNSAVED CHANGES" if dirty else "")

    # ── pages ──────────────────────────────────────────────────────────────
    def show_section(self, key: str):
        self.current = key
        self.side_buttons[key].setChecked(True)
        if key not in self._pages:
            page = self._build(key)
            scroll = QScrollArea()
            scroll.setWidgetResizable(True)
            scroll.setFrameShape(QFrame.Shape.NoFrame)
            scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
            scroll.setWidget(page)
            self._pages[key] = scroll
            self.stack.addWidget(scroll)
        self.stack.setCurrentWidget(self._pages[key])
        self.reset_btn.setVisible(key not in ("general", "applied", "colours"))
        self.preview.update()

    def _page(self, title: str, text: str) -> tuple[QWidget, QVBoxLayout]:
        page = QWidget()
        v = QVBoxLayout(page)
        v.setContentsMargins(20, 18, 20, 20)
        v.setSpacing(12)
        v.addWidget(widgets.label(title, 18, 600))
        if text:
            d = widgets.label(text, 12, tone="muted")
            d.setWordWrap(True)
            v.addWidget(d)
        return page, v

    def _build(self, key: str) -> QWidget:
        if key == "general":
            return self._general_page()
        if key == "colours":
            return self._colours_page()
        if key == "applied":
            return self._applied_page()
        return self._section_page(key)

    def _general_page(self) -> QWidget:
        page, v = self._page("General", "The name, type and shape of the theme, and a picture behind the overlay.")
        t = self.t
        card, c = _card("NAME AND TYPE")
        form = QFormLayout()
        form.setSpacing(8)
        self.name = QLineEdit(t["name"])
        self.name.textChanged.connect(lambda txt: (self.set_title(f"Theme settings · {txt}"), self.changed()))
        form.addRow("Name", self.name)
        self.font = QFontComboBox()
        self.font.setCurrentFont(QFont(t.get("font_family", "Geist")))
        self.font.currentFontChanged.connect(lambda _f: self.changed())
        form.addRow("Font", self.font)
        c.addLayout(form)
        v.addWidget(card)

        card, c = _card("SHAPE")
        self.radius = _SliderRow(0, 30, int(t.get("radius", 10)), "px", self.changed)
        c.addWidget(widgets.label("Corner radius", 12.5))
        c.addWidget(self.radius)
        self.glow = QCheckBox("Accent glow around the overlay")
        self.glow.setChecked(bool(t.get("glow", True)))
        self.glow.toggled.connect(lambda _on: self.changed())
        c.addWidget(self.glow)
        v.addWidget(card)

        card, c = _card("PICTURES")
        self.bg_image = self._path_row(c, "Background image", t.get("background_image", ""), IMAGE_FILTER,
                                       "Shown behind the overlay content.")
        c.addWidget(widgets.label("Image opacity", 12.5))
        self.opacity = _SliderRow(0, 100, int(float(t.get("image_opacity", 0.35)) * 100), "%", self.changed)
        c.addWidget(self.opacity)
        self.icon = self._path_row(c, "Card icon", t.get("icon", ""), IMAGE_FILTER,
                                   "Shown on the theme card. Empty: the background image or a preview.")
        v.addWidget(card)
        v.addStretch()
        return page

    def _colours_page(self) -> QWidget:
        page, v = self._page("Colours", "Every colour of the theme. Any of them can be a gradient: open one and "
                                        "switch to Gradient. Modules, notifications and warnings use these unless "
                                        "you give them their own (left).")
        gen = widgets.button("Match everything to the accent", "ghost",
                             "Picks backgrounds, text and borders that go with the accent colour")
        gen.clicked.connect(self._generate)
        v.addWidget(gen, 0, Qt.AlignmentFlag.AlignLeft)
        rows = {}
        for title, roles in (("ACCENT", ("primary", "primary_hover", "primary_muted", "primary_text", "on_primary")),
                             ("BACKGROUNDS", ("bg_base", "bg_card", "bg_card_hover", "bg_frame", "bg_header",
                                              "border")),
                             ("TEXT", ("text_primary", "text_secondary", "text_muted")),
                             ("FOLDERS", ("folder_color", "folder_border"))):
            card, c = _card(title)
            for role in roles:
                row = RoleRow(role, dict(theme.COLOUR_ROLES)[role], self._global_value(role), self._set_global)
                rows[role] = row
                c.addWidget(row)
            v.addWidget(card)
        self._role_rows["colours"] = rows
        v.addStretch()
        return page

    def _applied_page(self) -> QWidget:
        page, v = self._page("When it is applied", "What else changes when you switch to this theme.")
        t = self.t
        card, c = _card("DESKTOP")
        self.wallpaper = self._path_row(
            c, "Desktop wallpaper", t.get("wallpaper", ""),
            "Pictures or Wallpaper Engine projects (*.jpg *.jpeg *.png *.webp *.bmp project.json *.json *.pkg *.mp4);;"
            "All files (*)",
            "A picture becomes the desktop wallpaper (through Noctalia, swww, hyprpaper or swaybg). A Wallpaper "
            "Engine project.json (or a file in its folder) is played with linux-wallpaperengine. Empty: the "
            "wallpaper is left alone.")
        v.addWidget(card)
        card, c = _card("SETTINGS PROFILE", "Profiles are made in Settings (top of the page). Each holds its own "
                                             "hotkeys, sizes, sounds…")
        self.profile = QComboBox()
        self.profile.addItem("Default settings", "")
        for p in self.launcher.settings.profiles():
            if p["id"] != "default":
                self.profile.addItem(f"Profile: {p['name']}", p["id"])
        self.profile.setCurrentIndex(max(0, self.profile.findData(t.get("profile", ""))))
        self.profile.currentIndexChanged.connect(lambda _i: self.changed())
        c.addWidget(self.profile)
        v.addWidget(card)
        cael = self.launcher.modules.get("caelestia")
        if cael is not None and not IS_WIN:
            n = len(((t.get("caelestia") or {}).get("colours")) or {})
            card, c = _card("CAELESTIA", "Caelestia colours saved for this theme are written to Caelestia's scheme "
                                          "whenever this theme is applied. Edit them in Caelestia & Glass → Colours "
                                          "→ Save for a Luminescent theme.")
            c.addWidget(widgets.label(f"{n} colour{'s' if n != 1 else ''} saved for this theme" if n else
                                      "None saved for this theme.", 12, tone="secondary"))
            if n:
                drop = widgets.button("Forget them", "ghost")
                drop.clicked.connect(lambda: (self.t.pop("caelestia", None), drop.setEnabled(False),
                                              self.changed()))
                c.addWidget(drop, 0, Qt.AlignmentFlag.AlignLeft)
            v.addWidget(card)
        v.addStretch()
        return page

    def _section_page(self, key: str) -> QWidget:
        title = self.section_title(key)
        desc = next((d for k, _t, _i, d in EXTRA_SECTIONS if k == key), f"Every window of {title}.")
        page, v = self._page(title, desc + " Anything left on THEME follows the theme, so it changes with it.")
        entry = self.t["modules"].get(key) or {}
        card, c = _card("ACCENT", "Quickest: one colour, and its lighter, darker and muted shades are worked out "
                                  "for you.")
        row = QHBoxLayout()
        acc = colourpick.ColourSwatch(entry.get("accent"), f"{title}: accent", unset_text="Theme", width=170,
                                      live=True)
        acc.changed.connect(lambda val, k=key: self._set_section(k, "accent", val))
        row.addWidget(acc)
        clear = widgets.icon_button("✕", "Back to the theme's accent", 26, 11, ghost=True)
        clear.clicked.connect(lambda _c=False, k=key, s=acc: (s.set_value(None), self._set_section(k, "accent", None)))
        row.addWidget(clear)
        row.addStretch()
        match = widgets.button("Match all its colours to it", "ghost",
                               "Backgrounds, text and borders for this part only, picked to go with its accent")
        match.clicked.connect(lambda _c=False, k=key: self._generate_section(k))
        row.addWidget(match)
        c.addLayout(row)
        v.addWidget(card)

        rows = {}
        for ctitle, roles in (("ACCENT SHADES", ("primary_hover", "primary_muted", "primary_text", "on_primary")),
                              ("BACKGROUNDS", ("bg_base", "bg_card", "bg_card_hover", "bg_frame", "bg_header",
                                               "border")),
                              ("TEXT", ("text_primary", "text_secondary", "text_muted"))):
            card, c = _card(ctitle)
            for role in roles:
                r = RoleRow(role, dict(theme.COLOUR_ROLES)[role], self._section_value(key, role),
                            lambda rl, val, k=key: self._set_section(k, rl, val), unset_text="Theme")
                rows[role] = r
                c.addWidget(r)
            v.addWidget(card)
        rows["primary"] = acc
        self._role_rows[key] = rows

        card, c = _card("SHAPE")
        own = QCheckBox("Its own corner radius")
        own.setChecked("radius" in entry)
        rad = _SliderRow(0, 30, int(entry.get("radius", self.radius_value())), "px",
                         lambda k=key: self._set_section(k, "radius", rad.value()) if own.isChecked() else None)
        rad.setEnabled(own.isChecked())
        own.toggled.connect(lambda on, k=key: (rad.setEnabled(on),
                                               self._set_section(k, "radius", rad.value() if on else None)))
        c.addWidget(own)
        c.addWidget(rad)
        v.addWidget(card)

        copy_btn = widgets.button("Copy the look of…", "ghost", "Take the colours of another part of the theme")
        copy_btn.clicked.connect(lambda _c=False, k=key, b=copy_btn: self._copy_menu(k, b))
        v.addWidget(copy_btn, 0, Qt.AlignmentFlag.AlignLeft)
        v.addStretch()
        return page

    def _path_row(self, layout, label, value, flt, help_text) -> QLineEdit:
        layout.addWidget(widgets.label(label, 12.5))
        row = QHBoxLayout()
        edit = QLineEdit(value)
        edit.textChanged.connect(lambda _t: self.changed())
        b = widgets.button("Browse…")
        b.clicked.connect(lambda: self._browse(edit, flt))
        c = widgets.icon_button("✕", "Clear", 26, 11, ghost=True)
        c.clicked.connect(lambda: edit.setText(""))
        row.addWidget(edit, 1)
        row.addWidget(b)
        row.addWidget(c)
        layout.addLayout(row)
        h = widgets.label(help_text, 11, tone="muted")
        h.setWordWrap(True)
        layout.addWidget(h)
        return edit

    def _browse(self, edit: QLineEdit, flt: str):
        p, _ = QFileDialog.getOpenFileName(self, "Choose a file", edit.text(), flt)
        if p:
            edit.setText(p)

    # ── values ─────────────────────────────────────────────────────────────
    def radius_value(self) -> int:
        return self.radius.value() if hasattr(self, "radius") else int(self.t.get("radius", 10))

    def _global_value(self, role: str):
        return self.t["gradients"].get(role) or self.t["colours"].get(role) or theme.PRESETS["dark"][role]

    def _set_global(self, role: str, value):
        if isinstance(value, dict):
            g = theme.clean_gradient(value)
            if g:
                self.t["gradients"][role] = g
                self.t["colours"][role] = theme.gradient_mid(g)
        elif value:
            self.t["gradients"].pop(role, None)
            self.t["colours"][role] = value
        self.changed()

    def _section_value(self, key: str, role: str):
        e = self.t["modules"].get(key) or {}
        return (e.get("gradients") or {}).get(role) or (e.get("colours") or {}).get(role)

    def _set_section(self, key: str, what: str, value):
        e = self.t["modules"].setdefault(key, {"colours": {}, "gradients": {}})
        e.setdefault("colours", {})
        e.setdefault("gradients", {})
        if what == "accent":
            if value:
                e["accent"] = value
            else:
                e.pop("accent", None)
        elif what == "radius":
            if value is None:
                e.pop("radius", None)
            else:
                e["radius"] = int(value)
        else:
            e["colours"].pop(what, None)
            e["gradients"].pop(what, None)
            if isinstance(value, dict):
                g = theme.clean_gradient(value)
                if g:
                    e["gradients"][what] = g
            elif value:
                e["colours"][what] = value
        self.changed()

    def result_theme(self) -> dict:
        t = copy.deepcopy(self.t)
        if hasattr(self, "name"):
            t["name"] = self.name.text().strip() or t["name"]
            t.update({"font_family": self.font.currentFont().family(), "radius": self.radius.value(),
                      "glow": self.glow.isChecked(), "background_image": self.bg_image.text().strip(),
                      "image_opacity": self.opacity.value() / 100.0, "icon": self.icon.text().strip()})
        if hasattr(self, "wallpaper"):
            t.update({"wallpaper": self.wallpaper.text().strip(), "profile": self.profile.currentData() or ""})
        t["modules"] = {k: v for k, v in ((k, theme.clean_section(v)) for k, v in t["modules"].items()) if v}
        t["gradients"] = theme.clean_gradients(t.get("gradients"))
        return t

    # ── helpers ────────────────────────────────────────────────────────────
    def _generate(self):
        accent = self.t["colours"].get("primary", theme.P.primary)
        light = theme.luminance(self.t["colours"].get("bg_base", "#131317")) > 140
        for role, value in theme.generate(accent, light).items():
            if role == "primary":
                continue
            self.t["colours"][role] = value
            self.t["gradients"].pop(role, None)
        for role, row in self._role_rows.get("colours", {}).items():
            row.set_value(self._global_value(role))
        self.changed()

    def _generate_section(self, key: str):
        e = self.t["modules"].get(key) or {}
        accent = e.get("accent")
        if not accent:
            widgets.message(self, "Match its colours", "Pick an accent for it first.")
            return
        base = self.base_palette()
        light = theme.luminance((e.get("colours") or {}).get("bg_base", base.bg_base)) > 140
        for role, value in theme.generate(accent, light).items():
            if role != "primary":
                self._set_section(key, role, value)
        self._refresh_rows(key)

    def _refresh_rows(self, key: str):
        for role, row in self._role_rows.get(key, {}).items():
            if role == "primary":
                row.set_value((self.t["modules"].get(key) or {}).get("accent"))
            else:
                row.set_value(self._section_value(key, role))

    def _reset_section(self):
        key = self.current
        if key == "colours":
            self._generate()
            return
        if not widgets.confirm(self, "Reset", f"Put {self.section_title(key)} back to the theme's own look?",
                               yes="Reset"):
            return
        self.t["modules"].pop(key, None)
        old = self._pages.pop(key, None)
        if old is not None:
            self.stack.removeWidget(old)
            old.deleteLater()
        self._role_rows.pop(key, None)
        self.show_section(key)
        self.changed()

    def _copy_menu(self, key: str, anchor: QWidget):
        m = widgets.menu(self)
        m.addAction("The theme's own colours (clear this part)", lambda: self._copy_from(key, None))
        others = [(k, self.section_title(k)) for k in self.t["modules"] if k != key]
        if others:
            m.addSeparator()
        for k, title in others:
            m.addAction(title, lambda src=k: self._copy_from(key, src))
        m.exec(anchor.mapToGlobal(anchor.rect().bottomLeft()))

    def _copy_from(self, key: str, src: str | None):
        if src is None:
            self.t["modules"].pop(key, None)
        else:
            self.t["modules"][key] = copy.deepcopy(self.t["modules"][src])
        old = self._pages.pop(key, None)
        if old is not None:
            self.stack.removeWidget(old)
            old.deleteLater()
        self._role_rows.pop(key, None)
        self.show_section(key)
        self.changed()

    def reject(self):
        if json.dumps(self.result_theme(), sort_keys=True) != self._saved and not widgets.confirm(
                self, "Unsaved changes", "Close without saving your changes to this theme?", yes="Discard",
                no="Keep editing"):
            return
        super().reject()


class _SliderRow(QWidget):
    """A slider with its value next to it."""

    def __init__(self, lo: int, hi: int, value: int, unit: str, on_change):
        super().__init__()
        lay = QHBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(12)
        self.slider = QSlider(Qt.Orientation.Horizontal)
        self.slider.setRange(lo, hi)
        self.slider.setValue(value)
        self.lbl = widgets.label(f"{value} {unit}", 11.5, mono=True, tone="secondary")
        self.lbl.setFixedWidth(52)
        self.lbl.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
        self.slider.valueChanged.connect(lambda v: (self.lbl.setText(f"{v} {unit}"), on_change()))
        lay.addWidget(self.slider, 1)
        lay.addWidget(self.lbl)

    def value(self) -> int:
        return self.slider.value()
