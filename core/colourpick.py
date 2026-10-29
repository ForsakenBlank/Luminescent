"""
Luminescent's own colour picker, used everywhere instead of Qt's QColorDialog, and its gradient editor.

    value = colourpick.pick(parent, "#7B6CF6", "Accent")                    "#RRGGBB" or None (cancelled)
    value = colourpick.pick(parent, start, "Background", gradient=True)     "#RRGGBB", a gradient spec or None
    value = colourpick.pick(..., on_change=fn)                              fn(value) live while you pick

A gradient spec is {"type": "linear" | "radial" | "conical", "angle": degrees, "stops": [[0..1, "#RRGGBB"], ...]}
(see core/theme.py). The picker: a saturation / brightness square, a hue strip, the colour you started with
(click it to go back), a hex box that also takes rgb(...) and colour names, copy, an eyedropper (Luminescent's
screen colour picker), how readable text is on it, and rows of recent picks, this theme's colours and basics.
In gradient mode a bar shows every stop: click the bar to add one, drag a stop to move it, drag it off the bar
(or press Delete) to remove it; the square and strip edit the selected stop.

ColourSwatch is the button that shows a colour (or gradient) with its hex code and opens the picker.
"""
import os
import colorsys

from PyQt6.QtWidgets import (QWidget, QHBoxLayout, QVBoxLayout, QPushButton, QLineEdit, QComboBox,
                             QSlider, QSizePolicy, QApplication, QGridLayout, QFrame)
from PyQt6.QtCore import Qt, QRectF, QPointF, QTimer, pyqtSignal
from PyQt6.QtGui import QPainter, QColor, QLinearGradient, QPainterPath, QPen, QBrush, QImage

from core import theme, widgets, DATA_DIR, save_json, load_json

RECENT_PATH = os.path.join(DATA_DIR, "picker.json")
BASICS = ["#FFFFFF", "#9CA3AF", "#111111", "#EF4444", "#F97316", "#FACC15", "#22C55E", "#14B8A6", "#3B82F6",
          "#8B5CF6", "#EC4899"]
PRESETS = [
    ("Sunset", {"type": "linear", "angle": 135, "stops": [[0, "#FF7E5F"], [1, "#FEB47B"]]}),
    ("Ocean", {"type": "linear", "angle": 135, "stops": [[0, "#2E3192"], [1, "#1BFFFF"]]}),
    ("Aurora", {"type": "linear", "angle": 120, "stops": [[0, "#00C9A7"], [0.5, "#845EC2"], [1, "#FF6F91"]]}),
    ("Ember", {"type": "radial", "angle": 0, "stops": [[0, "#F9D423"], [1, "#E65C00"]]}),
    ("Night", {"type": "linear", "angle": 90, "stops": [[0, "#232526"], [1, "#0F0F12"]]}),
    ("Prism", {"type": "conical", "angle": 0, "stops": [[0, "#FF0080"], [0.33, "#7928CA"], [0.66, "#2AF598"],
                                                         [1, "#FF0080"]]}),
]


# ── colour helpers ────────────────────────────────────────────────────────────
def parse(text: str) -> str | None:
    """'#abc', 'abcdef', 'rgb(1, 2, 3)', 'hsl(...)' or a colour name -> '#RRGGBB' (None if it is not a colour)."""
    t = (text or "").strip()
    if not t:
        return None
    low = t.lower().replace(" ", "")
    try:
        if low.startswith("rgb(") and low.endswith(")"):
            r, g, b = (int(float(x)) for x in low[4:-1].split(",")[:3])
            return "#" + "".join(f"{max(0, min(255, v)):02X}" for v in (r, g, b))
        if low.startswith("hsl(") and low.endswith(")"):
            h, s_, l_ = low[4:-1].split(",")[:3]
            r, g, b = colorsys.hls_to_rgb(float(h) / 360, float(l_.rstrip("%")) / 100, float(s_.rstrip("%")) / 100)
            return "#" + "".join(f"{round(v * 255):02X}" for v in (r, g, b))
    except ValueError:
        return None
    if not t.startswith("#") and len(t) in (3, 6) and all(c in "0123456789abcdefABCDEF" for c in t):
        t = "#" + t
    c = QColor(t)
    return c.name().upper() if c.isValid() else None


def hsv_of(hex_colour: str) -> tuple[float, float, float]:
    c = QColor(hex_colour)
    h, s, v, _a = c.getHsvF()
    return (max(0.0, h) * 360.0, s, v)


def hex_of(h: float, s: float, v: float) -> str:
    return QColor.fromHsvF((h % 360) / 360.0, max(0.0, min(1.0, s)), max(0.0, min(1.0, v))).name().upper()


def contrast(a: str, b: str) -> float:
    """WCAG contrast ratio of two colours (1..21)."""
    def lum(hx):
        out = []
        for i in (1, 3, 5):
            c = int(hx[i:i + 2], 16) / 255
            out.append(c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4)
        return 0.2126 * out[0] + 0.7152 * out[1] + 0.0722 * out[2]
    la, lb = lum(a), lum(b)
    hi, lo = max(la, lb), min(la, lb)
    return (hi + 0.05) / (lo + 0.05)


