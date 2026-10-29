"""
Shared widgets: dim overlays, floating panels, anchor bar, styled dialogs.
"""
import logging
import subprocess

from PyQt6.QtWidgets import (
    QApplication, QWidget, QFrame, QLabel, QPushButton, QVBoxLayout, QHBoxLayout, QSizePolicy,
    QMessageBox, QMenu, QDialog, QLineEdit,
    QDateTimeEdit
)
from PyQt6.QtCore import (Qt, QRect, QPoint, QSize, QObject, QEvent, QDateTime, pyqtSignal, QTimer,
                          QEasingCurve)
from PyQt6.QtGui import QPainter, QColor, QPixmap

from core import IS_WIN, theme, hypr, anim

log = logging.getLogger("widgets")
_uid = 0


def style(w, css: str):
    """Apply `css` to this one widget only.

    A plain w.setStyleSheet("background: x") has no selector, so Qt applies it
    to every descendant too, which silently overrides the app-wide button/input
    styling. Scoping by objectName avoids that.
    """
    global _uid
    if not w.objectName():
        _uid += 1
        w.setObjectName(f"scoped{_uid}")
    w.setStyleSheet(f"#{w.objectName()} {{ {css} }}")


ANCHOR_ICONS = {
    "tl": "↖", "tc": "↑", "tr": "↗",
    "ml": "←", "mc": "·", "mr": "→",
    "bl": "↙", "bc": "↓", "br": "↘",
}

_TOP_TOOL = Qt.WindowType.FramelessWindowHint | Qt.WindowType.WindowStaysOnTopHint | Qt.WindowType.Tool


# ── focus ─────────────────────────────────────────────────────────────────────
def bring_to_front(w: QWidget):
    """Give `w` keyboard focus even though the hotkey's key press went to another program.

    Wayland lets no program take focus by itself, so Hyprland is asked to focus it (right away, or as
    soon as it has mapped the window).
    """
    w.activateWindow()
    if IS_WIN:
        _win_foreground(w)
        return
    sync = hypr.sync()
    if sync is not None and sync.enabled:
        sync.focus(w)


def _win_foreground(w: QWidget):
    """Windows refuses SetForegroundWindow from a background process, so a window opened by a global hotkey
    used to appear without keyboard focus. Attaching to the foreground thread's input lifts that."""
    import ctypes
    u32 = ctypes.windll.user32
    try:
        hwnd = int(w.winId())
        fg = u32.GetForegroundWindow()
        if not fg or fg == hwnd:
            return
        me = ctypes.windll.kernel32.GetCurrentThreadId()
        other = u32.GetWindowThreadProcessId(fg, None)
        attached = bool(other and other != me and u32.AttachThreadInput(other, me, True))
        try:
            u32.BringWindowToTop(hwnd)
            u32.SetForegroundWindow(hwnd)
        finally:
            if attached:
                u32.AttachThreadInput(other, me, False)
        w.activateWindow()
    except Exception:
        pass


def raise_window(w: QWidget):
    w.raise_()
    sync = hypr.sync()
    if sync is not None and sync.enabled:
        sync.raise_window(w)


def _drawing_up() -> bool:
    """A Doodle drawing is on screen. Hyprland's own window move then does nothing (it is stuck on the
    see-through drawing layer), so panels move themselves instead."""
    return any(w.isVisible() and w.property("lum_drag_blocker") for w in QApplication.topLevelWidgets())


def global_cursor(e) -> QPoint:
    """The pointer in global coordinates for a mouse event. On Wayland Qt only knows where it thinks the
    window is, which lags behind while the window is being moved, so ask Hyprland."""
    if hypr.wayland() and hypr.available():
        return hypr.cursor_pos()
    return e.globalPosition().toPoint()


def system_move(w: QWidget, done=None) -> bool:
    """Let the compositor drag window `w` (smoother than moving it ourselves). False = move it yourself."""
    sync = hypr.sync()
    handle = w.windowHandle()
    if sync is None or not sync.enabled or handle is None or _drawing_up():
        return False
    sync.raise_window(w)
    if not handle.startSystemMove():
        return False
    sync.follow(w, done)
    return True


def system_resize(w: QWidget, edges: Qt.Edge, done=None) -> bool:
    sync = hypr.sync()
    handle = w.windowHandle()
    if sync is None or not sync.enabled or handle is None or _drawing_up():
        return False
    sync.raise_window(w)
    if not handle.startSystemResize(edges):
        return False
    sync.follow(w, done)
    return True


def grab_screen(scr) -> QPixmap:
    """A screenshot of one screen. Qt can not grab the screen on Wayland, grim can."""
    if hypr.wayland():
        g = scr.geometry()
        try:
            out = subprocess.run(["grim", "-g", f"{g.x()},{g.y()} {g.width()}x{g.height()}", "-t", "ppm", "-"],
                                 capture_output=True, timeout=10).stdout
            px = QPixmap()
            if out and px.loadFromData(out, "PPM"):
                px.setDevicePixelRatio(px.width() / max(1, g.width()))
                return px
        except (OSError, subprocess.SubprocessError) as e:
            log.warning("grim failed: %s (install grim)", e)
    return scr.grabWindow(0)


# ── screens ───────────────────────────────────────────────────────────────────
def primary_geometry() -> QRect:
    return hypr.available_geometry(QApplication.primaryScreen())


