"""
A big warning in the middle of the screen, above everything (the Automation "Warning on screen" node,
and "Show a notification" set to "A warning in the middle"). Unlike a toast it stays until you click
"Got it" (or press Enter / Esc), unless it was given a time to close by itself.

A new warning with the same title replaces the old one, so a node that fires every few seconds does
not pile them up.
"""
from PyQt6.QtWidgets import QWidget, QVBoxLayout, QHBoxLayout, QFrame, QLabel, QPushButton
from PyQt6.QtCore import Qt, QTimer, QPropertyAnimation, QEasingCurve

from core import theme, widgets

LEVELS = [("warning", "Warning (amber)"), ("danger", "Danger (red)"), ("info", "Info (theme colour)"),
          ("ok", "Good news (green)")]
ICONS = {"warning": "⚠", "danger": "⛔", "info": "✦", "ok": "✔"}
_open: dict[str, "CenterAlert"] = {}


def colour(level: str) -> str:
    return {"warning": "#F2B705", "danger": theme.DANGER, "ok": "#3FBF74"}.get(level, theme.P.primary)


class CenterAlert(QWidget):
    def __init__(self, title: str, message: str, level: str = "warning", seconds: float = 0, on_close=None):
        super().__init__(None, Qt.WindowType.FramelessWindowHint | Qt.WindowType.WindowStaysOnTopHint
                         | Qt.WindowType.Tool)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setAttribute(Qt.WidgetAttribute.WA_DeleteOnClose)
        self.setWindowTitle("Luminescent warning")
        self.key = title
        self.on_close = on_close
        self._closed = False
        p = theme.P
        c = colour(level)
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        box = QFrame()
        box.setObjectName("alertBox")
        box.setStyleSheet(f"#alertBox {{ background: {p.bg_base}; border: 2px solid {c};"
                          f" border-radius: {p.radius + 4}px; }}")
        outer.addWidget(box)
        lay = QVBoxLayout(box)
        lay.setContentsMargins(34, 28, 34, 24)
        lay.setSpacing(12)
        head = QHBoxLayout()
        head.setSpacing(14)
        icon = QLabel(ICONS.get(level, "⚠"))
        icon.setFont(theme.qfont(34, 600))
        icon.setStyleSheet(f"color: {c};")
        head.addWidget(icon, 0, Qt.AlignmentFlag.AlignTop)
        t = widgets.label(title, 24, 700)
        t.setWordWrap(True)
        head.addWidget(t, 1, Qt.AlignmentFlag.AlignVCenter)
        lay.addLayout(head)
        if message:
            m = widgets.label(message if len(message) < 1500 else message[:1499] + "…", 16, tone="secondary")
            m.setWordWrap(True)
            m.setTextInteractionFlags(Qt.TextInteractionFlag.TextSelectableByMouse)
            lay.addWidget(m)
        row = QHBoxLayout()
        self.count = widgets.label("", 12, tone="muted")
        row.addWidget(self.count)
        row.addStretch()
        ok = QPushButton("Got it")
        ok.setProperty("variant", "primary")
        ok.setMinimumWidth(120)
        ok.setStyleSheet(f"QPushButton {{ background: {c}; color: #111; border: none; font-weight: 600;"
                         f" padding: 9px 18px; font-size: {theme.fs(14)}px; }}")
        ok.clicked.connect(self.close)
        ok.setDefault(True)
        row.addWidget(ok)
        lay.addLayout(row)
        self.setFixedWidth(min(640, max(420, widgets.cursor_screen_geometry().width() // 3)))
        self.adjustSize()
        self._left = int(seconds)
        if self._left > 0:
            self._tick = QTimer(self)
            self._tick.setInterval(1000)
            self._tick.timeout.connect(self._count_down)
            self._tick.start()
            self._count_down(first=True)

    def _count_down(self, first: bool = False):
        if not first:
            self._left -= 1
        if self._left <= 0:
            self.close()
            return
        self.count.setText(f"closes in {self._left} s")

    def popup(self):
        g = widgets.cursor_screen_geometry()
        self.move(g.center().x() - self.width() // 2, g.center().y() - self.height() // 2)
        self.show()
        self.raise_()
        widgets.bring_to_front(self)
        widgets.raise_window(self)
        a = QPropertyAnimation(self, b"windowOpacity", self)
        a.setDuration(160)
        a.setStartValue(0.0)
        a.setEndValue(1.0)
        a.setEasingCurve(QEasingCurve.Type.OutCubic)
        a.start()

    def keyPressEvent(self, e):
        if e.key() in (Qt.Key.Key_Return, Qt.Key.Key_Enter, Qt.Key.Key_Escape, Qt.Key.Key_Space):
            self.close()
            return
        super().keyPressEvent(e)

    def closeEvent(self, e):
        if not self._closed:
            self._closed = True
            if _open.get(self.key) is self:
                del _open[self.key]
            if self.on_close:
                self.on_close()
        super().closeEvent(e)


def show(title: str, message: str = "", level: str = "warning", seconds: float = 0, on_close=None) -> CenterAlert:
    old = _open.pop(title, None)
    if old is not None:
        old.on_close = None
        old.close()
    a = CenterAlert(title, message, level, seconds, on_close)
    _open[title] = a
    a.popup()
    return a
