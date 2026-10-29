"""
Luminescent-style toasts: small cards in the theme's colours, stacked in a corner of a screen, that fade out
by themselves. They grow to fit their message; a very long one shows its start with "Click to read all",
and clicking it opens the whole text. Click again (or the ✕) to dismiss, right-click to copy the text.

Used instead of desktop notifications when Settings → General → Notification style is "Luminescent".
They never take the keyboard focus. Corner, screen, width, how many show and how long they stay are in
Settings → General → Notifications.
"""
import re

from PyQt6.QtWidgets import QWidget, QLabel, QVBoxLayout, QHBoxLayout, QFrame, QApplication
from PyQt6.QtCore import Qt, QTimer, QEasingCurve

from core import theme, widgets, anim
from core.guard import alive

GAP = 10
MARGIN = 16
SHORT = 260                         # characters shown before "Click to read all"
LONGEST = 6000


def _breakable(text: str) -> str:
    """Long unbroken runs (paths, URLs, hashes) get invisible break points, so the label can wrap them."""
    return re.sub(r"\S{28,}", lambda m: "​".join(m.group(0)[i:i + 24] for i in range(0, len(m.group(0)), 24)),
                  text)


class _Toast(QWidget):
    def __init__(self, manager: "ToastManager", title: str, message: str, ms: int):
        super().__init__()
        self.manager = manager
        self.message = (message or "")[:LONGEST]
        self.expanded = False
        self.setWindowFlags(Qt.WindowType.FramelessWindowHint | Qt.WindowType.WindowStaysOnTopHint | Qt.WindowType.Tool)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating)
        self.setWindowTitle("Luminescent toast")
        self.width_ = manager.width()
        self.setFixedWidth(self.width_)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        p = theme.P
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        box = QFrame()
        box.setObjectName("toastBox")
        box.setStyleSheet(f"#toastBox {{ background: {theme.rgba(p.bg_base, manager.opacity())};"
                          f" border: 1px solid {theme.mix(p.border, p.text_muted, 0.25)}; border-radius: {p.radius}px; }}")
        outer.addWidget(box)
        self.box = box
        row = QHBoxLayout(box)
        row.setContentsMargins(0, 0, 8, 0)
        row.setSpacing(12)
        bar = QFrame()
        bar.setFixedWidth(3)
        bar.setStyleSheet(f"background: {p.primary}; border-top-left-radius: {p.radius}px;"
                          f" border-bottom-left-radius: {p.radius}px;")
        row.addWidget(bar)
        mark = QLabel()
        mark.setPixmap(theme.sparkle_pixmap(14))
        mark.setFixedWidth(14)
        mark.setContentsMargins(0, 14, 0, 0)
        row.addWidget(mark, 0, Qt.AlignmentFlag.AlignTop)
        col = QVBoxLayout()
        col.setContentsMargins(0, 12, 0, 12)
        col.setSpacing(3)
        self.title = widgets.label(_breakable(title), 13, 600)
        self.title.setWordWrap(True)
        col.addWidget(self.title)
        self.text = widgets.label("", 12, tone="secondary")
        self.text.setWordWrap(True)
        self.text.setVisible(bool(self.message))
        col.addWidget(self.text)
        self.more = widgets.label("Click to read all", 11, 500, tone="accent")
        self.more.hide()
        col.addWidget(self.more)
        row.addLayout(col, 1)
        self.close_btn = widgets.icon_button("✕", "Dismiss", 22, 10, ghost=True)
        self.close_btn.setFocusPolicy(Qt.FocusPolicy.NoFocus)
        self.close_btn.clicked.connect(self.fade_out)
        row.addWidget(self.close_btn, 0, Qt.AlignmentFlag.AlignTop)
        self.close_btn.setContentsMargins(0, 8, 0, 0)
        for w in self.findChildren(QWidget):
            if w is not self.close_btn:
                w.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        self._show_text()
        self._life = QTimer(self)
        self._life.setSingleShot(True)
        self._life.timeout.connect(self.fade_out)
        # long messages stay up longer: about a second for every 20 characters, up to 30 s
        reading = 2500 + 50 * min(len(self.message), SHORT * 2)
        self._ms = int(max(1500, ms, min(30000, reading)) * manager.time_scale())
        self._life.start(self._ms)
        self._closing = False

    def _show_text(self):
        long = len(self.message) > SHORT
        shown = self.message if self.expanded or not long else self.message[:SHORT].rstrip() + "…"
        self.text.setText(_breakable(shown))
        self.more.setVisible(long and not self.expanded)
        self.fit()

    def fit(self):
        """The window as tall as its text at this width (a top-level window does not do this by itself:
        word-wrapped labels would otherwise be cut off after the first line). Capped at 80% of the screen."""
        lay = self.layout()
        for w in (self.title, self.text, self.more, self.box):
            w.updateGeometry()
        self.box.layout().invalidate()
        lay.invalidate()
        lay.activate()
        h = lay.heightForWidth(self.width_) if lay.hasHeightForWidth() else lay.sizeHint().height()
        g = widgets.primary_geometry()
        self.setFixedHeight(max(48, min(h, int(g.height() * 0.8))))

    def enterEvent(self, e):
        self._life.stop()                 # hovering keeps it up
        super().enterEvent(e)

    def leaveEvent(self, e):
        if not self._closing:
            self._life.start(max(1500, self._ms // 2) if self.expanded else 1500)
        super().leaveEvent(e)

    def mousePressEvent(self, e):
        if e.button() == Qt.MouseButton.RightButton:
            QApplication.clipboard().setText(f"{self.title.text()}\n{self.message}".replace("​", "").strip())
            self.more.setText("Copied")
            self.more.show()
            self.fit()
            self.manager.relayout()
            return
        if self.more.isVisible() and not self.expanded:
            self.expanded = True
            self._show_text()
            self.manager.relayout()
            return
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

    def width(self) -> int:
        try:
            return max(240, min(720, int(self.settings.get("general.toast_width", 360))))
        except (TypeError, ValueError):
            return 360

    def time_scale(self) -> float:
        try:
            return max(0.3, min(5.0, float(self.settings.get("general.toast_time", 1.0))))
        except (TypeError, ValueError):
            return 1.0

    def opacity(self) -> float:
        try:
            return max(0.5, min(1.0, float(self.settings.get("general.toast_opacity", 1.0))))
        except (TypeError, ValueError):
            return 1.0

    def max_shown(self) -> int:
        try:
            return max(1, min(12, int(self.settings.get("general.toast_max", 5))))
        except (TypeError, ValueError):
            return 5

    def show(self, title: str, message: str, ms: int = 4000):
        self._toasts = [t for t in self._toasts if alive(t)]
        while len(self._toasts) >= self.max_shown():
            old = self._toasts.pop(0)
            old.close()
            old.deleteLater()
        t = _Toast(self, title, message, ms)
        self._toasts.append(t)
        self.relayout()
        t.show()
        anim.fade(t.box, 0.0, 1.0, "toasts", 170)

    def removed(self, t: _Toast):
        if t in self._toasts:
            self._toasts.remove(t)
            self.relayout()

    def _screen(self):
        if self.settings.get("general.toast_screen", "primary") == "cursor":
            return widgets.cursor_screen_geometry()
        return widgets.primary_geometry()

    def relayout(self):
        g = self._screen()
        corner = str(self.settings.get("general.toast_corner", "tr") or "tr")
        bottom = corner.startswith("b")
        self._toasts = [t for t in self._toasts if alive(t)]
        y = g.bottom() - MARGIN if bottom else g.top() + MARGIN
        for t in reversed(self._toasts):            # newest nearest the corner
            w = t.width()
            x = {"l": g.left() + MARGIN, "c": g.left() + (g.width() - w) // 2}.get(corner[1:2], g.right() - w - MARGIN)
            if bottom:
                y -= t.height()
                t.move(x, y)
                y -= GAP
            else:
                t.move(x, y)
                y += t.height() + GAP

    _layout = relayout

    def clear(self):
        for t in self._toasts:
            if alive(t):
                t.close()
                t.deleteLater()
        self._toasts = []