def cursor_screen():
    return hypr.screen_at_cursor()


def cursor_screen_geometry() -> QRect:
    return hypr.available_geometry(cursor_screen())


def home_geometry() -> QRect:
    """The screen new panels and pop-ups go to: the one with the mouse, or the primary screen when
    Settings → Overlay & Interface → Open the overlay on says so."""
    s = anim._settings
    if s is not None and s.get("ui.overlay_screen", "cursor") == "primary":
        return primary_geometry()
    g = cursor_screen_geometry()
    return g if not g.isEmpty() else primary_geometry()


def anchor_position(w: int, h: int, key: str, padding: int, screen: QRect | None = None) -> QPoint:
    """Top-left point that places a w×h box at anchor `key` ('tl'..'br') on `screen`."""
    s = screen or home_geometry()
    v, hz = key[0], key[1]
    if v == "t":
        y = s.top() + padding
    elif v == "m":
        y = s.top() + (s.height() - h) // 2
    else:
        y = s.bottom() - h - padding
    if hz == "l":
        x = s.left() + padding
    elif hz == "c":
        x = s.left() + (s.width() - w) // 2
    else:
        x = s.right() - w - padding
    return QPoint(x, y)


# ── dim overlay ───────────────────────────────────────────────────────────────
class _DimScreen(QWidget):
    clicked = pyqtSignal()

    def __init__(self, geom: QRect, opacity: float, click_through: bool):
        super().__init__()
        self.setWindowFlags(_TOP_TOOL)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating)
        self.setWindowTitle("Luminescent dim")
        if click_through:
            # a real empty input region (the widget attribute alone does nothing for the window on Wayland)
            self.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
            self.setWindowFlag(Qt.WindowType.WindowTransparentForInput, True)
        self._alpha = int(max(0.0, min(1.0, opacity)) * 255)
        self.setGeometry(geom)

    def paintEvent(self, event):
        p = QPainter(self)
        p.fillRect(self.rect(), QColor(0, 0, 0, self._alpha))
        p.end()

    def mousePressEvent(self, event):
        self.clicked.emit()


class DimOverlay:
    """Darkens every screen. `clicked` fires when a non-click-through overlay is clicked."""

    def __init__(self, opacity: float = 0.35, click_through: bool = True, on_click=None):
        self._screens = []
        for scr in QApplication.screens():
            w = _DimScreen(scr.geometry(), opacity, click_through)
            if on_click:
                w.clicked.connect(on_click)
            self._screens.append(w)

    def show(self):
        for w in self._screens:
            w.show()
            w.raise_()

    def hide(self):
        for w in self._screens:
            w.hide()

    def close(self):
        for w in self._screens:
            w.close()
            w.deleteLater()
        self._screens = []

    def is_visible(self) -> bool:
        return any(w.isVisible() for w in self._screens)


