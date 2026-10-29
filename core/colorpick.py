"""
Luminescent's own colour picker, used everywhere a colour is chosen (themes, settings, folders, Caelestia).

    ColourPicker      the widget: a saturation / brightness square, a hue bar, a hex box with R G B,
                      the old and new colour side by side, and rows of swatches (the theme's colours,
                      Caelestia's scheme, your recent picks and a few presets)
    pick_colour()     the same in a dialog: returns "#RRGGBB" or None when cancelled

Recent picks are shared with the Colour Picker tool (data/colours.json).
"""
import os

from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLineEdit, QSpinBox, QDialog, QFrame, QSizePolicy
)
from PyQt6.QtCore import Qt, QRectF, QPointF, pyqtSignal
from PyQt6.QtGui import QPainter, QColor, QLinearGradient, QPen, QPainterPath

from core import theme, widgets, DATA_DIR, save_json, load_json

RECENT_FILE = os.path.join(DATA_DIR, "colours.json")
PRESETS = ["#F38BA8", "#FAB387", "#F9E2AF", "#A6E3A1", "#94E2D5", "#89DCEB", "#89B4FA", "#7B6CF6",
           "#CBA6F7", "#F5C2E7", "#FFFFFF", "#9399B2", "#45475A", "#1E1E2E", "#000000"]


def norm(c: str) -> str:
    q = QColor(c)
    return q.name().upper() if q.isValid() else ""


def recent() -> list[str]:
    d = load_json(RECENT_FILE, {})
    return [norm(c) for c in (d.get("recent", []) if isinstance(d, dict) else []) if norm(c)][:12]


def remember(c: str):
    c = norm(c)
    if not c:
        return
    d = load_json(RECENT_FILE, {})
    d = d if isinstance(d, dict) else {}
    d["recent"] = [c] + [x for x in recent() if x != c][:11]
    try:
        os.makedirs(DATA_DIR, exist_ok=True)
        save_json(RECENT_FILE, d)
    except OSError:
        pass


class _Plane(QWidget):
    """Saturation left to right, brightness bottom to top, for one hue."""
    moved = pyqtSignal(float, float)

    def __init__(self):
        super().__init__()
        self.h, self.s, self.v = 0.0, 1.0, 1.0
        self.setMinimumSize(240, 170)
        self.setContentsMargins(0, 0, 0, 0)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        self.setCursor(Qt.CursorShape.CrossCursor)

    def paintEvent(self, e):
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        r = QRectF(self.rect()).adjusted(1, 1, -1, -1)
        rad = max(3, theme.P.radius - 4)
        clip = QPainterPath()
        clip.addRoundedRect(r, rad, rad)
        p.setClipPath(clip)
        p.fillRect(r, QColor.fromHsvF(self.h, 1.0, 1.0))
        g = QLinearGradient(r.topLeft(), r.topRight())
        g.setColorAt(0, QColor(255, 255, 255, 255))
        g.setColorAt(1, QColor(255, 255, 255, 0))
        p.fillRect(r, g)
        g = QLinearGradient(r.topLeft(), r.bottomLeft())
        g.setColorAt(0, QColor(0, 0, 0, 0))
        g.setColorAt(1, QColor(0, 0, 0, 255))
        p.fillRect(r, g)
        p.setClipping(False)
        p.setPen(QPen(QColor(theme.P.border), 1))
        p.setBrush(Qt.BrushStyle.NoBrush)
        p.drawPath(clip)
        x = r.left() + self.s * r.width()
        y = r.top() + (1 - self.v) * r.height()
        p.setPen(QPen(QColor(0, 0, 0, 120), 3))
        p.drawEllipse(QPointF(x, y), 7, 7)
        p.setPen(QPen(QColor("#FFFFFF"), 2))
        p.setBrush(QColor.fromHsvF(self.h, self.s, self.v))
        p.drawEllipse(QPointF(x, y), 7, 7)
        p.end()

    def _at(self, pos):
        r = QRectF(self.rect()).adjusted(1, 1, -1, -1)
        s = min(1.0, max(0.0, (pos.x() - r.left()) / max(1.0, r.width())))
        v = min(1.0, max(0.0, 1 - (pos.y() - r.top()) / max(1.0, r.height())))
        self.moved.emit(s, v)

    def mousePressEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton:
            self._at(e.position())

    def mouseMoveEvent(self, e):
        if e.buttons() & Qt.MouseButton.LeftButton:
            self._at(e.position())


