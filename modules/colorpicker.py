"""
Colour picker (eyedropper).

The hotkey grabs every screen once, then shows that frozen picture full-screen
with a loupe (11×11 pixels at 13×, ringed centre, HEX and RGB) that follows the
cursor. Click copies the colour in the chosen format; arrow keys nudge the
cursor 1 px; Esc or right-click cancels. The last 10 colours are kept (tray
menu and command palette).
"""
import os
import colorsys

from PyQt6.QtWidgets import QApplication, QWidget, QMenu
from PyQt6.QtCore import Qt, QRect, QRectF, QPoint, QTimer
from PyQt6.QtGui import QPainter, QPainterPath, QColor, QPen, QPixmap, QIcon

from core import DATA_DIR, theme, widgets, hypr, save_json, load_json, clip
from core.module import Module

SAVE_PATH = os.path.join(DATA_DIR, "colours.json")
ZOOM, CELLS = 13, 11
CARD_W, PAD, FOOT_H = 156, 6, 41


def colour_text(c: QColor, fmt: str, upper: bool = True) -> str:
    if fmt == "rgb":
        return f"rgb({c.red()}, {c.green()}, {c.blue()})"
    if fmt == "hsl":
        h, l, s = colorsys.rgb_to_hls(c.redF(), c.greenF(), c.blueF())
        return f"hsl({round(h * 360)}, {round(s * 100)}%, {round(l * 100)}%)"
    hx = c.name()
    return hx.upper() if upper else hx