# ── anchor bar ────────────────────────────────────────────────────────────────
class AnchorBar(QWidget):
    anchor_chosen = pyqtSignal(str)

    def __init__(self, hint: str = ""):
        super().__init__()
        self.setWindowFlags(_TOP_TOOL)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating)

        layout = QHBoxLayout(self)
        layout.setContentsMargins(10, 6, 10, 6)
        layout.setSpacing(4)

        if hint:
            lbl = QLabel(hint)
            lbl.setStyleSheet(f"color: {theme.P.text_secondary}; font-size: {theme.fs(11)}px; padding-right: 6px;")
            layout.addWidget(lbl)

        for key, icon in ANCHOR_ICONS.items():
            btn = QPushButton(icon)
            btn.setFixedSize(28, 28)
            btn.setToolTip(f"Snap to {key}")
            btn.setCursor(Qt.CursorShape.PointingHandCursor)
            btn.setStyleSheet("QPushButton { padding: 0; font-size: 14px; }")
            btn.clicked.connect(lambda _, k=key: self.anchor_chosen.emit(k))
            layout.addWidget(btn)

        self.adjustSize()
        s = home_geometry()
        self.move(s.center().x() - self.sizeHint().width() // 2, s.top() + 8)

    def paintEvent(self, event):
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        p.setBrush(QColor(theme.P.bg_base))
        p.setPen(QColor(theme.P.border))
        p.drawRoundedRect(self.rect().adjusted(0, 0, -1, -1), 8, 8)
        p.end()


# ── small buttons ─────────────────────────────────────────────────────────────
def icon_button(text: str, tooltip: str = "", size: int = 26, font_px: int = 13, ghost: bool = False) -> QPushButton:
    b = QPushButton(text)
    b.setFixedSize(size, size)
    b.setToolTip(tooltip)
    b.setCursor(Qt.CursorShape.PointingHandCursor)
    if ghost:
        b.setProperty("variant", "ghost")
    # round, so the hover highlight is a circle and not a box poking into a rounded corner
    b.setStyleSheet(f"QPushButton {{ padding: 0; font-size: {font_px}px; border-radius: {size // 2}px; }}")
    return b


def label(text: str = "", px: float = 13, weight: int = 400, mono: bool = False, spacing: float = 0.0,
          tone: str = "primary") -> QLabel:
    """A QLabel in the design's type scale. `tone` (primary/secondary/muted/accent) picks a themed colour."""
    lbl = QLabel(text)
    lbl.setFont(theme.qfont(px, weight, mono, spacing))
    if mono:
        lbl.setProperty("mono", True)
    if tone:
        lbl.setProperty("tone", tone)
    return lbl


class ElidedLabel(QLabel):
    """A one-line label that ends in "…" when it does not fit, instead of being cut off mid-letter."""

    def __init__(self, text: str = "", mode=Qt.TextElideMode.ElideRight):
        super().__init__(text)
        self._mode = mode
        self.setSizePolicy(QSizePolicy.Policy.Ignored, QSizePolicy.Policy.Preferred)

    def minimumSizeHint(self):
        return QSize(10, super().minimumSizeHint().height())

    def paintEvent(self, e):
        r = self.contentsRect()
        text = self.fontMetrics().elidedText(self.text(), self._mode, r.width())
        qp = QPainter(self)
        self.style().drawItemText(qp, r, int(self.alignment()), self.palette(), self.isEnabled(), text,
                                  self.foregroundRole())
        qp.end()


def elided(text: str = "", px: float = 13, weight: int = 400, mono: bool = False, spacing: float = 0.0,
           tone: str = "primary", mode=Qt.TextElideMode.ElideRight) -> ElidedLabel:
    """label(), but ending in "…" when there is no room."""
    lbl = ElidedLabel(text, mode)
    lbl.setFont(theme.qfont(px, weight, mono, spacing))
    if mono:
        lbl.setProperty("mono", True)
    if tone:
        lbl.setProperty("tone", tone)
    return lbl


def rule(vertical: bool = False, length: int = 0) -> QFrame:
    """A 1 px line in the border colour."""
    f = QFrame()
    f.setObjectName("lumRule")
    if vertical:
        f.setFixedWidth(1)
        if length:
            f.setFixedHeight(length)
    else:
        f.setFixedHeight(1)
        if length:
            f.setFixedWidth(length)
    return f


def button(text: str, variant: str = "", tooltip: str = "") -> QPushButton:
    b = QPushButton(text)
    b.setCursor(Qt.CursorShape.PointingHandCursor)
    if variant:
        b.setProperty("variant", variant)
    if tooltip:
        b.setToolTip(tooltip)
    return b


# ── floating panel ────────────────────────────────────────────────────────────
class _ResizeHandle(QFrame):
    THICK = 7
    CURSORS = {
        "l": Qt.CursorShape.SizeHorCursor, "r": Qt.CursorShape.SizeHorCursor,
        "t": Qt.CursorShape.SizeVerCursor, "b": Qt.CursorShape.SizeVerCursor,
        "tl": Qt.CursorShape.SizeFDiagCursor, "br": Qt.CursorShape.SizeFDiagCursor,
        "tr": Qt.CursorShape.SizeBDiagCursor, "bl": Qt.CursorShape.SizeBDiagCursor,
    }

    def __init__(self, edge: str, panel: "FloatingPanel"):
        super().__init__(panel)
        self.edge = edge
        self.panel = panel
        self._start = None
        self._geo = None
        self.setCursor(self.CURSORS[edge])
        style(self, "background: transparent;")

    def place(self, w: int, h: int):
        t = self.THICK
        rects = {
            "l": (0, t, t, h - 2 * t), "r": (w - t, t, t, h - 2 * t),
            "t": (t, 0, w - 2 * t, t), "b": (t, h - t, w - 2 * t, t),
            "tl": (0, 0, t, t), "tr": (w - t, 0, t, t),
            "bl": (0, h - t, t, t), "br": (w - t, h - t, t, t),
        }
        self.setGeometry(*rects[self.edge])
        self.raise_()

    def mousePressEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton and self.panel.can_resize():
            edges = Qt.Edge(0)
            for ch, edge in (("l", Qt.Edge.LeftEdge), ("r", Qt.Edge.RightEdge),
                             ("t", Qt.Edge.TopEdge), ("b", Qt.Edge.BottomEdge)):
                if ch in self.edge:
                    edges |= edge
            if system_resize(self.panel, edges, self.panel.geometry_changed.emit):
                return
            self._start = global_cursor(e)
            self._geo = QRect(self.panel.geometry())

    def mouseMoveEvent(self, e):
        if self._start is None:
            return
        d = global_cursor(e) - self._start
        g = QRect(self._geo)
        mw, mh = self.panel.minimumWidth(), self.panel.minimumHeight()
        if "l" in self.edge:
            g.setLeft(min(g.left() + d.x(), g.right() - mw))
        if "r" in self.edge:
            g.setRight(max(g.right() + d.x(), g.left() + mw))
        if "t" in self.edge:
            g.setTop(min(g.top() + d.y(), g.bottom() - mh))
        if "b" in self.edge:
            g.setBottom(max(g.bottom() + d.y(), g.top() + mh))
        self.panel.setGeometry(g)

    def mouseReleaseEvent(self, e):
        if self._start is not None:
            self._start = None
            self.panel.geometry_changed.emit()


class FloatingPanel(QWidget):
    """
    Frameless, always-on-top, translucent rounded panel with a header (accent bar,
    title, optional extra widgets, close button), a `body` layout for content,
    drag-by-header, edge resizing, and an edit-mode look.
    """
    geometry_changed = pyqtSignal()
    hidden = pyqtSignal()
    escape_pressed = pyqtSignal()

    def __init__(self, title: str, min_size=(200, 120), opacity: float = 0.85, radius: int | None = None,
                 resizable: bool = True, drag_needs_edit_mode: bool = False, close_hides: bool = True,
                 header_height: int = 38, activate_on_show: bool = True):
        super().__init__()
        self._opacity = opacity
        self._radius = theme.P.radius if radius is None else radius
        self._resizable = resizable
        self._drag_needs_edit = drag_needs_edit_mode
        self._close_hides = close_hides
        self._edit_mode = False
        self._drag_offset = None
        self._handles = []
        self._pal = theme.P                 # kept: a module's own colours are only in theme.P while it builds

        self.setWindowFlags(_TOP_TOOL)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        if not activate_on_show:
            self.setAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating)
        self.setMinimumSize(*min_size)
        if theme.overridden():
            # built inside `with theme.module_colours(...)`: this module has its own accent in the theme
            self.setStyleSheet(theme.stylesheet())

        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        self._outer = outer
        self.container = QFrame()
        self.container.setObjectName("panelContainer")
        outer.addWidget(self.container)
        self._apply_container_style()

        root = QVBoxLayout(self.container)
        root.setContentsMargins(0, 0, 0, 0)
        root.setSpacing(0)

        # header: a strip in the header colour, mono caps title, a short accent tick under its left end
        self.header = QFrame()
        self.header.setObjectName("panelHeader")
        self.header.setFixedHeight(header_height)
        rad = max(0, self._radius - 1)
        self.header.setStyleSheet(f"""
            #panelHeader {{
                background: {theme.fill("bg_header")}; border: none; border-bottom: 1px solid {theme.P.border};
                border-top-left-radius: {rad}px; border-top-right-radius: {rad}px;
            }}
        """)
        h = QHBoxLayout(self.header)
        h.setContentsMargins(14, 0, 6, 0)
        h.setSpacing(8)
        self._tick = QFrame(self.header)
        self._tick.setStyleSheet(f"background: {theme.fill('primary')}; border: none;")
        self._tick.setGeometry(0, header_height - 1, 40, 1)

        self.header_left = QHBoxLayout()
        self.header_left.setSpacing(4)
        h.addLayout(self.header_left)

        self.title_label = label(title.upper(), 10.5, 600, mono=True, spacing=0.16)
        h.addWidget(self.title_label)
        self.subtitle_label = label("", 10, mono=True, tone="muted")
        self.subtitle_label.hide()
        h.addWidget(self.subtitle_label)
        h.addStretch()

        self.header_right = QHBoxLayout()
        self.header_right.setSpacing(2)
        h.addLayout(self.header_right)

        self.edit_indicator = label("● EDIT", 9.5, 600, mono=True, spacing=0.12, tone="accent")
        self.edit_indicator.hide()
        h.addWidget(self.edit_indicator)

        self.close_btn = icon_button("✕", "Close", 26, 11, ghost=True)
        self.close_btn.clicked.connect(self.close_requested)
        h.addWidget(self.close_btn)
        root.addWidget(self.header)

        # body
        self.body_frame = QFrame()
        style(self.body_frame, "background: transparent; border: none;")
        self.body = QVBoxLayout(self.body_frame)
        self.body.setContentsMargins(0, 0, 0, 0)
        self.body.setSpacing(0)
        root.addWidget(self.body_frame, 1)

        if resizable:
            for edge in ("l", "r", "t", "b", "tl", "tr", "bl", "br"):
                self._handles.append(_ResizeHandle(edge, self))

    # ── look ───────────────────────────────────────────────────────────────
    def _apply_container_style(self):
        if getattr(self, "_embedded", False):
            self.container.setStyleSheet("#panelContainer { background: transparent; border: none; }")
            return
        p = self._pal
        border = theme.fill("primary", 1.0, p) if self._edit_mode else theme.fill("border", 1.0, p)
        self.container.setStyleSheet(f"""
            #panelContainer {{
                background: {theme.fill("bg_base", self._opacity, p)};
                border: 1px solid {border};
                border-radius: {self._radius}px;
            }}
        """)

    def set_opacity(self, opacity: float):
        self._opacity = opacity
        self._apply_container_style()

    def set_title(self, text: str, sub: str = ""):
        self.title_label.setText(text.upper())
        self.subtitle_label.setText(f"/ {sub.upper()}" if sub else "")
        self.subtitle_label.setVisible(bool(sub))

    def set_edit_mode(self, on: bool):
        self._edit_mode = on
        self.edit_indicator.setVisible(on)
        self._apply_container_style()

    @property
    def edit_mode(self) -> bool:
        return self._edit_mode

    def can_resize(self) -> bool:
        return self._resizable and (self._edit_mode or not self._drag_needs_edit)

    def flash(self, times: int = 6, colour: str | None = None):
        """Briefly alternate the border colour to grab attention."""
        p = self._pal
        colour = colour or p.primary
        state = {"n": 0}

        def step():
            on = state["n"] % 2 == 0
            self.container.setStyleSheet(f"""
                #panelContainer {{
                    background: {theme.fill("bg_base", self._opacity, p)};
                    border: 2px solid {colour if on else p.border};
                    border-radius: {self._radius}px;
                }}
            """)
            state["n"] += 1
            if state["n"] < times * 2:
                QTimer.singleShot(250, step)
            else:
                self._apply_container_style()
        step()

    # ── behaviour ──────────────────────────────────────────────────────────
    def close_requested(self):
        if self._close_hides:
            self.fade_hide()
        else:
            self.close()

    def fade_hide(self):
        """Hide, fading the panel out first when panel animations are on."""
        if not self.isVisible() or self.property("fading_out"):
            return
        if not anim.on("panels"):
            self.hide()
            return
        self.setProperty("fading_out", True)

        def done():
            self.setProperty("fading_out", False)
            self.hide()
            self.container.setGraphicsEffect(None)
        anim.fade(self.container, 1.0, 0.0, "panels", 140, then=done, curve=QEasingCurve.Type.InCubic)

    def show_and_raise(self, activate: bool = True):
        if self.property("fading_out"):
            anim.stop(self.container)
            self.setProperty("fading_out", False)
            self.container.setGraphicsEffect(None)
        elif not self.isVisible():
            self.follow_cursor_screen()
        self.show()
        raise_window(self)
        if activate and not self.testAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating):
            bring_to_front(self)

    def toggle(self):
        if self.isVisible() and not self.property("fading_out"):
            self.fade_hide()
        else:
            self.show_and_raise()

    def showEvent(self, e):
        super().showEvent(e)
        if not e.spontaneous():
            anim.fade(self.container, 0.0, 1.0, "panels", 160)
            if anim.on("panels") and anim.style() == "caelestia":
                # rises a few pixels into place while it fades in (the window itself can not move smoothly
                # on Wayland, so the panel moves inside it)
                lay = self._outer
                anim.value(14.0, 0.0, lambda v: lay.setContentsMargins(0, int(v), 0, 14 - int(v)), "panels", 260,
                           owner=self, tag="rise")

    # sticky notes and the like stay exactly where they were put
    stays_put = False

    def follow_cursor_screen(self):
        """Panels used to reopen wherever they were saved, often another screen than the overlay (which
        opens on the screen with the mouse). Now a panel goes to the overlay's screen, placed the way
        Settings → Overlay & Interface → Panels like Notes open says."""
        s = anim._settings
        if self.stays_put or s is None:
            return
        target = home_geometry()
        if target.isEmpty():
            return
        how = s.get("ui.panel_place", "centre")
        if how == "centre":
            self.center_on(target)
        elif how == "cursor":
            c = hypr.cursor_pos()
            x = min(max(c.x() + 16, target.left()), target.right() - self.width())
            y = min(max(c.y() + 16, target.top()), target.bottom() - self.height())
            self.move(max(target.left(), x), max(target.top(), y))
        elif target.contains(self.geometry().center()):
            return
        else:
            src = next((hypr.available_geometry(scr) for scr in QApplication.screens()
                        if hypr.available_geometry(scr).contains(self.geometry().center())), None)
            if src is None or src.isEmpty():
                self.center_on(target)
            else:
                # keep the spot relative to the screen, so "top right" stays top right on a bigger or smaller one
                fx = (self.x() - src.x()) / max(1, src.width() - self.width())
                fy = (self.y() - src.y()) / max(1, src.height() - self.height())
                fx, fy = min(1.0, max(0.0, fx)), min(1.0, max(0.0, fy))
                self.move(target.x() + int(fx * max(0, target.width() - self.width())),
                          target.y() + int(fy * max(0, target.height() - self.height())))
        self.geometry_changed.emit()

    def center_on(self, screen: QRect | None = None):
        s = screen or home_geometry()
        self.move(s.center().x() - self.width() // 2, s.center().y() - self.height() // 2)

    def snap_to(self, key: str, padding: int, screen: QRect | None = None):
        self.move(anchor_position(self.width(), self.height(), key, padding, screen))
        self.geometry_changed.emit()

    def clamp_to_screens(self):
        """Make sure at least the header is on some screen (after monitor changes)."""
        for scr in QApplication.screens():
            if scr.geometry().intersects(QRect(self.pos(), QSize(self.width(), self.header.height()))):
                return
        self.center_on()

    def hideEvent(self, e):
        super().hideEvent(e)
        self.hidden.emit()

    def resizeEvent(self, e):
        super().resizeEvent(e)
        for h in self._handles:
            h.place(self.width(), self.height())

    def keyPressEvent(self, e):
        if e.key() == Qt.Key.Key_Escape and not getattr(self, "_embedded", False):
            self.escape_pressed.emit()
            return
        super().keyPressEvent(e)

    def mousePressEvent(self, e):
        if getattr(self, "_embedded", False):
            return super().mousePressEvent(e)
        if e.button() == Qt.MouseButton.LeftButton and (self._edit_mode or not self._drag_needs_edit):
            if self.header.geometry().contains(e.position().toPoint()):
                if system_move(self, self.geometry_changed.emit):
                    return
                self._drag_offset = global_cursor(e) - self.pos()
                raise_window(self)

    def mouseMoveEvent(self, e):
        if self._drag_offset is not None:
            self.move(global_cursor(e) - self._drag_offset)

    def mouseReleaseEvent(self, e):
        if self._drag_offset is not None:
            self._drag_offset = None
            self.geometry_changed.emit()


def embed(panel: FloatingPanel):
    """Turn a module's floating panel into a page (an overlay page or a dashboard box): no window, header or
    resize edges of its own, in a scroll area so a small box never squashes it. Returns the scroll area."""
    from PyQt6.QtWidgets import QScrollArea
    panel._embedded = True
    panel.setWindowFlags(Qt.WindowType.Widget)
    panel.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground, False)
    panel.header.hide()
    for h in panel._handles:
        h.hide()
    panel._handles = []
    panel._apply_container_style()
    need = panel.minimumSize()
    panel.setMinimumSize(min(need.width(), 340), min(need.height(), 600))
    scroll = QScrollArea()
    scroll.setWidgetResizable(True)
    scroll.setFrameShape(QFrame.Shape.NoFrame)
    scroll.setWidget(panel)
    scroll.panel = panel
    return scroll