def recent() -> list[str]:
    d = load_json(RECENT_PATH, {})
    out = [c for c in (d.get("recent") or [] if isinstance(d, dict) else []) if parse(c)]
    eye = load_json(os.path.join(DATA_DIR, "colours.json"), {})      # the screen colour picker's own list
    out += [c.upper() for c in (eye.get("recent") or [] if isinstance(eye, dict) else []) if parse(c)]
    return list(dict.fromkeys(c.upper() for c in out))[:11]


def remember(hex_colour: str):
    d = load_json(RECENT_PATH, {})
    d = d if isinstance(d, dict) else {}
    old = [c for c in (d.get("recent") or []) if c != hex_colour]
    d["recent"] = [hex_colour] + old[:15]
    try:
        save_json(RECENT_PATH, d)
    except OSError:
        pass


def theme_colours() -> list[str]:
    p = theme.P
    cols = [p.primary, p.primary_text, p.primary_muted, p.bg_base, p.bg_card, p.bg_header, p.border,
            p.text_primary, p.text_secondary, p.text_muted, p.folder_border]
    return list(dict.fromkeys(c.upper() for c in cols if parse(c)))[:11]


def _rounded(r: QRectF, rad: float) -> QPainterPath:
    path = QPainterPath()
    path.addRoundedRect(r, rad, rad)
    return path


def paint_value(qp: QPainter, r: QRectF, value, rad: float):
    """Fill `r` (rounded) with a colour or a gradient spec."""
    qp.save()
    qp.setPen(Qt.PenStyle.NoPen)
    qp.setBrush(theme.qbrush(value if value else "#000000", r))
    qp.drawPath(_rounded(r, rad))
    qp.restore()


# ── the pieces ────────────────────────────────────────────────────────────────
class SVSquare(QWidget):
    """Saturation left to right, brightness bottom to top, for one hue."""
    changed = pyqtSignal(float, float)

    def __init__(self):
        super().__init__()
        self.setMinimumSize(200, 150)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        self.setCursor(Qt.CursorShape.CrossCursor)
        self.setFocusPolicy(Qt.FocusPolicy.ClickFocus)
        self.h, self.s, self.v = 0.0, 1.0, 1.0
        self._img: QImage | None = None
        self._img_key = None

    def set_hsv(self, h, s, v):
        self.h, self.s, self.v = h, s, v
        self.update()

    def _area(self) -> QRectF:
        return QRectF(self.rect()).adjusted(9, 9, -9, -9)          # room for the knob at the edges

    def _image(self, r: QRectF) -> QImage:
        key = (round(self.h, 1), int(r.width()), int(r.height()))
        if self._img is None or self._img_key != key:
            img = QImage(int(r.width()), int(r.height()), QImage.Format.Format_ARGB32_Premultiplied)
            img.fill(0)
            p = QPainter(img)
            whole = QRectF(0, 0, r.width(), r.height())
            g = QLinearGradient(0, 0, r.width(), 0)
            g.setColorAt(0, QColor("#FFFFFF"))
            g.setColorAt(1, QColor.fromHsvF((self.h % 360) / 360, 1, 1))
            p.fillRect(whole, g)
            g2 = QLinearGradient(0, 0, 0, r.height())
            g2.setColorAt(0, QColor(0, 0, 0, 0))
            g2.setColorAt(1, QColor(0, 0, 0, 255))
            p.fillRect(whole, g2)
            p.end()
            self._img, self._img_key = img, key
        return self._img

    def paintEvent(self, e):
        qp = QPainter(self)
        qp.setRenderHint(QPainter.RenderHint.Antialiasing)
        r = self._area()
        qp.save()
        qp.setClipPath(_rounded(r, 8))
        qp.drawImage(r.topLeft(), self._image(r))
        qp.restore()
        x = r.left() + self.s * r.width()
        y = r.top() + (1 - self.v) * r.height()
        qp.setBrush(QColor(hex_of(self.h, self.s, self.v)))
        qp.setPen(QPen(QColor(0, 0, 0, 120), 3))
        qp.drawEllipse(QPointF(x, y), 8, 8)
        qp.setPen(QPen(QColor("#FFFFFF"), 2))
        qp.drawEllipse(QPointF(x, y), 8, 8)
        qp.end()

    def _at(self, pos):
        r = self._area()
        self.s = max(0.0, min(1.0, (pos.x() - r.left()) / max(1.0, r.width())))
        self.v = 1.0 - max(0.0, min(1.0, (pos.y() - r.top()) / max(1.0, r.height())))
        self.update()
        self.changed.emit(self.s, self.v)

    def mousePressEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton:
            self._at(e.position())

    def mouseMoveEvent(self, e):
        if e.buttons() & Qt.MouseButton.LeftButton:
            self._at(e.position())

    def keyPressEvent(self, e):
        step = 0.1 if e.modifiers() & Qt.KeyboardModifier.ShiftModifier else 0.01
        d = {Qt.Key.Key_Left: (-step, 0), Qt.Key.Key_Right: (step, 0), Qt.Key.Key_Up: (0, step),
             Qt.Key.Key_Down: (0, -step)}.get(e.key())
        if d is None:
            return super().keyPressEvent(e)
        self.s = max(0.0, min(1.0, self.s + d[0]))
        self.v = max(0.0, min(1.0, self.v + d[1]))
        self.update()
        self.changed.emit(self.s, self.v)


