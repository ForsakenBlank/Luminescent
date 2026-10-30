"""
Luminescent-style toasts: cards in the theme's colours, stacked in a corner of the primary screen, that fade
out by themselves. They never take the keyboard focus.

The card is painted by hand, so the accent sits inside its rounded shape instead of a separate bar laid over
the corner. Settings → General → Notifications sets where they go, how wide they are, the accent style
(a soft glow, a stripe, a line along the top, an outline or none), how long they stay, how see-through they are
and the text size; Themes → Settings… → Notifications gives them colours (and gradients) of their own.

Long messages are never cut off: the card grows to fit up to about half the screen. Anything longer shows
the first lines and "Click to read it all"; a click opens it fully. Hover a card to keep it, ✕ to close it,
right-click to copy the text. Errors ("… error", "failed") get a red accent.

Used instead of desktop notifications when Settings → General → Notification style is "Luminescent".
"""
from PyQt6.QtWidgets import QWidget, QLabel, QVBoxLayout, QHBoxLayout, QApplication, QSizePolicy
from PyQt6.QtCore import Qt, QTimer, QEasingCurve, QRectF, QPointF
from PyQt6.QtGui import QPainter, QPainterPath, QColor, QPen, QLinearGradient, QRadialGradient, QBrush

from core import theme, widgets, anim
from core.guard import alive

GAP = 10
MARGIN = 16
STYLES = [("glow", "A soft glow of the accent"), ("stripe", "A stripe down the left side"),
          ("top", "A line along the top"), ("outline", "An accent-coloured outline"), ("none", "No accent")]
POSITIONS = [("tr", "Top right"), ("tc", "Top centre"), ("tl", "Top left"), ("br", "Bottom right"),
             ("bc", "Bottom centre"), ("bl", "Bottom left")]
LEVEL_COLOURS = {"error": theme.DANGER, "warning": "#E0AE3F", "ok": "#3FBF74"}
LEVEL_ICONS = {"error": "✕", "warning": "!", "ok": "✓"}
COLLAPSED_LINES = 6


def card_style(settings) -> str:
    s = settings.get("general.toast_style", "glow") if settings is not None else "glow"
    return s if s in dict(STYLES) else "glow"


def level_of(title: str, message: str = "") -> str:
    low = title.lower()
    if "error" in low or "failed" in low or "could not" in low:
        return "error"
    if low.startswith("⚠") or "warning" in low:
        return "warning"
    return "info"


def paint_card(qp: QPainter, r: QRectF, p, style: str, accent: str | None = None, opacity: float = 1.0):
    """The card: rounded background (its own gradient if the theme gives it one), a 1 px border, and the accent in
    the chosen style, clipped to the rounded shape so nothing pokes out of the corners."""
    accent = accent or p.primary
    rad = float(p.radius)
    shape = QPainterPath()
    shape.addRoundedRect(r.adjusted(0.5, 0.5, -0.5, -0.5), rad, rad)
    qp.save()
    qp.setPen(Qt.PenStyle.NoPen)
    qp.setBrush(theme.qbrush("bg_base", r, opacity, p))
    qp.drawPath(shape)
    qp.setClipPath(shape)
    acc = QColor(accent)
    if style == "glow":
        g = QRadialGradient(QPointF(r.left() + 22, r.top() + 20), max(r.width() * 0.75, 120))
        c0, c1 = QColor(acc), QColor(acc)
        c0.setAlphaF(0.22 * opacity)
        c1.setAlphaF(0.0)
        g.setColorAt(0, c0)
        g.setColorAt(1, c1)
        qp.fillRect(r, QBrush(g))
    elif style == "stripe":
        qp.fillRect(QRectF(r.left(), r.top(), 4, r.height()), theme.qbrush("primary", r, 1.0, p)
                    if accent == p.primary else acc)
    elif style == "top":
        lg = QLinearGradient(r.left(), 0, r.right(), 0)
        fade = QColor(acc)
        fade.setAlphaF(0.0)
        lg.setColorAt(0, acc)
        lg.setColorAt(0.65, acc)
        lg.setColorAt(1, fade)
        qp.fillRect(QRectF(r.left(), r.top(), r.width(), 2.5), QBrush(lg))
    qp.restore()
    qp.save()
    edge = acc if style == "outline" else QColor(p.border)
    qp.setPen(QPen(edge, 1.5 if style == "outline" else 1))
    qp.setBrush(Qt.BrushStyle.NoBrush)
    qp.drawPath(shape)
    qp.restore()


