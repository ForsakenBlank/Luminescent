"""
Luminescent-style toasts: small cards in the theme's colours, stacked in a corner of the
primary screen (below the bar), that fade out by themselves. Click one to dismiss it.

Used instead of desktop notifications when Settings → General → Notification style is
"Luminescent". They never take the keyboard focus.
"""
from PyQt6.QtWidgets import QWidget, QLabel, QVBoxLayout, QHBoxLayout, QFrame
from PyQt6.QtCore import Qt, QTimer, QEasingCurve

from core import theme, widgets, anim
from core.guard import alive

WIDTH = 340
GAP = 10
MARGIN = 16
MAX_SHOWN = 5


class _Toast(QWidget):
    def __init__(self, manager: "ToastManager", title: str, message: str, ms: int):
        super().__init__()
        self.manager = manager
        self.setWindowFlags(Qt.WindowType.FramelessWindowHint | Qt.WindowType.WindowStaysOnTopHint | Qt.WindowType.Tool)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating)
        self.setWindowTitle("Luminescent toast")
        self.setFixedWidth(WIDTH)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        p = theme.P
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        box = QFrame()
        box.setObjectName("toastBox")
        box.setStyleSheet(f"#toastBox {{ background: {p.bg_base}; border: 1px solid {p.border};"
                          f" border-radius: {p.radius}px; }}")
        outer.addWidget(box)
        self.box = box
        row = QHBoxLayout(box)
        row.setContentsMargins(0, 0, 14, 0)
        row.setSpacing(12)
        bar = QFrame()
        bar.setFixedWidth(3)
        bar.setStyleSheet(f"background: {p.primary}; border-top-left-radius: {p.radius}px;"
                          f" border-bottom-left-radius: {p.radius}px;")
        row.addWidget(bar)
        mark = QLabel()
        mark.setPixmap(theme.sparkle_pixmap(14))
        mark.setFixedWidth(14)
        row.addWidget(mark, 0, Qt.AlignmentFlag.AlignTop)
        col = QVBoxLayout()
        col.setContentsMargins(0, 12, 0, 12)
        col.setSpacing(3)
        t = widgets.label(title, 13, 600)
        t.setWordWrap(True)
        col.addWidget(t)
        if message:
            m = widgets.label(message if len(message) < 400 else message[:399] + "…", 12, tone="secondary")
            m.setWordWrap(True)
            col.addWidget(m)
        row.addLayout(col, 1)
        mark.setContentsMargins(0, 14, 0, 0)
        for w in self.findChildren(QWidget):
            w.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        self.adjustSize()
        self._life = QTimer(self)
        self._life.setSingleShot(True)
        self._life.timeout.connect(self.fade_out)
        self._life.start(max(1500, int(ms)))
        self._closing = False

    def enterEvent(self, e):
        self._life.stop()                 # hovering keeps it up
        super().enterEvent(e)

    def leaveEvent(self, e):
        if not self._closing:
            self._life.start(1500)
        super().leaveEvent(e)

    def mousePressEvent(self, e):
        self.fade_out()

    def fade_out(self):
        if self._closing:
            return
        self._closing = True
        # window opacity does nothing on Wayland, so the card inside the window fades
        anim.fade(self.box, 1.0, 0.0, "toasts", 180, then=self._gone, curve=QEasingCurve.Type.InCubic)

    def _gone(self):
        if not alive(self):
            return
        self.manager.removed(self)
        self.close()
        self.deleteLater()


class ToastManager:
    def __init__(self, settings):
        self.settings = settings
        self._toasts: list[_Toast] = []

    def perf(self) -> bool:
        return bool(self.settings.get("ui.performance_mode", False))

    def show(self, title: str, message: str, ms: int = 4000):
        self._toasts = [t for t in self._toasts if alive(t)]
        while len(self._toasts) >= MAX_SHOWN:
            old = self._toasts.pop(0)
            old.close()
            old.deleteLater()
        t = _Toast(self, title, message, ms)
        self._toasts.append(t)
        self._layout()
        t.show()
        anim.fade(t.box, 0.0, 1.0, "toasts", 170)

    def removed(self, t: _Toast):
        if t in self._toasts:
            self._toasts.remove(t)
            self._layout()

    def _layout(self):
        g = widgets.primary_geometry()
        y = g.top() + MARGIN
        self._toasts = [t for t in self._toasts if alive(t)]
        for t in reversed(self._toasts):            # newest on top
            t.move(g.right() - WIDTH - MARGIN, y)
            y += t.height() + GAP

    def clear(self):
        for t in self._toasts:
            if alive(t):
                t.close()
                t.deleteLater()
        self._toasts = []