class HueStrip(QWidget):
    changed = pyqtSignal(float)

    def __init__(self):
        super().__init__()
        self.setFixedHeight(18)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.setFocusPolicy(Qt.FocusPolicy.ClickFocus)
        self.h = 0.0

    def set_hue(self, h: float):
        self.h = h
        self.update()

    def _track(self) -> QRectF:
        return QRectF(self.rect()).adjusted(9, 3, -9, -3)

    def paintEvent(self, e):
        qp = QPainter(self)
        qp.setRenderHint(QPainter.RenderHint.Antialiasing)
        r = self._track()
        g = QLinearGradient(r.left(), 0, r.right(), 0)
        for i in range(7):
            g.setColorAt(i / 6, QColor.fromHsvF((i / 6) % 1.0, 1, 1))
        qp.setPen(Qt.PenStyle.NoPen)
        qp.setBrush(QBrush(g))
        qp.drawRoundedRect(r, r.height() / 2, r.height() / 2)
        x = r.left() + (self.h % 360) / 360 * r.width()
        qp.setBrush(QColor.fromHsvF((self.h % 360) / 360, 1, 1))
        qp.setPen(QPen(QColor(0, 0, 0, 110), 3))
        qp.drawEllipse(QPointF(x, self.height() / 2), 8, 8)
        qp.setPen(QPen(QColor("#FFFFFF"), 2))
        qp.drawEllipse(QPointF(x, self.height() / 2), 8, 8)
        qp.end()

    def _at(self, x: float):
        r = self._track()
        self.h = max(0.0, min(359.9, (x - r.left()) / max(1.0, r.width()) * 360))
        self.update()
        self.changed.emit(self.h)

    def mousePressEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton:
            self._at(e.position().x())

    def mouseMoveEvent(self, e):
        if e.buttons() & Qt.MouseButton.LeftButton:
            self._at(e.position().x())

    def keyPressEvent(self, e):
        step = 10 if e.modifiers() & Qt.KeyboardModifier.ShiftModifier else 1
        d = {Qt.Key.Key_Left: -step, Qt.Key.Key_Right: step}.get(e.key())
        if d is None:
            return super().keyPressEvent(e)
        self.h = (self.h + d) % 360
        self.update()
        self.changed.emit(self.h)


class Chip(QPushButton):
    """A small round swatch (a colour or a gradient) in the rows under the picker."""

    def __init__(self, value, tip: str = "", size: int = 20):
        super().__init__()
        self.value = value
        self.setFixedSize(size, size)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.setToolTip(tip or (value if isinstance(value, str) else ""))
        self.setStyleSheet("QPushButton { border: none; background: transparent; padding: 0; }")
        self.active = False

    def paintEvent(self, e):
        qp = QPainter(self)
        qp.setRenderHint(QPainter.RenderHint.Antialiasing)
        r = QRectF(self.rect()).adjusted(2, 2, -2, -2)
        paint_value(qp, r, self.value, r.height() / 2)
        ring = QColor(theme.P.primary) if self.active else (QColor(theme.P.text_muted) if self.underMouse()
                                                            else QColor(theme.P.border))
        qp.setPen(QPen(ring, 2 if self.active else 1))
        qp.setBrush(Qt.BrushStyle.NoBrush)
        qp.drawEllipse(r.adjusted(-1, -1, 1, 1) if self.active else r)
        qp.end()

    def enterEvent(self, e):
        self.update()
        super().enterEvent(e)

    def leaveEvent(self, e):
        self.update()
        super().leaveEvent(e)


class OldNew(QPushButton):
    """The colour you started with (left half, click it to go back) next to the new one (right half)."""

    def __init__(self, old):
        super().__init__()
        self.old, self.new = old, old
        self.setFixedSize(64, 30)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.setToolTip("Left: the colour you started with (click to go back) · right: the new one")
        self.setStyleSheet("QPushButton { border: none; background: transparent; padding: 0; }")

    def set_new(self, v):
        self.new = v
        self.update()

    def paintEvent(self, e):
        qp = QPainter(self)
        qp.setRenderHint(QPainter.RenderHint.Antialiasing)
        r = QRectF(self.rect()).adjusted(1, 1, -1, -1)
        whole = _rounded(r, 8)
        qp.save()
        qp.setClipPath(whole)
        half = QRectF(r.left(), r.top(), r.width() / 2, r.height())
        qp.fillRect(half, theme.qbrush(self.old, half))
        rest = QRectF(half.right(), r.top(), r.width() / 2, r.height())
        qp.fillRect(rest, theme.qbrush(self.new, rest))
        qp.restore()
        qp.setPen(QPen(QColor(theme.P.text_muted) if self.underMouse() else QColor(theme.P.border), 1))
        qp.setBrush(Qt.BrushStyle.NoBrush)
        qp.drawPath(whole)
        qp.end()

    def enterEvent(self, e):
        self.update()
        super().enterEvent(e)

    def leaveEvent(self, e):
        self.update()
        super().leaveEvent(e)


