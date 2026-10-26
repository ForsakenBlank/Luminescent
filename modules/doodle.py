"""
Screen Doodle: draw on top of everything.

The draw key is a Hyprland press bind plus a release bind (core.hotkeys).  A
missed release can not leave the overlay swallowing clicks: while drawing,
Hyprland is asked every 100 ms whether the key is still held, Esc stops
drawing, and there is a panic hotkey and a tray action that resets it.  The
overlays are click-through (an empty input region) and never take keyboard
focus, except that they take the mouse while drawing is active.

Alt as the draw key: drawing only starts once the key has been held on its own
for `solo_delay_ms`, and any other Luminescent hotkey pressed meanwhile (Alt+N,
Alt+T...) cancels it, so Alt shortcuts do not flash the overlay on.
Ctrl/Shift/Super already held when Alt goes down (Ctrl+Alt+... hotkeys) also
keep it off.

Undo is one history shared by every screen: Alt+F undoes the last stroke
wherever it was drawn (each screen used to undo its own last stroke at once).

Tools: the strip at the top of the primary screen picks pen, highlighter, line, rectangle,
circle, arrow or text, and toggles filled shapes; Shift-dragging the pen draws a straight line.
The text tool: click where the text goes, let go of the draw key and type; Enter puts it on the
drawing, Esc throws it away. The mouse wheel
changes the brush size; the cursor is a circle of the real size (a cursor
pixmap, so moving the mouse costs no repaints).
"""
import os
import math
import logging
from datetime import datetime

from PyQt6.QtWidgets import QApplication, QWidget, QLineEdit
from PyQt6.QtCore import Qt, QRect, QRectF, QTimer, QPoint, QPointF, QLineF, pyqtSignal
from PyQt6.QtGui import QPainter, QPen, QColor, QPixmap, QBrush, QCursor, QFont, QFontMetrics, QPainterPath

from core.module import Module
from core import hotkeys as hk, theme, widgets, hypr, clip

log = logging.getLogger("doodle")

TOOLS = [("pen", "✎", "Pen (Shift-drag: straight line)"), ("hl", "▌", "Highlighter"), ("line", "╱", "Line"),
         ("rect", "▭", "Rectangle"), ("ellipse", "○", "Circle / ellipse"), ("arrow", "→", "Arrow"), ("text", "T", "Text")]
FILLABLE = ("rect", "ellipse")
TOOL_BTN = 34


def _set_click_through(w: QWidget, enabled: bool):
    """An empty input region lets every click fall through to the window underneath."""
    handle = w.windowHandle()
    if handle is not None:
        handle.setFlag(Qt.WindowType.WindowTransparentForInput, enabled)


