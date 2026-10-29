"""
Shared widgets: dim overlays, floating panels, anchor bar, styled dialogs.
"""
import logging
import subprocess

from PyQt6.QtWidgets import (
    QApplication, QWidget, QFrame, QLabel, QPushButton, QVBoxLayout, QHBoxLayout,
    QMessageBox, QMenu, QSizePolicy, QDialog, QLineEdit, QGraphicsOpacityEffect,
    QDateTimeEdit
)
from PyQt6.QtCore import (Qt, QRect, QPoint, QSize, QObject, QEvent, QDateTime, pyqtSignal, QTimer,
                          QPropertyAnimation, QEasingCurve)
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


def anchor_position(w: int, h: int, key: str, padding: int, screen: QRect | None = None) -> QPoint:
    """Top-left point that places a w×h box at anchor `key` ('tl'..'br') on `screen`."""
    s = screen or primary_geometry()
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
        s = primary_geometry()
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
    b.setStyleSheet(f"QPushButton {{ padding: 0; font-size: {font_px}px; }}")
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
                background: {theme.P.bg_header}; border: none; border-bottom: 1px solid {theme.P.border};
                border-top-left-radius: {rad}px; border-top-right-radius: {rad}px;
            }}
        """)
        h = QHBoxLayout(self.header)
        h.setContentsMargins(14, 0, 6, 0)
        h.setSpacing(8)
        self._tick = QFrame(self.header)
        self._tick.setStyleSheet(f"background: {theme.P.primary}; border: none;")
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
        border = theme.P.primary if self._edit_mode else theme.P.border
        self.container.setStyleSheet(f"""
            #panelContainer {{
                background-color: {theme.rgba(theme.P.bg_base, self._opacity)};
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
        colour = colour or theme.P.primary
        state = {"n": 0}

        def step():
            on = state["n"] % 2 == 0
            self.container.setStyleSheet(f"""
                #panelContainer {{
                    background-color: {theme.rgba(theme.P.bg_base, self._opacity)};
                    border: 2px solid {colour if on else theme.P.border};
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
        on_cursor = s.get("ui.overlay_screen", "cursor") == "cursor"
        target = cursor_screen_geometry() if on_cursor else primary_geometry()
        if target.isEmpty():
            return
        how = s.get("ui.panel_place", "kept")
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
        s = screen or primary_geometry()
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
        if e.key() == Qt.Key.Key_Escape:
            self.escape_pressed.emit()
            return
        super().keyPressEvent(e)

    def mousePressEvent(self, e):
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


# ── dialogs ───────────────────────────────────────────────────────────────────
DIALOG_HEAD = 38


class _CardLook(QObject):
    """Makes a QDialog a frameless rounded card like the panels: an outline, a header strip with the title,
    an accent tick and ✕, and dragging by the header. Every Luminescent dialog goes through card_dialog()."""

    def __init__(self, dlg: QDialog, closable: bool):
        super().__init__(dlg)
        self.dlg = dlg
        self._drag = None
        self.head = QFrame(dlg)
        self.head.setObjectName("dlgHead")
        self.head.setStyleSheet("#dlgHead { background: transparent; border: none; }")
        self.head.setCursor(Qt.CursorShape.SizeAllCursor)
        h = QHBoxLayout(self.head)
        h.setContentsMargins(16, 0, 8, 0)
        h.setSpacing(8)
        self.title = label("", 10.5, 600, mono=True, spacing=0.16)
        self.title.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        h.addWidget(self.title)
        h.addStretch()
        self.close_btn = icon_button("✕", "Close (Esc)", 26, 11, ghost=True)
        self.close_btn.setAutoDefault(False)
        self.close_btn.setFocusPolicy(Qt.FocusPolicy.NoFocus)
        self.close_btn.clicked.connect(dlg.reject)
        self.close_btn.setVisible(closable)
        h.addWidget(self.close_btn)
        self.head.installEventFilter(self)
        dlg.installEventFilter(self)
        self._sync_title()

    def _sync_title(self):
        self.title.setText(hypr_strip(self.dlg.windowTitle()).upper())

    def eventFilter(self, obj, e):
        t = e.type()
        if obj is self.dlg:
            if t == QEvent.Type.Paint:
                self._paint()
            elif t == QEvent.Type.Resize:
                self.head.setGeometry(1, 1, self.dlg.width() - 2, DIALOG_HEAD - 1)
                self.head.raise_()
            elif t == QEvent.Type.WindowTitleChange:
                self._sync_title()
            return False
        if obj is self.head:
            if t == QEvent.Type.MouseButtonPress and e.button() == Qt.MouseButton.LeftButton:
                if not system_move(self.dlg):
                    self._drag = global_cursor(e) - self.dlg.pos()
                return True
            if t == QEvent.Type.MouseMove and self._drag is not None:
                self.dlg.move(global_cursor(e) - self._drag)
                return True
            if t == QEvent.Type.MouseButtonRelease:
                self._drag = None
                return True
        return False

    def _paint(self):
        from PyQt6.QtGui import QPainterPath, QPen
        from PyQt6.QtCore import QRectF
        p = theme.P
        r = max(4, p.radius)
        painter = QPainter(self.dlg)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        rect = QRectF(self.dlg.rect()).adjusted(0.5, 0.5, -0.5, -0.5)
        path = QPainterPath()
        path.addRoundedRect(rect, r, r)
        painter.fillPath(path, QColor(p.bg_base))
        # the header strip, clipped to the rounded top
        painter.save()
        painter.setClipPath(path)
        painter.fillRect(QRectF(0, 0, self.dlg.width(), DIALOG_HEAD), QColor(p.bg_header))
        painter.fillRect(QRectF(0, DIALOG_HEAD - 1, self.dlg.width(), 1), QColor(p.border))
        painter.fillRect(QRectF(0, DIALOG_HEAD - 1, 40, 1), QColor(p.primary))
        painter.restore()
        pen = QPen(QColor(theme.mix(p.border, p.text_muted, 0.35)))
        pen.setWidthF(1.0)
        painter.setPen(pen)
        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.drawPath(path)
        painter.end()


def hypr_strip(title: str) -> str:
    """A window title without the invisible Hyprland markers."""
    return "".join(c for c in title if c not in (hypr.NO_ACTIVATE, hypr.NO_FOCUS, hypr.NO_PIN, hypr.BLUR))


def card_dialog(dlg: QDialog, closable: bool = True) -> QDialog:
    """Give a dialog the rounded card look (call it in __init__, before the dialog is shown). Its layout keeps
    its own margins; the header strip sits above it."""
    if getattr(dlg, "_card_look", None) is not None:
        return dlg
    dlg.setWindowFlags(dlg.windowFlags() | Qt.WindowType.FramelessWindowHint | Qt.WindowType.WindowStaysOnTopHint)
    dlg.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
    dlg.setProperty("card", True)
    dlg.setContentsMargins(0, DIALOG_HEAD, 0, 0)
    dlg._card_look = _CardLook(dlg, closable)
    return dlg


def _on_top(dlg: QDialog):
    return card_dialog(dlg)


class _AskDialog(QDialog):
    """The body of message / confirm / choose / ask_text: a text, an optional field and a row of buttons."""

    def __init__(self, parent, title: str, text: str, width: int = 420):
        super().__init__(parent)
        self.setWindowTitle(title)
        card_dialog(self)
        self.clicked = None
        self.lay = QVBoxLayout(self)
        self.lay.setContentsMargins(20, 16, 20, 18)
        self.lay.setSpacing(14)
        if text:
            self.text = label(text, 13)
            self.text.setWordWrap(True)
            self.text.setTextInteractionFlags(Qt.TextInteractionFlag.TextSelectableByMouse)
            self.text.setMinimumWidth(width - 40)
            self.lay.addWidget(self.text)
        self.buttons = QHBoxLayout()
        self.buttons.setSpacing(8)
        self.buttons.addStretch()
        self.setMinimumWidth(width)

    def add_button(self, text: str, variant: str = "", accept: bool = True, default: bool = False) -> QPushButton:
        b = button(text, variant)
        b.setMinimumWidth(84)
        b.setAutoDefault(False)
        b.setDefault(default)
        b.clicked.connect(lambda: self._done(b, accept))
        self.buttons.addWidget(b)
        return b

    def _done(self, b, accept: bool):
        self.clicked = b
        (self.accept if accept else self.reject)()

    def finish(self):
        self.lay.addLayout(self.buttons)
        self.adjustSize()


def message(parent, title: str, text: str, icon=QMessageBox.Icon.Information):
    dlg = _AskDialog(parent, title, ("⚠  " if icon == QMessageBox.Icon.Warning else "") + text)
    dlg.add_button("OK", "primary", default=True)
    dlg.finish()
    dlg.exec()


def error(parent, title: str, text: str):
    message(parent, title, text, QMessageBox.Icon.Warning)


def confirm(parent, title: str, text: str, yes: str = "Yes", no: str = "No", danger: bool = False) -> bool:
    dlg = _AskDialog(parent, title, text)
    no_btn = dlg.add_button(no, "ghost", accept=False, default=danger)
    yes_btn = dlg.add_button(yes, "danger" if danger else "primary", default=not danger)
    if danger:
        yes_btn.setStyleSheet(f"QPushButton {{ border: 1px solid {theme.DANGER}; }}")
    dlg.finish()
    (no_btn if danger else yes_btn).setFocus()
    dlg.exec()
    return dlg.clicked is yes_btn


def choose(parent, title: str, text: str, buttons: list[str], cancel: str = "Cancel") -> str | None:
    """A message with custom buttons. Returns the clicked label or None for cancel."""
    dlg = _AskDialog(parent, title, text, max(420, 120 + 110 * len(buttons)))
    dlg.add_button(cancel, "ghost", accept=False)
    made = {}
    for i, text_ in enumerate(buttons):
        made[text_] = dlg.add_button(text_, "primary" if i == len(buttons) - 1 else "", default=i == len(buttons) - 1)
    dlg.finish()
    dlg.exec()
    for text_, b in made.items():
        if b is dlg.clicked:
            return text_
    return None


def ask_text(parent, title: str, label_: str, default: str = "") -> tuple[str, bool]:
    dlg = _AskDialog(parent, title, label_)
    edit = QLineEdit(default)
    edit.setFixedHeight(34)
    edit.selectAll()
    dlg.lay.addWidget(edit)
    dlg.add_button("Cancel", "ghost", accept=False)
    dlg.add_button("OK", "primary", default=True)
    dlg.finish()
    edit.setFocus()
    ok = dlg.exec() == QDialog.DialogCode.Accepted
    return edit.text(), ok


def ask_choice(parent, title: str, label_: str, items: list[str], current: int = 0) -> tuple[str, bool]:
    from PyQt6.QtWidgets import QListWidget
    dlg = _AskDialog(parent, title, label_)
    lst = QListWidget()
    lst.addItems(items)
    lst.setCurrentRow(max(0, min(current, len(items) - 1)))
    lst.setMinimumHeight(min(320, 36 * max(3, len(items))))
    lst.itemDoubleClicked.connect(lambda _it: dlg.accept())
    dlg.lay.addWidget(lst)
    dlg.add_button("Cancel", "ghost", accept=False)
    dlg.add_button("OK", "primary", default=True)
    dlg.finish()
    lst.setFocus()
    ok = dlg.exec() == QDialog.DialogCode.Accepted and lst.currentItem() is not None
    return (lst.currentItem().text() if lst.currentItem() else ""), ok


def ask_datetime(parent, title: str, label_: str, default=None):
    """A date + time picker. Returns a datetime, or None when cancelled."""
    dlg = _AskDialog(parent, title, label_)
    edit = QDateTimeEdit(QDateTime(default) if default is not None else QDateTime.currentDateTime().addSecs(3600))
    edit.setCalendarPopup(True)
    edit.setDisplayFormat("ddd d MMM yyyy  HH:mm")
    edit.setFixedHeight(34)
    dlg.lay.addWidget(edit)
    dlg.add_button("Cancel", "ghost", accept=False)
    dlg.add_button("OK", "primary", default=True)
    dlg.finish()
    if dlg.exec() != QDialog.DialogCode.Accepted:
        return None
    return edit.dateTime().toPyDateTime().replace(second=0, microsecond=0)


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