class Preview(QWidget):
    """The colour (or gradient) as it will look, big."""

    def __init__(self, height: int = 40):
        super().__init__()
        self.setFixedHeight(height)
        self.value = "#000000"

    def set_value(self, v):
        self.value = v
        self.update()

    def paintEvent(self, e):
        qp = QPainter(self)
        qp.setRenderHint(QPainter.RenderHint.Antialiasing)
        r = QRectF(self.rect()).adjusted(0.5, 0.5, -0.5, -0.5)
        paint_value(qp, r, self.value, 8)
        qp.setPen(QPen(QColor(theme.P.border), 1))
        qp.setBrush(Qt.BrushStyle.NoBrush)
        qp.drawRoundedRect(r, 8, 8)
        qp.end()


class GradientBar(QWidget):
    """The stops of a gradient on a bar: click the bar to add a stop, drag one to move it, drag it down off the
    bar or press Delete to remove it."""
    changed = pyqtSignal()
    selected = pyqtSignal(int)
    BAR_H, KNOB = 22, 9

    def __init__(self):
        super().__init__()
        self.setFixedHeight(self.BAR_H + 2 * self.KNOB + 10)
        self.setFocusPolicy(Qt.FocusPolicy.ClickFocus)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.stops: list[list] = [[0.0, "#000000"], [1.0, "#FFFFFF"]]
        self.current = 0
        self._drag = None
        self._removing = False

    def set_stops(self, stops: list, current: int = 0):
        self.stops = [list(s) for s in stops]
        self.current = max(0, min(len(self.stops) - 1, current))
        self.update()

    def _track(self) -> QRectF:
        return QRectF(self.KNOB + 1, 2, self.width() - 2 * self.KNOB - 2, self.BAR_H)

    def _x(self, pos: float) -> float:
        r = self._track()
        return r.left() + pos * r.width()

    def _pos(self, x: float) -> float:
        r = self._track()
        return max(0.0, min(1.0, (x - r.left()) / max(1.0, r.width())))

    def paintEvent(self, e):
        qp = QPainter(self)
        qp.setRenderHint(QPainter.RenderHint.Antialiasing)
        r = self._track()
        g = QLinearGradient(r.left(), 0, r.right(), 0)
        for pos, col in sorted(self.stops, key=lambda s: s[0]):
            g.setColorAt(pos, QColor(col))
        qp.setPen(QPen(QColor(theme.P.border), 1))
        qp.setBrush(QBrush(g))
        qp.drawRoundedRect(r, 6, 6)
        ky = r.bottom() + self.KNOB + 3
        for i, (pos, col) in enumerate(self.stops):
            x = self._x(pos)
            sel = i == self.current
            gone = sel and self._removing
            qp.setPen(QPen(QColor(theme.P.text_muted), 1))
            qp.drawLine(QPointF(x, r.bottom() - 4), QPointF(x, ky - self.KNOB + 1))
            c = QColor(col)
            if gone:
                c.setAlphaF(0.3)
            qp.setBrush(c)
            qp.setPen(QPen(QColor(theme.P.primary) if sel else QColor(theme.P.bg_base), 3 if sel else 2))
            rad = self.KNOB if sel else self.KNOB - 2
            qp.drawEllipse(QPointF(x, ky), rad, rad)
            if sel:
                qp.setPen(QPen(QColor("#FFFFFF"), 1))
                qp.setBrush(Qt.BrushStyle.NoBrush)
                qp.drawEllipse(QPointF(x, ky), rad - 2.5, rad - 2.5)
        qp.end()

    def _hit(self, pt: QPointF) -> int:
        ky = self._track().bottom() + self.KNOB + 3
        best, dist = -1, 1e9
        for i, (pos, _c) in enumerate(self.stops):
            d = abs(pt.x() - self._x(pos))
            if d < dist and d <= self.KNOB + 3 and abs(pt.y() - ky) <= self.KNOB + 6:
                best, dist = i, d
        return best

    def mousePressEvent(self, e):
        if e.button() != Qt.MouseButton.LeftButton:
            return
        i = self._hit(e.position())
        if i < 0:
            pos = self._pos(e.position().x())
            g = {"stops": sorted(self.stops, key=lambda s: s[0])}
            self.stops.append([pos, theme.gradient_at(g, pos)])
            i = len(self.stops) - 1
            self.changed.emit()
        self.current = i
        self._drag = i
        self._removing = False
        self.selected.emit(i)
        self.update()

    def mouseMoveEvent(self, e):
        if self._drag is None or not e.buttons() & Qt.MouseButton.LeftButton:
            return
        self._removing = len(self.stops) > 2 and e.position().y() > self.height() + 24
        self.stops[self._drag][0] = self._pos(e.position().x())
        self.update()
        self.changed.emit()

    def mouseReleaseEvent(self, e):
        if self._drag is not None and self._removing:
            self.remove(self._drag)
        self._drag = None
        self._removing = False
        self.update()

    def remove(self, i: int):
        if len(self.stops) <= 2 or not 0 <= i < len(self.stops):
            return
        self.stops.pop(i)
        self.current = max(0, min(len(self.stops) - 1, i - 1))
        self.changed.emit()
        self.selected.emit(self.current)
        self.update()

    def keyPressEvent(self, e):
        if e.key() in (Qt.Key.Key_Delete, Qt.Key.Key_Backspace):
            self.remove(self.current)
            return
        if e.key() in (Qt.Key.Key_Left, Qt.Key.Key_Right) and self.stops:
            step = 0.05 if e.modifiers() & Qt.KeyboardModifier.ShiftModifier else 0.01
            s = self.stops[self.current]
            s[0] = max(0.0, min(1.0, s[0] + (step if e.key() == Qt.Key.Key_Right else -step)))
            self.changed.emit()
            self.update()
            return
        super().keyPressEvent(e)