class _Card(QWidget):
    """The painted card inside the toast window (window opacity does nothing on Wayland, so this fades)."""

    def __init__(self, toast: "_Toast"):
        super().__init__(toast)
        self.toast = toast

    def paintEvent(self, e):
        qp = QPainter(self)
        qp.setRenderHint(QPainter.RenderHint.Antialiasing)
        t = self.toast
        paint_card(qp, QRectF(self.rect()), t.pal, t.style, t.accent, t.opacity)
        qp.end()


class _Toast(QWidget):
    def __init__(self, manager: "ToastManager", title: str, message: str, ms: int, level: str = ""):
        super().__init__()
        self.manager = manager
        self.setWindowFlags(Qt.WindowType.FramelessWindowHint | Qt.WindowType.WindowStaysOnTopHint | Qt.WindowType.Tool)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setProperty("lum_glass", widgets.GLASS)
        self.setAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating)
        self.setWindowTitle("Luminescent toast")
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        s = manager.settings
        self.width_px = int(s.get("general.toast_width", 360) or 360)
        self.setFixedWidth(self.width_px)
        self.level = level or level_of(title, message)
        self.style = card_style(s)
        self.opacity = max(0.3, min(1.0, float(s.get("general.toast_opacity", 1.0) or 1.0)))
        scale = float(s.get("general.toast_text", 1.0) or 1.0)
        with theme.module_colours("toasts"):
            self.pal = theme.P
        p = self.pal
        self.accent = LEVEL_COLOURS.get(self.level) or p.primary
        self.title, self.message = title, message

        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        self.box = _Card(self)
        outer.addWidget(self.box)
        row = QHBoxLayout(self.box)
        row.setContentsMargins(16, 12, 12, 13)
        row.setSpacing(12)
        self.has_icon = bool(s.get("general.toast_icon", True))
        if self.has_icon:
            icon = QLabel()
            icon.setFixedSize(18, 18)
            icon.setAlignment(Qt.AlignmentFlag.AlignCenter)
            if self.level in LEVEL_ICONS:
                icon.setText(LEVEL_ICONS[self.level])
                icon.setFont(theme.qfont(10 * scale, 700))
                icon.setStyleSheet(f"background: {self.accent}; color: #FFFFFF; border-radius: 9px;")
            else:
                icon.setPixmap(theme.sparkle_pixmap(15, p.primary))
            row.addWidget(icon, 0, Qt.AlignmentFlag.AlignTop)
        col = QVBoxLayout()
        col.setContentsMargins(0, 1, 0, 0)
        col.setSpacing(3)
        self.title_lbl = QLabel(title)
        self.title_lbl.setFont(theme.qfont(13 * scale, 600))
        self.title_lbl.setStyleSheet(f"color: {p.text_primary};")
        self.title_lbl.setWordWrap(True)
        col.addWidget(self.title_lbl)
        self.msg_lbl = None
        self.more_lbl = None
        self.expanded = False
        if message:
            self.msg_lbl = QLabel()
            self.msg_lbl.setFont(theme.qfont(12 * scale, 400))
            self.msg_lbl.setStyleSheet(f"color: {p.text_secondary};")
            self.msg_lbl.setWordWrap(True)
            self.msg_lbl.setTextFormat(Qt.TextFormat.PlainText)
            self.msg_lbl.setSizePolicy(QSizePolicy.Policy.Preferred, QSizePolicy.Policy.Minimum)
            col.addWidget(self.msg_lbl)
            self.more_lbl = QLabel("Click to read it all")
            self.more_lbl.setFont(theme.qfont(10.5 * scale, 500, mono=True, spacing=0.06))
            self.more_lbl.setStyleSheet(f"color: {p.primary_text};")
            self.more_lbl.hide()
            col.addWidget(self.more_lbl)
        row.addLayout(col, 1)
        self.close_btn = widgets.icon_button("✕", "Close", 20, 10, ghost=True)
        self.close_btn.clicked.connect(self.fade_out)
        self.close_btn.setStyleSheet(f"QPushButton {{ padding: 0; font-size: {theme.fs(10)}px; color: {p.text_muted};"
                                     f" background: transparent; border: none; }}"
                                     f"QPushButton:hover {{ color: {p.text_primary}; }}")
        keep = self.close_btn.sizePolicy()
        keep.setRetainSizeWhenHidden(True)            # the text keeps its width when ✕ shows on hover
        self.close_btn.setSizePolicy(keep)
        self.close_btn.setVisible(False)
        row.addWidget(self.close_btn, 0, Qt.AlignmentFlag.AlignTop)
        for w in self.findChildren(QLabel):
            w.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        self.title_lbl.setFixedWidth(self._text_width())
        if self.msg_lbl is not None:
            self.msg_lbl.setFixedWidth(self._text_width())
        self._fill_message()
        self._life = QTimer(self)
        self._life.setSingleShot(True)
        self._life.timeout.connect(self.fade_out)
        # long messages stay up longer: about a second for every 15 words past the first few
        words = len((message or "").split())
        self._ms = max(1500, int(ms * float(s.get("general.toast_time", 1.0) or 1.0)) + max(0, words - 12) * 70)
        self._life.start(self._ms)
        self._closing = False

    # ── size ───────────────────────────────────────────────────────────────
    def _text_width(self) -> int:
        icon = 30 if self.has_icon else 0
        return self.width_px - 16 - 12 - icon - 12 - 20

    def _fill_message(self):
        if self.msg_lbl is None:
            self._fit()
            return
        text = self.message if len(self.message) < 6000 else self.message[:5999] + "…"
        self.msg_lbl.setText(text)
        fm = self.msg_lbl.fontMetrics()
        w = self._text_width()
        lines = fm.boundingRect(0, 0, w, 100000, Qt.TextFlag.TextWordWrap, text).height() / max(1, fm.lineSpacing())
        scr = widgets.primary_geometry()
        limit = max(COLLAPSED_LINES, int(scr.height() * 0.45 / max(1, fm.lineSpacing())))
        if not self.expanded and lines > limit:
            # show what fits, then "click to read it all"
            shown = self._clip_lines(text, fm, w, COLLAPSED_LINES)
            self.msg_lbl.setText(shown)
            self.more_lbl.show()
        else:
            self.more_lbl.hide()
        self._fit()

    @staticmethod
    def _clip_lines(text: str, fm, width: int, n: int) -> str:
        words, out = text.split(" "), ""
        for wd in words:
            trial = (out + " " + wd).strip()
            h = fm.boundingRect(0, 0, width, 100000, Qt.TextFlag.TextWordWrap, trial + " …").height()
            if h > n * fm.lineSpacing():
                break
            out = trial
        return (out or text[:200]) + " …"

    def _fit(self):
        lay = self.layout()
        lay.activate()
        h = lay.totalHeightForWidth(self.width_px) if lay.hasHeightForWidth() else lay.totalSizeHint().height()
        scr = widgets.primary_geometry()
        self.setFixedHeight(max(46, min(h, int(scr.height() * 0.9))))

    # ── behaviour ──────────────────────────────────────────────────────────
    def enterEvent(self, e):
        self._life.stop()                 # hovering keeps it up
        self.close_btn.setVisible(True)
        super().enterEvent(e)

    def leaveEvent(self, e):
        self.close_btn.setVisible(False)
        if not self._closing:
            self._life.start(max(2500, self._ms // 2) if self.expanded else 1500)
        super().leaveEvent(e)

    def mousePressEvent(self, e):
        if e.button() == Qt.MouseButton.RightButton:
            m = widgets.menu(self)
            m.addAction("Copy the text", lambda: QApplication.clipboard().setText(
                f"{self.title}\n{self.message}".strip()))
            m.addAction("Close", self.fade_out)
            m.exec(e.globalPosition().toPoint())
            return
        if self.more_lbl is not None and self.more_lbl.isVisible():
            self.expanded = True
            self._fill_message()
            self.manager._layout()
            return
        self.fade_out()

    def fade_out(self):
        if self._closing:
            return
        self._closing = True
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

    def show(self, title: str, message: str, ms: int = 4000, level: str = ""):
        self._toasts = [t for t in self._toasts if alive(t)]
        most = max(1, int(self.settings.get("general.toast_max", 5) or 5))
        while len(self._toasts) >= most:
            old = self._toasts.pop(0)
            old.close()
            old.deleteLater()
        t = _Toast(self, title, message, ms, level)
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
        where = self.settings.get("general.toast_position", "tr") or "tr"
        bottom = where.startswith("b")
        self._toasts = [t for t in self._toasts if alive(t)]
        y = g.bottom() - MARGIN if bottom else g.top() + MARGIN
        for t in reversed(self._toasts):            # newest nearest the edge
            if where.endswith("l"):
                x = g.left() + MARGIN
            elif where.endswith("c"):
                x = g.center().x() - t.width() // 2
            else:
                x = g.right() - t.width() - MARGIN
            if bottom:
                y -= t.height()
                t.move(x, y)
                y -= GAP
            else:
                t.move(x, y)
                y += t.height() + GAP

    def clear(self):
        for t in self._toasts:
            if alive(t):
                t.close()
                t.deleteLater()
        self._toasts = []
