"""
Screen ruler: measure the pixels between two points.

The hotkey freezes every screen (like the colour picker) and shows crosshairs. Drag from one point to
another: the box shows its width × height and the straight distance, in real screen pixels. Hold Shift
to keep the line level or upright. Click (or Enter) copies "W × H", Ctrl+click copies the distance;
arrow keys nudge the pointer 1 px (Shift: 10 px). Esc or right-click closes.
"""
import math

from PyQt6.QtWidgets import QApplication, QWidget
from PyQt6.QtCore import Qt, QRect, QRectF, QPoint, QPointF, QLineF
from PyQt6.QtGui import QPainter, QColor, QPen, QPixmap

from core import theme, widgets, hypr, clip
from core.module import Module


class _RulerScreen(QWidget):
    def __init__(self, m: "RulerModule", screen, shot: QPixmap):
        super().__init__()
        self.m = m
        self.shot = shot
        self.dpr = shot.devicePixelRatio() or 1.0
        self.setWindowFlags(Qt.WindowType.FramelessWindowHint | Qt.WindowType.WindowStaysOnTopHint | Qt.WindowType.Tool)
        self.setGeometry(screen.geometry())
        self.setMouseTracking(True)
        self.setCursor(Qt.CursorShape.CrossCursor)
        self._pos: QPoint | None = None
        self._start: QPoint | None = None
        self._end: QPoint | None = None

    # ── measuring ──────────────────────────────────────────────────────────
    def _locked(self, a: QPoint, b: QPoint, shift: bool) -> QPoint:
        if not shift:
            return b
        return QPoint(b.x(), a.y()) if abs(b.x() - a.x()) >= abs(b.y() - a.y()) else QPoint(a.x(), b.y())

    def measure(self) -> tuple[int, int, float] | None:
        """(width, height, distance) in physical pixels, or None before a drag."""
        if self._start is None or self._end is None:
            return None
        dx = abs(self._end.x() - self._start.x()) * self.dpr
        dy = abs(self._end.y() - self._start.y()) * self.dpr
        return round(dx), round(dy), math.hypot(dx, dy)

    # ── painting ───────────────────────────────────────────────────────────
    def paintEvent(self, e):
        p = QPainter(self)
        p.drawPixmap(self.rect(), self.shot)
        p.fillRect(self.rect(), QColor(0, 0, 0, 40))
        pal = theme.P
        accent = QColor(pal.primary)
        if self._pos is not None and self._start is None:
            p.setPen(QPen(QColor(255, 255, 255, 150), 1, Qt.PenStyle.DashLine))
            p.drawLine(0, self._pos.y(), self.width(), self._pos.y())
            p.drawLine(self._pos.x(), 0, self._pos.x(), self.height())
        got = self.measure()
        if got is not None:
            a, b = self._start, self._end
            box = QRect(a, b).normalized()
            fill = QColor(accent)
            fill.setAlpha(40)
            p.fillRect(box, fill)
            p.setPen(QPen(accent, 1))
            p.drawRect(box)
            p.setRenderHint(QPainter.RenderHint.Antialiasing)
            p.setPen(QPen(QColor("#FFFFFF"), 2))
            p.drawLine(QLineF(QPointF(a), QPointF(b)))
            for pt in (a, b):
                p.setBrush(accent)
                p.drawEllipse(QPointF(pt), 4, 4)
            w, h, dist = got
            text = f"{w} × {h} px   ·   {dist:.1f} px"
            p.setFont(theme.qfont(12, 600, mono=True))
            fm = p.fontMetrics()
            tw, th = fm.horizontalAdvance(text) + 20, fm.height() + 10
            tag = QRect(b.x() + 14, b.y() + 14, tw, th)
            if tag.right() > self.width():
                tag.moveRight(b.x() - 14)
            if tag.bottom() > self.height():
                tag.moveBottom(b.y() - 14)
            p.setPen(QPen(QColor(pal.border), 1))
            p.setBrush(QColor(pal.bg_base))
            p.drawRoundedRect(QRectF(tag).adjusted(0.5, 0.5, -0.5, -0.5), 8, 8)
            p.setPen(QColor(pal.text_primary))
            p.drawText(tag, Qt.AlignmentFlag.AlignCenter, text)
        if self.screen() is QApplication.primaryScreen() or self._pos is not None:
            hint = "DRAG TO MEASURE  ·  SHIFT LEVEL  ·  CLICK COPY W×H  ·  CTRL+CLICK COPY DISTANCE  ·  ESC"
            p.setFont(theme.qfont(11, mono=True))
            fm = p.fontMetrics()
            r = QRect((self.width() - fm.horizontalAdvance(hint) - 40) // 2, 22, fm.horizontalAdvance(hint) + 40, 32)
            p.setRenderHint(QPainter.RenderHint.Antialiasing)
            p.setPen(QPen(QColor(pal.border), 1))
            p.setBrush(QColor(pal.bg_base))
            p.drawRoundedRect(QRectF(r).adjusted(0.5, 0.5, -0.5, -0.5), 16, 16)
            p.setPen(QColor(pal.text_secondary))
            p.drawText(r, Qt.AlignmentFlag.AlignCenter, hint)
        p.end()

    # ── input ──────────────────────────────────────────────────────────────
    def mousePressEvent(self, e):
        if e.button() != Qt.MouseButton.LeftButton:
            self.m.cancel()
            return
        self._start = self._end = e.position().toPoint()
        self.update()

    def mouseMoveEvent(self, e):
        self._pos = e.position().toPoint()
        if self._start is not None and e.buttons() & Qt.MouseButton.LeftButton:
            self._end = self._locked(self._start, self._pos, bool(e.modifiers() & Qt.KeyboardModifier.ShiftModifier))
        self.update()

    def mouseReleaseEvent(self, e):
        if e.button() != Qt.MouseButton.LeftButton or self._start is None:
            return
        self._end = self._locked(self._start, e.position().toPoint(),
                                 bool(e.modifiers() & Qt.KeyboardModifier.ShiftModifier))
        got = self.measure()
        if got and (got[0] or got[1]):
            self.m.copy(got, distance=bool(e.modifiers() & Qt.KeyboardModifier.ControlModifier))
        self.update()

    def keyPressEvent(self, e):
        k = e.key()
        moves = {Qt.Key.Key_Left: (-1, 0), Qt.Key.Key_Right: (1, 0), Qt.Key.Key_Up: (0, -1), Qt.Key.Key_Down: (0, 1)}
        if k == Qt.Key.Key_Escape:
            self.m.cancel()
        elif k in moves:
            dx, dy = moves[k]
            step = 10 if e.modifiers() & Qt.KeyboardModifier.ShiftModifier else 1
            hypr.move_cursor(hypr.cursor_pos() + QPoint(dx * step, dy * step))
        elif k in (Qt.Key.Key_Return, Qt.Key.Key_Enter):
            got = self.measure()
            if got:
                self.m.copy(got, distance=bool(e.modifiers() & Qt.KeyboardModifier.ControlModifier))
                self.m.cancel()


class RulerModule(Module):
    key = "ruler"
    title = "Screen Ruler"
    icon = "📏"

    def __init__(self, ctx):
        super().__init__(ctx)
        self._screens: list[_RulerScreen] = []

    def start(self):
        self.hotkey("hotkey", self.measure, "screen ruler")

    def stop(self):
        self.cancel()

    def actions(self):
        return [("measure", "Measure pixels on screen", self.measure)]

    def measure(self):
        if self._screens:
            return
        for scr in QApplication.screens():
            self._screens.append(_RulerScreen(self, scr, widgets.grab_screen(scr)))
        for w in self._screens:
            w.show()
            w.raise_()
        here = widgets.cursor_screen()
        front = next((w for w in self._screens if here and w.geometry() == here.geometry()), self._screens[0])
        widgets.bring_to_front(front)
        front.setFocus()

    def cancel(self):
        for w in self._screens:
            w.close()
            w.deleteLater()
        self._screens = []

    def copy(self, got: tuple[int, int, float], distance: bool = False):
        w, h, dist = got
        text = f"{dist:.0f}" if distance else f"{w} × {h}"
        clip.set_text(text if distance else f"{w}x{h}")
        self.ctx.notify(self.title, f"{text} px copied", 1500)