# ── the dialog ────────────────────────────────────────────────────────────────
class ColourDialog(widgets.RoundedDialog):
    """See the module docstring. `value()` is the result once it was accepted."""
    live = pyqtSignal(object)

    def __init__(self, parent=None, start="#7B6CF6", title: str = "Pick a colour", gradient: bool = False):
        super().__init__(parent, title or "Pick a colour", resizable=False, modal=False)
        self.allow_gradient = gradient
        g = theme.clean_gradient(start) if isinstance(start, dict) else None
        self.start = g or (parse(start) if isinstance(start, str) else None) or "#000000"
        self.mode = "gradient" if g else "solid"
        self.solid = theme.gradient_at(g, 0.0) if g else self.start
        self.grad = g or {"type": "linear", "angle": 90,
                          "stops": [[0.0, self.solid], [1.0, theme.mix(self.solid, "#000000", 0.5)]]}
        self._eye = None
        self.body.setContentsMargins(16, 12, 16, 14)
        self.body.setSpacing(10)

        if gradient:
            seg = QFrame()
            seg.setObjectName("cpSeg")
            p = theme.P
            seg.setStyleSheet(f"#cpSeg {{ background: {p.bg_frame}; border: 1px solid {p.border}; border-radius: 14px; }}"
                              f"#cpSeg QPushButton {{ background: transparent; border: none; border-radius: 11px;"
                              f" padding: 4px 14px; color: {p.text_muted}; font-size: {theme.fs(12)}px; }}"
                              f"#cpSeg QPushButton:checked {{ background: {theme.fill('primary')};"
                              f" color: {p.on_primary}; }}")
            sl = QHBoxLayout(seg)
            sl.setContentsMargins(2, 2, 2, 2)
            sl.setSpacing(0)
            self._mode_btns = {}
            for key, text in (("solid", "Solid"), ("gradient", "Gradient")):
                b = QPushButton(text)
                b.setCheckable(True)
                b.setAutoExclusive(True)
                b.setCursor(Qt.CursorShape.PointingHandCursor)
                b.clicked.connect(lambda _c=False, k=key: self.set_mode(k))
                sl.addWidget(b)
                self._mode_btns[key] = b
            self._mode_btns[self.mode].setChecked(True)
            self.header_right.addWidget(seg)

        # gradient tools
        self.gbox = QWidget()
        gl = QVBoxLayout(self.gbox)
        gl.setContentsMargins(0, 0, 0, 0)
        gl.setSpacing(8)
        self.gpreview = Preview(56)
        gl.addWidget(self.gpreview)
        self.bar = GradientBar()
        self.bar.changed.connect(self._bar_changed)
        self.bar.selected.connect(self._stop_selected)
        gl.addWidget(self.bar)
        hint = widgets.label("Click the bar to add a colour · drag a dot to move it, down off the bar to remove it",
                             10.5, tone="muted")
        hint.setWordWrap(True)
        gl.addWidget(hint)
        row = QHBoxLayout()
        row.setSpacing(6)
        self.gtype = QComboBox()
        for k, name in theme.GRADIENT_TYPES:
            self.gtype.addItem(name, k)
        self.gtype.currentIndexChanged.connect(self._type_changed)
        row.addWidget(self.gtype)
        self.angle = QSlider(Qt.Orientation.Horizontal)
        self.angle.setRange(0, 359)
        self.angle.valueChanged.connect(self._angle_changed)
        row.addWidget(self.angle, 1)
        self.angle_lbl = widgets.label("90°", 11, mono=True, tone="secondary")
        self.angle_lbl.setFixedWidth(34)
        row.addWidget(self.angle_lbl)
        gl.addLayout(row)
        tools = QHBoxLayout()
        tools.setSpacing(6)
        for text, tip, fn in (("↻ 45°", "Turn it a little", lambda: self.angle.setValue((self.angle.value() + 45) % 360)),
                              ("⇄ Reverse", "Swap the ends", self._reverse),
                              ("Even out", "Space the colours evenly", self._even),
                              ("✕ Stop", "Remove the selected colour (Delete)", lambda: self.bar.remove(self.bar.current))):
            b = widgets.button(text, "ghost", tip)
            b.setFixedHeight(26)
            b.clicked.connect(fn)
            tools.addWidget(b)
        tools.addStretch()
        gl.addLayout(tools)
        pre = QHBoxLayout()
        pre.setSpacing(4)
        pre.addWidget(widgets.label("PRESETS", 9.5, 600, mono=True, spacing=0.12, tone="muted"))
        pre.addSpacing(4)
        acc = {"type": "linear", "angle": 135, "stops": [[0, theme.P.primary], [1, theme.P.primary_text]]}
        for name, spec in [("This theme's accent", acc)] + PRESETS:
            c = Chip(spec, name, 22)
            c.clicked.connect(lambda _c=False, s=spec: self._use_preset(s))
            pre.addWidget(c)
        pre.addStretch()
        gl.addLayout(pre)
        self.body.addWidget(self.gbox)

        # the picker itself
        self.sv = SVSquare()
        self.sv.changed.connect(self._sv_changed)
        self.body.addWidget(self.sv, 1)
        self.hue = HueStrip()
        self.hue.changed.connect(self._hue_changed)
        self.body.addWidget(self.hue)

        row = QHBoxLayout()
        row.setSpacing(6)
        self.oldnew = OldNew(self.start)
        self.oldnew.clicked.connect(self._revert)
        row.addWidget(self.oldnew)
        self.hex = QLineEdit()
        self.hex.setProperty("mono", True)
        self.hex.setFixedHeight(30)
        self.hex.setPlaceholderText("#RRGGBB, rgb(…) or a name")
        self.hex.textEdited.connect(self._typed)
        self.hex.returnPressed.connect(self.accept)
        row.addWidget(self.hex, 1)
        cp = widgets.icon_button("⧉", "Copy the hex code", 30, 13)
        cp.clicked.connect(lambda: QApplication.clipboard().setText(self.hex.text()))
        row.addWidget(cp)
        eye = widgets.icon_button("⌖", "Pick a colour from the screen", 30, 15)
        eye.clicked.connect(self._eyedropper)
        row.addWidget(eye)
        self.body.addLayout(row)

        self.readable = widgets.label("", 10.5, mono=True, tone="muted")
        self.body.addWidget(self.readable)

        self.rows = QGridLayout()
        self.rows.setHorizontalSpacing(2)
        self.rows.setVerticalSpacing(4)
        self.chips: list[Chip] = []
        for r_, (label, cols) in enumerate((("RECENT", recent()), ("THEME", theme_colours()), ("BASICS", BASICS))):
            if not cols:
                continue
            lbl = widgets.label(label, 9, 600, mono=True, spacing=0.12, tone="muted")
            lbl.setFixedWidth(52)
            self.rows.addWidget(lbl, r_, 0)
            for i, col in enumerate(cols[:11]):
                c = Chip(col, col, 20)
                c.clicked.connect(lambda _c=False, v=col: self.set_colour(v))
                self.rows.addWidget(c, r_, i + 1)
                self.chips.append(c)
        self.rows.setColumnStretch(12, 1)
        self.body.addLayout(self.rows)

        btns = QHBoxLayout()
        btns.addStretch()
        cancel = widgets.button("Cancel", "ghost")
        cancel.clicked.connect(self.reject)
        done = widgets.button("Done", "primary")
        done.setDefault(True)
        done.clicked.connect(self.accept)
        btns.addWidget(cancel)
        btns.addWidget(done)
        self.body.addLayout(btns)

        self.setMinimumWidth(340)
        self._load_gradient()
        self.set_mode(self.mode)
        self.set_colour(self.solid if self.mode == "solid" else self.grad["stops"][0][1])

    # ── state ──────────────────────────────────────────────────────────────
    def value(self):
        if self.mode == "gradient":
            return theme.clean_gradient(self.grad)
        return self.solid

    def set_mode(self, mode: str):
        self.mode = mode if self.allow_gradient else "solid"
        self.gbox.setVisible(self.mode == "gradient")
        if self.allow_gradient:
            self._mode_btns[self.mode].setChecked(True)
        if self.mode == "gradient":
            self.set_colour(self.grad["stops"][self.bar.current][1], emit=False)
        else:
            self.set_colour(self.solid, emit=False)
        self.adjustSize()
        self.resize(max(self.width(), 360), self.sizeHint().height())
        self._emit()

    def _load_gradient(self):
        self.bar.set_stops(self.grad["stops"], 0)
        self.gtype.blockSignals(True)
        self.gtype.setCurrentIndex(max(0, self.gtype.findData(self.grad["type"])))
        self.gtype.blockSignals(False)
        self.angle.blockSignals(True)
        self.angle.setValue(int(self.grad["angle"]) % 360)
        self.angle.blockSignals(False)
        self.angle_lbl.setText(f"{int(self.grad['angle']) % 360}°")
        self.angle.setEnabled(self.grad["type"] != "radial")
        self.gpreview.set_value(self.grad)

    def set_colour(self, hex_colour: str, emit: bool = True, from_hsv: tuple | None = None):
        """Make `hex_colour` the colour being edited (the solid colour, or the selected gradient stop)."""
        hex_colour = parse(hex_colour) or "#000000"
        h, s, v = from_hsv or hsv_of(hex_colour)
        if from_hsv is None and (s <= 0.001 or v <= 0.001):
            h = self.hue.h                         # greys have no hue: keep the one you had
        self.sv.set_hsv(h, s, v)
        self.hue.set_hue(h)
        if not self.hex.hasFocus():
            self.hex.setText(hex_colour)
        self.oldnew.set_new(hex_colour)
        for c in self.chips:
            c.active = c.value == hex_colour
            c.update()
        bg = theme.P.bg_base
        best = max(("#FFFFFF", "#111111"), key=lambda t: contrast(hex_colour, t))
        self.readable.setText(f"TEXT ON IT: {'WHITE' if best == '#FFFFFF' else 'BLACK'} "
                              f"{contrast(hex_colour, best):.1f}:1   ·   AGAINST THE BACKGROUND "
                              f"{contrast(hex_colour, bg):.1f}:1")
        if self.mode == "gradient":
            self.grad["stops"] = [list(st) for st in self.bar.stops]
            if 0 <= self.bar.current < len(self.grad["stops"]):
                self.grad["stops"][self.bar.current][1] = hex_colour
                self.bar.stops[self.bar.current][1] = hex_colour
                self.bar.update()
            self.gpreview.set_value(self.grad)
        else:
            self.solid = hex_colour
        if emit:
            self._emit()

    def _emit(self):
        v = self.value()
        if v is not None:
            self.live.emit(v)

    # ── reactions ──────────────────────────────────────────────────────────
    def _sv_changed(self, s, v):
        self.set_colour(hex_of(self.hue.h, s, v), from_hsv=(self.hue.h, s, v))

    def _hue_changed(self, h):
        self.set_colour(hex_of(h, self.sv.s, self.sv.v), from_hsv=(h, self.sv.s, self.sv.v))

    def _typed(self, text):
        c = parse(text)
        self.hex.setProperty("invalid", c is None and bool(text.strip()))
        self.hex.style().unpolish(self.hex)
        self.hex.style().polish(self.hex)
        if c:
            self.set_colour(c)

    def _revert(self):
        if self.allow_gradient and isinstance(self.start, dict):
            self.grad = dict(self.start, stops=[list(s) for s in self.start["stops"]])
            self._load_gradient()
            self.set_mode("gradient")
            return
        if self.mode == "gradient" and isinstance(self.start, str):
            self.set_mode("solid")
        self.set_colour(self.start if isinstance(self.start, str) else self.solid)

    def _bar_changed(self):
        self.grad["stops"] = [list(s) for s in self.bar.stops]
        self.gpreview.set_value(self.grad)
        self._emit()

    def _stop_selected(self, i):
        if 0 <= i < len(self.bar.stops):
            self.set_colour(self.bar.stops[i][1], emit=False)

    def _type_changed(self, _i):
        self.grad["type"] = self.gtype.currentData()
        self.angle.setEnabled(self.grad["type"] != "radial")
        self.gpreview.set_value(self.grad)
        self._emit()

    def _angle_changed(self, v):
        self.grad["angle"] = v
        self.angle_lbl.setText(f"{v}°")
        self.gpreview.set_value(self.grad)
        self._emit()

    def _reverse(self):
        self.bar.set_stops([[1.0 - p, c] for p, c in self.bar.stops], self.bar.current)
        self._bar_changed()

    def _even(self):
        stops = sorted(self.bar.stops, key=lambda s: s[0])
        n = len(stops)
        cur = self.bar.stops[self.bar.current]
        self.bar.set_stops([[i / (n - 1), c] for i, (_p, c) in enumerate(stops)],
                           next((i for i, s in enumerate(stops) if s is cur), 0))
        self._bar_changed()

    def _use_preset(self, spec):
        self.grad = theme.clean_gradient(spec) or self.grad
        self._load_gradient()
        self.set_colour(self.grad["stops"][0][1], emit=False)
        self._emit()

    def _eyedropper(self):
        launcher = _launcher()
        m = launcher.modules.get("colorpicker") if launcher is not None else None
        if m is None or not hasattr(m, "pick"):
            widgets.error(self, "Pick from the screen", "The Colour Picker module is not available.")
            return
        self.hide()

        def got(hex_colour):
            self.show()
            self.raise_()
            if hex_colour:
                self.set_colour(hex_colour)
        QTimer.singleShot(150, lambda: m.pick(on_pick=got, parent=self))

    def done(self, r):
        if r and self.mode == "solid":
            remember(self.solid)
        elif r:
            for _p, c in self.grad["stops"]:
                remember(c)
        super().done(r)

    def keyPressEvent(self, e):
        if e.key() in (Qt.Key.Key_Return, Qt.Key.Key_Enter) and not self.hex.hasFocus():
            self.accept()
            return
        super().keyPressEvent(e)