class _HueBar(QWidget):
    moved = pyqtSignal(float)

    def __init__(self):
        super().__init__()
        self.h = 0.0
        self.setFixedHeight(18)
        self.setCursor(Qt.CursorShape.PointingHandCursor)

    def paintEvent(self, e):
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        r = QRectF(self.rect()).adjusted(8, 3, -8, -3)
        g = QLinearGradient(r.topLeft(), r.topRight())
        for i in range(7):
            g.setColorAt(i / 6, QColor.fromHsvF((i / 6) % 1.0, 1.0, 1.0))
        path = QPainterPath()
        path.addRoundedRect(r, r.height() / 2, r.height() / 2)
        p.fillPath(path, g)
        x = r.left() + self.h * r.width()
        p.setPen(QPen(QColor(0, 0, 0, 120), 3))
        p.setBrush(Qt.BrushStyle.NoBrush)
        p.drawEllipse(QPointF(x, self.height() / 2), 7, 7)
        p.setPen(QPen(QColor("#FFFFFF"), 2))
        p.setBrush(QColor.fromHsvF(self.h, 1.0, 1.0))
        p.drawEllipse(QPointF(x, self.height() / 2), 7, 7)
        p.end()

    def _at(self, pos):
        r = QRectF(self.rect()).adjusted(8, 0, -8, 0)
        self.moved.emit(min(0.9999, max(0.0, (pos.x() - r.left()) / max(1.0, r.width()))))

    def mousePressEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton:
            self._at(e.position())

    def mouseMoveEvent(self, e):
        if e.buttons() & Qt.MouseButton.LeftButton:
            self._at(e.position())


class Swatch(QFrame):
    """One clickable colour chip."""

    def __init__(self, colour: str, tip: str, on_click, size: int = 22):
        super().__init__()
        self.colour = colour
        self.setFixedSize(size, size)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.setToolTip(f"{tip}  {colour}" if tip else colour)
        self.on_click = on_click
        self.setStyleSheet(f"background: {colour}; border: 1px solid {theme.mix(theme.P.border, theme.P.text_muted, 0.3)};"
                           f" border-radius: {max(3, size // 4)}px;")

    def mousePressEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton:
            self.on_click(self.colour)


class _Compare(QWidget):
    """The colour you started with (left, click to go back to it) and the new one (right)."""
    back = pyqtSignal()

    def __init__(self):
        super().__init__()
        self.old = self.new = "#000000"
        self.setFixedSize(84, 34)
        self.setToolTip("Left: the colour before. Click it to go back.")

    def paintEvent(self, e):
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        r = QRectF(self.rect()).adjusted(0.5, 0.5, -0.5, -0.5)
        rad = max(3, theme.P.radius - 4)
        path = QPainterPath()
        path.addRoundedRect(r, rad, rad)
        p.setClipPath(path)
        p.fillRect(QRectF(r.left(), r.top(), r.width() / 2, r.height()), QColor(self.old))
        p.fillRect(QRectF(r.center().x(), r.top(), r.width() / 2, r.height()), QColor(self.new))
        p.setClipping(False)
        p.setPen(QPen(QColor(theme.P.border), 1))
        p.drawPath(path)
        p.end()

    def mousePressEvent(self, e):
        if e.position().x() < self.width() / 2:
            self.back.emit()