class _PickScreen(QWidget):
    def __init__(self, m: "ColorPickerModule", screen, shot: QPixmap, parent=None):
        super().__init__(parent)
        self.m = m
        self.shot = shot
        self.img = shot.toImage()
        self.dpr = shot.devicePixelRatio() or 1.0
        self.setWindowFlags(Qt.WindowType.FramelessWindowHint | Qt.WindowType.WindowStaysOnTopHint | Qt.WindowType.Tool)
        self.setGeometry(screen.geometry())
        self.setMouseTracking(True)
        self.setCursor(Qt.CursorShape.CrossCursor)
        self._pos: QPoint | None = None

    def pixel(self, p: QPoint) -> QColor:
        x, y = int(p.x() * self.dpr), int(p.y() * self.dpr)
        x = max(0, min(self.img.width() - 1, x))
        y = max(0, min(self.img.height() - 1, y))
        return self.img.pixelColor(x, y)

    def _loupe_rect(self, p: QPoint) -> QRect:
        h = PAD * 2 + CELLS * ZOOM + FOOT_H
        x, y = p.x() + 30, p.y() + 30
        if x + CARD_W + 4 > self.width():
            x = p.x() - 30 - CARD_W
        if y + h + 4 > self.height():
            y = p.y() - 30 - h
        return QRect(x, y, CARD_W, h)

    def _hint_rect(self) -> QRect:
        return QRect((self.width() - 460) // 2, 22, 460, 32)

    def _paint_hint(self, p: QPainter):
        pal = theme.P
        r = self._hint_rect()
        p.setPen(QPen(QColor(pal.border), 1))
        p.setBrush(QColor(pal.bg_base))
        p.drawRoundedRect(QRectF(r).adjusted(0.5, 0.5, -0.5, -0.5), 16, 16)
        parts = [f"CLICK COPY {self.m.s('format', 'hex').upper()}", "ARROWS NUDGE", "ENTER PICK", "ESC"]
        f = theme.qfont(11, mono=True)
        p.setFont(f)
        fm = p.fontMetrics()
        dot = "  ·  "
        total = 13 + 16 + sum(fm.horizontalAdvance(t) for t in parts) + fm.horizontalAdvance(dot) * (len(parts) - 1)
        x = r.center().x() - total // 2
        p.drawPixmap(x, r.center().y() - 6, theme.sparkle_pixmap(13))
        x += 13 + 16
        for i, t in enumerate(parts):
            if i:
                p.setPen(QColor(pal.text_muted))
                p.drawText(QRect(x, r.y(), 200, r.height()), Qt.AlignmentFlag.AlignVCenter, dot)
                x += fm.horizontalAdvance(dot)
            p.setPen(QColor(pal.text_secondary))
            p.drawText(QRect(x, r.y(), 300, r.height()), Qt.AlignmentFlag.AlignVCenter, t)
            x += fm.horizontalAdvance(t)

    def paintEvent(self, e):
        p = QPainter(self)
        p.drawPixmap(self.rect(), self.shot)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        if self.screen() is QApplication.primaryScreen() or self._pos is not None:
            self._paint_hint(p)
        if self._pos is None:
            p.end()
            return
        pal = theme.P
        r = self._loupe_rect(self._pos)
        rad = pal.radius
        p.setPen(QPen(QColor(pal.border), 1))
        p.setBrush(QColor(pal.bg_base))
        p.drawRoundedRect(QRectF(r).adjusted(0.5, 0.5, -0.5, -0.5), rad, rad)
        zoom = QRect(r.x() + PAD, r.y() + PAD, CELLS * ZOOM, CELLS * ZOOM)
        cx, cy = int(self._pos.x() * self.dpr), int(self._pos.y() * self.dpr)
        src = QRect(cx - CELLS // 2, cy - CELLS // 2, CELLS, CELLS)
        p.save()
        clip = QPainterPath()
        clip.addRoundedRect(QRectF(zoom), max(0, rad - 5), max(0, rad - 5))
        p.setClipPath(clip)
        p.setRenderHint(QPainter.RenderHint.SmoothPixmapTransform, False)
        p.drawImage(zoom, self.img, src)
        p.restore()
        mid = QRect(zoom.x() + (CELLS // 2) * ZOOM, zoom.y() + (CELLS // 2) * ZOOM, ZOOM, ZOOM)
        p.setBrush(Qt.BrushStyle.NoBrush)
        p.setPen(QPen(QColor("#000000"), 1))
        p.drawRect(QRectF(mid).adjusted(-2.5, -2.5, 2.5, 2.5))
        p.setPen(QPen(QColor("#FFFFFF"), 1.5))
        p.drawRect(QRectF(mid).adjusted(-1, -1, 1, 1))
        c = self.pixel(self._pos)
        fy = zoom.bottom() + PAD + 1
        p.setPen(QPen(QColor(pal.border), 1))
        p.drawLine(r.x() + 1, fy, r.right() - 1, fy)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(c)
        p.drawRoundedRect(QRectF(r.x() + 10, fy + 8, 24, 24), 5, 5)
        p.setPen(QColor(pal.text_primary))
        p.setFont(theme.qfont(12, 600, mono=True))
        p.drawText(QRect(r.x() + 42, fy + 7, r.width() - 46, 14), Qt.AlignmentFlag.AlignVCenter, c.name().upper())
        p.setFont(theme.qfont(10, mono=True))
        p.setPen(QColor(pal.text_muted))
        p.drawText(QRect(r.x() + 42, fy + 22, r.width() - 46, 12), Qt.AlignmentFlag.AlignVCenter,
                   f"RGB {c.red()} {c.green()} {c.blue()}")
        p.end()

    def mouseMoveEvent(self, e):
        old = self._loupe_rect(self._pos).adjusted(-4, -4, 4, 4) if self._pos else QRect()
        self._pos = e.position().toPoint()
        self.update(old.united(self._loupe_rect(self._pos).adjusted(-4, -4, 4, 4)))

    def mousePressEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton:
            self.m.picked(self.pixel(e.position().toPoint()))
        else:
            self.m.cancel()

    def keyPressEvent(self, e):
        k = e.key()
        moves = {Qt.Key.Key_Left: (-1, 0), Qt.Key.Key_Right: (1, 0), Qt.Key.Key_Up: (0, -1), Qt.Key.Key_Down: (0, 1)}
        if k == Qt.Key.Key_Escape:
            self.m.cancel()
        elif k in moves:
            dx, dy = moves[k]
            step = 10 if e.modifiers() & Qt.KeyboardModifier.ShiftModifier else 1
            hypr.move_cursor(hypr.cursor_pos() + QPoint(dx * step, dy * step))
        elif k in (Qt.Key.Key_Return, Qt.Key.Key_Enter, Qt.Key.Key_Space):
            self.m.picked(self.pixel(self.mapFromGlobal(hypr.cursor_pos())))


class ColorPickerModule(Module):
    key = "colorpicker"
    title = "Colour Picker"
    icon = "🎯"

    def __init__(self, ctx):
        super().__init__(ctx)
        self._screens: list[_PickScreen] = []
        self.recent: list[str] = []
        self._on_pick = None            # set while Luminescent's colour picker borrows the eyedropper

    def start(self):
        d = load_json(SAVE_PATH, {})
        self.recent = [c for c in (d.get("recent", []) if isinstance(d, dict) else []) if QColor(c).isValid()][:10]
        self.hotkey("hotkey", self.pick, "pick a colour")

    def stop(self):
        self.cancel()

    def actions(self):
        return [("pick", "Pick a colour from the screen", self.pick)]

    def tray_menu(self, menu: QMenu):
        if not self.recent:
            return
        sub = menu.addMenu(f"{self.icon}  Recent colours")
        for c in self.recent:
            px = QPixmap(14, 14)
            px.fill(QColor(c))
            one = sub.addMenu(QIcon(px), self.fmt(QColor(c)))
            one.addAction("Copy", lambda col=c: self.copy(QColor(col)))
            one.addSeparator()
            one.addAction("Use as the theme's accent", lambda col=c: self.to_theme(col))
            one.addAction("New dark theme from it", lambda col=c: self.new_theme(col, False))
            one.addAction("New light theme from it", lambda col=c: self.new_theme(col, True))

    def palette_entries(self, Entry) -> list:
        out = []
        for c in self.recent:
            out.append(Entry(f"colour:{c}", f"Colour {self.fmt(QColor(c))}", lambda col=c: self.copy(QColor(col)),
                             "copy", "colour"))
            out.append(Entry(f"colour-accent:{c}", f"Accent colour {c}", lambda col=c: self.to_theme(col),
                             "use as the theme's accent", "colour"))
            out.append(Entry(f"colour-theme:{c}", f"New theme from {c}", lambda col=c: self.new_theme(col, False),
                             "a dark theme around this colour", "colour"))
        return out

    # ── into a theme ───────────────────────────────────────────────────────
    def to_theme(self, colour: str):
        """The active theme gets this colour as its accent."""
        launcher = self.ctx.launcher
        if launcher is None:
            return
        store = launcher.themes
        store.set_accent(store.active()["id"], colour)
        self.ctx.notify(self.title, f"{colour.upper()} is now the accent of {store.active()['name']}.", 2500)

    def new_theme(self, colour: str, light: bool):
        launcher = self.ctx.launcher
        if launcher is None:
            return
        t = launcher.themes.from_accent(colour, light, f"Picked {colour.upper()}")
        launcher.apply_theme(t["id"])
        self.ctx.notify(self.title, f"New theme “{t['name']}” (Themes page to rename or change it).", 3000)

    def fmt(self, c: QColor) -> str:
        return colour_text(c, self.s("format", "hex"), bool(self.s("uppercase", True)))

    # ── picking ────────────────────────────────────────────────────────────
    def pick(self, on_pick=None, parent=None):
        """Freeze the screens and let you click a colour. With `on_pick` the colour goes to on_pick("#RRGGBB")
        (None when cancelled) instead of the clipboard; `parent` keeps the screens usable over a modal window."""
        if self._screens:
            return
        self._on_pick = on_pick
        for scr in QApplication.screens():
            w = _PickScreen(self, scr, widgets.grab_screen(scr), parent)
            if parent is not None:
                w.setWindowFlags(Qt.WindowType.FramelessWindowHint | Qt.WindowType.WindowStaysOnTopHint |
                                 Qt.WindowType.Tool)
                w.setGeometry(scr.geometry())
            self._screens.append(w)
        for w in self._screens:
            w.show()
            w.raise_()
        here = widgets.cursor_screen()
        front = next((w for w in self._screens if here and w.geometry() == here.geometry()), self._screens[0])
        widgets.bring_to_front(front)
        front.setFocus()
        pos = front.mapFromGlobal(hypr.cursor_pos())
        if front.rect().contains(pos):
            front._pos = pos
            front.update()

    def cancel(self):
        for w in self._screens:
            w.close()
            w.deleteLater()
        self._screens = []
        cb, self._on_pick = self._on_pick, None
        if cb is not None:
            cb(None)

    def picked(self, c: QColor):
        cb, self._on_pick = self._on_pick, None
        self.cancel()
        name = c.name().upper()
        if cb is not None:
            cb(name)
        else:
            self.copy(c)
        self.recent = [name] + [r for r in self.recent if r != name][:9]
        try:
            save_json(SAVE_PATH, {"recent": self.recent})
        except OSError:
            pass
        if self.ctx.launcher is not None:
            QTimer.singleShot(0, self.ctx.launcher._build_menu)      # refresh the Recent colours submenu

    def copy(self, c: QColor):
        text = self.fmt(c)
        clip.set_text(text)
        self.ctx.notify(self.title, f"{text} copied", 2000)