def _launcher():
    app = QApplication.instance()
    return getattr(app, "luminescent", None)


def pick(parent, start="#7B6CF6", title: str = "Pick a colour", gradient: bool = False, on_change=None):
    """Open the picker and wait for it. Returns "#RRGGBB", a gradient spec (only with gradient=True) or None."""
    dlg = ColourDialog(parent, start, title, gradient)
    if on_change is not None:
        dlg.live.connect(on_change)
    dlg.adjustSize()
    near = parent.window() if isinstance(parent, QWidget) else None
    if near is not None and near.isVisible():
        g = near.frameGeometry()
        scr = widgets.cursor_screen_geometry()
        x = min(max(scr.left() + 8, g.center().x() - dlg.width() // 2), scr.right() - dlg.width() - 8)
        y = min(max(scr.top() + 8, g.center().y() - dlg.height() // 2), scr.bottom() - dlg.height() - 8)
        dlg.move(x, y)
    else:
        dlg.fit_to_screen(dlg.width(), dlg.height())
    ok = dlg.exec() == 1
    result = dlg.value() if ok else None
    if not ok and on_change is not None:
        on_change(start)                        # cancelled: put back what was there
    dlg.deleteLater()
    return result


def pick_colour(parent, start: str, title: str = "Pick a colour") -> str | None:
    """Solid colours only."""
    v = pick(parent, start, title)
    return v if isinstance(v, str) else None


# ── the swatch button ─────────────────────────────────────────────────────────
class ColourSwatch(QPushButton):
    """A framed swatch (colour or gradient) with its hex code; click it to pick. `changed(value)` after a pick.
    With `unset_text` it can also be empty (None), showing that text, e.g. "Theme"."""
    changed = pyqtSignal(object)

    def __init__(self, value=None, title: str = "Pick a colour", gradient: bool = False, unset_text: str = "",
                 width: int = 150, live: bool = False):
        super().__init__()
        self._value = value
        self.title = title
        self.gradient = gradient
        self.unset_text = unset_text
        self.live = live
        self.setFixedSize(width, 30)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.setStyleSheet(f"QPushButton {{ background: {theme.P.bg_frame}; padding: 0; }}"
                           f"QPushButton:hover {{ border-color: {theme.P.text_muted}; }}")
        self.clicked.connect(self.open_picker)

    def value(self):
        return self._value

    def set_value(self, v):
        self._value = v
        self.setToolTip("" if v is None else (v if isinstance(v, str) else
                                              " → ".join(c for _p, c in v.get("stops", []))))
        self.update()

    def open_picker(self):
        before = self._value
        start = before if before is not None else theme.P.primary
        def follow(v):                          # live: the swatch (and whoever listens) follows the picker
            self._value = v
            self.update()
            self.changed.emit(v)
        v = pick(self, start, self.title, self.gradient, on_change=follow if self.live else None)
        if v is not None:
            self.set_value(v)
            self.changed.emit(v)
        elif self.live:
            self.set_value(before)
            self.changed.emit(before)

    def paintEvent(self, e):
        super().paintEvent(e)
        qp = QPainter(self)
        qp.setRenderHint(QPainter.RenderHint.Antialiasing)
        chip = QRectF(6, 5, 20, 20)
        v = self._value
        if v is None:
            qp.setPen(QPen(QColor(theme.P.text_muted), 1, Qt.PenStyle.DashLine))
            qp.setBrush(Qt.BrushStyle.NoBrush)
            qp.drawRoundedRect(chip, 5, 5)
            text = self.unset_text or "None"
        else:
            if isinstance(v, dict):
                chip = QRectF(6, 5, 38, 20)
            paint_value(qp, chip, v, 5)
            qp.setPen(QPen(QColor(0, 0, 0, 60), 1))
            qp.setBrush(Qt.BrushStyle.NoBrush)
            qp.drawRoundedRect(chip, 5, 5)
            text = v if isinstance(v, str) else f"{len(v.get('stops', []))} COLOURS"
        qp.setFont(theme.qfont(11.5, 500, mono=True))
        qp.setPen(QColor(theme.P.text_muted if v is None else theme.P.text_primary))
        x = int(chip.right()) + 8
        qp.drawText(x, 0, self.width() - x - 6, self.height(), Qt.AlignmentFlag.AlignVCenter, text.upper())
        qp.end()
