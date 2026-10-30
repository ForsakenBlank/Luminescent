"""
The Automation page: a huge workspace you move around in, with nodes you wire together.

Drag from a port (the dots on a node's edges) to another to wire them: round ports carry pulses
(something happened: run the next thing), diamonds carry values (text a field reads instead of its
box). Right-click or double-click the open space to add a node, middle- or right-drag (or Space +
drag) to move around, the wheel zooms. Pulses run along the wires while the page is open.

The page only edits core.automation's data; the engine runs it, overlay open or not. Each workspace
gets its own scene, built the first time it is shown and kept, so switching is instant.
"""
import os
import json
import time
import logging

from PyQt6.QtWidgets import (
    QWidget, QFrame, QLabel, QVBoxLayout, QHBoxLayout, QPushButton, QLineEdit, QPlainTextEdit, QCheckBox,
    QComboBox, QGraphicsView, QGraphicsScene, QGraphicsObject, QGraphicsPathItem, QGraphicsProxyWidget,
    QGraphicsItem, QListWidget, QListWidgetItem, QFileDialog, QDialog, QTableWidget, QTableWidgetItem,
    QHeaderView, QDateEdit, QApplication, QProxyStyle, QStyle
)
from PyQt6.QtCore import Qt, QSize, QRectF, QPointF, QPoint, QTimer, QDate, QEvent, QEasingCurve
from PyQt6.QtGui import QPainter, QColor, QPen, QPainterPath, QPainterPathStroker, QPolygonF, QFontMetrics, QCursor

from core import theme, widgets, anim
from core.automation import TYPES, CATEGORIES, F, choices_of, default_fields, _id

log = logging.getLogger("automation.ui")

HEAD = 32
ROW = 22
PAD = 12
FOOT = 24
VALUE_COLOUR = "#4CA7F5"
CAT_COLOUR = {c: col for c, _n, col in CATEGORIES}
CAT_NAME = {c: n for c, n, _col in CATEGORIES}
_clipboard: dict | None = None


def _snap(v: float, g: int) -> float:
    return round(v / g) * g if g > 1 else v


def _port_label(port: str) -> str:
    return port.split(":", 1)[1].replace("_", " ")


def _kind(port: str) -> str:
    return "value" if port.startswith(("val:", "field:")) else "flow"


def _is_source(port: str) -> bool:
    return port.startswith(("out:", "val:"))


def compatible(a: str, b: str) -> bool:
    """Can port a be wired to port b (in either order)?"""
    return {a.split(":")[0], b.split(":")[0]} in ({"out", "in"}, {"val", "field"})