class ColourPicker(QWidget):
    """Pick a colour without leaving the window. `changed` fires on every change with "#RRGGBB"."""
    changed = pyqtSignal(str)

    def __init__(self, colour: str = "#7B6CF6", extra_rows: list[tuple[str, list[tuple[str, str]]]] | None = None,
                 compact: bool = False):
        super().__init__()
        self._busy = False
        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(10)
        self.plane = _Plane()
        if compact:
            self.plane.setMinimumSize(200, 130)
        self.plane.moved.connect(self._sv)
        lay.addWidget(self.plane, 1)
        self.hue = _HueBar()
        self.hue.moved.connect(self._h)
        lay.addWidget(self.hue)

        row = QHBoxLayout()
        row.setSpacing(8)
        self.compare = _Compare()
        self.compare.back.connect(lambda: self.set_colour(self.compare.old, emit=True))
        row.addWidget(self.compare)
        self.hex = QLineEdit()
        self.hex.setProperty("mono", True)
        self.hex.setFixedHeight(32)
        self.hex.setMaxLength(9)
        self.hex.setFixedWidth(96)
        self.hex.setToolTip("A colour code: #RRGGBB, #RGB or a name like teal")
        self.hex.editingFinished.connect(self._typed)
        self.hex.textEdited.connect(self._typing)
        row.addWidget(self.hex)
        self.rgb = []
        for ch in "RGB":
            sp = QSpinBox()
            sp.setRange(0, 255)
            sp.setFixedSize(68, 32)
            sp.setPrefix(f"{ch} ")
            sp.valueChanged.connect(self._rgb)
            self.rgb.append(sp)
            row.addWidget(sp)
        row.addStretch()
        lay.addLayout(row)

        self.rows = QVBoxLayout()
        self.rows.setSpacing(6)
        lay.addLayout(self.rows)
        self._extra = list(extra_rows or [])
        self.fill_swatches()
        self.set_colour(colour)
        self.compare.old = self.colour()

    # ── swatch rows ────────────────────────────────────────────────────────
    def fill_swatches(self):
        widgets.clear_layout(self.rows)
        p = theme.P
        groups = list(self._extra)
        groups.append(("THEME", [(p.primary, "Accent"), (p.primary_text, "Accent text"), (p.bg_base, "Background"),
                                 (p.bg_card, "Cards"), (p.border, "Borders"), (p.text_primary, "Text"),
                                 (p.text_muted, "Muted text")]))
        try:
            from modules.caelestia import scheme
            cs = scheme()
        except Exception:
            cs = {}
        if cs:
            names = ["primary", "secondary", "tertiary", "surface", "surfaceContainer", "onSurface", "outline",
                     "error", "success"]
            groups.append(("CAELESTIA", [(cs[n], n) for n in names if n in cs]))
        rec = recent()
        if rec:
            groups.append(("RECENT", [(c, "") for c in rec]))
        groups.append(("PRESETS", [(c, "") for c in PRESETS]))
        for title, cols in groups:
            if not cols:
                continue
            line = QHBoxLayout()
            line.setSpacing(5)
            head = widgets.label(title, 9.5, 600, mono=True, spacing=0.12, tone="muted")
            head.setFixedWidth(84)
            line.addWidget(head)
            for c, tip in cols[:15]:
                if norm(c):
                    line.addWidget(Swatch(norm(c), tip, lambda x: self.set_colour(x, emit=True)))
            line.addStretch()
            self.rows.addLayout(line)

    # ── value ──────────────────────────────────────────────────────────────
    def colour(self) -> str:
        return QColor.fromHsvF(self.plane.h, self.plane.s, self.plane.v).name().upper()

    def set_colour(self, c: str, emit: bool = False):
        q = QColor(c)
        if not q.isValid():
            return
        h, s, v, _a = q.getHsvF()
        if h < 0:                       # greys have no hue: keep the one the bar is on
            h = self.hue.h
        self.plane.h = self.hue.h = h
        self.plane.s, self.plane.v = s, v
        self._show(emit)

    def set_original(self, c: str):
        self.compare.old = norm(c) or self.compare.old
        self.compare.update()

    def _show(self, emit: bool):
        c = self.colour()
        self._busy = True
        if not self.hex.hasFocus():
            self.hex.setText(c)
        q = QColor(c)
        for sp, val in zip(self.rgb, (q.red(), q.green(), q.blue())):
            sp.setValue(val)
        self._busy = False
        self.compare.new = c
        self.compare.update()
        self.plane.update()
        self.hue.update()
        if emit:
            self.changed.emit(c)

    def _sv(self, s, v):
        self.plane.s, self.plane.v = s, v
        self._show(True)

    def _h(self, h):
        self.plane.h = self.hue.h = h
        self._show(True)

    def _typing(self, text: str):
        t = text.strip()
        if not t.startswith("#") and all(ch in "0123456789abcdefABCDEF" for ch in t) and len(t) in (3, 6):
            t = "#" + t
        ok = QColor.isValidColorName(t)
        self.hex.setProperty("invalid", not ok and bool(t))
        self.hex.style().unpolish(self.hex)
        self.hex.style().polish(self.hex)
        if ok and len(t.lstrip("#")) in (3, 6):
            self.set_colour(t, emit=True)

    def _typed(self):
        t = self.hex.text().strip()
        if t and not t.startswith("#") and len(t) in (3, 6):
            t = "#" + t
        if QColor.isValidColorName(t):
            self.set_colour(t, emit=True)
        self.hex.setText(self.colour())

    def _rgb(self, _v):
        if self._busy:
            return
        q = QColor(*(sp.value() for sp in self.rgb))
        self.set_colour(q.name(), emit=True)