# ── dialogs ───────────────────────────────────────────────────────────────────
class RoundedDialog(QDialog):
    """A dialog in Luminescent's own look: frameless, rounded corners in the theme's radius, a header with a mono
    caps title and ✕, dragged by the header and (if `resizable`) resized from any edge. Put content in `body`."""
    geometry_changed = pyqtSignal()

    def __init__(self, parent=None, title: str = "", resizable: bool = False, modal: bool = True):
        super().__init__(parent)
        self.setWindowFlags(Qt.WindowType.Dialog | Qt.WindowType.FramelessWindowHint |
                            Qt.WindowType.WindowStaysOnTopHint)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setModal(modal)
        self.setWindowTitle(title or "Luminescent")
        self._pal = theme.P
        self._resizable = resizable
        self._drag = None
        if theme.overridden():
            self.setStyleSheet(theme.stylesheet())
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        self.frame = QFrame()
        self.frame.setObjectName("lumDialog")
        p = self._pal
        rad = p.radius + 4
        self.frame.setStyleSheet(f"#lumDialog {{ background: {theme.fill('bg_base', 1.0, p)};"
                                 f" border: 1px solid {theme.fill('border', 1.0, p)}; border-radius: {rad}px; }}")
        outer.addWidget(self.frame)
        root = QVBoxLayout(self.frame)
        root.setContentsMargins(0, 0, 0, 0)
        root.setSpacing(0)
        self.header = QFrame()
        self.header.setObjectName("lumDialogHead")
        self.header.setFixedHeight(40)
        self.header.setStyleSheet(f"#lumDialogHead {{ background: transparent; border: none;"
                                  f" border-bottom: 1px solid {p.border}; }}")
        h = QHBoxLayout(self.header)
        h.setContentsMargins(16, 0, 8, 0)
        h.setSpacing(8)
        mark = QLabel()
        mark.setPixmap(theme.sparkle_pixmap(12, p.primary))
        h.addWidget(mark)
        self.title_label = label((title or "").upper(), 10.5, 600, mono=True, spacing=0.16)
        h.addWidget(self.title_label)
        h.addStretch()
        self.header_right = QHBoxLayout()
        self.header_right.setSpacing(4)
        h.addLayout(self.header_right)
        close = icon_button("✕", "Close (Esc)", 26, 11, ghost=True)
        close.clicked.connect(self.reject)
        h.addWidget(close)
        root.addWidget(self.header)
        self.body = QVBoxLayout()
        self.body.setContentsMargins(16, 14, 16, 16)
        self.body.setSpacing(10)
        root.addLayout(self.body, 1)
        self._handles = [_ResizeHandle(e, self) for e in ("l", "r", "t", "b", "tl", "tr", "bl", "br")] \
            if resizable else []

    def set_title(self, text: str):
        self.setWindowTitle(text)
        self.title_label.setText(text.upper())

    def can_resize(self) -> bool:
        return self._resizable

    def fit_to_screen(self, w: int, h: int, near: QWidget | None = None):
        """Size it to w×h (smaller on a small screen) and centre it on the screen with the mouse (on the
        window that opened it when the mouse is over that window)."""
        area, scr = hypr.dialog_area(near)
        if scr.isEmpty():
            area = scr = primary_geometry()
        w = min(w, max(self.minimumWidth(), scr.width() - 60))
        h = min(h, max(self.minimumHeight(), scr.height() - 60))
        self.resize(w, h)
        self.move(max(scr.left(), min(area.center().x() - w // 2, scr.right() - w)),
                  max(scr.top(), min(area.center().y() - h // 2, scr.bottom() - h)))

    def showEvent(self, e):
        if not e.spontaneous() and not self.testAttribute(Qt.WidgetAttribute.WA_Moved):
            # nobody placed it: the middle of the screen with the mouse (Qt would centre it on the parent,
            # which can be a window on another monitor)
            area, scr = hypr.dialog_area(self.parentWidget())
            if not scr.isEmpty():
                self.move(max(scr.left(), min(area.center().x() - self.width() // 2, scr.right() - self.width())),
                          max(scr.top(), min(area.center().y() - self.height() // 2, scr.bottom() - self.height())))
        super().showEvent(e)
        if not e.spontaneous():
            anim.fade(self.frame, 0.0, 1.0, "panels", 150)
            QTimer.singleShot(0, lambda: bring_to_front(self))

    def resizeEvent(self, e):
        super().resizeEvent(e)
        for hd in self._handles:
            hd.place(self.width(), self.height())

    def mousePressEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton and self.header.geometry().contains(e.position().toPoint()):
            if system_move(self, self.geometry_changed.emit):
                return
            self._drag = global_cursor(e) - self.pos()
            return
        super().mousePressEvent(e)

    def mouseMoveEvent(self, e):
        if self._drag is not None:
            self.move(global_cursor(e) - self._drag)
            return
        super().mouseMoveEvent(e)

    def mouseReleaseEvent(self, e):
        if self._drag is not None:
            self._drag = None
            self.geometry_changed.emit()
            return
        super().mouseReleaseEvent(e)


class _AskDialog(RoundedDialog):
    """The message boxes and questions below, in Luminescent's look. Long text scrolls instead of being cut off."""

    GLYPHS = {QMessageBox.Icon.Information: "✦", QMessageBox.Icon.Warning: "!", QMessageBox.Icon.Critical: "✕",
              QMessageBox.Icon.Question: "?"}

    def __init__(self, parent, title: str, text: str, icon=None):
        super().__init__(parent if isinstance(parent, QWidget) else None, title)
        self.choice = None
        p = self._pal
        top = QHBoxLayout()
        top.setSpacing(14)
        if icon in self.GLYPHS:
            colour = theme.DANGER if icon in (QMessageBox.Icon.Warning, QMessageBox.Icon.Critical) else p.primary
            bubble = QLabel(self.GLYPHS[icon])
            bubble.setFixedSize(30, 30)
            bubble.setAlignment(Qt.AlignmentFlag.AlignCenter)
            bubble.setFont(theme.qfont(14, 700))
            bubble.setStyleSheet(f"background: {theme.rgba(colour, 0.16)}; color: {colour}; border-radius: 15px;")
            top.addWidget(bubble, 0, Qt.AlignmentFlag.AlignTop)
        self.width_px = 460 if len(text) < 400 else 600
        msg = label(text, 13.5, tone="")
        msg.setStyleSheet(f"color: {p.text_primary};")
        msg.setWordWrap(True)
        msg.setTextInteractionFlags(Qt.TextInteractionFlag.TextSelectableByMouse)
        room = int(cursor_screen_geometry().height() * 0.55)
        if msg.heightForWidth(self.width_px - 90) > room:
            from PyQt6.QtWidgets import QScrollArea
            sc = QScrollArea()
            sc.setWidgetResizable(True)
            sc.setFrameShape(QFrame.Shape.NoFrame)
            sc.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
            sc.setWidget(msg)
            sc.setFixedHeight(room)
            top.addWidget(sc, 1)
        else:
            top.addWidget(msg, 1)
        self.body.addLayout(top)
        self.extra = QVBoxLayout()
        self.extra.setSpacing(6)
        self.body.addLayout(self.extra)
        self.buttons = QHBoxLayout()
        self.buttons.setSpacing(8)
        self.buttons.addStretch()
        self.body.addSpacing(4)
        self.body.addLayout(self.buttons)

    def add_button(self, text: str, variant: str = "", accept: bool = True, default: bool = False) -> QPushButton:
        b = button(text, variant)
        b.setMinimumWidth(84)
        b.clicked.connect(lambda: self._chosen(text, accept))
        if default:
            b.setDefault(True)
            b.setAutoDefault(True)
            QTimer.singleShot(0, b.setFocus)
        else:
            b.setAutoDefault(False)
        self.buttons.addWidget(b)
        return b

    def _chosen(self, text: str, accept: bool):
        self.choice = text if accept else None
        (self.accept if accept else self.reject)()

    def run(self) -> bool:
        self.setFixedWidth(self.width_px)
        lay = self.layout()
        lay.activate()
        h = lay.totalHeightForWidth(self.width_px) if lay.hasHeightForWidth() else self.sizeHint().height()
        self.fit_to_screen(self.width_px, max(h, 120), self.parentWidget())
        return self.exec() == QDialog.DialogCode.Accepted


def message(parent, title: str, text: str, icon=QMessageBox.Icon.Information):
    dlg = _AskDialog(parent, title, text, icon)
    dlg.add_button("OK", "primary", default=True)
    dlg.run()
    dlg.deleteLater()


def error(parent, title: str, text: str):
    message(parent, title, text, QMessageBox.Icon.Warning)


def confirm(parent, title: str, text: str, yes: str = "Yes", no: str = "No", danger: bool = False) -> bool:
    dlg = _AskDialog(parent, title, text, QMessageBox.Icon.Question)
    dlg.add_button(no, "ghost", accept=False, default=danger)
    dlg.add_button(yes, "danger" if danger else "primary", default=not danger)
    ok = dlg.run() and dlg.choice == yes
    dlg.deleteLater()
    return ok


def choose(parent, title: str, text: str, buttons: list[str], cancel: str = "Cancel") -> str | None:
    """Show a message box with custom buttons. Returns the clicked label or None for cancel."""
    dlg = _AskDialog(parent, title, text)
    dlg.add_button(cancel, "ghost", accept=False)
    for i, b in enumerate(buttons):
        dlg.add_button(b, "primary" if i == len(buttons) - 1 else "", default=i == len(buttons) - 1)
    got = dlg.choice if dlg.run() else None
    dlg.deleteLater()
    return got


def ask_text(parent, title: str, label: str, default: str = "") -> tuple[str, bool]:
    dlg = _AskDialog(parent, title, label)
    edit = QLineEdit(default)
    edit.setFixedHeight(32)
    edit.selectAll()
    dlg.extra.addWidget(edit)
    dlg.add_button("Cancel", "ghost", accept=False)
    dlg.add_button("OK", "primary", default=True)
    edit.returnPressed.connect(lambda: dlg._chosen("OK", True))
    QTimer.singleShot(0, edit.setFocus)
    ok = dlg.run()
    text = edit.text()
    dlg.deleteLater()
    return text, ok


def ask_choice(parent, title: str, label: str, items: list[str], current: int = 0) -> tuple[str, bool]:
    from PyQt6.QtWidgets import QListWidget
    dlg = _AskDialog(parent, title, label)
    lst = QListWidget()
    lst.addItems(items)
    lst.setMinimumHeight(min(320, 34 * max(3, len(items)) + 8))
    if items:
        lst.setCurrentRow(max(0, min(len(items) - 1, current)))
    lst.itemDoubleClicked.connect(lambda _it: dlg._chosen("OK", True))
    dlg.extra.addWidget(lst)
    dlg.add_button("Cancel", "ghost", accept=False)
    dlg.add_button("OK", "primary", default=True)
    ok = dlg.run()
    it = lst.currentItem()
    text = it.text() if it is not None else ""
    dlg.deleteLater()
    return text, ok and it is not None


def ask_datetime(parent, title: str, label: str, default=None):
    """A date + time picker. Returns a datetime, or None when cancelled."""
    dlg = _AskDialog(parent, title, label)
    edit = QDateTimeEdit(QDateTime(default) if default is not None else QDateTime.currentDateTime().addSecs(3600))
    edit.setCalendarPopup(True)
    edit.setDisplayFormat("ddd d MMM yyyy  HH:mm")
    edit.setFixedHeight(32)
    dlg.extra.addWidget(edit)
    dlg.add_button("Cancel", "ghost", accept=False)
    dlg.add_button("OK", "primary", default=True)
    ok = dlg.run()
    when = edit.dateTime().toPyDateTime().replace(second=0, microsecond=0)
    dlg.deleteLater()
    return when if ok else None


def menu(parent=None) -> QMenu:
    m = QMenu(parent)
    m.setWindowFlags(m.windowFlags() | Qt.WindowType.WindowStaysOnTopHint)
    return m


def clear_layout(layout):
    while layout.count():
        item = layout.takeAt(0)
        w = item.widget()
        if w is not None:
            w.setParent(None)
            w.deleteLater()
        elif item.layout() is not None:
            clear_layout(item.layout())


# ── drag to reorder ───────────────────────────────────────────────────────────
class DragReorder(QObject):
    """Drag the rows of a box layout up and down to put them in a new order.

    add(row, handle) makes `handle` (the row itself by default) the thing you grab. The row moves live
    while you drag; `moved(old, new)` fires on release with positions counted among the added rows only.
    A press that does not move is left alone, so clicks still work.
    """
    moved = pyqtSignal(int, int)
    THRESHOLD = 6

    def __init__(self, layout, parent=None):
        super().__init__(parent)
        self.layout = layout
        self.enabled = True
        self._rows: list[QWidget] = []
        self._handles: dict = {}
        self._press = None              # (row, start y)
        self._dragging = None
        self._start = 0

    def add(self, row: QWidget, handle: QWidget | None = None):
        handle = handle or row
        self._rows.append(row)
        self._handles[handle] = row
        handle.installEventFilter(self)
        if handle is not row:
            handle.setCursor(Qt.CursorShape.OpenHandCursor)

    def clear(self):
        self._rows, self._handles, self._press, self._dragging = [], {}, None, None

    @property
    def dragging(self) -> bool:
        return self._dragging is not None

    def _order(self) -> list[QWidget]:
        rows = [r for r in self._rows if self.layout.indexOf(r) >= 0]
        return sorted(rows, key=self.layout.indexOf)

    def eventFilter(self, obj, e):
        row = self._handles.get(obj)
        if row is None or not self.enabled:
            return False
        t = e.type()
        if t == QEvent.Type.MouseButtonPress and e.button() == Qt.MouseButton.LeftButton:
            self._press = (row, e.globalPosition().y())
        elif t == QEvent.Type.MouseMove and self._press is not None:
            if self._dragging is None and abs(e.globalPosition().y() - self._press[1]) >= self.THRESHOLD:
                self._dragging = self._press[0]
                self._start = self._order().index(self._dragging)
                self._dragging.raise_()
                QApplication.setOverrideCursor(Qt.CursorShape.ClosedHandCursor)
            if self._dragging is not None:
                self._follow(e.globalPosition().toPoint())
                return True
        elif t == QEvent.Type.MouseButtonRelease and self._press is not None:
            self._press = None
            if self._dragging is not None:
                QApplication.restoreOverrideCursor()
                end = self._order().index(self._dragging)
                self._dragging = None
                if end != self._start:
                    self.moved.emit(self._start, end)
                return True
        return False

    def _follow(self, pos: QPoint):
        host = self.layout.parentWidget()
        if host is None:
            return
        y = host.mapFromGlobal(pos).y()
        others = [r for r in self._order() if r is not self._dragging]
        target = None
        for r in others:
            if y < r.geometry().center().y():
                target = self.layout.indexOf(r)
                break
        if target is None:
            target = self.layout.indexOf(others[-1]) + 1 if others else self.layout.indexOf(self._dragging)
        cur = self.layout.indexOf(self._dragging)
        if target > cur:
            target -= 1
        if target != cur:
            self.layout.removeWidget(self._dragging)
            self.layout.insertWidget(target, self._dragging)