# ── items ─────────────────────────────────────────────────────────────────────
class NodeItem(QGraphicsObject):
    def __init__(self, page: "AutomationPage", ws: dict, data: dict):
        super().__init__()
        self.page = page
        self.ws = ws
        self.data = data
        self.spec = TYPES[data["type"]]
        self.colour = QColor(CAT_COLOUR.get(self.spec.cat, "#888888"))
        self.setFlags(QGraphicsItem.GraphicsItemFlag.ItemIsMovable | QGraphicsItem.GraphicsItemFlag.ItemIsSelectable
                      | QGraphicsItem.GraphicsItemFlag.ItemSendsGeometryChanges)
        self.setZValue(1)
        self.setCacheMode(QGraphicsItem.CacheMode.DeviceCoordinateCache)
        self.wires: list["WireItem"] = []
        self.ports: dict[str, QPointF] = {}
        self.editors: dict[str, QWidget] = {}
        self._flash_until = 0.0
        self.w = self.spec.width
        self._build()
        self.setPos(float(data.get("x", 0)), float(data.get("y", 0)))
        self.setOpacity(0.45 if data.get("off") else 1.0)

    # ── building ───────────────────────────────────────────────────────────
    def _build(self):
        left = ["in:" + p for p in self.spec.ins]
        right = ["out:" + p for p in self.spec.outs] + ["val:" + p for p in self.spec.values]
        rows = max(len(left), len(right))
        y0 = HEAD + 6
        for i, p in enumerate(left):
            self.ports[p] = QPointF(0, y0 + i * ROW + ROW / 2)
        for i, p in enumerate(right):
            self.ports[p] = QPointF(self.w, y0 + i * ROW + ROW / 2)
        fy = y0 + rows * ROW + (6 if rows else 0)
        self.proxy = None
        if self.spec.fields:
            form = QWidget()
            form.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
            widgets.style(form, "background: transparent;")
            lay = QVBoxLayout(form)
            lay.setContentsMargins(0, 0, 0, 0)
            lay.setSpacing(4)
            for f in self.spec.fields:
                ed = self._editor(f)
                if f.label and f.kind != "bool":
                    lbl = widgets.label(f.label, 10.5, tone="muted")
                    lbl.setWordWrap(True)
                    lay.addWidget(lbl)
                lay.addWidget(ed)
                self.editors[f.key] = ed
            form.setFixedWidth(self.w - 2 * PAD)
            form.resize(self.w - 2 * PAD, form.sizeHint().height())
            lay.activate()
            self.proxy = QGraphicsProxyWidget(self)
            self.proxy.setWidget(form)
            self.proxy.setPos(PAD, fy)
            for f in self.spec.fields:
                if f.pin:
                    ed = self.editors[f.key]
                    y = fy + ed.mapTo(form, QPoint(0, ed.height() // 2)).y()
                    self.ports["field:" + f.key] = QPointF(0, y)
            fy += form.height() + 6
        self.h = fy + FOOT
        self.run_rect = QRectF(self.w - 30, 6, 22, 20) if (self.spec.cat == "trigger" or self.spec.ins) else QRectF()

    def _editor(self, f: F) -> QWidget:
        val = self.data["fields"].get(f.key, f.default)

        def store(v):
            if self.data["fields"].get(f.key) != v:
                self.data["fields"][f.key] = v
                self.page.field_edited(self, f.key)

        if f.kind == "bool":
            ed = QCheckBox(f.label)
            ed.setChecked(bool(val))
            ed.toggled.connect(store)
            return ed
        if f.kind == "multiline":
            ed = QPlainTextEdit(str(val))
            ed.setPlaceholderText(f.hint)
            ed.setFixedHeight({"command": 96, "comment": 140}.get(self.spec.id, 64))
            ed.setTabChangesFocus(True)
            ed.textChanged.connect(lambda: store(ed.toPlainText()))
            return ed
        if f.kind == "choice":
            ed = QComboBox()
            opts = choices_of(f, self.page.eng)
            for v, label in opts:
                ed.addItem(str(label), v)
            if val not in [v for v, _l in opts] and val not in ("", None):
                ed.addItem(f"(missing) {val}", val)
            i = ed.findData(val)
            ed.setCurrentIndex(max(0, i))
            if i < 0 and opts and f.key not in self.data["fields"]:
                self.data["fields"][f.key] = opts[0][0]
            ed.currentIndexChanged.connect(lambda _i: store(ed.currentData()))
            ed.setSizeAdjustPolicy(QComboBox.SizeAdjustPolicy.AdjustToMinimumContentsLengthWithIcon)
            ed.setMinimumContentsLength(8)
            return ed
        if f.kind == "hotkey":
            from settings_ui import HotkeyEdit
            ed = HotkeyEdit()
            ed.set_value(str(val or ""))
            ed.recording_changed.connect(self.page.launcher.hotkeys.set_suspended)
            ed.edit.textChanged.connect(lambda _t: store(ed.value()))
            return ed
        if f.kind == "date":
            ed = QDateEdit()
            ed.setCalendarPopup(True)
            ed.setDisplayFormat("yyyy-MM-dd")
            d = QDate.fromString(str(val), "yyyy-MM-dd")
            ed.setDate(d if d.isValid() else QDate.currentDate().addDays(1))
            if not d.isValid():
                self.data["fields"][f.key] = ed.date().toString("yyyy-MM-dd")
            ed.dateChanged.connect(lambda d: store(d.toString("yyyy-MM-dd")))
            return ed
        line = QLineEdit("" if val is None else (str(int(val)) if isinstance(val, float) and val.is_integer()
                                                  else str(val)))
        line.setPlaceholderText(f.hint or ("16:00" if f.kind == "time" else ""))
        if f.kind == "number":
            def num(t):
                try:
                    v = float(t.replace(",", "."))
                    store(int(v) if v.is_integer() else v)
                except ValueError:
                    store(t)
            line.textChanged.connect(num)
        else:
            line.textChanged.connect(store)
        if f.kind not in ("file", "folder"):
            return line
        row = QWidget()
        widgets.style(row, "background: transparent;")
        rl = QHBoxLayout(row)
        rl.setContentsMargins(0, 0, 0, 0)
        rl.setSpacing(4)
        rl.addWidget(line, 1)
        pick = widgets.icon_button("…", "Choose", 26, 13)

        def choose():
            start = os.path.expanduser(line.text() or "~")
            if f.kind == "folder":
                p = QFileDialog.getExistingDirectory(None, f.label or "Folder", start)
            else:
                p, _ = QFileDialog.getOpenFileName(None, f.label or "File", start)
            if p:
                line.setText(p)
        pick.clicked.connect(choose)
        rl.addWidget(pick)
        row.line = line
        return row

    def set_wired(self, key: str, on: bool):
        ed = self.editors.get(key)
        if ed is None:
            return
        ed.setEnabled(not on)
        ed.setToolTip("Takes its value from the wire plugged into it" if on else "")

    # ── geometry ───────────────────────────────────────────────────────────
    def boundingRect(self) -> QRectF:
        return QRectF(-10, -6, self.w + 20, self.h + 12)

    def port_scene(self, port: str) -> QPointF:
        return self.mapToScene(self.ports.get(port, QPointF(0, 0)))

    def itemChange(self, change, value):
        if change == QGraphicsItem.GraphicsItemChange.ItemPositionChange:
            g = self.page.snap()
            return QPointF(_snap(value.x(), g), _snap(value.y(), g))
        if change == QGraphicsItem.GraphicsItemChange.ItemPositionHasChanged:
            for w in self.wires:
                w.refresh()
        if change == QGraphicsItem.GraphicsItemChange.ItemSelectedHasChanged:
            self.update()
        return super().itemChange(change, value)

    def flash(self):
        self._flash_until = time.monotonic() + 0.45
        self.update()

    # ── painting ───────────────────────────────────────────────────────────
    def paint(self, p: QPainter, opt, widget=None):
        P = theme.P
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        r = QRectF(0, 0, self.w, self.h)
        hot = time.monotonic() < self._flash_until
        rt = self.page.eng.node_rt(self.ws["id"], self.data["id"])
        if hot:
            c = QColor(self.colour)
            c.setAlphaF(0.55)
            p.setPen(QPen(c, 6))
            p.setBrush(Qt.BrushStyle.NoBrush)
            p.drawRoundedRect(r.adjusted(-3, -3, 3, 3), 13, 13)
        sel = self.isSelected()
        # selected looks like a selected dashboard box: accent edge and a faint accent tint
        p.setBrush(QColor(theme.mix(P.bg_card, P.primary, 0.07) if sel else P.bg_card))
        p.setPen(QPen(QColor(P.primary if sel else P.border), 1))
        p.drawRoundedRect(r, 10, 10)
        # header
        head = QPainterPath()
        head.addRoundedRect(QRectF(0.5, 0.5, self.w - 1, HEAD), 10, 10)
        clip = QPainterPath()
        clip.addRect(QRectF(0, 0, self.w, HEAD))
        p.save()
        p.setClipPath(clip)
        p.fillPath(head, QColor(P.bg_header))
        p.fillRect(QRectF(0.5, HEAD - 0.5, self.w - 1, 1), QColor(P.border))
        p.restore()
        p.fillRect(QRectF(10, 11, 3, HEAD - 22), self.colour)
        p.setPen(QColor(P.text_primary))
        p.setFont(theme.qfont(12, 600, scaled=False))
        fm = QFontMetrics(p.font())
        title = self.data.get("title") or self.spec.title
        room = self.w - 24 - (34 if not self.run_rect.isNull() else 8)
        p.drawText(QRectF(20, 0, room, HEAD), Qt.AlignmentFlag.AlignVCenter,
                   fm.elidedText(title, Qt.TextElideMode.ElideRight, int(room)))
        if not self.run_rect.isNull():
            p.setPen(QPen(QColor(P.border), 1))
            p.setBrush(QColor(P.bg_card))
            p.drawRoundedRect(self.run_rect, 5, 5)
            p.setPen(QColor(P.primary_text))
            p.setFont(theme.qfont(10, 600, scaled=False))
            p.drawText(self.run_rect, Qt.AlignmentFlag.AlignCenter, "▶")
        # port labels
        p.setFont(theme.qfont(10.5, 500, mono=True, scaled=False))
        p.setPen(QColor(P.text_secondary))
        for port, pt in self.ports.items():
            if port.startswith("field:"):
                continue
            text = _port_label(port)
            if port.startswith("in:"):
                p.drawText(QRectF(12, pt.y() - ROW / 2, self.w / 2, ROW), Qt.AlignmentFlag.AlignVCenter, text)
            else:
                p.drawText(QRectF(self.w / 2 - 12, pt.y() - ROW / 2, self.w / 2, ROW),
                           Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignRight, text)
        # ports
        for port, pt in self.ports.items():
            self._paint_port(p, port, pt)
        # status and progress
        status, prog = rt.info() if rt is not None else ("", None)
        if self.data.get("off"):
            status = "turned off"
        elif rt is None and not self.page.eng.running(self.ws) and self.spec.cat == "trigger":
            status = "workspace off"
        if status:
            p.setFont(theme.qfont(10.5, 500, mono=True, scaled=False))
            p.setPen(QColor(theme.DANGER if status.startswith("⚠") else P.text_muted))
            fm = QFontMetrics(p.font())
            p.drawText(QRectF(12, self.h - FOOT, self.w - 24, FOOT - 4), Qt.AlignmentFlag.AlignVCenter,
                       fm.elidedText(status, Qt.TextElideMode.ElideRight, int(self.w - 24)))
        if prog is not None:
            p.fillRect(QRectF(10, self.h - 4, self.w - 20, 2), QColor(P.border))
            p.fillRect(QRectF(10, self.h - 4, (self.w - 20) * max(0.0, min(1.0, prog)), 2), self.colour)

    def _paint_port(self, p: QPainter, port: str, pt: QPointF):
        P = theme.P
        wired = any(port in (w.data["src"][1], w.data["dst"][1]) and self in (w.src, w.dst) for w in self.wires)
        if _kind(port) == "value":
            c = QColor(VALUE_COLOUR)
            poly = QPolygonF([pt + QPointF(0, -6), pt + QPointF(6, 0), pt + QPointF(0, 6), pt + QPointF(-6, 0)])
            p.setPen(QPen(c, 1.6))
            p.setBrush(c if wired else QColor(P.bg_card))
            p.drawPolygon(poly)
        else:
            c = QColor(self.colour if port.startswith("out:") else P.primary)
            p.setPen(QPen(c, 1.6))
            p.setBrush(c if wired else QColor(P.bg_card))
            p.drawEllipse(pt, 5.5, 5.5)

    # ── mouse ──────────────────────────────────────────────────────────────
    def mousePressEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton and not self.run_rect.isNull() and self.run_rect.contains(e.pos()):
            self.page.run_node(self)
            e.accept()
            return
        self.page.press_snapshot()
        self.setZValue(self.page.next_z())
        super().mousePressEvent(e)

    def mouseReleaseEvent(self, e):
        super().mouseReleaseEvent(e)
        self.page.nodes_moved()

    def mouseDoubleClickEvent(self, e):
        if e.pos().y() <= HEAD:
            self.page.rename_node(self)
            return
        super().mouseDoubleClickEvent(e)


class WireItem(QGraphicsPathItem):
    def __init__(self, page: "AutomationPage", data: dict, src: NodeItem, dst: NodeItem):
        super().__init__()
        self.page = page
        self.data = data
        self.src = src
        self.dst = dst
        self.value = data["src"][1].startswith("val:")
        self.setFlag(QGraphicsItem.GraphicsItemFlag.ItemIsSelectable)
        self.setZValue(0)
        self._hot_until = 0.0
        src.wires.append(self)
        if dst is not src:
            dst.wires.append(self)
        self.refresh()

    def refresh(self):
        self.setPath(wire_path(self.src.port_scene(self.data["src"][1]), self.dst.port_scene(self.data["dst"][1])))

    def shape(self) -> QPainterPath:
        s = QPainterPathStroker()
        s.setWidth(12)
        return s.createStroke(self.path())

    def heat(self):
        self._hot_until = time.monotonic() + 0.5
        self.update()

    def paint(self, p: QPainter, opt, widget=None):
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        c = QColor(VALUE_COLOUR if self.value else self.src.colour)
        hot = time.monotonic() < self._hot_until
        if self.isSelected():
            c = QColor(theme.P.text_primary)
        if hot:
            g = QColor(c)
            g.setAlphaF(0.35)
            p.setPen(QPen(g, 7, Qt.PenStyle.SolidLine, Qt.PenCapStyle.RoundCap))
            p.drawPath(self.path())
        c.setAlphaF(1.0 if hot or self.isSelected() else 0.75)
        pen = QPen(c, 2.2 if not self.value else 1.8, Qt.PenStyle.DashLine if self.value else Qt.PenStyle.SolidLine,
                   Qt.PenCapStyle.RoundCap)
        p.setPen(pen)
        p.drawPath(self.path())


def wire_path(a: QPointF, b: QPointF) -> QPainterPath:
    dx = max(60.0, abs(b.x() - a.x()) * 0.5)
    path = QPainterPath(a)
    path.cubicTo(a + QPointF(dx, 0), b - QPointF(dx, 0), b)
    return path


class PulseLayer(QGraphicsObject):
    """Every pulse on its way along a wire, as a glowing dot with a peek at what it carries."""
    DURATION = 0.55

    def __init__(self):
        super().__init__()
        self.pulses: list[tuple[WireItem, float, str]] = []
        self.setZValue(5)
        self._rect = QRectF()

    def add(self, wire: WireItem, text: str):
        if len(self.pulses) < 300:
            self.pulses.append((wire, time.monotonic(), text))
            self._grow()

    def _grow(self):
        r = QRectF()
        for w, _t, _x in self.pulses:
            r = r.united(w.path().boundingRect())
        self.prepareGeometryChange()
        self._rect = r.adjusted(-30, -40, 160, 30)

    def step(self) -> bool:
        now = time.monotonic()
        before = len(self.pulses)
        self.pulses = [x for x in self.pulses if now - x[1] < self.DURATION and x[0].scene() is not None]
        if len(self.pulses) != before:
            self._grow()
        self.update()
        return bool(self.pulses)

    def boundingRect(self) -> QRectF:
        return self._rect

    def paint(self, p: QPainter, opt, widget=None):
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        now = time.monotonic()
        p.setFont(theme.qfont(10, 500, mono=True, scaled=False))
        for w, t0, text in self.pulses:
            t = min(1.0, (now - t0) / self.DURATION)
            t = 1 - (1 - t) ** 2
            pt = w.path().pointAtPercent(t)
            c = QColor(VALUE_COLOUR if w.value else w.src.colour)
            halo = QColor(c)
            halo.setAlphaF(0.3)
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(halo)
            p.drawEllipse(pt, 10, 10)
            p.setBrush(c)
            p.drawEllipse(pt, 5, 5)
            p.setBrush(QColor("#FFFFFF"))
            p.drawEllipse(pt, 2, 2)
            if text:
                short = text.replace("\n", " ⏎ ")
                short = short[:28] + ("…" if len(short) > 28 else "")
                fm = QFontMetrics(p.font())
                box = QRectF(pt.x() + 10, pt.y() - 26, fm.horizontalAdvance(short) + 12, 18)
                p.setBrush(QColor(theme.P.bg_base))
                p.setPen(QPen(c, 1))
                p.drawRoundedRect(box, 6, 6)
                p.setPen(QColor(theme.P.text_primary))
                p.drawText(box, Qt.AlignmentFlag.AlignCenter, short)


class GraphScene(QGraphicsScene):
    def __init__(self, page: "AutomationPage", ws: dict):
        super().__init__()
        self.page = page
        self.ws = ws
        self.nodes: dict[str, NodeItem] = {}
        self.wires: dict[str, WireItem] = {}
        self.pulses = PulseLayer()
        self.addItem(self.pulses)
        self.setSceneRect(QRectF(-60000, -60000, 120000, 120000))
        self.setItemIndexMethod(QGraphicsScene.ItemIndexMethod.NoIndex)
        for nd in ws["nodes"]:
            self.add_node(nd)
        for w in ws["wires"]:
            self.add_wire(w)

    def add_node(self, nd: dict) -> NodeItem:
        item = NodeItem(self.page, self.ws, nd)
        self.addItem(item)
        self.nodes[nd["id"]] = item
        return item

    def add_wire(self, w: dict) -> WireItem | None:
        a, b = self.nodes.get(w["src"][0]), self.nodes.get(w["dst"][0])
        if a is None or b is None:
            return None
        item = WireItem(self.page, w, a, b)
        self.addItem(item)
        self.wires[w["id"]] = item
        if w["dst"][1].startswith("field:"):
            b.set_wired(w["dst"][1][6:], True)
        a.update()
        b.update()
        return item

    def remove_wire(self, item: WireItem):
        self.wires.pop(item.data["id"], None)
        for n in (item.src, item.dst):
            if item in n.wires:
                n.wires.remove(item)
            n.update()
        if item.data["dst"][1].startswith("field:"):
            item.dst.set_wired(item.data["dst"][1][6:], False)
        self.removeItem(item)

    def remove_node(self, item: NodeItem):
        for w in list(item.wires):
            self.remove_wire(w)
        self.nodes.pop(item.data["id"], None)
        self.removeItem(item)
        item.deleteLater()

    def port_at(self, sp: QPointF, radius: float = 10) -> tuple[NodeItem, str] | None:
        best, dist = None, radius
        for item in self.items(QRectF(sp.x() - 40, sp.y() - 40, 80, 80)):
            if isinstance(item, NodeItem):
                for port, pt in item.ports.items():
                    d = (item.mapToScene(pt) - sp).manhattanLength()
                    if d < dist:
                        best, dist = (item, port), d
        return best


# ── the view ──────────────────────────────────────────────────────────────────
class _NoBandStyle(QProxyStyle):
    """Qt's own selection rectangle is a grey system one: skip it, the view draws the dashboard's instead."""

    def drawControl(self, el, opt, p, w=None):
        if el == QStyle.ControlElement.CE_RubberBand:
            return
        super().drawControl(el, opt, p, w)


class GraphView(QGraphicsView):
    def __init__(self, page: "AutomationPage"):
        super().__init__(page)
        self.page = page
        self.setRenderHints(QPainter.RenderHint.Antialiasing | QPainter.RenderHint.TextAntialiasing)
        self.setDragMode(QGraphicsView.DragMode.RubberBandDrag)
        # the same selection box as the dashboard (#lumBand in the theme's stylesheet)
        self._band_style = _NoBandStyle()
        self.setStyle(self._band_style)
        self._band = QFrame(self.viewport())
        self._band.setObjectName("lumBand")
        self._band.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        self._band.hide()
        self.rubberBandChanged.connect(self._band_moved)
        # zooming keeps the point under the mouse in place by itself (_apply_zoom): Qt's own anchor goes by where
        # it last saw the cursor, which can be stale and made the view jump
        self.setTransformationAnchor(QGraphicsView.ViewportAnchor.NoAnchor)
        self.setResizeAnchor(QGraphicsView.ViewportAnchor.AnchorViewCenter)
        # the view is see-through (the overlay shows behind it): scrolling by copying pixels smeared that
        # background around while panning, so every move repaints the whole view instead
        self.setViewportUpdateMode(QGraphicsView.ViewportUpdateMode.FullViewportUpdate)
        self.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setFrameShape(QFrame.Shape.NoFrame)
        widgets.style(self, "background: transparent; border: none;")
        self.viewport().setAutoFillBackground(False)
        self.setMouseTracking(True)
        self._pan = None
        self._space = False
        self._zoom_to = None                 # where a smooth zoom is heading

    def zoom(self) -> float:
        return self.transform().m11()

    def _band_moved(self, rect, *_):
        if rect.isNull():
            self._band.hide()
            return
        self._band.setGeometry(rect)
        self._band.show()
        self._band.raise_()

    def set_zoom(self, z: float, at: QPoint | None = None):
        anim.stop(self, "zoom")
        self._zoom_to = None
        self._apply_zoom(z, at)
        self.page.zoom_changed()

    def _apply_zoom(self, z: float, at: QPoint | None = None):
        z = max(0.2, min(2.5, z))
        f = z / max(0.01, self.zoom())
        if abs(f - 1.0) <= 1e-4:
            return
        at = at if at is not None else self.viewport().rect().center()
        before = self.mapToScene(at)
        self.scale(f, f)
        d = (self.mapToScene(at) - before) * self.zoom()
        h, v = self.horizontalScrollBar(), self.verticalScrollBar()
        h.setValue(round(h.value() - d.x()))
        v.setValue(round(v.value() - d.y()))

    def wheelEvent(self, e):
        steps = e.angleDelta().y() / 120
        if not steps:
            return
        at = e.position().toPoint()
        if not anim.on("zoom"):
            self.set_zoom(self.zoom() * (1.15 ** steps), at)
            return
        # smooth: every notch moves the target, the view glides after it (around the mouse)
        target = max(0.2, min(2.5, (self._zoom_to or self.zoom()) * (1.15 ** steps)))
        self._zoom_to = target

        def done():
            self._zoom_to = None
            self.page.zoom_changed()
        anim.value(self.zoom(), target, lambda z: self._apply_zoom(z, at), "zoom", 140, done, self,
                   QEasingCurve.Type.OutCubic, "zoom")

    def _proxy_at(self, pos) -> bool:
        item = self.itemAt(pos)
        while item is not None:
            if isinstance(item, QGraphicsProxyWidget):
                return True
            item = item.parentItem()
        return False

    def mousePressEvent(self, e):
        pos = e.position().toPoint()
        sp = self.mapToScene(pos)
        b = e.button()
        if (b in (Qt.MouseButton.MiddleButton, Qt.MouseButton.RightButton) and not self._proxy_at(pos)) or \
                (b == Qt.MouseButton.LeftButton and self._space):
            # [last position, button, moved, where it started]: moved step by step from the last position, so a
            # zoom in the middle of a pan does not throw the view back to where the pan started
            self._pan = [e.position(), b, False, e.position()]
            self.viewport().setCursor(Qt.CursorShape.ClosedHandCursor)
            return
        if b == Qt.MouseButton.LeftButton and self.scene() is not None:
            hit = self.scene().port_at(sp)
            if hit is not None:
                self.page.begin_wire(hit[0], hit[1], sp)
                return
            item = self.itemAt(pos)
            if isinstance(item, WireItem) and e.modifiers() & Qt.KeyboardModifier.AltModifier:
                self.page.delete_wires([item])
                return
        super().mousePressEvent(e)

    def mouseMoveEvent(self, e):
        if self._pan is not None:
            last, _b, moved, start = self._pan
            pos = e.position()
            if not moved:
                if abs(pos.x() - start.x()) + abs(pos.y() - start.y()) <= 4:
                    return
                self._pan[2] = True
            d = pos - last
            dx, dy = round(d.x()), round(d.y())
            if dx or dy:
                self.horizontalScrollBar().setValue(self.horizontalScrollBar().value() - dx)
                self.verticalScrollBar().setValue(self.verticalScrollBar().value() - dy)
                # only the whole pixels were used: the rest carries over to the next move
                self._pan[0] = last + QPointF(dx, dy)
            return
        if self.page.wiring:
            self.page.move_wire(self.mapToScene(e.position().toPoint()))
            return
        if self.scene() is not None:
            hit = self.scene().port_at(self.mapToScene(e.position().toPoint()))
            self.viewport().setCursor(Qt.CursorShape.CrossCursor if hit else Qt.CursorShape.ArrowCursor)
        super().mouseMoveEvent(e)

    def mouseReleaseEvent(self, e):
        if self._pan is not None:
            _last, b, moved, _start = self._pan
            self._pan = None
            self.viewport().setCursor(Qt.CursorShape.ArrowCursor)
            if b == Qt.MouseButton.RightButton and not moved:
                self.page.context_menu(e.position().toPoint(), e.globalPosition().toPoint())
            self.page.view_moved()
            return
        if self.page.wiring:
            self.page.end_wire(self.mapToScene(e.position().toPoint()), e.globalPosition().toPoint())
            return
        super().mouseReleaseEvent(e)

    def mouseDoubleClickEvent(self, e):
        pos = e.position().toPoint()
        if self.itemAt(pos) is None and e.button() == Qt.MouseButton.LeftButton:
            self.page.pick_node(self.mapToScene(pos), e.globalPosition().toPoint())
            return
        super().mouseDoubleClickEvent(e)

    def contextMenuEvent(self, e):
        if self._proxy_at(e.pos()):
            super().contextMenuEvent(e)
        else:
            e.accept()               # right-click is handled on release (it also pans)

    def keyPressEvent(self, e):
        if e.key() == Qt.Key.Key_Space and not e.isAutoRepeat() and not self._typing():
            self._space = True
            self.viewport().setCursor(Qt.CursorShape.OpenHandCursor)
            return
        super().keyPressEvent(e)

    def keyReleaseEvent(self, e):
        if e.key() == Qt.Key.Key_Space and not e.isAutoRepeat():
            self._space = False
            self.viewport().setCursor(Qt.CursorShape.ArrowCursor)
        super().keyReleaseEvent(e)

    @staticmethod
    def _typing() -> bool:
        from PyQt6.QtWidgets import QAbstractSpinBox
        return isinstance(QApplication.focusWidget(), (QLineEdit, QPlainTextEdit, QAbstractSpinBox, QComboBox))

    def drawBackground(self, p: QPainter, rect: QRectF):
        P = theme.P
        c = QColor(P.bg_frame)
        c.setAlphaF(0.72)
        p.fillRect(rect, c)
        step = 24.0
        while step * self.zoom() < 12:
            step *= 2
        dot = QColor(P.text_primary)
        dot.setAlphaF(0.10 if not theme.is_light() else 0.16)
        p.setPen(QPen(dot, 2.0 / max(0.3, self.zoom())))
        x0, y0 = int(rect.left() // step) * step, int(rect.top() // step) * step
        pts = QPolygonF()
        y = y0
        while y < rect.bottom():
            x = x0
            while x < rect.right():
                pts.append(QPointF(x, y))
                x += step
            y += step
        p.drawPoints(pts)


# ── the node picker ───────────────────────────────────────────────────────────
class NodePicker(QFrame):
    """A search box over every node type. `want` limits it to nodes with a port that fits a dragged wire.
    Each node shows its name and what it does, so you can pick one without hovering for a tooltip."""

    def __init__(self, parent: QWidget, on_pick, want: str = ""):
        super().__init__(parent, Qt.WindowType.Popup)
        self.setObjectName("paletteBox")
        self.on_pick = on_pick
        self.want = want
        lay = QVBoxLayout(self)
        lay.setContentsMargins(10, 10, 10, 10)
        lay.setSpacing(8)
        self.search = QLineEdit()
        self.search.setPlaceholderText("Add a node: type to search…")
        self.search.setFont(theme.qfont(15))
        self.search.setMinimumHeight(38)
        self.search.textChanged.connect(self._fill)
        self.search.installEventFilter(self)
        lay.addWidget(self.search)
        self.list = QListWidget()
        self.list.setObjectName("nodePickList")
        p = theme.P
        self.list.setStyleSheet(f"#nodePickList {{ background: transparent; border: none; }}"
                                f" #nodePickList::item {{ border-radius: {max(0, p.radius - 4)}px; padding: 0; }}"
                                f" #nodePickList::item:selected, #nodePickList::item:hover"
                                f" {{ background: {p.bg_card_hover}; }}")
        self.list.setVerticalScrollMode(QListWidget.ScrollMode.ScrollPerPixel)
        self.list.itemActivated.connect(self._picked)
        self.list.itemClicked.connect(self._picked)
        lay.addWidget(self.list)
        self.setFixedSize(theme.fs(440), theme.fs(560))
        self._fill("")

    def _fits(self, t) -> bool:
        if not self.want:
            return True
        ports = (["in:" + p for p in t.ins] + ["out:" + p for p in t.outs] + ["val:" + p for p in t.values]
                 + ["field:" + f.key for f in t.fields if f.pin])
        return any(compatible(self.want, p) for p in ports)

    def _row(self, t, col: str) -> QWidget:
        w = QWidget()
        w.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        rl = QHBoxLayout(w)
        rl.setContentsMargins(10, 7, 10, 7)
        rl.setSpacing(10)
        dot = QFrame()
        dot.setFixedSize(4, theme.fs(30))
        dot.setStyleSheet(f"background: {col}; border-radius: 2px;")
        rl.addWidget(dot, 0, Qt.AlignmentFlag.AlignTop)
        text = QVBoxLayout()
        text.setSpacing(2)
        text.addWidget(widgets.label(t.title, 14, 600))
        if t.help:
            h = widgets.label(t.help if len(t.help) < 140 else t.help[:139].rsplit(" ", 1)[0] + "…", 12,
                              tone="muted")
            h.setWordWrap(True)
            text.addWidget(h)
        text.addStretch()
        rl.addLayout(text, 1)
        return w

    def _fill(self, text: str):
        q = text.strip().lower()
        self.list.clear()
        first = None
        width = self.width() - 40
        for cat, name, col in CATEGORIES:
            types = [t for t in TYPES.values() if t.cat == cat and self._fits(t)
                     and (not q or q in t.title.lower() or q in t.help.lower() or q in t.id)]
            if not types:
                continue
            head = QListWidgetItem(name.upper())
            head.setFlags(Qt.ItemFlag.NoItemFlags)
            head.setForeground(QColor(col))
            head.setFont(theme.qfont(11, 600, mono=True, spacing=0.1))
            self.list.addItem(head)
            for t in types:
                it = QListWidgetItem()
                it.setData(Qt.ItemDataRole.UserRole, t.id)
                it.setToolTip(t.help)
                row = self._row(t, col)
                row.setFixedWidth(width)
                lay = row.layout()
                h = lay.heightForWidth(width) if lay.hasHeightForWidth() else row.sizeHint().height()
                it.setSizeHint(QSize(width, h))
                self.list.addItem(it)
                self.list.setItemWidget(it, row)
                first = first or it
        if first is not None:
            self.list.setCurrentItem(first)

    def eventFilter(self, obj, e):
        if obj is self.search and e.type() == QEvent.Type.KeyPress:
            if e.key() in (Qt.Key.Key_Down, Qt.Key.Key_Up):
                step = 1 if e.key() == Qt.Key.Key_Down else -1
                i = self.list.currentRow() + step
                while 0 <= i < self.list.count() and not self.list.item(i).data(Qt.ItemDataRole.UserRole):
                    i += step
                if 0 <= i < self.list.count():
                    self.list.setCurrentRow(i)
                return True
            if e.key() in (Qt.Key.Key_Return, Qt.Key.Key_Enter):
                it = self.list.currentItem()
                if it is not None:
                    self._picked(it)
                return True
        return False

    def _picked(self, it: QListWidgetItem):
        tid = it.data(Qt.ItemDataRole.UserRole)
        if tid:
            self.close()
            self.on_pick(tid)

    def popup(self, at: QPoint):
        self.move(at)
        self.show()
        self.search.setFocus()


# ── the page ──────────────────────────────────────────────────────────────────
class AutomationPage(QWidget):
    def __init__(self, launcher, host: QWidget | None = None):
        super().__init__(host)
        self.launcher = launcher
        self.eng = launcher.automation
        self.wiring = None                 # (node item, port, temp path item) while dragging a wire
        self._scenes: dict[str, GraphScene] = {}
        self._undo: dict[str, list[str]] = {}
        self._redo: dict[str, list[str]] = {}
        self._last_cp = (None, 0.0)
        self._press_cp = None
        self._z = 2.0
        if host is not None:
            self.setGeometry(host.rect())
        self.view = GraphView(self)

        self.dock = QFrame(self)
        self.dock.setObjectName("lumDock")
        self.dock.mousePressEvent = lambda e: e.accept()
        dl = QHBoxLayout(self.dock)
        dl.setContentsMargins(5, 5, 5, 5)
        dl.setSpacing(6)
        self.ws_box = QHBoxLayout()
        self.ws_box.setSpacing(2)
        dl.addLayout(self.ws_box)
        self.onoff = QPushButton()
        self.onoff.setObjectName("boardTab")
        self.onoff.setCheckable(True)
        self.onoff.setFixedHeight(32)
        self.onoff.setCursor(Qt.CursorShape.PointingHandCursor)
        self.onoff.clicked.connect(self._toggle_ws)
        dl.addWidget(self.onoff)
        sep = QFrame()
        sep.setObjectName("lumDockSep")
        sep.setFixedSize(1, 18)
        dl.addWidget(sep)
        from ui.dashboard import dock_button
        add = dock_button("Add node", "＋")
        add.clicked.connect(lambda: self.pick_node(self.view.mapToScene(self.view.viewport().rect().center()),
                                                   add.mapToGlobal(QPoint(0, -470))))
        dl.addWidget(add)
        fit = dock_button("Fit", variant="ghost")
        fit.setToolTip("Show every node (F)")
        fit.clicked.connect(self.fit)
        dl.addWidget(fit)
        self.zoom_lbl = dock_button("100 %", variant="ghost")
        self.zoom_lbl.setToolTip("Back to 100 %")
        self.zoom_lbl.clicked.connect(lambda: self.view.set_zoom(1.0))
        dl.addWidget(self.zoom_lbl)
        vars_btn = dock_button("Variables", variant="ghost")
        vars_btn.clicked.connect(self.edit_vars)
        dl.addWidget(vars_btn)
        sep2 = QFrame()
        sep2.setObjectName("lumDockSep")
        sep2.setFixedSize(1, 18)
        dl.addWidget(sep2)
        self.hint = widgets.label("Right-click or double-click to add · drag from a dot to wire · right-drag to "
                                  "move around · wheel zooms", 12, tone="muted")
        self.hint.setContentsMargins(8, 0, 10, 0)
        dl.addWidget(self.hint)

        self._anim = QTimer(self)
        self._anim.setInterval(16)
        self._anim.timeout.connect(self._step_pulses)
        self._live = QTimer(self)
        self._live.setInterval(100)
        self._live.timeout.connect(self._tick_live)
        self.eng.pulse.connect(self._pulse)
        self.eng.node_changed.connect(self._node_changed)
        self.eng.changed.connect(self._ws_changed)
        self.show_ws(self.eng.active_ws())

    # ── workspaces ─────────────────────────────────────────────────────────
    @property
    def ws(self) -> dict:
        return self.eng.active_ws()

    @property
    def scene(self) -> GraphScene:
        return self._scenes[self.ws["id"]]

    def show_ws(self, ws: dict):
        old = self.view.scene()
        if isinstance(old, GraphScene) and old.ws is not ws:
            self._remember_view(old.ws)
        self.eng.set_active(ws["id"])
        sc = self._scenes.get(ws["id"])
        if sc is None:
            sc = self._scenes[ws["id"]] = GraphScene(self, ws)
        self.view.setScene(sc)
        v = ws.get("view") or {}
        self.view.resetTransform()
        z = float(v.get("zoom", 1.0) or 1.0)
        self.view.scale(z, z)
        self.view.centerOn(float(v.get("x", 0)), float(v.get("y", 0)))
        self.zoom_changed()
        self._build_dock()

    def _remember_view(self, ws: dict):
        c = self.view.mapToScene(self.view.viewport().rect().center())
        ws["view"] = {"x": round(c.x()), "y": round(c.y()), "zoom": round(self.view.zoom(), 3)}
        self.eng.schedule_save()

    def view_moved(self):
        self._remember_view(self.ws)

    def zoom_changed(self):
        self.zoom_lbl.setText(f"{self.view.zoom() * 100:.0f} %")
        self._remember_view(self.ws)

    def _ws_changed(self):
        known = {w["id"] for w in self.eng.workspaces()}
        for wid in list(self._scenes):
            if wid not in known:
                self._scenes.pop(wid).deleteLater()
        if self.view.scene() is not self._scenes.get(self.ws["id"]):
            self.show_ws(self.ws)
        else:
            self._build_dock()
        for sc in self._scenes.values():
            for n in sc.nodes.values():
                n.update()

    def _build_dock(self):
        widgets.clear_layout(self.ws_box)
        wss = self.eng.workspaces()
        cur = self.ws
        fm = self.fontMetrics()
        compact = len(wss) > 1 and sum(fm.horizontalAdvance(w["name"]) + 30 for w in wss) + 900 > self.width() - 80
        if len(wss) > 1 and not compact:
            for w in wss:
                b = QPushButton(("● " if w["enabled"] and not self.eng.paused else "") + w["name"])
                b.setObjectName("boardTab")
                b.setCheckable(True)
                b.setChecked(w["id"] == cur["id"])
                b.setCursor(Qt.CursorShape.PointingHandCursor)
                b.setFixedHeight(32)
                b.clicked.connect(lambda _c, ws=w: self.show_ws(ws))
                b.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
                b.customContextMenuRequested.connect(lambda pos, ws=w, btn=b: self._ws_menu(ws, btn.mapToGlobal(pos)))
                self.ws_box.addWidget(b)
        from ui.dashboard import dock_button
        if compact:
            more = QPushButton(f"{cur['name']}  ▾")
            more.setObjectName("boardTab")
            more.setCheckable(True)
            more.setChecked(True)
            more.setFixedHeight(32)
        else:
            more = dock_button("▾" if len(wss) > 1 else f"{cur['name']}  ▾", variant="ghost")
        more.setCursor(Qt.CursorShape.PointingHandCursor)
        more.setToolTip("Workspaces: switch, new, rename, export…")
        more.clicked.connect(lambda: self._ws_menu(cur, more.mapToGlobal(more.rect().topLeft()), above=True))
        self.ws_box.addWidget(more)
        for i in range(self.ws_box.count()):
            w = self.ws_box.itemAt(i).widget()
            if w is not None:
                w.show()
        on = bool(cur["enabled"])
        self.onoff.setChecked(on and not self.eng.paused)
        if self.eng.paused:
            self.onoff.setText("⏸ All paused")
            self.onoff.setToolTip("Every automation is paused (Settings → Automation, the tray, or its hotkey). "
                                  "Click to resume.")
        else:
            self.onoff.setText("● Running" if on else "○ Off")
            self.onoff.setToolTip("This workspace runs in the background. Click to switch it off." if on else
                                  "Switched off: nothing in this workspace runs. Click to switch it on.")
        self._place_dock()

    def _toggle_ws(self):
        if self.eng.paused:
            self.eng.toggle_paused()
        else:
            self.eng.set_enabled(self.ws["id"], not self.ws["enabled"])
        self._build_dock()

    def _ws_menu(self, ws: dict, pos: QPoint, above: bool = False):
        m = widgets.menu(self)
        wss = self.eng.workspaces()
        if len(wss) > 1:
            for w in wss:
                m.addAction(("●  " if w["id"] == self.ws["id"] else "     ") + w["name"]
                            + ("" if w["enabled"] else "   (off)"), lambda w=w: self.show_ws(w))
            m.addSeparator()
        m.addAction("New workspace…", self._new_ws)
        m.addAction(f"Duplicate “{ws['name']}”…", lambda: self._new_ws(ws))
        m.addAction(f"Rename “{ws['name']}”…", lambda: self._rename_ws(ws))
        m.addSeparator()
        m.addAction("Export to a file…", lambda: self._export(ws))
        m.addAction("Import from a file…", self._import)
        m.addSeparator()
        d = m.addAction(f"Delete “{ws['name']}”…", lambda: self._delete_ws(ws))
        d.setEnabled(len(wss) > 1)
        if above:
            pos = QPoint(pos.x(), pos.y() - m.sizeHint().height() - 4)
        m.exec(pos)

    def _new_ws(self, copy: dict | None = None):
        name, ok = widgets.ask_text(self, "Workspace", "Name (e.g. Morning, Backups):",
                                    f"{copy['name']} copy" if copy else "")
        if ok and name.strip():
            self._remember_view(self.ws)
            self.show_ws(self.eng.add_ws(name.strip(), copy))

    def _rename_ws(self, ws: dict):
        name, ok = widgets.ask_text(self, "Rename workspace", "New name:", ws["name"])
        if ok and name.strip():
            self.eng.rename_ws(ws["id"], name)

    def _delete_ws(self, ws: dict):
        if widgets.confirm(self, "Delete workspace", f"Delete “{ws['name']}” with its {len(ws['nodes'])} node(s)?",
                           yes="Delete", danger=True):
            self.eng.delete_ws(ws["id"])

    def _export(self, ws: dict):
        path, _ = QFileDialog.getSaveFileName(self, "Export workspace", os.path.expanduser(f"~/{ws['name']}.lumflow"),
                                              "Luminescent automation (*.lumflow);;JSON (*.json)")
        if path:
            try:
                with open(path, "w", encoding="utf-8") as f:
                    json.dump({k: ws[k] for k in ("name", "nodes", "wires")}, f, indent=2)
            except OSError as e:
                widgets.error(self, "Export", str(e))

    def _import(self):
        path, _ = QFileDialog.getOpenFileName(self, "Import workspace", os.path.expanduser("~"),
                                              "Luminescent automation (*.lumflow *.json);;All files (*)")
        if not path:
            return
        try:
            with open(path, encoding="utf-8") as f:
                d = json.load(f)
            d["nodes"] = [n for n in d.get("nodes", []) if isinstance(n, dict) and n.get("type") in TYPES]
            for n in d["nodes"]:
                n["fields"] = {**default_fields(n["type"]), **(n.get("fields") or {})}
                n.setdefault("state", {})
            ids = {n["id"] for n in d["nodes"]}
            d["wires"] = [w for w in d.get("wires", []) if w.get("src", [None])[0] in ids
                          and w.get("dst", [None])[0] in ids]
        except (OSError, ValueError, KeyError, TypeError) as e:
            widgets.error(self, "Import", f"Could not read that file: {e}")
            return
        self.show_ws(self.eng.import_ws(d))
        self.launcher.notify("Automation", "Imported. It is switched off until you turn it on.")

    # ── layout, show / hide ────────────────────────────────────────────────
    def resizeEvent(self, e):
        super().resizeEvent(e)
        self.view.setGeometry(self.rect())
        self._place_dock()

    def _place_dock(self):
        self.hint.show()
        self.dock.layout().activate()
        self.dock.adjustSize()
        if self.dock.width() > self.width() - 48:
            self.hint.hide()
            self.dock.layout().activate()
            self.dock.adjustSize()
        self.dock.move((self.width() - self.dock.width()) // 2, self.height() - 28 - self.dock.height())
        self.dock.raise_()

    def on_show(self):
        self._live.start()
        self._build_dock()
        for n in self.scene.nodes.values():
            n.update()
        QTimer.singleShot(0, self.view.setFocus)

    def focus_target(self):
        self.view.setFocus()

    def on_hide(self):
        self._live.stop()
        self._anim.stop()
        self._remember_view(self.ws)
        self.launcher.hotkeys.set_suspended(False)

    def teardown(self):
        self.on_hide()
        for sig, slot in ((self.eng.pulse, self._pulse), (self.eng.node_changed, self._node_changed),
                          (self.eng.changed, self._ws_changed)):
            try:
                sig.disconnect(slot)
            except TypeError:
                pass

    def settings_applied(self):
        for sc in self._scenes.values():
            for n in sc.nodes.values():
                n.update()
        self._build_dock()

    def snap(self) -> int:
        return max(0, int(self.launcher.settings.get("dashboard.snap", 8) or 0))

    def next_z(self) -> float:
        self._z += 0.001
        return self._z

    # ── live bits ──────────────────────────────────────────────────────────
    def _animate(self) -> bool:
        return self.isVisible() and anim.on("pulses")

    def _pulse(self, wid: str, wire_id: str, text: str):
        sc = self._scenes.get(wid)
        if sc is None or not self.isVisible() or self.view.scene() is not sc:
            return
        w = sc.wires.get(wire_id)
        if w is None:
            return
        w.heat()
        w.dst.flash()
        w.src.flash()
        if self._animate():
            sc.pulses.add(w, text)
            if not self._anim.isActive():
                self._anim.start()

    def _step_pulses(self):
        sc = self.view.scene()
        if not isinstance(sc, GraphScene) or not sc.pulses.step():
            self._anim.stop()

    def _node_changed(self, wid: str, nid: str):
        sc = self._scenes.get(wid)
        if sc is not None and self.isVisible():
            n = sc.nodes.get(nid)
            if n is not None:
                n.update()

    def _tick_live(self):
        sc = self.view.scene()
        if not isinstance(sc, GraphScene):
            return
        for nid, item in sc.nodes.items():
            rt = self.eng.node_rt(sc.ws["id"], nid)
            if rt is not None and (rt.live or time.monotonic() < item._flash_until + 0.3):
                item.update()
        for w in sc.wires.values():
            if time.monotonic() < w._hot_until + 0.3:
                w.update()

    # ── undo ───────────────────────────────────────────────────────────────
    def _snapshot(self) -> str:
        return json.dumps({"nodes": self.ws["nodes"], "wires": self.ws["wires"]})

    def checkpoint(self, key: str = ""):
        """Remember the workspace before a change. Typing in one field counts as one change."""
        now = time.monotonic()
        if key and self._last_cp[0] == key and now - self._last_cp[1] < 1.5:
            self._last_cp = (key, now)
            return
        self._last_cp = (key, now)
        stack = self._undo.setdefault(self.ws["id"], [])
        stack.append(self._snapshot())
        del stack[:-60]
        self._redo[self.ws["id"]] = []

    def press_snapshot(self):
        self._press_cp = (self._snapshot(), {i: (n.pos().x(), n.pos().y()) for i, n in self.scene.nodes.items()})

    def undo(self, redo: bool = False):
        src = (self._redo if redo else self._undo).get(self.ws["id"]) or []
        if not src:
            return
        (self._undo if redo else self._redo).setdefault(self.ws["id"], []).append(self._snapshot())
        self._restore(src.pop())

    def _restore(self, snap: str):
        d = json.loads(snap)
        ws = self.ws
        ws["nodes"], ws["wires"] = d["nodes"], d["wires"]
        self.eng.edited(ws["id"])
        old = self._scenes.pop(ws["id"], None)
        self._scenes[ws["id"]] = GraphScene(self, ws)
        t = self.view.transform()
        c = self.view.mapToScene(self.view.viewport().rect().center())
        self.view.setScene(self._scenes[ws["id"]])
        self.view.setTransform(t)
        self.view.centerOn(c)
        if old is not None:
            old.deleteLater()
        self._last_cp = (None, 0.0)

    # ── editing ────────────────────────────────────────────────────────────
    def add_node(self, tid: str, sp: QPointF, link: tuple | None = None) -> NodeItem:
        self.checkpoint()
        g = self.snap()
        nd = {"id": _id(), "type": tid, "x": _snap(sp.x(), g), "y": _snap(sp.y(), g), "title": "", "off": False,
              "state": {}, "fields": default_fields(tid)}
        self.ws["nodes"].append(nd)
        item = self.scene.add_node(nd)
        item.setZValue(self.next_z())
        if link is not None:
            src_item, port = link
            other = next((p for p in item.ports if compatible(port, p)), None)
            if other is not None:
                if not _is_source(port):
                    item.setPos(sp.x() - item.w, sp.y() - item.ports[other].y())
                    nd["x"], nd["y"] = item.pos().x(), item.pos().y()
                else:
                    item.setPos(sp.x(), sp.y() - item.ports[other].y())
                    nd["x"], nd["y"] = item.pos().x(), item.pos().y()
                self._connect(src_item, port, item, other, checkpoint=False)
        self.eng.edited(self.ws["id"])
        self.scene.clearSelection()
        item.setSelected(True)
        return item

    def delete_nodes(self, items: list[NodeItem]):
        if not items:
            return
        self.checkpoint()
        gone = {i.data["id"] for i in items}
        self.ws["nodes"] = [n for n in self.ws["nodes"] if n["id"] not in gone]
        self.ws["wires"] = [w for w in self.ws["wires"] if w["src"][0] not in gone and w["dst"][0] not in gone]
        for i in items:
            self.scene.remove_node(i)
        self.eng.edited(self.ws["id"])

    def delete_wires(self, items: list[WireItem]):
        if not items:
            return
        self.checkpoint()
        gone = {i.data["id"] for i in items}
        self.ws["wires"] = [w for w in self.ws["wires"] if w["id"] not in gone]
        for i in items:
            self.scene.remove_wire(i)
        self.eng.edited(self.ws["id"])

    def _connect(self, a: NodeItem, pa: str, b: NodeItem, pb: str, checkpoint: bool = True):
        if not compatible(pa, pb):
            return
        if not _is_source(pa):
            a, pa, b, pb = b, pb, a, pa
        if any(w["src"] == [a.data["id"], pa] and w["dst"] == [b.data["id"], pb] for w in self.ws["wires"]):
            return
        if checkpoint:
            self.checkpoint()
        if pb.startswith("field:"):
            for item in [w for w in self.scene.wires.values() if w.data["dst"] == [b.data["id"], pb]]:
                self.ws["wires"].remove(item.data)
                self.scene.remove_wire(item)
        w = {"id": _id(), "src": [a.data["id"], pa], "dst": [b.data["id"], pb]}
        self.ws["wires"].append(w)
        self.scene.add_wire(w)
        if checkpoint:
            self.eng.edited(self.ws["id"])

    def field_edited(self, item: NodeItem, key: str):
        self.checkpoint(f"{item.data['id']}:{key}")
        self.eng.edited(self.ws["id"], structural=False, nid=item.data["id"])
        item.update()

    def nodes_moved(self):
        if self._press_cp is None:
            return
        snap, before = self._press_cp
        self._press_cp = None
        moved = False
        for nid, item in self.scene.nodes.items():
            x, y = item.pos().x(), item.pos().y()
            if before.get(nid) != (x, y):
                item.data["x"], item.data["y"] = x, y
                moved = True
        if moved:
            stack = self._undo.setdefault(self.ws["id"], [])
            stack.append(snap)
            del stack[:-60]
            self._redo[self.ws["id"]] = []
            self.eng.schedule_save()

    def rename_node(self, item: NodeItem):
        name, ok = widgets.ask_text(self, "Rename node", "Name (empty = its type):", item.data.get("title", ""))
        if ok:
            self.checkpoint()
            item.data["title"] = name.strip()
            item.update()
            self.eng.schedule_save()

    def toggle_off(self, items: list[NodeItem]):
        self.checkpoint()
        off = not all(i.data.get("off") for i in items)
        for i in items:
            i.data["off"] = off
            i.setOpacity(0.45 if off else 1.0)
            i.update()
        self.eng.edited(self.ws["id"])

    def run_node(self, item: NodeItem):
        item.flash()
        self.eng.run_node(self.ws["id"], item.data["id"])

    # ── wiring by dragging ─────────────────────────────────────────────────
    def begin_wire(self, item: NodeItem, port: str, sp: QPointF):
        if not _is_source(port):
            # dragging out of an input picks its wire up again (the last one plugged in)
            plugged = [w for w in item.wires if w.dst is item and w.data["dst"][1] == port]
            if plugged:
                w = plugged[-1]
                src, sport = w.src, w.data["src"][1]
                self.delete_wires([w])
                item, port = src, sport
        temp = QGraphicsPathItem()
        temp.setZValue(4)
        c = QColor(VALUE_COLOUR if _kind(port) == "value" else theme.P.primary)
        temp.setPen(QPen(c, 2, Qt.PenStyle.DashLine))
        self.scene.addItem(temp)
        self.wiring = (item, port, temp)
        self.move_wire(sp)

    def move_wire(self, sp: QPointF):
        item, port, temp = self.wiring
        a = item.port_scene(port)
        hit = self.scene.port_at(sp, 18)
        b = hit[0].port_scene(hit[1]) if hit and compatible(port, hit[1]) else sp
        temp.setPath(wire_path(a, b) if _is_source(port) else wire_path(b, a))

    def end_wire(self, sp: QPointF, gp: QPoint):
        item, port, temp = self.wiring
        self.wiring = None
        self.scene.removeItem(temp)
        hit = self.scene.port_at(sp, 18)
        if hit is not None and compatible(port, hit[1]):
            self._connect(item, port, hit[0], hit[1])
            return
        if hit is None and (sp - item.port_scene(port)).manhattanLength() > 30:
            self.pick_node(sp, gp, link=(item, port))

    # ── menus ──────────────────────────────────────────────────────────────
    def pick_node(self, sp: QPointF, gp: QPoint, link: tuple | None = None):
        pk = NodePicker(self, lambda tid: self.add_node(tid, sp, link), link[1] if link else "")
        scr = QApplication.screenAt(gp) or QApplication.primaryScreen()
        g = scr.availableGeometry()
        at = QPoint(min(gp.x(), g.right() - pk.width()), min(gp.y(), g.bottom() - pk.height()))
        pk.popup(at)

    def context_menu(self, vp: QPoint, gp: QPoint):
        sp = self.view.mapToScene(vp)
        item = self.view.itemAt(vp)
        while item is not None and not isinstance(item, (NodeItem, WireItem)):
            item = item.parentItem()
        m = widgets.menu(self)
        if isinstance(item, NodeItem):
            if not item.isSelected():
                self.scene.clearSelection()
                item.setSelected(True)
            sel = self._selected_nodes()
            head = m.addAction(item.spec.help or item.spec.title)
            head.setEnabled(False)
            m.addSeparator()
            if not item.run_rect.isNull():
                m.addAction("▶  Run it now", lambda: self.run_node(item))
            m.addAction("Rename…", lambda: self.rename_node(item))
            m.addAction("Duplicate   (Ctrl+D)", self.duplicate)
            m.addAction("Copy   (Ctrl+C)", self.copy)
            m.addAction("Turn on" if all(i.data.get("off") for i in sel) else "Turn off (it stays, but does nothing)",
                        lambda: self.toggle_off(sel))
            m.addSeparator()
            m.addAction(f"Delete {len(sel)} nodes" if len(sel) > 1 else "Delete   (Del)",
                        lambda: self.delete_nodes(sel))
        elif isinstance(item, WireItem):
            m.addAction("Delete wire", lambda: self.delete_wires([item]))
        else:
            for cat, name, _c in CATEGORIES:
                sub = m.addMenu(name)
                for t in TYPES.values():
                    if t.cat == cat:
                        a = sub.addAction(t.title, lambda tid=t.id: self.add_node(tid, sp))
                        a.setToolTip(t.help)
            m.addAction("Search for a node…", lambda: self.pick_node(sp, gp))
            m.addSeparator()
            a = m.addAction("Paste   (Ctrl+V)", lambda: self.paste(sp))
            a.setEnabled(_clipboard is not None)
            m.addAction("Select all   (Ctrl+A)", lambda: [i.setSelected(True) for i in self.scene.nodes.values()])
            m.addAction("Fit everything in view   (F)", self.fit)
            m.addSeparator()
            u = m.addAction("Undo   (Ctrl+Z)", self.undo)
            u.setEnabled(bool(self._undo.get(self.ws["id"])))
            r = m.addAction("Redo   (Ctrl+Y)", lambda: self.undo(redo=True))
            r.setEnabled(bool(self._redo.get(self.ws["id"])))
            m.addAction("Variables…", self.edit_vars)
        m.exec(gp)

    def _selected_nodes(self) -> list[NodeItem]:
        return [i for i in self.scene.selectedItems() if isinstance(i, NodeItem)]

    # ── clipboard ──────────────────────────────────────────────────────────
    def copy(self) -> bool:
        global _clipboard
        sel = self._selected_nodes()
        if not sel:
            return False
        ids = {i.data["id"] for i in sel}
        _clipboard = json.loads(json.dumps({
            "nodes": [i.data for i in sel],
            "wires": [w for w in self.ws["wires"] if w["src"][0] in ids and w["dst"][0] in ids]}))
        return True

    def paste(self, at: QPointF | None = None):
        if not _clipboard or not _clipboard["nodes"]:
            return
        self.checkpoint()
        d = json.loads(json.dumps(_clipboard))
        x0 = min(n["x"] for n in d["nodes"])
        y0 = min(n["y"] for n in d["nodes"])
        if at is None:
            at = QPointF(x0 + 40, y0 + 40)
        remap = {n["id"]: _id() for n in d["nodes"]}
        self.scene.clearSelection()
        for n in d["nodes"]:
            n["id"] = remap[n["id"]]
            n["x"], n["y"] = n["x"] - x0 + at.x(), n["y"] - y0 + at.y()
            n["state"] = {}
            self.ws["nodes"].append(n)
            self.scene.add_node(n).setSelected(True)
        for w in d["wires"]:
            w["id"], w["src"][0], w["dst"][0] = _id(), remap[w["src"][0]], remap[w["dst"][0]]
            self.ws["wires"].append(w)
            self.scene.add_wire(w)
        self.eng.edited(self.ws["id"])

    def duplicate(self):
        if self.copy():
            sel = self._selected_nodes()
            self.paste(QPointF(min(i.pos().x() for i in sel) + 40, min(i.pos().y() for i in sel) + 40))

    def fit(self):
        r = QRectF()
        for n in self.scene.nodes.values():
            r = r.united(n.sceneBoundingRect())
        if r.isNull():
            self.view.resetTransform()
            self.view.centerOn(0, 0)
        else:
            self.view.fitInView(r.adjusted(-80, -120, 80, 160), Qt.AspectRatioMode.KeepAspectRatio)
            if self.view.zoom() > 1.2:
                self.view.set_zoom(1.2)
        self.zoom_changed()

    # ── keys (from the overlay, while no text box has the focus) ───────────
    def overlay_key(self, e) -> bool:
        k = e.key()
        ctrl = bool(e.modifiers() & Qt.KeyboardModifier.ControlModifier)
        shift = bool(e.modifiers() & Qt.KeyboardModifier.ShiftModifier)
        if k == Qt.Key.Key_Escape:
            if self.wiring is not None:
                self.scene.removeItem(self.wiring[2])
                self.wiring = None
                return True
            if self.scene.selectedItems():
                self.scene.clearSelection()
                return True
            return False
        if k in (Qt.Key.Key_Delete, Qt.Key.Key_Backspace):
            nodes = self._selected_nodes()
            wires = [i for i in self.scene.selectedItems() if isinstance(i, WireItem)]
            if nodes:
                self.delete_nodes(nodes)
            elif wires:
                self.delete_wires(wires)
            return bool(nodes or wires)
        if ctrl and k == Qt.Key.Key_Z:
            self.undo(redo=shift)
            return True
        if ctrl and k == Qt.Key.Key_Y:
            self.undo(redo=True)
            return True
        if ctrl and k == Qt.Key.Key_C:
            return self.copy()
        if ctrl and k == Qt.Key.Key_V:
            vp = self.view.mapFromGlobal(QCursor.pos())
            self.paste(self.view.mapToScene(vp) if self.view.rect().contains(vp) else None)
            return True
        if ctrl and k == Qt.Key.Key_D:
            self.duplicate()
            return True
        if ctrl and k == Qt.Key.Key_A:
            for i in self.scene.nodes.values():
                i.setSelected(True)
            return True
        if not ctrl and k == Qt.Key.Key_F:
            self.fit()
            return True
        return False

    # ── variables ──────────────────────────────────────────────────────────
    def edit_vars(self):
        dlg = widgets.RoundedDialog(self, "Variables", resizable=True)
        dlg.setMinimumSize(520, 420)
        lay = dlg.body
        hint = QLabel("Kept between restarts. Use them with a Variable node, Set variable, or {var:name} in any field.")
        hint.setObjectName("lumHint")
        hint.setWordWrap(True)
        lay.addWidget(hint)
        table = QTableWidget(0, 2)
        table.setHorizontalHeaderLabels(["Name", "Value"])
        table.horizontalHeader().setSectionResizeMode(1, QHeaderView.ResizeMode.Stretch)
        table.verticalHeader().hide()
        for k, v in sorted(self.eng.data["vars"].items()):
            r = table.rowCount()
            table.insertRow(r)
            table.setItem(r, 0, QTableWidgetItem(k))
            table.setItem(r, 1, QTableWidgetItem(str(v)))
        lay.addWidget(table, 1)
        row = QHBoxLayout()
        add = widgets.button("＋ Add")
        add.clicked.connect(lambda: (table.insertRow(table.rowCount()),
                                     table.setItem(table.rowCount() - 1, 0, QTableWidgetItem("name")),
                                     table.setItem(table.rowCount() - 1, 1, QTableWidgetItem(""))))
        rem = widgets.button("Delete", "danger")
        rem.clicked.connect(lambda: table.removeRow(table.currentRow()) if table.currentRow() >= 0 else None)
        row.addWidget(add)
        row.addWidget(rem)
        row.addStretch()
        save = widgets.button("Save", "primary")
        save.clicked.connect(dlg.accept)
        cancel = widgets.button("Cancel", "ghost")
        cancel.clicked.connect(dlg.reject)
        row.addWidget(cancel)
        row.addWidget(save)
        lay.addLayout(row)
        dlg.fit_to_screen(560, 460, self)
        if dlg.exec() != QDialog.DialogCode.Accepted:
            return
        new = {}
        for r in range(table.rowCount()):
            k = (table.item(r, 0).text() if table.item(r, 0) else "").strip()
            if k:
                new[k] = table.item(r, 1).text() if table.item(r, 1) else ""
        self.eng.data["vars"] = new
        self.eng.schedule_save()
        self.eng.vars_changed.emit()