class ColourDialog(QDialog):
    def __init__(self, parent, colour: str, title: str, live=None):
        super().__init__(parent)
        self.setWindowTitle(title)
        widgets.card_dialog(self)
        self._live = live
        lay = QVBoxLayout(self)
        lay.setContentsMargins(18, 14, 18, 16)
        lay.setSpacing(12)
        self.picker = ColourPicker(colour)
        if live:
            self.picker.changed.connect(live)
        lay.addWidget(self.picker, 1)
        btns = QHBoxLayout()
        btns.addStretch()
        cancel = widgets.button("Cancel", "ghost")
        cancel.setAutoDefault(False)
        cancel.clicked.connect(self.reject)
        ok = widgets.button("Use this colour", "primary")
        ok.setAutoDefault(False)
        ok.setDefault(True)
        ok.clicked.connect(self.accept)
        btns.addWidget(cancel)
        btns.addWidget(ok)
        lay.addLayout(btns)
        self.resize(520, 470)


def pick_colour(parent, colour: str = "", title: str = "Pick a colour", live=None) -> str | None:
    """A colour from the picker dialog, or None when cancelled. `live(colour)` is called while choosing (and
    once more with the starting colour when cancelled, so a live preview can go back)."""
    start = norm(colour) or theme.P.primary
    dlg = ColourDialog(parent, start, title, live)
    if dlg.exec() == QDialog.DialogCode.Accepted:
        c = dlg.picker.colour()
        remember(c)
        return c
    if live:
        live(start)
    return None


class ColourField(QWidget):
    """A swatch + hex code that opens the picker; `changed(str)` on a new colour."""
    changed = pyqtSignal(str)

    def __init__(self, colour: str = "#000000", title: str = "Pick a colour", width: int = 130):
        super().__init__()
        self.title = title
        self._value = norm(colour) or "#000000"
        self.setFixedHeight(32)
        self.setFixedWidth(width)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        lay = QHBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        self.box = QFrame()
        self.box.setObjectName("keyBox")
        bl = QHBoxLayout(self.box)
        bl.setContentsMargins(6, 0, 10, 0)
        bl.setSpacing(8)
        self.chip = QFrame()
        self.chip.setFixedSize(20, 20)
        self.hex = widgets.label("", 12, mono=True)
        bl.addWidget(self.chip)
        bl.addWidget(self.hex, 1)
        lay.addWidget(self.box)
        for w in (self.box, self.chip, self.hex):
            w.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        self.set_value(self._value)

    def value(self) -> str:
        return self._value

    def set_value(self, c: str):
        self._value = norm(c) or self._value
        self.chip.setStyleSheet(f"background: {self._value}; border: 1px solid {theme.P.border}; border-radius: 4px;")
        self.hex.setText(self._value)

    def mousePressEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton:
            c = pick_colour(self, self._value, self.title)
            if c and c != self._value:
                self.set_value(c)
                self.changed.emit(c)