def circle_cursor(diameter: int, colour: str) -> QCursor:
    d = max(3, diameter)
    size = d + 6
    px = QPixmap(size, size)
    px.fill(QColor(0, 0, 0, 0))
    p = QPainter(px)
    p.setRenderHint(QPainter.RenderHint.Antialiasing)
    r = QRectF(3, 3, d, d)
    p.setPen(QPen(QColor(0, 0, 0, 160), 3))
    p.drawEllipse(r)
    p.setPen(QPen(QColor(colour), 1.4))
    p.drawEllipse(r)
    p.setPen(QPen(QColor(255, 255, 255, 200), 1))
    c = size / 2
    p.drawLine(QPointF(c - 2, c), QPointF(c + 2, c))
    p.drawLine(QPointF(c, c - 2), QPointF(c, c + 2))
    p.end()
    return QCursor(px, size // 2, size // 2)


class TextBox(QLineEdit):
    """The text tool's box: typed into straight on the drawing. Enter keeps it, Esc throws it away."""
    done = pyqtSignal(bool)

    def __init__(self, parent: QWidget, pos: QPoint, px: int, colour: str):
        super().__init__(parent)
        self._finished = False
        f = theme.qfont(px, 600)
        self.setFont(f)
        self.setFrame(False)
        self.setStyleSheet(f"QLineEdit {{ background: rgba(0, 0, 0, 70); color: {colour};"
                           f" border: 1px dashed {colour}; border-radius: 4px; padding: 0 4px; }}")
        self.setPlaceholderText("Type…")
        self.textChanged.connect(self._fit)
        h = QFontMetrics(f).height() + 8
        self.move(pos.x(), pos.y() - h // 2)
        self.resize(80, h)
        self._fit()

    def _fit(self):
        fm = QFontMetrics(self.font())
        self.resize(max(80, fm.horizontalAdvance(self.text() or self.placeholderText()) + 24), self.height())

    def keyPressEvent(self, e):
        if e.key() in (Qt.Key.Key_Return, Qt.Key.Key_Enter):
            self.finish(True)
        elif e.key() == Qt.Key.Key_Escape:
            self.finish(False)
        else:
            super().keyPressEvent(e)

    def focusOutEvent(self, e):
        super().focusOutEvent(e)
        QTimer.singleShot(0, lambda: self.finish(True))

    def finish(self, keep: bool):
        if not self._finished:
            self._finished = True
            self.done.emit(keep)


class DoodleOverlay(QWidget):
    def __init__(self, module: "DoodleModule", geometry: QRect, primary: bool):
        super().__init__()
        self.m = module
        self.primary = primary
        # not WindowDoesNotAcceptFocus: Hyprland's no_focus rule would also stop the mouse reaching it
        self.setWindowFlags(
            Qt.WindowType.FramelessWindowHint | Qt.WindowType.WindowStaysOnTopHint | Qt.WindowType.Tool
        )
        self.setWindowTitle("Luminescent Doodle")
        self.setProperty("lum_drag_blocker", True)      # panels move themselves while this is on screen
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating)
        self.setMouseTracking(True)
        self.setGeometry(geometry)
        # the tool strip goes below whatever bar reserves the top of this screen (Noctalia...)
        scr = QApplication.screenAt(geometry.center())
        self._top = max(0, hypr.available_geometry(scr).top() - geometry.top()) if scr else 0

        self._canvas = QPixmap(self.width(), self.height())
        self._canvas.fill(QColor(0, 0, 0, 0))
        self._undo: list[QPixmap] = []
        self._drawing = False
        self._erasing = False
        self._last: QPoint | None = None
        self._start: QPoint | None = None        # press point of a shape / straight line
        self._shape: str = ""                    # tool used by the stroke in progress ("" = freehand)
        self._points: list[QPoint] = []          # the highlighter stroke in progress
        self.text_box: TextBox | None = None
        self._active = False
        self._show_bg = False
        self._label_pos: QPoint | None = None    # "8 px" after a wheel turn
        self._label_timer = QTimer(self)
        self._label_timer.setSingleShot(True)
        self._label_timer.timeout.connect(self._hide_label)

        self.has_content = False           # anything drawn (an undo may bring a drawing back, so: maybe)
        self.winId()                       # create the native window so the flag can be set before it maps
        _set_click_through(self, True)
        # not shown yet: the module shows the overlays only while they have something to do

    # ── state ──────────────────────────────────────────────────────────────
    def set_active(self, active: bool):
        self._active = active
        _set_click_through(self, not active)
        if active:
            self.refresh_cursor()
            widgets.raise_window(self)
        else:
            self.setCursor(Qt.CursorShape.ArrowCursor)
            self._hover = ""
            self._drawing = self._erasing = False
            self._last = self._start = None
            self._shape = ""
            self._label_pos = None
        self.update()

    def refresh_cursor(self):
        if self._active:
            self.setCursor(circle_cursor(self.m.effective_size(), self.m.s("colour", "#FF0000")))

    def set_show_bg(self, show: bool):
        self._show_bg = show
        self.update()

    def clear(self):
        self._canvas.fill(QColor(0, 0, 0, 0))
        self.has_content = False
        self.update()

    def undo(self):
        if self._undo:
            self._canvas = self._undo.pop()
            self.has_content = True
            self.update()

    def push_undo(self):
        """Only called through DoodleModule.record_undo, which keeps the shared order."""
        self._undo.append(self._canvas.copy())

    def drop_oldest_undo(self):
        if self._undo:
            self._undo.pop(0)

    # ── tool strip (primary screen, while drawing) ─────────────────────────
    def _strip_parts(self) -> list[tuple[str, QRect]]:
        """The strip's pieces left to right: the tools, a divider, the colour, the size and ✕."""
        fm = QFontMetrics(theme.qfont(11, mono=True))
        size_w = fm.horizontalAdvance(f"{self.m.effective_size()} PX") + 16
        widths = [(tid, TOOL_BTN) for tid, _g, _t in TOOLS] + [("sep", 13), ("fill", TOOL_BTN), ("colour", 28),
                                                                   ("size", size_w), ("close", TOOL_BTN)]
        total = sum(w for _k, w in widths) + 8
        x = (self.width() - total) // 2 + 4
        y = self._top + 18 + 4
        parts = []
        for k, w in widths:
            parts.append((k, QRect(x, y, w, TOOL_BTN)))
            x += w
        return parts

    def _strip_rect(self) -> QRect:
        parts = self._strip_parts()
        return QRect(parts[0][1].left() - 4, self._top + 18, parts[-1][1].right() - parts[0][1].left() + 9,
                     TOOL_BTN + 8)

    def _strip_visible(self) -> bool:
        return self.primary and self._active and bool(self.m.s("show_tools", True))

    def _tool_at(self, pos: QPoint) -> str:
        if not self._strip_visible() or not self._strip_rect().contains(pos):
            return ""
        for k, r in self._strip_parts():
            if r.contains(pos):
                return k if k in {t[0] for t in TOOLS} or k in ("close", "fill") else "strip"
        return "strip"

    def _set_hover(self, key: str):
        """Highlight the strip button under the mouse; a hand cursor there, the brush everywhere else."""
        key = "" if key == "strip" else key
        if key == getattr(self, "_hover", ""):
            return
        self._hover = key
        if key:
            self.setCursor(Qt.CursorShape.PointingHandCursor)
        else:
            self.refresh_cursor()
        self.update(self._strip_rect().adjusted(-2, -2, 2, 2))

    def _paint_strip(self, p: QPainter):
        pal = theme.P
        r = self._strip_rect()
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        p.setPen(QPen(QColor(pal.border), 1))
        p.setBrush(QColor(pal.bg_base))
        p.drawRoundedRect(QRectF(r).adjusted(0.5, 0.5, -0.5, -0.5), pal.radius + 2, pal.radius + 2)
        glyphs = {tid: g for tid, g, _t in TOOLS}
        hover = getattr(self, "_hover", "")
        for k, b in self._strip_parts():
            if k == hover and k != self.m.tool:
                p.setPen(Qt.PenStyle.NoPen)
                hc = QColor(pal.text_primary)
                hc.setAlpha(28)
                p.setBrush(hc)
                rad = max(0, pal.radius - 3)
                p.drawRoundedRect(QRectF(b), rad, rad)
            if k in glyphs:
                p.setFont(QFont("Segoe UI Symbol", 11))
                if k == self.m.tool:
                    p.setPen(Qt.PenStyle.NoPen)
                    p.setBrush(QColor(pal.primary_muted))
                    rad = max(0, pal.radius - 3)
                    p.drawRoundedRect(QRectF(b), rad, rad)
                    p.setPen(QColor(pal.primary_text))
                else:
                    p.setPen(QColor(pal.text_secondary))
                p.drawText(b, Qt.AlignmentFlag.AlignCenter, glyphs[k])
            elif k == "fill":
                # a square that is filled while shapes get filled
                c = QColor(pal.primary_text if self.m.fill else pal.text_secondary)
                p.setPen(QPen(c, 1.6))
                p.setBrush(c if self.m.fill else Qt.BrushStyle.NoBrush)
                p.drawRoundedRect(QRectF(b.center().x() - 7, b.center().y() - 7, 14, 14), 3, 3)
            elif k == "sep":
                p.setPen(QPen(QColor(pal.border), 1))
                p.drawLine(b.center().x(), b.center().y() - 10, b.center().x(), b.center().y() + 10)
            elif k == "colour":
                p.setPen(Qt.PenStyle.NoPen)
                p.setBrush(QColor(self.m.s("colour", "#FF0000")))
                p.drawEllipse(QRectF(b.center().x() - 8, b.center().y() - 8, 16, 16))
            elif k == "size":
                p.setFont(theme.qfont(11, mono=True))
                p.setPen(QColor(pal.text_secondary))
                p.drawText(b, Qt.AlignmentFlag.AlignCenter, f"{self.m.effective_size()} PX")
            elif k == "close":
                p.setFont(QFont("Segoe UI Symbol", 9))
                p.setPen(QColor(pal.text_primary if k == hover else pal.text_muted))
                p.drawText(b, Qt.AlignmentFlag.AlignCenter, "✕")

    # ── painting ───────────────────────────────────────────────────────────
    def paintEvent(self, event):
        p = QPainter(self)
        if self._show_bg:
            p.fillRect(self.rect(), QColor(0, 0, 0, int(float(self.m.s("bg_opacity", 1.0)) * 255)))
        elif self._active:
            # a faint tint so you can see that the screen is taking the mouse
            alpha = max(1, int(float(self.m.s("passive_opacity", 0.01)) * 255))
            p.fillRect(self.rect(), QColor(0, 0, 0, alpha))
        p.drawPixmap(0, 0, self._canvas)
        if self._drawing and self._shape and self._start is not None and self._last is not None:
            self._paint_shape(p, self._shape, self._start, self._last, self._erasing, preview=True)
        if self._strip_visible():
            self._paint_strip(p)
        if self._label_pos is not None:
            text = f"{self.m.effective_size()} px"
            p.setFont(QFont(theme.P.font_family, 10, QFont.Weight.Bold))
            br = p.fontMetrics().boundingRect(text).adjusted(-6, -3, 6, 3)
            br.moveTopLeft(self._label_pos)
            p.setRenderHint(QPainter.RenderHint.Antialiasing)
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(QColor(0, 0, 0, 170))
            p.drawRoundedRect(QRectF(br), 5, 5)
            p.setPen(QColor("#FFFFFF"))
            p.drawText(br, Qt.AlignmentFlag.AlignCenter, text)
        p.end()

    def _pen(self, erase: bool) -> QPen:
        colour = QColor(self.m.s("colour", "#FF0000"))
        size = self.m.effective_size() * (3 if erase else 1)
        return QPen(colour, size, Qt.PenStyle.SolidLine, Qt.PenCapStyle.RoundCap, Qt.PenJoinStyle.RoundJoin)

    def _pen_painter(self, erase: bool) -> QPainter:
        p = QPainter(self._canvas)
        smooth = self.m.s("brush_style", "smooth") == "smooth"
        p.setRenderHint(QPainter.RenderHint.Antialiasing, smooth)
        if erase:
            p.setCompositionMode(QPainter.CompositionMode.CompositionMode_Clear)
        p.setPen(self._pen(erase))
        p.setBrush(QBrush(QColor(self.m.s("colour", "#FF0000"))))
        return p

    def _hl_pen(self) -> QPen:
        colour = QColor(self.m.s("colour", "#FF0000"))
        colour.setAlphaF(max(0.05, min(1.0, float(self.m.s("highlighter_opacity", 0.35)))))
        return QPen(colour, max(10, self.m.effective_size() * 4), Qt.PenStyle.SolidLine, Qt.PenCapStyle.SquareCap,
                    Qt.PenJoinStyle.RoundJoin)

    def _hl_path(self) -> QPainterPath:
        path = QPainterPath()
        if self._points:
            path.moveTo(QPointF(self._points[0]))
            for pt in self._points[1:]:
                path.lineTo(QPointF(pt))
            if len(self._points) == 1:
                path.lineTo(QPointF(self._points[0]) + QPointF(0.1, 0))
        return path

    def _paint_shape(self, p: QPainter, shape: str, a: QPoint, b: QPoint, erase: bool, preview: bool = False):
        smooth = self.m.s("brush_style", "smooth") == "smooth"
        p.setRenderHint(QPainter.RenderHint.Antialiasing, smooth)
        if shape == "hl":
            # one path for the whole stroke, so the see-through ink does not darken where it overlaps itself
            p.setPen(self._hl_pen())
            p.setBrush(Qt.BrushStyle.NoBrush)
            p.drawPath(self._hl_path())
            return
        pen = self._pen(erase)
        if preview and erase:
            pen.setColor(QColor(255, 255, 255, 120))
        p.setPen(pen)
        if shape in FILLABLE and self.m.fill:
            p.setBrush(QColor(255, 255, 255, 60) if preview and erase else QColor(self.m.s("colour", "#FF0000")))
        else:
            p.setBrush(Qt.BrushStyle.NoBrush)
        if shape == "line":
            p.drawLine(a, b)
        elif shape == "rect":
            p.drawRect(QRect(a, b).normalized())
        elif shape == "ellipse":
            p.drawEllipse(QRect(a, b).normalized())
        elif shape == "arrow":
            p.drawLine(a, b)
            if a != b:
                ang = math.atan2(b.y() - a.y(), b.x() - a.x())
                head = max(10.0, self.m.effective_size() * 4.0)
                for d in (math.radians(25), -math.radians(25)):
                    tip = QPointF(b.x() - head * math.cos(ang + d), b.y() - head * math.sin(ang + d))
                    p.drawLine(QLineF(QPointF(b), tip))

    def _commit_shape(self, shape: str, a: QPoint, b: QPoint, erase: bool):
        self.has_content = True
        p = QPainter(self._canvas)
        try:
            if erase:
                p.setCompositionMode(QPainter.CompositionMode.CompositionMode_Clear)
            self._paint_shape(p, shape, a, b, erase)
        finally:
            p.end()
        self.update()

    def _draw_segment(self, a: QPoint, b: QPoint, erase: bool):
        self.has_content = True
        if self.m.s("brush_style", "smooth") == "pixel" and not erase:
            self._draw_pixels(a, b)
        else:
            p = self._pen_painter(erase)
            try:
                if a == b:
                    # a single click: draw a dot (drawLine with identical points paints nothing)
                    r = max(1.0, self.m.effective_size() * (3 if erase else 1) / 2)
                    p.setPen(Qt.PenStyle.NoPen)
                    if erase:
                        p.setBrush(QBrush(QColor(0, 0, 0, 255)))
                    p.drawEllipse(QPointF(a), r, r)
                else:
                    p.drawLine(a, b)
            finally:
                p.end()   # an exception here must never leave the canvas locked by a half-open painter
        pad = self.m.effective_size() * 3 + 4
        self.update(QRect(a, b).normalized().adjusted(-pad, -pad, pad, pad))

    def _draw_pixels(self, a: QPoint, b: QPoint):
        s = max(1, int(self.m.s("pixel_size", 4)))
        colour = QColor(self.m.s("colour", "#FF0000"))
        p = QPainter(self._canvas)
        p.setRenderHint(QPainter.RenderHint.Antialiasing, False)
        x0, y0, x1, y1 = a.x() // s, a.y() // s, b.x() // s, b.y() // s
        dx, dy = abs(x1 - x0), abs(y1 - y0)
        sx, sy = (1 if x0 < x1 else -1), (1 if y0 < y1 else -1)
        err = dx - dy
        while True:
            p.fillRect(x0 * s, y0 * s, s, s, colour)
            if x0 == x1 and y0 == y1:
                break
            e2 = 2 * err
            if e2 > -dy:
                err -= dy
                x0 += sx
            if e2 < dx:
                err += dx
                y0 += sy
        p.end()

    # ── mouse ──────────────────────────────────────────────────────────────
    def mousePressEvent(self, e):
        if not self._active:
            return
        pos = e.position().toPoint()
        tool = self._tool_at(pos)
        if tool == "close":
            self.m.deactivate()
            return
        if tool:
            if tool == "fill":
                self.m.toggle_fill()
            elif tool != "strip":
                self.m.set_tool(tool)
            return
        if self.text_box is not None:
            self.text_box.finish(True)          # a click somewhere else keeps the text being typed
        if e.button() == Qt.MouseButton.LeftButton and self.m.tool == "text":
            self.open_text(pos)
            return
        if e.button() in (Qt.MouseButton.LeftButton, Qt.MouseButton.RightButton):
            self.m.mask_menu_key()
            self.m.record_undo([self])
            self._drawing = True
            self._erasing = e.button() == Qt.MouseButton.RightButton
            self._last = self._start = pos
            shift = bool(e.modifiers() & Qt.KeyboardModifier.ShiftModifier)
            tool = self.m.tool
            if self._erasing and tool in ("hl", "text"):
                tool = "pen"                     # the right button always rubs out
            self._shape = tool if tool != "pen" else ("line" if shift else "")
            self._points = [pos] if self._shape == "hl" else []
            if not self._shape:
                self._draw_segment(pos, pos, self._erasing)
            elif self._shape == "hl":
                self.update(QRect(pos, pos).adjusted(-60, -60, 60, 60))

    def mouseMoveEvent(self, e):
        if self._active and not self._drawing:
            self._set_hover(self._tool_at(e.position().toPoint()))
        if not (self._active and self._drawing):
            return
        cur = e.position().toPoint()
        if self._shape == "hl":
            pad = max(12, self.m.effective_size() * 4) + 4
            self._points.append(cur)
            self.update(QRect(self._last, cur).normalized().adjusted(-pad, -pad, pad, pad))
            self._last = cur
            return
        if self._shape:
            pad = max(12, self.m.effective_size() * 4 + 4)
            old = QRect(self._start, self._last).normalized().adjusted(-pad, -pad, pad, pad)
            self._last = cur
            self.update(old.united(QRect(self._start, cur).normalized().adjusted(-pad, -pad, pad, pad)))
            return
        if self._last is not None:
            self._draw_segment(self._last, cur, self._erasing)
        self._last = cur

    def mouseReleaseEvent(self, e):
        if self._active and self._drawing:
            if self._shape and self._start is not None:
                self._commit_shape(self._shape, self._start, e.position().toPoint(), self._erasing)
            self._drawing = False
            self._erasing = False
            self._last = self._start = None
            self._shape = ""
            self._points = []

    # ── text tool ──────────────────────────────────────────────────────────
    def open_text(self, pos: QPoint):
        box = TextBox(self, pos, int(self.m.s("text_size", 28)), self.m.s("colour", "#FF0000"))
        box.done.connect(lambda keep, b=box: self._text_done(b, keep))
        self.text_box = box
        self.m.text_opened(self)
        box.show()
        widgets.bring_to_front(self)
        box.setFocus()

    def _text_done(self, box: TextBox, keep: bool):
        if self.text_box is box:
            self.text_box = None
        text = box.text().strip()
        if keep and text:
            self.m.record_undo([self])
            self.has_content = True
            p = QPainter(self._canvas)
            p.setRenderHint(QPainter.RenderHint.TextAntialiasing)
            p.setFont(box.font())
            p.setPen(QColor(self.m.s("colour", "#FF0000")))
            p.drawText(box.geometry().adjusted(5, 0, 0, 0), int(Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter),
                       box.text())
            p.end()
        box.hide()
        box.deleteLater()
        self.update()
        self.m.text_closed()

    def wheelEvent(self, e):
        if not self._active or not self.m.s("wheel_size", True):
            return
        steps = e.angleDelta().y() // 120 or (1 if e.angleDelta().y() > 0 else -1 if e.angleDelta().y() < 0 else 0)
        if steps:
            self.m.change_size(steps)
            old = QRect(self._label_pos, QPoint(self._label_pos.x() + 80, self._label_pos.y() + 30)) \
                if self._label_pos else QRect()
            self._label_pos = e.position().toPoint() + QPoint(self.m.effective_size() // 2 + 14, 10)
            self.update(old.united(QRect(self._label_pos, QPoint(self._label_pos.x() + 80, self._label_pos.y() + 30))))
            self._label_timer.start(1000)

    def _hide_label(self):
        if self._label_pos is not None:
            r = QRect(self._label_pos, QPoint(self._label_pos.x() + 80, self._label_pos.y() + 30))
            self._label_pos = None
            self.update(r)


class DoodleModule(Module):
    key = "doodle"
    title = "Screen Doodle"
    icon = "✏"

    def __init__(self, ctx):
        super().__init__(ctx)
        self._overlays: list[DoodleOverlay] = []
        self._active = False
        self._show_bg = False
        self._hold_mode = True
        self._wait_release = False      # Esc/panic while holding: stay off until the key is let go
        self._history: list[list[DoodleOverlay]] = []   # undo groups, oldest first
        self._interrupted = False       # another key went down while the draw key was held
        self._masked = False            # dummy key already sent during this hold
        self._own_keys: set[str] = set()
        self.tool = "pen"
        self.fill = False
        self._typing: DoodleOverlay | None = None    # an overlay with an open text box
        self._off_after_text = False
        self._solo_timer = QTimer(self)
        self._solo_timer.setSingleShot(True)
        self._solo_timer.timeout.connect(self._solo_elapsed)
        self._size_save = QTimer(self)
        self._size_save.setSingleShot(True)
        self._size_save.setInterval(1000)
        self._size_save.timeout.connect(lambda: self.settings.save(emit=False))
        self._screens_connected = False

    # ── lifecycle ──────────────────────────────────────────────────────────
    def start(self):
        self._build_overlays()
        if not self._screens_connected:
            self.ctx.app.screenAdded.connect(self._screens_changed)
            self.ctx.app.screenRemoved.connect(self._screens_changed)
            self._screens_connected = True

        hotkey = self.s("hotkey", "alt")
        self._hold_mode = self.s("mode", "hold") == "hold"
        if self._hold_mode:
            err = self.hotkeys.register_hold(self.key, hotkey, self._hold_pressed, self._hold_released,
                                             self._hold_interrupted)
            if err:
                self.ctx.notify(self.title, f"Hold key {hotkey!r} not usable ({err}); using toggle mode instead.")
                self._hold_mode = False
                self.hotkeys.register(self.key, hotkey, self.toggle, "toggle drawing")
        else:
            self.hotkey("hotkey", self.toggle, "toggle drawing")

        self.hotkey("clear_hotkey", self.clear, "clear")
        self.hotkey("undo_hotkey", self.undo, "undo")
        self.hotkey("bg_hotkey", self.toggle_bg, "blackboard")
        self.hotkey("exit_hotkey", self.panic, "panic")
        self.hotkey("copy_hotkey", self.copy_screen, "copy screen + drawing")
        # Esc is bound for as long as the module runs, without taking it from the app you are in.
        # Holding the draw key adds itself to the combo, so bind that too.
        self.hotkeys.register(self.key, "esc", self._escape, "stop drawing", non_consuming=True)
        if self._hold_mode and hk.is_modifier(hotkey):
            self.hotkeys.register(self.key, f"{hk.normalize(hotkey)}+esc", self._escape, "stop drawing",
                                  non_consuming=True)
        # keys of doodle's own shortcuts (x, f, g, c, shift, q, esc) don't count as "using an Alt shortcut"
        hold = hk.normalize(hotkey)
        self._own_keys = {"esc"}
        for k in ("clear_hotkey", "undo_hotkey", "bg_hotkey", "exit_hotkey", "copy_hotkey"):
            self._own_keys.update(p for p in hk.normalize(self.s(k, "")).split("+") if p and p != hold)
        self.log.info("ready with %d overlay(s)", len(self._overlays))

    def stop(self):
        self.deactivate()
        self._show_bg = False
        self._wait_release = False
        self._destroy_overlays()
        if self._size_save.isActive():
            self._size_save.stop()
            self.settings.save(emit=False)

    def tray_actions(self):
        return [("Clear doodles & stop drawing", self.panic)]

    def actions(self):
        return [("panic", "Clear doodles & stop drawing", self.panic),
                ("copy", "Copy the screen with the drawing", self.copy_screen),
                ("blackboard", "Toggle the doodle blackboard", self.toggle_bg)]

    # ── overlays ───────────────────────────────────────────────────────────
    def _build_overlays(self):
        self._destroy_overlays()
        prim = self.ctx.app.primaryScreen()
        for scr in self.ctx.app.screens():
            o = DoodleOverlay(self, scr.geometry(), scr is prim)
            if self._show_bg:
                o.set_show_bg(True)
            if self._active:
                o.set_active(True)
            self._overlays.append(o)
        self._sync_visibility()

    def _sync_visibility(self):
        """Overlays are on screen only while drawing, or while there is a drawing or blackboard to show.
        A hidden overlay is no window at all, so Luminescent is not in the dock / task list because of it."""
        for o in self._overlays:
            want = self._active or self._show_bg or o.has_content
            if want and not o.isVisible():
                o.show()
                if self._active:
                    o.set_active(True)          # raise it now that it is mapped
            elif not want and o.isVisible():
                o.hide()

    def _destroy_overlays(self):
        self._typing = None
        for o in self._overlays:
            o.close()
            o.deleteLater()
        self._overlays = []
        self._history = []

    def _screens_changed(self, *_):
        if self.running:
            QTimer.singleShot(500, self._build_overlays)

    # ── brush ──────────────────────────────────────────────────────────────
    def effective_size(self) -> int:
        key = "pixel_size" if self.s("brush_style", "smooth") == "pixel" else "brush_size"
        return max(1, int(self.s(key, 4)))

    def change_size(self, steps: int):
        key = "pixel_size" if self.s("brush_style", "smooth") == "pixel" else "brush_size"
        n = max(1, min(60, int(self.s(key, 4)) + steps))
        self.settings.set(f"doodle.{key}", n)
        self._size_save.start()             # saved quietly a second later, without restarting the module
        for o in self._overlays:
            o.refresh_cursor()
            if o.primary:
                o.update(o._strip_rect().adjusted(-40, -2, 40, 2))     # the "N PX" in the strip

    def set_tool(self, tool: str):
        self.tool = tool
        self._update_strip()

    def toggle_fill(self):
        self.fill = not self.fill
        self._update_strip()

    def _update_strip(self):
        for o in self._overlays:
            if o.primary:
                o.update(o._strip_rect().adjusted(-2, -2, 2, 2))

    # ── text tool: drawing stays on while you type, even after the draw key is let go ──
    def text_opened(self, overlay: DoodleOverlay):
        self._typing = overlay

    def text_closed(self):
        self._typing = None
        self._sync_visibility()
        if self._off_after_text:
            self._off_after_text = False
            self.deactivate()

    def _close_text(self, keep: bool):
        if self._typing is not None and self._typing.text_box is not None:
            self._typing.text_box.finish(keep)

    # ── actions ────────────────────────────────────────────────────────────
    def _hold_pressed(self):
        if self._wait_release:
            return
        self._interrupted = False
        delay = int(self.s("solo_delay_ms", 120)) if self.s("solo_only", True) else 0
        if delay <= 0:
            self.activate()
        else:
            self._solo_timer.start(delay)

    def _other_modifier_down(self) -> bool:
        hold = hk.normalize(self.s("hotkey", "alt"))
        # (Ctrl+Alt... never reaches here anyway: the hold binds only match Alt on its own)
        return any(m != hold and hk.key_state(m) is True for m in ("ctrl", "shift", "windows"))

    def _solo_elapsed(self):
        if self._interrupted or self._wait_release or not hk.key_is_physically_down(self.s("hotkey", "alt")):
            return
        if self._other_modifier_down():
            return              # Ctrl+Alt+..., Win+Alt+...: a hotkey on its way, not a doodle
        self.activate()

    def _hold_interrupted(self, name: str):
        """Another key went down while the draw key is held."""
        if not self.s("solo_only", True):
            return
        name = (name or "").lower()
        for word in ("left ", "right "):
            if name.startswith(word):
                name = name[len(word):]
        if name in self._own_keys:
            return
        self._interrupted = True
        self._solo_timer.stop()
        if self._active and not self._any_drawing():
            # Alt held a moment, then Alt+Tab / Alt+N: get out of the way until Alt is released
            self.deactivate()
            self._wait_release = True

    def _any_drawing(self) -> bool:
        return any(o._drawing for o in self._overlays)

    def _hold_released(self):
        self._solo_timer.stop()
        self._wait_release = False
        self.deactivate()

    def mask_menu_key(self):
        """Windows needed a dummy key so a lone Alt tap did not open the menu bar; Linux apps don't do that."""
        self._masked = True

    def _escape(self):
        if self._typing is not None:
            self._off_after_text = False
            self._close_text(False)
        if self._active:
            self.deactivate()
            self._wait_release = self._hold_mode

    def activate(self):
        if self._active:
            return
        self._active = True
        # drawing clicks give the overlay keyboard focus; remember who had it to hand it back afterwards
        aw = hypr.query("activewindow") if hypr.available() else None
        self._prev_focus = aw.get("address", "") if isinstance(aw, dict) and aw.get("class") != hypr.APP_ID else ""
        self._sync_visibility()
        for o in self._overlays:
            o.set_active(True)

    def deactivate(self):
        self._masked = False
        if not self._active:
            return
        if self._typing is not None:
            self._off_after_text = True       # the key was let go to type: stop once the text is done
            return
        self._active = False
        for o in self._overlays:
            o.set_active(False)
        if self._hold_mode and self.s("clear_on_release", False):
            self.clear()
        self._sync_visibility()
        prev, self._prev_focus = getattr(self, "_prev_focus", ""), ""
        aw = hypr.query("activewindow") if prev and hypr.available() else None
        if prev and isinstance(aw, dict) and aw.get("class") == hypr.APP_ID:
            hypr.dispatch(f"hl.dsp.focus({{ window = {hypr.lstr('address:' + prev)} }})")

    def toggle(self):
        self.deactivate() if self._active else self.activate()

    def record_undo(self, overlays: list):
        """Snapshot `overlays` as one undo step (a stroke = one screen, Clear = every screen)."""
        for o in overlays:
            o.push_undo()
        self._history.append(list(overlays))
        limit = max(1, int(self.s("undo_levels", 30)))
        while len(self._history) > limit:
            for o in self._history.pop(0):
                o.drop_oldest_undo()

    def clear(self):
        self.record_undo(self._overlays)
        for o in self._overlays:
            o.clear()
        self._sync_visibility()

    def undo(self):
        if self._history:
            for o in self._history.pop():
                o.undo()
            self._sync_visibility()

    def toggle_bg(self):
        self._show_bg = not self._show_bg
        for o in self._overlays:
            o.set_show_bg(self._show_bg)
        self._sync_visibility()

    def panic(self):
        """Clear everything, drop the blackboard and make every overlay click-through again."""
        self._show_bg = False
        self._off_after_text = False
        self._close_text(False)
        for o in self._overlays:
            o.set_show_bg(False)
            o.clear()
        self.deactivate()
        self._wait_release = self._hold_mode and hk.key_is_physically_down(self.s("hotkey", "alt"))
        # belt and braces: force click-through even if state got confused
        for o in self._overlays:
            try:
                _set_click_through(o, True)
            except Exception:
                pass
        self._sync_visibility()

    # ── copy screen + drawing ──────────────────────────────────────────────
    def copy_screen(self):
        """Screenshot every screen, paint the drawings (and blackboard) on top, put it on the clipboard.

        On Wayland grim captures what is on screen, drawing and blackboard included. Elsewhere the
        canvases are composited by hand (overlay windows are not reliably part of a Qt screen grab).
        """
        screens = QApplication.screens()
        if not screens:
            return
        virt = screens[0].geometry()
        for scr in screens[1:]:
            virt = virt.united(scr.geometry())
        scale = max(scr.devicePixelRatio() for scr in screens)
        out = QPixmap(int(virt.width() * scale), int(virt.height() * scale))
        out.fill(QColor(0, 0, 0))
        p = QPainter(out)
        p.setRenderHint(QPainter.RenderHint.SmoothPixmapTransform)
        for scr in screens:
            g = scr.geometry()
            target = QRectF((g.x() - virt.x()) * scale, (g.y() - virt.y()) * scale, g.width() * scale, g.height() * scale)
            shot = widgets.grab_screen(scr)
            p.drawPixmap(target, shot, QRectF(shot.rect()))
        bg_alpha = int(float(self.s("bg_opacity", 1.0)) * 255)
        for o in ([] if hypr.wayland() else self._overlays):
            og = o.geometry()
            target = QRectF((og.x() - virt.x()) * scale, (og.y() - virt.y()) * scale, og.width() * scale, og.height() * scale)
            if o._show_bg:
                p.fillRect(target, QColor(0, 0, 0, bg_alpha))
            p.drawPixmap(target, o._canvas, QRectF(o._canvas.rect()))
        p.end()
        clip.set_image(out)
        msg = "Screenshot with your drawing copied to the clipboard."
        if self.s("save_copies", False):
            folder = os.path.join(os.path.expanduser("~"), "Pictures", "Luminescent")
            try:
                os.makedirs(folder, exist_ok=True)
                path = os.path.join(folder, f"doodle-{datetime.now():%Y%m%d-%H%M%S}.png")
                if out.save(path, "PNG"):
                    msg += f"\nSaved to {path}"
            except OSError as e:
                msg += f"\nCould not save it: {e}"
        self.ctx.notify(self.title, msg, 2500)
