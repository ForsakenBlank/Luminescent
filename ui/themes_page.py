"""
Themes page: a grid of theme cards grouped in colour-coded folders.

Click a card to apply it. Right-click → Settings… opens the theme editor: every colour
(each one can be a gradient), font, corner radius, glow, background image + its opacity,
icon, wallpaper, settings profile, and a whole look of their own (colours, gradients,
corner radius) for each module, the notifications and the on-screen warnings, and the Caelestia shell's colours
for the theme. Also
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
from PyQt6.QtGui import QPainter, QPainterPath, QColor, QPixmap, QFont, QPen, QShortcut, QKeySequence

from core import theme, widgets, colourpick, hypr, IS_WIN
from core.guard import alive
from core.themes import ThemeStore, new_theme
from ui import theme_extras, colour_tools

COLS, GAP = 5, 16
PREV_W, PREV_H = 190, 112
IMAGE_FILTER = "Images (*.png *.jpg *.jpeg *.bmp *.gif *.webp);;All files (*)"
BUILTIN_GROUP, MINE_GROUP = "builtin", "mine"
MINE_COLOUR = "#3DBE96"
DESKTOP_TITLES = {"caelestia_colours": "Caelestia colours", "caelestia_look": "Caelestia look", "glass": "Glass",
                  "borders": "Window borders", "terminal": "Terminal"}


def _pal(t: dict) -> theme.Palette:
    """The palette a theme would give, without switching to it."""
    return theme.palette_from(ThemeStore.palette_values(t))


def _text(qp: QPainter, r: QRectF, text: str, colour: str, px: float, weight: int = 400, mono: bool = False,
          align=Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter, spacing: float = 0.0):
    qp.setPen(QColor(colour))
    qp.setFont(theme.qfont(px, weight, mono, spacing, scaled=False))
    qp.drawText(r, align, text)


def paint_overlay(qp: QPainter, r: QRectF, p: theme.Palette, s: float = 1.0, reg=None):
    """A small mock-up of the overlay in palette `p`: page switcher, a title, three cards. `reg` (optional)
    collects where each colour is drawn (see colour_tools)."""
    add = reg.add if reg is not None else (lambda *_a: None)
    add(r, "bg_base")
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
    add(nav, "bg_header")
    pill = QRectF(nav.left() + 4 * s, nav.top() + 3 * s, nav.width() * 0.3, 9 * s)
    add(pill, "primary")
    qp.setBrush(theme.qbrush("primary", pill, 1.0, p))
    qp.drawRoundedRect(pill, 4.5 * s, 4.5 * s)
    line = QRectF(nav.left(), nav.top(), nav.width(), max(1.0, 1.5 * s))
    qp.fillRect(line, theme.qbrush("primary", line, 1.0, p))
    for i in range(2):
        t = QColor(p.text_secondary)
        t.setAlphaF(0.6)
        qp.setBrush(t)
        qp.drawRoundedRect(QRectF(pill.right() + (8 + i * 26) * s, pill.top() + 3 * s, 18 * s, 3 * s), 1.5, 1.5)
        add(QRectF(pill.right() + (8 + i * 26) * s, pill.top(), 18 * s, 9 * s), "text_secondary")
    # a title, an accent tick
    top = nav.bottom() + 9 * s
    tc = QColor(p.text_primary)
    qp.setBrush(tc)
    qp.drawRoundedRect(QRectF(r.left() + pad, top, 46 * s, 5 * s), 2.5 * s, 2.5 * s)
    add(QRectF(r.left() + pad, top - 2 * s, 46 * s, 9 * s), "text_primary")
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
        add(c, "bg_card", "border")
        qp.setPen(Qt.PenStyle.NoPen)
        dot = QRectF(c.left() + 6 * s, c.top() + 6 * s, 5 * s, 5 * s)
        add(dot, "primary")
        qp.setBrush(theme.qbrush("primary", dot, 1.0, p))
        qp.drawEllipse(dot)
        for j in range(3):
            lc = QColor(p.text_secondary if j else p.text_primary)
            lc.setAlphaF(0.7)
            qp.setBrush(lc)
            qp.drawRoundedRect(QRectF(c.left() + 6 * s, c.top() + (16 + j * 8) * s, c.width() * (0.7 - j * 0.15),
                                      3 * s), 1.5, 1.5)
    qp.restore()


def paint_panel(qp: QPainter, r: QRectF, p: theme.Palette, title: str = "PANEL", reg=None):
    """A floating panel in palette `p`: header, accent tick, a card, a field, a button, a switch, a slider."""
    add = reg.add if reg is not None else (lambda *_a: None)
    rad = float(p.radius)
    add(r, "bg_base", "border")
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
    add(head, "bg_header")
    qp.setPen(QPen(QColor(p.border), 1))
    qp.drawLine(QPointF(head.left(), head.bottom()), QPointF(head.right(), head.bottom()))
    tick = QRectF(head.left(), head.bottom() - 1, 36, 2)
    qp.fillRect(tick, theme.qbrush("primary", tick, 1.0, p))
    add(tick.adjusted(0, -2, 0, 2), "primary")
    _text(qp, head.adjusted(14, 0, -30, 0), title.upper(), p.text_primary, 10, 600, True, spacing=0.16)
    _text(qp, head.adjusted(0, 0, -12, 0), "✕", p.text_secondary, 11,
          align=Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
    x, y, w = r.left() + 14, head.bottom() + 12, r.width() - 28
    card = QRectF(x, y, w, 58)
    qp.setPen(QPen(theme.qbrush("border", card, 1.0, p), 1))
    qp.setBrush(theme.qbrush("bg_card", card, 1.0, p))
    qp.drawRoundedRect(card, max(0, rad - 2), max(0, rad - 2))
    add(card, "bg_card", "border")
    add(card.adjusted(12, 8, -80, -32), "text_primary")
    add(card.adjusted(12, 26, -80, -16), "text_secondary")
    add(card.adjusted(12, 40, -80, -2), "text_muted")
    _text(qp, card.adjusted(12, 8, -8, -32), "Heading text", p.text_primary, 12.5, 600)
    _text(qp, card.adjusted(12, 26, -8, -16), "Secondary text", p.text_secondary, 11)
    _text(qp, card.adjusted(12, 40, -8, -2), "Muted · hint", p.text_muted, 10.5)
    badge = QRectF(card.right() - 62, card.top() + 9, 52, 18)
    qp.setPen(Qt.PenStyle.NoPen)
    qp.setBrush(theme.qbrush("primary_muted", badge, 1.0, p))
    qp.drawRoundedRect(badge, 9, 9)
    add(badge, "primary_muted", "primary_text")
    _text(qp, badge, "TAG", p.primary_text, 9, 600, True, Qt.AlignmentFlag.AlignCenter, 0.1)
    y = card.bottom() + 10
    field = QRectF(x, y, w * 0.62, 28)
    qp.setPen(QPen(QColor(p.primary), 1))
    qp.setBrush(theme.qbrush("bg_frame", field, 1.0, p))
    qp.drawRoundedRect(field, max(0, rad - 4), max(0, rad - 4))
    add(field, "bg_frame")
    _text(qp, field.adjusted(10, 0, 0, 0), "A field", p.text_primary, 11.5)
    btn = QRectF(field.right() + 8, y, w - field.width() - 8, 28)
    qp.setPen(Qt.PenStyle.NoPen)
    qp.setBrush(theme.qbrush("primary", btn, 1.0, p))
    qp.drawRoundedRect(btn, max(0, rad - 4), max(0, rad - 4))
    add(btn, "primary")
    add(btn.adjusted(btn.width() * 0.25, 6, -btn.width() * 0.25, -6), "on_primary")
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
        name = widgets.elided(t["name"], 13, 500)
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
        self.setToolTip((t["name"] + "\n" + tip + "\nClick to use · right-click for settings").replace("\n\n", "\n"))

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
        cols = max(1, min(COLS, (avail + GAP) // (170 + GAP)))     # fewer columns on a narrow page, never cut off
        card_w = max(150, (avail - GAP * (cols - 1)) // cols)
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
                    grid.addWidget(c, i // cols, i % cols)
                h = max((c.sizeHint().height() for c in cards), default=160)
                n = len(cards)
                grid.addWidget(AddCard(self, folder["id"] if folder else "", card_w, h), n // cols, n % cols)
                group.addLayout(grid)
            v.addWidget(box)
            self._reorder.add(box, header)
            header.setCursor(Qt.CursorShape.PointingHandCursor)    # still a click to fold, so keep the hand
            self._order.append(key)
        v.addStretch()
        # switching a theme rebuilds the cards: stay where you were instead of jumping to the top
        keep = self.scroll.verticalScrollBar().value()
        old = self.scroll.takeWidget()
        self.scroll.setWidget(holder)
        if old is not None:
            old.deleteLater()
        if keep:
            self._scroll_to(keep)

    def _scroll_to(self, y: int):
        bar = self.scroll.verticalScrollBar()
        bar.setValue(y)
        if bar.value() != y:                     # the new cards are not laid out yet: once they are
            QTimer.singleShot(0, lambda: bar.setValue(y))

    def state(self) -> dict:
        return {"scroll": self.scroll.verticalScrollBar().value()}

    def restore_state(self, st: dict):
        if isinstance(st, dict) and st.get("scroll"):
            self._scroll_to(int(st["scroll"]))

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
        self.setMouseTracking(True)
        self.reg = colour_tools._Reg()
        self.scale, self.top = 1.0, 0.0

    def _key_at(self, pos) -> str | None:
        if self.dlg.current not in ("colours", "caelestia_colours"):
            return None
        return self.reg.at(QPointF((pos.x()) / self.scale, (pos.y() - self.top) / self.scale))

    def mouseMoveEvent(self, e):
        k = self._key_at(e.position())
        if k is None:
            self.setToolTip("")
            self.unsetCursor()
            return
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        if self.dlg.current == "caelestia_colours":
            _g, name, where = colour_tools.cael_info(k)
        else:
            name, where = colour_tools.lum_name(k), ROLE_HELP.get(k, "")
        self.setToolTip(f"{name}\n{where}\nClick to find it on the left")

    def mousePressEvent(self, e):
        k = self._key_at(e.position())
        if k is not None:
            self.dlg.jump_to(k)

    def paintEvent(self, e):
        qp = QPainter(self)
        qp.setRenderHint(QPainter.RenderHint.Antialiasing)
        dlg = self.dlg
        key = dlg.current
        base = dlg.base_palette()
        w = self.width()
        y = 4.0
        self.reg = colour_tools._Reg()
        if key == "colours":
            self.scale, self.top = 1.0, y
            qp.translate(0, y)
            colour_tools.paint_luminescent(qp, w, dlg.lum_values(), self.reg, dlg.highlight)
            return
        if key in ("general", "applied"):
            h = min(w * 0.6, 260)
            paint_overlay(qp, QRectF(0, y, w, h), base, h / 112.0)
            y += h + 16
            paint_panel(qp, QRectF(0, y, w, 250), base, "A PANEL")
            return
        if key in ("caelestia_colours", "caelestia_look"):
            cols = dlg.cael_values()
            if not cols:
                _text(qp, QRectF(0, y, w, 20), "Caelestia's colours were not found.", base.text_muted, 11.5)
                return
            self.scale, self.top = w / colour_tools.PREVIEW_W, y
            qp.translate(0, y)
            qp.scale(self.scale, self.scale)
            colour_tools.paint_caelestia(qp, cols, dlg.cael_solid(), self.reg,
                                         dlg.highlight if key == "caelestia_colours" else None)
            return
        if key == "terminal":
            theme_extras.paint_terminal(qp, w, y, dlg)
            return
        if key == "glass":
            theme_extras.paint_glass(qp, w, y, dlg)
            return
        if key == "borders":
            theme_extras.paint_borders(qp, w, y, dlg)
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

    def __init__(self, role: str, label: str, value, on_change, unset_text: str = "", fav: bool | None = None,
                 on_star=None, on_hover=None):
        super().__init__()
        self.role = role
        self.on_hover = on_hover
        self.setObjectName("roleRow")
        self.setAttribute(Qt.WidgetAttribute.WA_StyledBackground)
        self.setStyleSheet(f"#roleRow {{ border-radius: 8px; }}"
                           f"#roleRow[hl=\"true\"] {{ background: {theme.rgba(theme.P.primary, 0.12)}; }}")
        lay = QHBoxLayout(self)
        lay.setContentsMargins(0, 3, 0, 3)
        lay.setSpacing(10)
        self.star = None
        if fav is not None:
            self.star = colour_tools.Star(fav)
            self.star.toggled.connect(lambda on: on_star(role, on) if on_star else None)
            lay.addWidget(self.star)
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
        grad = widgets.icon_button("◐", "Make it a gradient: adds a second colour that goes with it, then opens the "
                                        "gradient editor", 26, 12, ghost=True)

        def to_gradient():
            v = self.swatch.value()
            if not isinstance(v, dict):
                first = v if isinstance(v, str) and v else theme.P.primary
                g = {"type": "linear", "angle": 135, "stops": [[0.0, first], [1.0, colourpick.partner(first)]]}
                self.swatch.set_value(g)
                on_change(role, g)
            self.swatch.open_picker()
        grad.clicked.connect(to_gradient)
        lay.addWidget(grad)
        self.clear = widgets.icon_button("✕", "Back to the theme's colour", 26, 11, ghost=True)
        self.clear.setVisible(bool(unset_text))
        self.clear.clicked.connect(lambda: (self.swatch.set_value(None), on_change(role, None)))
        lay.addWidget(self.clear)

    def set_value(self, v):
        if self.swatch.value() != v:
            self.swatch.set_value(v)

    def set_fav(self, on: bool):
        if self.star is not None:
            self.star.blockSignals(True)
            self.star.setChecked(on)
            self.star.blockSignals(False)
            self.star._paint()

    def set_highlight(self, on: bool):
        self.setProperty("hl", on)
        self.style().unpolish(self)
        self.style().polish(self)

    def enterEvent(self, e):
        if self.on_hover:
            self.on_hover(self.role)
        super().enterEvent(e)

    def leaveEvent(self, e):
        if self.on_hover:
            self.on_hover(None)
        super().leaveEvent(e)


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
        self.highlight: str | None = None          # the colour the mouse is on: lit up in the preview
        self.cael_page: colour_tools.CaelestiaPage | None = None
        self._restoring = False
        self.history: colour_tools.History | None = None
        recent = (self.store_data().get("recent_colours") or {}).get(t.get("id", ""), [])
        self.recents: list[dict] = [r for r in recent if isinstance(r, dict) and "new" in r][:30]
        self._commit_timer = QTimer(self)
        self._commit_timer.setSingleShot(True)
        self._commit_timer.setInterval(700)        # a burst of edits (dragging in the picker) is one undo step
        self._commit_timer.timeout.connect(lambda: self._commit())
        self._refresh_timer = QTimer(self)
        self._refresh_timer.setSingleShot(True)
        self._refresh_timer.setInterval(60)
        self._refresh_timer.timeout.connect(self._refresh_colour_pages)
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
        heading("DESKTOP")
        if "caelestia" in launcher.modules and not IS_WIN:
            add("caelestia_colours", "◐  Caelestia colours")
            add("caelestia_look", "◐  Caelestia look")
            add("glass", "▢  Glass")
        if hypr.available():
            add("borders", "▭  Window borders")
        add("terminal", "▣  Terminal")
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
        self.undo_btn = widgets.icon_button("↶", "Undo (Ctrl+Z)", 30, 14, ghost=True)
        self.undo_btn.clicked.connect(self.undo)
        self.redo_btn = widgets.icon_button("↷", "Redo (Ctrl+Shift+Z or Ctrl+Y)", 30, 14, ghost=True)
        self.redo_btn.clicked.connect(self.redo)
        fl.addWidget(self.undo_btn)
        fl.addWidget(self.redo_btn)
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
        self.history = colour_tools.History(self.result_theme())
        for seq in (QKeySequence.StandardKey.Undo,):
            QShortcut(QKeySequence(seq), self, activated=self.undo)
        for seq in ("Ctrl+Shift+Z", "Ctrl+Y"):
            QShortcut(QKeySequence(seq), self, activated=self.redo)
        self._undo_buttons()

    # ── palettes for the preview ───────────────────────────────────────────
    def base_palette(self) -> theme.Palette:
        return _pal(self.result_theme())

    def section_palette(self, key: str) -> theme.Palette:
        return theme.section_palette(self.base_palette(), theme.clean_section(self.t["modules"].get(key) or {}))

    def section_title(self, key: str) -> str:
        if key in DESKTOP_TITLES:
            return DESKTOP_TITLES[key]
        for k, title, _i, _d in EXTRA_SECTIONS:
            if k == key:
                return title
        m = self.launcher.modules.get(key)
        return m.title if m else key

    def changed(self):
        self._redraw.start()
        dirty = json.dumps(self.result_theme(), sort_keys=True) != self._saved
        self.status.setText("UNSAVED CHANGES" if dirty else "")
        if self.history is not None and not self._restoring:
            self._commit_timer.start()

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
        self.reset_btn.setVisible(key not in ("general", "applied", "colours") and key not in DESKTOP_TITLES)
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
        if key == "caelestia_colours":
            return self._caelestia_page()
        if key == "caelestia_look":
            return theme_extras.caelestia_look_page(self)
        if key == "glass":
            return theme_extras.glass_page(self)
        if key == "borders":
            return theme_extras.borders_page(self)
        if key == "terminal":
            return theme_extras.terminal_page(self)
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

        from settings_ui import HotkeyEdit
        card, c = _card("KEY")
        hint = widgets.label("Press it anywhere to switch to this theme. Next and previous theme keys are in "
                             "Settings → Overlay & Interface.", 11, tone="muted")
        hint.setWordWrap(True)
        c.addWidget(hint)
        self.hotkey = HotkeyEdit()
        self.hotkey.set_value(t.get("hotkey", ""))
        self.hotkey.recording_changed.connect(self.launcher.hotkeys.set_suspended)
        self.hotkey.conflict_fn = self._key_conflict
        self.hotkey.edit.textChanged.connect(lambda _t: self.changed())
        c.addWidget(self.hotkey)
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

    def _key_conflict(self, combo: str) -> str:
        """What else uses `combo` ('' when nothing, or only this theme)."""
        from core import hotkeys as hk
        combo = hk.normalize(combo)
        if not combo:
            return ""
        other = next((o["name"] for o in self.store.themes()
                      if o["id"] != self.t["id"] and hk.normalize(str(o.get("hotkey") or "")) == combo), "")
        if other:
            return f"Already used by the theme {other}"
        owner = self.launcher.hotkeys.owner_of(combo)
        return f"Already used by {owner}" if owner and owner != "themes" else ""

    def _colours_page(self) -> QWidget:
        page, v = self._page("Colours", "Every colour of the theme. Any of them can be a gradient: open one and "
                                        "switch to Gradient. Modules, notifications and warnings use these unless "
                                        "you give them their own (left). Hover a colour to see it in the preview.")
        v.addWidget(widgets.label("QUICK TOOLS", 9.5, 600, mono=True, spacing=0.14, tone="muted"))
        v.addWidget(colour_tools.tool_bar(self, "lum"))
        self._lum_recent = colour_tools.RecentCard(self, "lum")
        v.addWidget(self._lum_recent)
        self._lum_fav_card, self._lum_fav_box = _card("★ FAVOURITES")
        v.addWidget(self._lum_fav_card)
        self._lum_fav_rows: dict[str, RoleRow] = {}
        rows = {}
        favs = self.favourites()
        for title, roles in (("ACCENT", ("primary", "primary_hover", "primary_muted", "primary_text", "on_primary")),
                             ("BACKGROUNDS", ("bg_base", "bg_card", "bg_card_hover", "bg_frame", "bg_header",
                                              "border")),
                             ("TEXT", ("text_primary", "text_secondary", "text_muted")),
                             ("FOLDERS", ("folder_color", "folder_border"))):
            card, c = _card(title)
            for role in roles:
                row = self._lum_row(role, f"lum:{role}" in favs)
                rows[role] = row
                c.addWidget(row)
            v.addWidget(card)
        self._role_rows["colours"] = rows
        self._fill_lum_favourites()
        v.addStretch()
        return page

    def _lum_row(self, role: str, fav: bool) -> "RoleRow":
        return RoleRow(role, dict(theme.COLOUR_ROLES)[role], self._global_value(role), self._set_global_tracked,
                       fav=fav, on_star=lambda r, on: self.toggle_favourite(f"lum:{r}", on),
                       on_hover=self.set_highlight)

    def _fill_lum_favourites(self):
        if not hasattr(self, "_lum_fav_box"):
            return
        box = self._lum_fav_box
        while box.count() > 1:                      # keep the title
            it = box.takeAt(1)
            if it.widget() is not None:
                it.widget().deleteLater()
        self._lum_fav_rows = {}
        roles = [f.split(":", 1)[1] for f in self.favourite_list() if f.startswith("lum:")]
        roles = [r for r in roles if r in dict(theme.COLOUR_ROLES)]
        for r in roles:
            row = self._lum_row(r, True)
            box.addWidget(row)
            self._lum_fav_rows[r] = row
        self._lum_fav_card.setVisible(bool(roles))

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
        if "caelestia" in self.launcher.modules and not IS_WIN:
            card, c = _card("CAELESTIA", "The Caelestia shell's colours change with the theme too.")
            go = widgets.button("Caelestia colours for this theme…", "ghost")
            go.clicked.connect(lambda: self.show_section("caelestia_colours"))
            c.addWidget(go, 0, Qt.AlignmentFlag.AlignLeft)
            v.addWidget(card)
        v.addStretch()
        return page

    # ── caelestia ──────────────────────────────────────────────────────────
    # luminescent's colour roles that each of caelestia's colours follows for "match caelestia to this theme"
    CAELESTIA_MATCH = {
        "primary": "primary", "surfaceTint": "primary", "onPrimary": "on_primary",
        "primaryContainer": "primary_muted", "onPrimaryContainer": "primary_text",
        "background": "bg_base", "surface": "bg_base", "surfaceDim": "bg_base", "base": "bg_base",
        "surfaceContainerLowest": "bg_base", "crust": "bg_base", "mantle": "bg_frame",
        "surfaceContainerLow": "bg_frame", "surfaceContainer": "bg_card", "surfaceVariant": "bg_card",
        "surface0": "bg_card", "surfaceContainerHigh": "bg_card_hover", "surfaceBright": "bg_card_hover",
        "surface1": "bg_card_hover", "surfaceContainerHighest": "bg_header", "surface2": "bg_header",
        "onBackground": "text_primary", "onSurface": "text_primary", "text": "text_primary",
        "onSurfaceVariant": "text_secondary", "subtext1": "text_secondary", "subtext0": "text_muted",
        "outline": "text_muted", "overlay0": "text_muted", "outlineVariant": "border",
    }

    def _cael_colours(self) -> dict:
        return self.t.setdefault("caelestia", {}).setdefault("colours", {})

    def _caelestia_page(self) -> QWidget:
        from modules import caelestia as cael
        page, v = self._page("Caelestia colours",
                             "The Caelestia shell's colours for this theme: its bar, launcher, dashboard and "
                             "notifications, and the apps Caelestia themes (terminal, btop, GTK and Qt apps...). "
                             "They go in whenever you switch to this theme; colours you leave alone follow "
                             "Caelestia. Quickest: Main colours, then fine-tune any colour below.")
        self._cael_now = cael.scheme()
        mod = self.launcher.modules.get("caelestia")
        if mod is not None and not mod.running:
            off = widgets.label("Caelestia & Glass is turned off, so these are only saved. Turn it on in Settings "
                                "to have them applied.", 11.5, tone="muted")
            off.setWordWrap(True)
            v.addWidget(off)
        if not self._cael_now:
            v.addWidget(widgets.label("Caelestia's colour scheme was not found (is Caelestia installed and "
                                      "running?).", 12, tone="secondary"))
            v.addStretch()
            return page
        self.cael_page = colour_tools.CaelestiaPage(self)
        self.cael_page.build(v)
        v.addStretch()
        return page

    def _fill_caelestia(self):
        if self.cael_page is not None:
            self.cael_page.refresh()

    def cael_values(self) -> dict:
        """Every Caelestia colour as it would be with this theme: Caelestia's own with the theme's on top."""
        if not hasattr(self, "_cael_now"):
            from modules import caelestia as cael
            self._cael_now = cael.scheme()
        return {**(self._cael_now or {}), **(((self.t.get("caelestia") or {}).get("colours")) or {})}

    def cael_solid(self) -> float:
        """How solid Caelestia's panels are with this theme (1 = not see-through)."""
        from modules import caelestia_look as look
        shell = (self.t.get("caelestia") or {}).get("shell") or {}
        if not hasattr(self, "_shell_now"):
            self._shell_now = look.read_shell()

        def opt(key, default):
            return shell.get(key, look.get(self._shell_now, key, default))
        try:
            return float(opt("appearance.transparency.base", 0.85)) if opt("appearance.transparency.enabled",
                                                                            False) else 1.0
        except (TypeError, ValueError):
            return 1.0

    def _set_caelestia(self, key: str, value):
        self.set_cael(key, value)

    def set_cael(self, key: str, value):
        own = self._cael_colours()
        before = own.get(key)
        new = value.upper() if isinstance(value, str) and value else None
        if (before or None) == new:
            return
        if new:
            own[key] = new
        else:
            own.pop(key, None)
        self._note_change("cael", {key: before}, {key: new}, colour_tools.cael_info(key)[1],
                          base={key: (self._cael_now or {}).get(key)})
        self._refresh_timer.start()
        self.changed()

    def _match_caelestia(self):
        changes = {}
        for k, role in self.CAELESTIA_MATCH.items():
            if k in (self._cael_now or {}):
                val = self._global_value(role)
                changes[k] = (theme.gradient_mid(val) if isinstance(val, dict) else val).upper()
        self.apply_bulk("cael", changes, f"Matched to this theme ({len(changes)} colours)")

    def _forget_caelestia(self):
        own = self._cael_colours()
        if own:
            self.apply_bulk("cael", {k: None for k in own}, f"Caelestia's own colours ({len(own)})")

    # ── favourites, recent changes, undo ───────────────────────────────────
    def store_data(self) -> dict:
        return self.launcher.themes.data

    def store_save(self):
        self.launcher.themes.save()

    def favourite_list(self) -> list[str]:
        return [f for f in (self.store_data().get("favourite_colours") or []) if isinstance(f, str)]

    def favourites(self) -> set[str]:
        return set(self.favourite_list())

    def toggle_favourite(self, fid: str, on: bool):
        favs = [f for f in self.favourite_list() if f != fid]
        if on:
            favs.append(fid)
        self.store_data()["favourite_colours"] = favs
        self.store_save()
        if fid.startswith("cael:") and self.cael_page is not None:
            self.cael_page.fill_favourites()
            self.cael_page.refresh()
        if fid.startswith("lum:"):
            self._fill_lum_favourites()
            for role, row in self._role_rows.get("colours", {}).items():
                row.set_fav(f"lum:{role}" in self.favourites())

    def lum_values(self) -> dict:
        return {role: self._global_value(role) for role, _l in theme.COLOUR_ROLES}

    def _set_global_tracked(self, role: str, value):
        before = self._global_value(role)
        if value is None or value == before:
            return
        self._set_global(role, value)
        self._note_change("lum", {role: before}, {role: value}, colour_tools.lum_name(role))
        self._refresh_timer.start()

    def _note_change(self, scope: str, old: dict, new: dict, label: str, base: dict | None = None):
        """Add to the recent changes. A run of changes to the same colour (dragging in the picker) is one entry
        that keeps the colour from before the run."""
        top = self.recents[0] if self.recents else None
        if top and top["scope"] == scope and list(top["new"]) == list(new) and len(new) == 1 and top.get("live"):
            top["new"] = dict(new)
            return
        self.recents.insert(0, {"scope": scope, "label": label, "old": dict(old), "new": dict(new),
                                "base": dict(base or {}), "live": len(new) == 1})
        del self.recents[30:]

    def apply_bulk(self, scope: str, changes: dict, label: str):
        """Many colours at once (a quick tool, a reset): one step to undo and one entry in recent changes."""
        if not changes:
            return
        self._commit()
        if scope == "cael":
            own = self._cael_colours()
            old = {k: own.get(k) for k in changes}
            for k, v in changes.items():
                if isinstance(v, str) and v:
                    own[k] = v.upper()
                else:
                    own.pop(k, None)
            base = {k: (self._cael_now or {}).get(k) for k in changes}
        elif scope == "term":
            own = self.t.setdefault("terminal", {}).setdefault("colours", {})
            old = {k: own.get(k) for k in changes}
            for k, v in changes.items():
                if isinstance(v, str) and v:
                    own[k] = v.upper()
                else:
                    own.pop(k, None)
            base = theme_extras._term_base(self)
            base = {k: base.get(k) for k in changes}
        else:
            old = {k: self._global_value(k) for k in changes}
            for k, v in changes.items():
                if v:
                    self._set_global(k, v)
            base = {}
        if self.recents:
            self.recents[0]["live"] = False
        self._note_change(scope, old, changes, label, base)
        self.recents[0]["live"] = False
        self._refresh_colour_pages()
        self.changed()
        self._commit(label)

    def reset_recent(self, i: int):
        if not (0 <= i < len(self.recents)):
            return
        r = self.recents[i]
        self.apply_bulk(r["scope"], dict(r["old"]), f"Back: {r['label']}")

    def _refresh_colour_pages(self):
        if self.cael_page is not None:
            self.cael_page.refresh()
        for role, row in self._role_rows.get("colours", {}).items():
            row.set_value(self._global_value(role))
        for role, row in getattr(self, "_lum_fav_rows", {}).items():
            row.set_value(self._global_value(role))
        if hasattr(self, "_lum_recent"):
            self._lum_recent.fill()
        if getattr(self, "_term_refresh", None) is not None:
            try:
                self._term_refresh()
            except RuntimeError:                # the terminal page was built again since
                self._term_refresh = None
        self._undo_buttons()
        self.preview.update()

    def set_highlight(self, key):
        if key != self.highlight:
            self.highlight = key
            self.preview.update()

    def jump_to(self, key: str):
        """The preview was clicked: show that colour's row."""
        if self.current == "caelestia_colours" and self.cael_page is not None:
            self.cael_page.jump_to(key)
        elif self.current == "colours":
            row = self._role_rows.get("colours", {}).get(key)
            if row is not None:
                self._pages["colours"].ensureWidgetVisible(row, 0, 120)
                row.set_highlight(True)
                QTimer.singleShot(1400, lambda: row.set_highlight(False) if alive(row) else None)
        self.set_highlight(key)

    def _commit(self, label: str = "Change"):
        self._commit_timer.stop()
        if self.history is not None and self.history.record(self.result_theme(), label):
            self._undo_buttons()

    def _undo_buttons(self):
        if self.history is None:
            return
        self.undo_btn.setEnabled(bool(self.history.undo_stack))
        self.redo_btn.setEnabled(bool(self.history.redo_stack))
        self.undo_btn.setToolTip(f"Undo {self.history.undo_stack[-1][0].lower()} (Ctrl+Z)"
                                 if self.history.undo_stack else "Nothing to undo")
        self.redo_btn.setToolTip(f"Redo {self.history.redo_stack[-1][0].lower()} (Ctrl+Shift+Z)"
                                 if self.history.redo_stack else "Nothing to redo")

    def undo(self):
        self._commit()
        step = self.history.undo()
        if step:
            self._restore(step[1])
            self.status.setText(f"UNDID: {step[0].upper()}")

    def redo(self):
        self._commit()
        step = self.history.redo()
        if step:
            self._restore(step[1])
            self.status.setText(f"REDID: {step[0].upper()}")

    def _restore(self, t: dict):
        """Put a snapshot back: the theme, the fields on the General and When pages, every colour row. Pages
        that hold parts of the theme in their controls are built again."""
        self._restoring = True
        try:
            self.t = copy.deepcopy(t)
            self.t.setdefault("colours", {})
            self.t["gradients"] = theme.clean_gradients(self.t.get("gradients"))
            self.t.setdefault("modules", {})
            if hasattr(self, "name"):
                for w, setter, val in ((self.name, self.name.setText, t.get("name", "")),
                                       (self.glow, self.glow.setChecked, bool(t.get("glow", True))),
                                       (self.bg_image, self.bg_image.setText, t.get("background_image", "")),
                                       (self.icon, self.icon.setText, t.get("icon", ""))):
                    w.blockSignals(True)
                    setter(val)
                    w.blockSignals(False)
                self.font.blockSignals(True)
                self.font.setCurrentFont(QFont(t.get("font_family", "Geist")))
                self.font.blockSignals(False)
                for sl, val in ((self.radius, int(t.get("radius", 10))),
                                (self.opacity, int(float(t.get("image_opacity", 0.35)) * 100))):
                    sl.slider.blockSignals(True)
                    sl.slider.setValue(val)
                    sl.lbl.setText(f"{val} {sl.lbl.text().split()[-1]}")
                    sl.slider.blockSignals(False)
            if hasattr(self, "wallpaper"):
                self.wallpaper.blockSignals(True)
                self.wallpaper.setText(t.get("wallpaper", ""))
                self.wallpaper.blockSignals(False)
                self.profile.blockSignals(True)
                self.profile.setCurrentIndex(max(0, self.profile.findData(t.get("profile", ""))))
                self.profile.blockSignals(False)
            keep = ("general", "applied", "colours", "caelestia_colours")
            for key in [k for k in self._pages if k not in keep]:
                old = self._pages.pop(key)
                self.stack.removeWidget(old)
                old.deleteLater()
                self._role_rows.pop(key, None)
            if self.current not in keep:
                self.show_section(self.current)
            self._refresh_colour_pages()
        finally:
            self._restoring = False
        self.changed()

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
        if hasattr(self, "hotkey"):
            t["hotkey"] = self.hotkey.value()
            if not t["hotkey"] or self._key_conflict(t["hotkey"]):
                t.pop("hotkey", None)
        if hasattr(self, "wallpaper"):
            t.update({"wallpaper": self.wallpaper.text().strip(), "profile": self.profile.currentData() or ""})
        t["modules"] = {k: v for k, v in ((k, theme.clean_section(v)) for k, v in t["modules"].items()) if v}
        t["gradients"] = theme.clean_gradients(t.get("gradients"))
        theme_extras.tidy(t)
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

    def accept(self):
        # the recent changes stay with the theme for next time
        tid = self.t.get("id", "")
        if tid:
            keep = [{k: v for k, v in r.items() if k != "live"} for r in self.recents[:12]]
            data = self.store_data()
            rec = dict(data.get("recent_colours") or {})
            rec[tid] = keep
            data["recent_colours"] = rec
        super().accept()

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
