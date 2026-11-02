"""
System Settings: the computer's own settings in one panel, on Linux and Windows.

  * Displays: every monitor drawn where it is. Drag one to move it (it snaps against the others), pick its
    resolution, refresh rate, scale, rotation, main screen and variable refresh, switch it off, show a big
    number on each screen. Changes ask "keep these?" and go back by themselves after 15 seconds, in case a
    screen goes black.
  * Brightness (DDC/CI for monitors, the backlight for laptops) and night light.
  * Mouse, touchpad, keyboard, windows & desktop (Hyprland), appearance, Explorer & taskbar (Windows), power.
  * Sound devices and volume, and what this computer is.

The talking to the system is in core/syscontrol.py. Hyprland forgets runtime changes when its config reloads,
so with "Keep my changes" on, what you changed here goes back on after a reload and when Luminescent starts.
"""
import threading
import logging

from PyQt6.QtWidgets import (QWidget, QVBoxLayout, QHBoxLayout, QGridLayout, QStackedWidget, QFrame, QPushButton,
                             QButtonGroup, QCheckBox, QComboBox, QSlider, QLineEdit, QScrollArea, QApplication,
                             QSizePolicy)
from PyQt6.QtCore import Qt, QTimer, QObject, QEvent, QRectF, QPointF, pyqtSignal
from PyQt6.QtGui import QPainter, QColor, QPen, QFont

from core import IS_WIN, theme, widgets, hypr, syscontrol as sc
from core.guard import alive
from core.module import Module

log = logging.getLogger("syscontrol")


# ── a worker thread that answers on the UI thread ────────────────────────────────
class _Bridge(QObject):
    done = pyqtSignal(object, object)


_bridge: _Bridge | None = None


def in_thread(fn, then=None):
    """Run fn() on a worker thread; then(result) runs back on the UI thread."""
    global _bridge
    if _bridge is None:
        _bridge = _Bridge()
        _bridge.done.connect(lambda cb, r: cb(r) if cb else None)

    def work():
        try:
            r = fn()
        except Exception as e:
            log.exception("system settings worker failed")
            r = f"{type(e).__name__}: {e}"
        _bridge.done.emit(then, r)
    threading.Thread(target=work, daemon=True, name="syscontrol").start()


class _NoWheel(QObject):
    """Scrolling the page over a slider or a list must not change a system setting: they only take the wheel
    after being clicked."""

    def eventFilter(self, obj, e):
        if e.type() == QEvent.Type.Wheel and not obj.hasFocus():
            page = obj.parentWidget()
            while page is not None and not isinstance(page, QScrollArea):
                page = page.parentWidget()
            if page is not None:                     # the page scrolls instead
                QApplication.sendEvent(page.verticalScrollBar(), e)
            return True
        return False


_no_wheel: _NoWheel | None = None


def calm(*ctls):
    global _no_wheel
    if _no_wheel is None:
        _no_wheel = _NoWheel()
    for c in ctls:
        c.setFocusPolicy(Qt.FocusPolicy.StrongFocus)
        c.installEventFilter(_no_wheel)


def _hint(text: str) -> QWidget:
    lbl = widgets.label(text, 12, tone="muted")
    lbl.setWordWrap(True)
    return lbl


def _heading(text: str) -> QWidget:
    return widgets.label(text.upper(), 9.5, 600, mono=True, spacing=0.14, tone="muted")


def _short(name: str) -> str:
    return name.replace("\\\\.\\", "").replace("\\.\\", "")


# ══ the monitor map ═══════════════════════════════════════════════════════════════
class MonitorMap(QWidget):
    """The monitors as boxes where they are. Click to pick one, drag to move it (it snaps to the others)."""
    selected = pyqtSignal(int)
    moved = pyqtSignal()

    def __init__(self):
        super().__init__()
        self.mons: list[dict] = []
        self.current = 0
        self._drag = None              # (index, grab offset in layout px)
        self._frozen = None            # the view while dragging, so it doesn't jump around
        self.setMinimumHeight(230)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        self.setMouseTracking(True)

    def set_monitors(self, mons: list[dict], current: int = 0):
        self.mons = mons
        self.current = max(0, min(current, len(mons) - 1))
        self.update()

    # layout ↔ widget
    def _rects(self) -> list[tuple[int, int, int, int] | None]:
        out = []
        for m in self.mons:
            if not m.get("enabled", True):
                out.append(None)
                continue
            w, h = sc.layout_size(m)
            out.append((int(m["x"]), int(m["y"]), w, h))
        return out

    def _view(self):
        if self._frozen:
            return self._frozen
        rects = [r for r in self._rects() if r]
        off_row = 54 if any(not m.get("enabled", True) for m in self.mons) else 0
        if not rects:
            return 0.0, 0.0, 1.0, off_row
        x0 = min(r[0] for r in rects)
        y0 = min(r[1] for r in rects)
        x1 = max(r[0] + r[2] for r in rects)
        y1 = max(r[1] + r[3] for r in rects)
        pad = 24
        avail_w, avail_h = max(10, self.width() - 2 * pad), max(10, self.height() - 2 * pad - off_row)
        s = min(avail_w / max(1, x1 - x0), avail_h / max(1, y1 - y0))
        ox = pad + (avail_w - (x1 - x0) * s) / 2 - x0 * s
        oy = pad + (avail_h - (y1 - y0) * s) / 2 - y0 * s
        return ox, oy, s, off_row

    def _box(self, i: int) -> QRectF:
        r = self._rects()[i]
        ox, oy, s, off_row = self._view()
        if r is None:                                   # switched off: a small box in the row at the bottom
            k = sum(1 for m in self.mons[:i] if not m.get("enabled", True))
            return QRectF(24 + k * 128, self.height() - off_row + 4, 118, 40)
        return QRectF(ox + r[0] * s, oy + r[1] * s, r[2] * s, r[3] * s).adjusted(2, 2, -2, -2)

    def paintEvent(self, e):
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        P = theme.P
        p.setPen(QPen(QColor(P.border), 1))
        p.setBrush(QColor(P.bg_frame))
        p.drawRoundedRect(QRectF(self.rect()).adjusted(0.5, 0.5, -0.5, -0.5), P.radius, P.radius)
        if not self.mons:
            p.setPen(QColor(P.text_muted))
            p.drawText(self.rect(), Qt.AlignmentFlag.AlignCenter, "No monitors could be read here.")
            p.end()
            return
        if any(not m.get("enabled", True) for m in self.mons):
            p.setPen(QColor(P.text_muted))
            p.setFont(theme.qfont(9, 600, True, 0.12))
            _ox, _oy, _s, off_row = self._view()
            p.drawText(QRectF(24, self.height() - off_row - 14, 300, 16), Qt.AlignmentFlag.AlignLeft, "SWITCHED OFF")
        for i, m in enumerate(self.mons):
            box = self._box(i)
            on = m.get("enabled", True)
            sel = i == self.current
            fill = QColor(P.primary_muted if sel else P.bg_card)
            if not on:
                fill.setAlpha(140)
            pen = QPen(QColor(P.primary if sel else P.border), 2 if sel else 1)
            if not on:
                pen.setStyle(Qt.PenStyle.DashLine)
            p.setPen(pen)
            p.setBrush(fill)
            p.drawRoundedRect(box, 8, 8)
            p.setPen(QColor(P.text_primary if on else P.text_muted))
            if on and box.height() > 50:
                p.setFont(theme.qfont(max(14, min(40, box.height() * 0.28)), 700))
                p.drawText(box.adjusted(0, 0, 0, -box.height() * 0.28), Qt.AlignmentFlag.AlignCenter, str(i + 1))
                p.setFont(theme.qfont(10, 500))
                sub = f"{_short(m['name'])}  ·  {m['width']}×{m['height']}"
                if m.get("primary"):
                    sub = "★ " + sub
                p.setPen(QColor(P.text_secondary))
                p.drawText(box.adjusted(6, box.height() * 0.5, -6, -6), Qt.AlignmentFlag.AlignCenter |
                           Qt.TextFlag.TextWordWrap, sub)
            else:
                p.setFont(theme.qfont(10, 600))
                p.drawText(box, Qt.AlignmentFlag.AlignCenter, f"{i + 1}  {_short(m['name'])}")
        p.end()

    def _hit(self, pos) -> int:
        for i in reversed(range(len(self.mons))):
            if self._box(i).contains(QPointF(pos)):
                return i
        return -1

    def mousePressEvent(self, e):
        i = self._hit(e.position())
        if i < 0:
            return
        self.current = i
        self.selected.emit(i)
        if self.mons[i].get("enabled", True):
            ox, oy, s, _ = self._view()
            self._frozen = (ox, oy, s, self._view()[3])
            lx, ly = (e.position().x() - ox) / s, (e.position().y() - oy) / s
            self._drag = (i, lx - self.mons[i]["x"], ly - self.mons[i]["y"])
            self.setCursor(Qt.CursorShape.ClosedHandCursor)
        self.update()

    def mouseMoveEvent(self, e):
        if self._drag is None:
            self.setCursor(Qt.CursorShape.OpenHandCursor if self._hit(e.position()) >= 0 else
                           Qt.CursorShape.ArrowCursor)
            return
        i, gx, gy = self._drag
        ox, oy, s, _ = self._frozen
        self.mons[i]["x"] = round((e.position().x() - ox) / s - gx)
        self.mons[i]["y"] = round((e.position().y() - oy) / s - gy)
        self.update()

    def mouseReleaseEvent(self, e):
        if self._drag is None:
            return
        i = self._drag[0]
        self._drag = None
        self._frozen = None
        self.setCursor(Qt.CursorShape.OpenHandCursor)
        place(self.mons, i)
        self.update()
        self.moved.emit()


def _overlap(a, b) -> bool:
    return a[0] < b[0] + b[2] and b[0] < a[0] + a[2] and a[1] < b[1] + b[3] and b[1] < a[1] + a[3]


def place(mons: list[dict], i: int):
    """Put monitor i against the nearest side of another one, lined up with its edges when it is close,
    never overlapping. Then move everything so the top-left is 0, 0."""
    me = mons[i]
    w, h = sc.layout_size(me)
    x0, y0 = int(me["x"]), int(me["y"])
    others = [(int(m["x"]), int(m["y"]), *sc.layout_size(m)) for j, m in enumerate(mons)
              if j != i and m.get("enabled", True)]
    if others and me.get("enabled", True):
        thr = max(w, h) * 0.08
        best = None
        for ox, oy, ow, oh in others:
            for side in "lrtb":
                if side in "lr":
                    x = ox - w if side == "l" else ox + ow
                    y = max(oy - h + 1, min(y0, oy + oh - 1))
                    for edge in (oy, oy + oh - h, oy + (oh - h) // 2):
                        if abs(y - edge) < thr:
                            y = edge
                            break
                else:
                    y = oy - h if side == "t" else oy + oh
                    x = max(ox - w + 1, min(x0, ox + ow - 1))
                    for edge in (ox, ox + ow - w, ox + (ow - w) // 2):
                        if abs(x - edge) < thr:
                            x = edge
                            break
                if any(_overlap((x, y, w, h), o) for o in others):
                    continue
                d = (x - x0) ** 2 + (y - y0) ** 2
                if best is None or d < best[0]:
                    best = (d, x, y)
        if best:
            me["x"], me["y"] = int(best[1]), int(best[2])
    normalise(mons)


def normalise(mons: list[dict]):
    on = [m for m in mons if m.get("enabled", True)]
    if not on:
        return
    dx, dy = min(int(m["x"]) for m in on), min(int(m["y"]) for m in on)
    for m in on:
        m["x"], m["y"] = int(m["x"]) - dx, int(m["y"]) - dy


def side_by_side(mons: list[dict]):
    x = 0
    for m in sorted((m for m in mons if m.get("enabled", True)), key=lambda m: (int(m["x"]), int(m["y"]))):
        m["x"], m["y"] = x, 0
        x += sc.layout_size(m)[0]


class _Identify(QWidget):
    """A big number in the middle of one screen for a few seconds."""

    def __init__(self, number: int, name: str, geom):
        super().__init__(None, widgets._TOP_TOOL)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating)
        self.setAttribute(Qt.WidgetAttribute.WA_DeleteOnClose)
        self.setWindowTitle("Luminescent identify")
        self.number, self.name = number, name
        self.resize(260, 220)
        self.move(geom.center().x() - 130, geom.center().y() - 110)
        QTimer.singleShot(3000, self.close)

    def paintEvent(self, e):
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        P = theme.P
        p.setPen(QPen(QColor(P.primary), 3))
        p.setBrush(QColor(P.bg_base))
        p.drawRoundedRect(QRectF(self.rect()).adjusted(2, 2, -2, -2), 24, 24)
        p.setPen(QColor(P.text_primary))
        p.setFont(theme.qfont(96, 700))
        p.drawText(QRectF(0, 0, self.width(), self.height() - 50), Qt.AlignmentFlag.AlignCenter, str(self.number))
        p.setFont(theme.qfont(13, 500))
        p.setPen(QColor(P.text_secondary))
        p.drawText(QRectF(0, self.height() - 60, self.width(), 40), Qt.AlignmentFlag.AlignCenter, self.name)
        p.end()


def identify(mons: list[dict]):
    """Show each monitor's number on the screen itself."""
    screens = QApplication.screens()
    for i, m in enumerate(mons):
        if not m.get("enabled", True):
            continue
        scr = next((s for s in screens if s.name() == m["name"]), None) or \
            next((s for s in screens if s.geometry().topLeft().x() == int(m["x"]) and
                  s.geometry().topLeft().y() == int(m["y"])), None)
        if scr is None:
            continue
        w = _Identify(i + 1, _short(m["name"]), scr.geometry())
        w.show()


class KeepDialog(widgets.RoundedDialog):
    """"Keep these display settings?" that answers No by itself after a while."""

    def __init__(self, parent, seconds: int = 15):
        super().__init__(parent, "Displays")
        self.left = seconds
        self.msg = widgets.label("", 13.5, tone="")
        self.msg.setWordWrap(True)
        self.body.addWidget(self.msg)
        row = QHBoxLayout()
        row.addStretch()
        back = widgets.button("Go back")
        back.clicked.connect(self.reject)
        keep = widgets.button("Keep them", "primary")
        keep.clicked.connect(self.accept)
        keep.setDefault(True)
        row.addWidget(back)
        row.addWidget(keep)
        self.body.addLayout(row)
        self._tick()
        self._timer = QTimer(self)
        self._timer.timeout.connect(self._tick)
        self._timer.start(1000)
        self.setFixedWidth(420)

    def _tick(self):
        if self.left <= 0:
            self.reject()
            return
        self.msg.setText(f"Keep these display settings?\n\nGoing back to how they were in {self.left} s.")
        self.left -= 1


# ══ pages ════════════════════════════════════════════════════════════════════════
class _Page(QScrollArea):
    def __init__(self, title: str, sub: str = ""):
        super().__init__()
        self.setWidgetResizable(True)
        self.setFrameShape(QFrame.Shape.NoFrame)
        inner = QWidget()
        self.lay = QVBoxLayout(inner)
        self.lay.setContentsMargins(20, 16, 20, 20)
        self.lay.setSpacing(10)
        self.lay.addWidget(widgets.label(title, 18, 600))
        if sub:
            self.lay.addWidget(_hint(sub))
        self.setWidget(inner)

    def finish(self):
        self.lay.addStretch()


class DisplaysPage(_Page):
    def __init__(self, m: "SystemModule"):
        super().__init__("Displays", "Drag a screen to move it; it snaps against the others. Changes show after "
                                     "Apply, and go back by themselves unless you keep them.")
        self.m = m
        self.mons: list[dict] = []
        self.map = MonitorMap()
        self.map.selected.connect(self._pick)
        self.map.moved.connect(self._dirty)
        self.lay.addWidget(self.map, 1)

        form = QGridLayout()
        form.setHorizontalSpacing(14)
        form.setVerticalSpacing(8)
        self.title = widgets.label("", 14, 600)
        self.sub = widgets.label("", 11.5, tone="muted")
        form.addWidget(self.title, 0, 0, 1, 4)
        form.addWidget(self.sub, 1, 0, 1, 4)
        self.enabled = QCheckBox("On")
        self.primary = QCheckBox("Main display")
        self.primary.setToolTip("Where the taskbar and new windows go.")
        self.res = QComboBox()
        self.rate = QComboBox()
        self.scale = QComboBox()
        for s in sc.SCALES:
            self.scale.addItem(f"{round(s * 100)} %", s)
        self.rot = QComboBox()
        for v, t in sc.ROTATIONS:
            self.rot.addItem(t, v)
        self.vrr = QComboBox()
        for v, t in ((-1, "As Hyprland is set"), (0, "Off"), (1, "On"), (2, "Fullscreen only")):
            self.vrr.addItem(t, v)
        self.mirror = QComboBox()
        rows = [("Resolution", self.res), ("Refresh rate", self.rate), ("Rotation", self.rot)]
        if sc.can_scale():
            rows.insert(2, ("Scale", self.scale))
        if sc.has_vrr():
            rows.append(("Variable refresh", self.vrr))
        if sc.backend() == "hyprland":
            rows.append(("Mirror", self.mirror))
        calm(self.res, self.rate, self.scale, self.rot, self.vrr, self.mirror)
        for n, (text, w) in enumerate(rows):
            r, c = 2 + n // 2, (n % 2) * 2
            form.addWidget(widgets.label(text, 12, tone="secondary"), r, c)
            form.addWidget(w, r, c + 1)
            w.setMinimumWidth(150)
        checks = QHBoxLayout()
        checks.addWidget(self.enabled)
        if sc.has_primary():
            checks.addWidget(self.primary)
        checks.addStretch()
        form.addLayout(checks, 2 + (len(rows) + 1) // 2, 0, 1, 4)
        form.setColumnStretch(1, 1)
        form.setColumnStretch(3, 1)
        self.lay.addLayout(form)

        buttons = QHBoxLayout()
        ident = widgets.button("Identify", tooltip="Show each screen's number on the screen itself")
        ident.clicked.connect(lambda: identify(self.mons))
        line = widgets.button("Side by side", tooltip="Put every screen in a row, in the order they are now")
        line.clicked.connect(self._line_up)
        copy = widgets.button("Copy as config", tooltip="These monitors as lines for your config file, to keep "
                                                        "them for good")
        copy.clicked.connect(self._copy)
        self.undo = widgets.button("Undo", tooltip="Forget the changes you haven't applied")
        self.undo.clicked.connect(self.reload)
        self.apply = widgets.button("Apply", "primary")
        self.apply.clicked.connect(self._apply)
        for b in (ident, line, copy):
            buttons.addWidget(b)
        buttons.addStretch()
        buttons.addWidget(self.undo)
        buttons.addWidget(self.apply)
        self.lay.addLayout(buttons)
        if not sc.can_change_monitors():
            self.lay.addWidget(_hint("Changing monitors needs Hyprland, an X11 session or Windows. They are shown "
                                     "here as they are."))
            self.apply.setEnabled(False)
        elif sc.backend() == "hyprland":
            self.forget = widgets.button("Forget my display changes",
                                         tooltip="Stop putting them back after a restart; Hyprland's config "
                                                 "decides again")
            self.forget.clicked.connect(self._forget)
            keep = QHBoxLayout()
            keep.addWidget(_hint("Hyprland's config decides at login. With Settings → System Settings → Keep my "
                                 "changes on, Luminescent puts these back after a reload or a restart. For good: "
                                 "Copy as config and paste it into monitors.lua."), 1)
            keep.addWidget(self.forget, 0, Qt.AlignmentFlag.AlignTop)
            self.lay.addLayout(keep)
        self.finish()

        self.enabled.toggled.connect(self._form_changed)
        self.primary.toggled.connect(self._form_changed)
        self.res.activated.connect(self._res_changed)
        for w in (self.rate, self.scale, self.rot, self.vrr, self.mirror):
            w.activated.connect(self._form_changed)
        self._filling = False
        self.reload()

    def reload(self):
        self.mons = sc.monitors()
        cur = next((i for i, m in enumerate(self.mons) if m.get("focused") or m.get("primary")), 0)
        self.map.set_monitors(self.mons, cur)
        self._pick(self.map.current)
        self.undo.setEnabled(False)

    def _dirty(self):
        self.undo.setEnabled(True)
        self.map.update()
        self._pick(self.map.current)

    def _pick(self, i: int):
        if not self.mons:
            self.title.setText("No monitors")
            return
        m = self.mons[i]
        self._filling = True
        self.title.setText(f"{i + 1}.  {m['label'] or _short(m['name'])}")
        self.sub.setText(f"{_short(m['name'])}  ·  at {m['x']}, {m['y']}  ·  "
                         f"{m['width']}×{m['height']} @ {float(m['refresh']):g} Hz")
        self.enabled.setChecked(m.get("enabled", True))
        self.primary.setChecked(bool(m.get("primary")))
        sizes = []
        for w, h, _hz in m["modes"]:
            if (w, h) not in sizes:
                sizes.append((w, h))
        if (m["width"], m["height"]) not in sizes:
            sizes.insert(0, (m["width"], m["height"]))
        self.res.clear()
        self._sizes = sizes
        for w, h in sizes:
            self.res.addItem(f"{w} × {h}")
        self.res.setCurrentIndex(sizes.index((m["width"], m["height"])))
        self._fill_rates(m)
        self.scale.setCurrentIndex(min(range(len(sc.SCALES)), key=lambda k: abs(sc.SCALES[k] - float(m["scale"]))))
        self.rot.setCurrentIndex(int(m.get("transform") or 0) % 4)
        self.vrr.setCurrentIndex(max(0, self.vrr.findData(-1 if m.get("vrr") is None else int(m["vrr"]))))
        self.mirror.clear()
        self.mirror.addItem("Nothing (its own desktop)", "")
        for o in self.mons:
            if o is not m:
                self.mirror.addItem(f"Same as {_short(o['name'])}", o["name"])
        self.mirror.setCurrentIndex(max(0, self.mirror.findData(m.get("mirror") or "")))
        for w in (self.res, self.rate, self.scale, self.rot, self.vrr, self.mirror, self.primary):
            w.setEnabled(m.get("enabled", True))
        self._filling = False

    def _fill_rates(self, m: dict):
        rates = sorted({hz for w, h, hz in m["modes"] if (w, h) == (m["width"], m["height"])}, reverse=True)
        if float(m["refresh"]) not in rates:
            rates.insert(0, float(m["refresh"]))
        self.rate.clear()
        for hz in rates:
            self.rate.addItem(f"{hz:g} Hz", hz)
        self.rate.setCurrentIndex(rates.index(float(m["refresh"])))

    def _res_changed(self):
        if self._filling or not self.mons:
            return
        m = self.mons[self.map.current]
        w, h = self._sizes[self.res.currentIndex()]
        rates = sorted({hz for mw, mh, hz in m["modes"] if (mw, mh) == (w, h)}, reverse=True)
        self._resize(m, lambda: m.update(width=w, height=h, refresh=rates[0] if rates else m["refresh"]))
        self._pick(self.map.current)
        self._dirty()

    def _resize(self, m: dict, change):
        """Change the monitor's size and move the screens right of it and below it along with its edges."""
        ow, oh = sc.layout_size(m)
        right, bottom = int(m["x"]) + ow, int(m["y"]) + oh
        change()
        nw, nh = sc.layout_size(m)
        for o in self.mons:
            if o is m or not o.get("enabled", True):
                continue
            if int(o["x"]) >= right:
                o["x"] = int(o["x"]) + nw - ow
            if int(o["y"]) >= bottom:
                o["y"] = int(o["y"]) + nh - oh
        normalise(self.mons)

    def _form_changed(self, *_):
        if self._filling or not self.mons:
            return
        i = self.map.current
        m = self.mons[i]
        on = self.enabled.isChecked()
        if not on and not any(o.get("enabled", True) for j, o in enumerate(self.mons) if j != i):
            self._filling = True
            self.enabled.setChecked(True)
            self._filling = False
            self.m.ctx.notify("Displays", "At least one screen has to stay on.")
            return
        was_on = m.get("enabled", True)
        self._resize(m, lambda: m.update(scale=self.scale.currentData(), transform=self.rot.currentData()))
        m["refresh"] = self.rate.currentData() or m["refresh"]
        m["vrr"] = None if self.vrr.currentData() == -1 else self.vrr.currentData()
        m["mirror"] = self.mirror.currentData() or ""
        m["enabled"] = on
        if self.primary.isChecked() and not m.get("primary"):
            for o in self.mons:
                o["primary"] = o is m
        elif not self.primary.isChecked():
            m["primary"] = False
        if on and not was_on:
            place(self.mons, i)                       # switched back on: put it next to the others
        normalise(self.mons)
        self._dirty()

    def _line_up(self):
        side_by_side(self.mons)
        self._dirty()

    def _copy(self):
        QApplication.clipboard().setText(sc.config_text(self.mons))
        self.m.ctx.notify("Displays", "Copied. Paste it into your config to keep it.")

    def _apply(self):
        before = sc.monitors()
        want = [dict(m) for m in self.mons]
        err = sc.apply_monitors(want)
        if err:
            widgets.error(self, "Displays", f"The displays could not be changed:\n{err}")
            sc.apply_monitors(before)
            self.reload()
            return
        if KeepDialog(self.window()).exec():
            self.m.remember_monitors(want)
            self.m.ctx.notify("Displays", "Display settings kept.")
        else:
            sc.apply_monitors(before)
        QTimer.singleShot(400, self.reload)

    def _forget(self):
        self.m.remember_monitors(None)
        self.m.ctx.notify("Displays", "Your display changes won't be put back any more. Hyprland's config "
                                      "decides after its next reload.")


class OptionsPage(_Page):
    """One group of settings from core.syscontrol, a row each."""

    def __init__(self, m: "SystemModule", title: str, opts: list):
        sub = {"desktop": "Gaps, borders, corners, see-through windows and blur, as Hyprland draws them.",
               "explorer": "File Explorer and the taskbar. Explorer may take a moment to catch up.",
               "power": "How the computer balances speed against heat, noise and battery."}.get(opts[0].group, "")
        super().__init__(title, sub)
        self.m = m
        self.rows = {}
        for o in opts:
            self._row(o)
        if any(o.keep for o in opts):
            row = QHBoxLayout()
            row.addWidget(_hint("Hyprland forgets these when its config reloads. With Settings → System Settings → "
                                "Keep my changes on, Luminescent puts them back."), 1)
            undo = widgets.button("Back to my Hyprland config",
                                  tooltip="Forget everything changed here and reload Hyprland's config")
            undo.clicked.connect(self._reset_hypr)
            row.addWidget(undo, 0, Qt.AlignmentFlag.AlignTop)
            self.lay.addSpacing(6)
            self.lay.addLayout(row)
        self.finish()

    def _row(self, o: sc.Option):
        box = QFrame()
        box.setObjectName("lumCard")
        lay = QVBoxLayout(box)
        lay.setContentsMargins(14, 10, 14, 10)
        lay.setSpacing(4)
        top = QHBoxLayout()
        top.setSpacing(10)
        value = o.get()
        if o.kind == "bool":
            ctl = QCheckBox(o.label)
            ctl.setChecked(bool(value))
            ctl.toggled.connect(lambda v, o=o: self._set(o, bool(v)))
            top.addWidget(ctl, 1)
        else:
            top.addWidget(widgets.label(o.label, 13), 1)
            if o.kind == "choice":
                ctl = QComboBox()
                for v, t in o.choices:
                    ctl.addItem(t, v)
                if value is not None and ctl.findData(value) < 0:
                    ctl.addItem(str(value), value)
                ctl.setCurrentIndex(max(0, ctl.findData(value)))
                ctl.activated.connect(lambda _i, o=o, c=ctl: self._set(o, c.currentData()))
                ctl.setMinimumWidth(200)
                top.addWidget(ctl)
            elif o.kind == "text":
                ctl = QLineEdit(str(value or ""))
                ctl.setMinimumWidth(200)
                ctl.editingFinished.connect(lambda o=o, c=ctl: self._set(o, c.text().strip()))
                top.addWidget(ctl)
            else:
                ctl = self._slider(o, value)
                top.addWidget(ctl.readout)
        lay.addLayout(top)
        if o.kind in ("choice", "int", "float"):
            calm(ctl)
        if o.kind in ("int", "float"):
            lay.addWidget(ctl)
        if o.help:
            lay.addWidget(_hint(o.help))
        if value is None:
            box.setEnabled(False)
            box.setToolTip("Could not be read on this computer.")
        self.rows[o.key] = ctl
        self.lay.addWidget(box)

    def _slider(self, o: sc.Option, value) -> QSlider:
        step = o.step or 1
        s = QSlider(Qt.Orientation.Horizontal)
        s.setRange(round(o.min / step), round(o.max / step))
        s.setValue(round(float(value or 0) / step))
        s.readout = widgets.label("", 12, 500, mono=True)

        def shown(v):
            x = v * step
            s.readout.setText((f"{x:.2f}".rstrip("0").rstrip(".") if o.kind == "float" else str(int(round(x))))
                              + o.suffix)
        shown(s.value())
        timer = QTimer(s)
        timer.setSingleShot(True)
        timer.setInterval(250)
        timer.timeout.connect(lambda: self._set(o, round(s.value() * step, 4) if o.kind == "float"
                                                else int(round(s.value() * step))))
        s.valueChanged.connect(lambda v: (shown(v), timer.start()))
        return s

    def _set(self, o: sc.Option, value):
        def done(err):
            if not alive(self):
                return
            if err:
                self.m.ctx.notify("System Settings", f"{o.label}: {err}")
                return
            if o.keep:
                self.m.remember(o.key, value)
        in_thread(lambda: o.set(value), done)

    def _reset_hypr(self):
        self.m.forget_options()
        hypr.request("reload")
        self.m.ctx.notify("System Settings", "Hyprland reloaded its config.")
        QTimer.singleShot(800, lambda: self.m.panel_reload_options())


class ScreenPage(_Page):
    """Brightness of every screen that allows it, and night light."""

    def __init__(self, m: "SystemModule"):
        super().__init__("Brightness & night light")
        self.m = m
        self.lay.addWidget(_heading("Brightness"))
        self.bright = QVBoxLayout()
        self.bright.setSpacing(8)
        self.loading = _hint("Asking the screens…" + ("" if IS_WIN else " (monitors answer over DDC/CI, which "
                                                                       "takes a second)"))
        self.bright.addWidget(self.loading)
        self.lay.addLayout(self.bright)
        self.lay.addSpacing(10)

        self.lay.addWidget(_heading("Night light"))
        tool = sc.NightLight.tool()
        if not tool:
            self.lay.addWidget(_hint("Needs hyprsunset, gammastep or wlsunset (sudo pacman -S hyprsunset)."))
        else:
            row = QHBoxLayout()
            self.night_on = QCheckBox("Warmer colours, easier on the eyes at night")
            row.addWidget(self.night_on, 1)
            self.night_read = widgets.label("", 12, 500, mono=True)
            row.addWidget(self.night_read)
            self.lay.addLayout(row)
            self.night = QSlider(Qt.Orientation.Horizontal)
            self.night.setRange(20, 64)                      # hundreds of kelvin
            calm(self.night)
            k = m.night.kelvin if m.night.kelvin < 6500 else int(m.s("night_kelvin", 4000) or 4000)
            self.night.setValue(max(20, min(64, k // 100)))
            self.night_on.setChecked(m.night.kelvin < 6500)
            self.lay.addWidget(self.night)
            presets = QHBoxLayout()
            for k, t in ((5500, "A little"), (4500, "Evening"), (3500, "Night"), (2700, "Candle")):
                b = widgets.button(t, "pill")
                b.clicked.connect(lambda _c=False, k=k: (self.night.setValue(k // 100), self.night_on.setChecked(True),
                                                         self._night()))
                presets.addWidget(b)
            presets.addStretch()
            self.lay.addLayout(presets)
            self.lay.addWidget(_hint(f"Uses {tool}." if not IS_WIN else
                                     "Changes the screen's colour ramp. Windows' own Night light is in Settings → "
                                     "Display."))
            self._night_timer = QTimer(self)
            self._night_timer.setSingleShot(True)
            self._night_timer.setInterval(300)
            self._night_timer.timeout.connect(self._night)
            self.night.valueChanged.connect(lambda v: (self._show_k(), self._night_timer.start()
                                                       if self.night_on.isChecked() else None))
            self.night_on.toggled.connect(lambda _v: self._night())
            self._show_k()
        self.finish()
        in_thread(sc.brightness_list, self._got_screens)

    def _show_k(self):
        self.night_read.setText(f"{self.night.value() * 100} K")

    def _night(self):
        k = self.night.value() * 100 if self.night_on.isChecked() else 6500
        err = self.m.set_night(k)
        if err:
            self.m.ctx.notify("Night light", err)

    def _got_screens(self, screens):
        if not alive(self):
            return
        self.loading.hide()
        if not isinstance(screens, list) or not screens:
            self.bright.addWidget(_hint("No screen here lets its brightness be changed. Monitors need DDC/CI "
                                        "switched on in their own menu" + (
                                            "" if IS_WIN else ", ddcutil installed and the i2c-dev module "
                                                              "loaded (sudo modprobe i2c-dev)") + "."))
            return
        for s in screens:
            row = QHBoxLayout()
            name = widgets.label(s["label"] + (f"  ·  {_short(s['monitor'])}" if s.get("monitor") else ""), 13)
            name.setMinimumWidth(220)
            row.addWidget(name)
            sl = QSlider(Qt.Orientation.Horizontal)
            sl.setRange(0, 100)
            calm(sl)
            sl.setValue(int(s["value"] or 0))
            sl.setEnabled(s["value"] is not None)
            val = widgets.label(f"{sl.value()} %", 12, 500, mono=True)
            val.setMinimumWidth(48)
            timer = QTimer(sl)
            timer.setSingleShot(True)
            timer.setInterval(350)
            timer.timeout.connect(lambda sid=s["id"], sl=sl: in_thread(
                lambda: sc.set_brightness(sid, sl.value()),
                lambda err: self.m.ctx.notify("Brightness", err) if err else None))
            sl.valueChanged.connect(lambda v, val=val, t=timer: (val.setText(f"{v} %"), t.start()))
            row.addWidget(sl, 1)
            row.addWidget(val)
            self.bright.addLayout(row)


class SoundPage(_Page):
    def __init__(self, m: "SystemModule"):
        super().__init__("Sound")
        self.m = m
        from core import system
        self.system = system
        grid = QGridLayout()
        grid.setHorizontalSpacing(14)
        grid.setVerticalSpacing(10)
        self.out = QComboBox()
        self.inp = QComboBox()
        grid.addWidget(widgets.label("Output (speakers, headphones)", 12, tone="secondary"), 0, 0)
        grid.addWidget(self.out, 0, 1)
        grid.addWidget(widgets.label("Input (microphone)", 12, tone="secondary"), 1, 0)
        grid.addWidget(self.inp, 1, 1)
        grid.setColumnStretch(1, 1)
        self.lay.addLayout(grid)
        self.out.activated.connect(lambda _i: self._pick("sinks", self.out))
        self.inp.activated.connect(lambda _i: self._pick("sources", self.inp))

        self.lay.addWidget(_heading("Volume"))
        row = QHBoxLayout()
        self.vol = QSlider(Qt.Orientation.Horizontal)
        self.vol.setRange(0, 100)
        calm(self.vol, self.out, self.inp)
        self.vol_read = widgets.label("", 12, 500, mono=True)
        self.vol_read.setMinimumWidth(48)
        self.mute = QCheckBox("Mute")
        row.addWidget(self.vol, 1)
        row.addWidget(self.vol_read)
        row.addWidget(self.mute)
        self.lay.addLayout(row)
        now = system.get_volume()
        if now is None:
            self.vol.setEnabled(False)
            self.mute.setEnabled(False)
        else:
            self.vol.setValue(int(round(min(100, now[0]))))
            self.mute.setChecked(now[1])
        self.vol_read.setText(f"{self.vol.value()} %")
        self._vt = QTimer(self)
        self._vt.setSingleShot(True)
        self._vt.setInterval(120)
        self._vt.timeout.connect(lambda: system.set_volume("set", self.vol.value()))
        self.vol.valueChanged.connect(lambda v: (self.vol_read.setText(f"{v} %"), self._vt.start()))
        self.mute.toggled.connect(lambda on: system.set_volume("mute" if on else "unmute"))
        if IS_WIN:
            more = widgets.button("Windows sound settings…")
            more.clicked.connect(lambda: m.open_uri(sc.settings_uri("sound")))
            self.lay.addWidget(more, 0, Qt.AlignmentFlag.AlignLeft)
        self.finish()
        in_thread(lambda: (sc.sound_devices("sinks"), sc.sound_devices("sources")), self._got)

    def _got(self, res):
        if not alive(self) or not isinstance(res, tuple):
            return
        for combo, (devs, default) in ((self.out, res[0]), (self.inp, res[1])):
            combo.clear()
            for did, name in devs:
                combo.addItem(name, did)
            combo.setCurrentIndex(max(0, combo.findData(default)))
            combo.setEnabled(bool(devs))
            if not devs:
                combo.addItem("None found")

    def _pick(self, kind: str, combo: QComboBox):
        dev = combo.currentData()
        if dev:
            in_thread(lambda: sc.set_sound_device(kind, dev),
                      lambda err: self.m.ctx.notify("Sound", err) if err else None)


class AboutPage(_Page):
    def __init__(self, m: "SystemModule"):
        super().__init__("This computer")
        self.grid = QGridLayout()
        self.grid.setHorizontalSpacing(18)
        self.grid.setVerticalSpacing(8)
        self.grid.setColumnStretch(1, 1)
        self.lay.addLayout(self.grid)
        self.info = []
        copy = widgets.button("Copy all")
        copy.clicked.connect(lambda: (QApplication.clipboard().setText(
            "\n".join(f"{k}: {v}" for k, v in self.info)), m.ctx.notify("System Settings", "Copied.")))
        self.lay.addWidget(copy, 0, Qt.AlignmentFlag.AlignLeft)
        self.finish()
        in_thread(sc.about, self._got)

    def _got(self, info):
        if not alive(self) or not isinstance(info, list):
            return
        self.info = info
        for r, (k, v) in enumerate(info):
            self.grid.addWidget(widgets.label(k, 12, tone="muted"), r, 0, Qt.AlignmentFlag.AlignTop)
            val = widgets.label(v, 13)
            val.setWordWrap(True)
            val.setTextInteractionFlags(Qt.TextInteractionFlag.TextSelectableByMouse)
            self.grid.addWidget(val, r, 1)


# ══ the panel ════════════════════════════════════════════════════════════════════
class SystemPanel(widgets.FloatingPanel):
    def __init__(self, m: "SystemModule"):
        self.m = m
        super().__init__("System Settings", min_size=(820, 560), opacity=float(m.s("bg_opacity", 0.96)),
                         close_hides=True)
        root = QHBoxLayout()
        root.setContentsMargins(0, 0, 0, 0)
        root.setSpacing(0)
        side = QFrame()
        side.setObjectName("setSidebar")
        side.setFixedWidth(200)
        self.side = QVBoxLayout(side)
        self.side.setContentsMargins(10, 12, 10, 12)
        self.side.setSpacing(2)
        self.group = QButtonGroup(self)
        self.buttons: dict[str, QPushButton] = {}
        self.stack = QStackedWidget()
        self.pages: dict[str, QWidget] = {}
        self.makers = {}
        self.opts: list = []
        root.addWidget(side)
        root.addWidget(self.stack, 1)
        self.body.addLayout(root)

        self._add("displays", "▭  Displays", lambda: DisplaysPage(m))
        self._add("screen", "☀  Brightness & night", lambda: ScreenPage(m))
        self._add("sound", "♪  Sound", lambda: SoundPage(m))
        self._opt_at = self.side.count()
        self.side.addStretch()
        self._add("about", "ⓘ  This computer", lambda: AboutPage(m))
        self.loading = _hint("  Reading settings…")
        self.side.insertWidget(self._opt_at, self.loading)
        self.show_page("displays")
        self.resize(940, 680)
        self.escape_pressed.connect(self.hide)
        self.load_options()

    def _add(self, key: str, text: str, make, at: int | None = None):
        b = QPushButton(text.replace("&", "&&"))      # a lone & would underline the next letter
        b.setProperty("side", True)
        b.setCheckable(True)
        b.setFixedHeight(30)
        b.setCursor(Qt.CursorShape.PointingHandCursor)
        b.clicked.connect(lambda _c=False, k=key: self.show_page(k))
        self.group.addButton(b)
        self.buttons[key] = b
        self.makers[key] = make
        if at is None:
            self.side.addWidget(b)
        else:
            self.side.insertWidget(at, b)

    def show_page(self, key: str):
        if key not in self.makers:
            return
        if key not in self.pages:
            try:
                self.pages[key] = self.makers[key]()
            except Exception as e:
                log.exception("could not build the %s page", key)
                self.pages[key] = _hint(f"⚠ This page could not be built: {type(e).__name__}: {e}")
            self.stack.addWidget(self.pages[key])
        self.stack.setCurrentWidget(self.pages[key])
        self.buttons[key].setChecked(True)

    def load_options(self):
        in_thread(sc.options, self._got_options)

    def _got_options(self, opts):
        if not alive(self):
            return
        self.loading.hide()
        self.opts = opts if isinstance(opts, list) else []
        at = self._opt_at + 1
        for gid, title, icon in sc.GROUPS:
            mine = [o for o in self.opts if o.group == gid]
            if not mine:
                continue
            old = self.pages.pop(gid, None)
            if old is not None:
                self.stack.removeWidget(old)
                old.deleteLater()
            if gid in self.buttons:
                self.makers[gid] = lambda t=title, o=mine: OptionsPage(self.m, t, o)
                continue
            self._add(gid, f"{icon}  {title}", lambda t=title, o=mine: OptionsPage(self.m, t, o), at)
            at += 1
        cur = next((k for k, b in self.buttons.items() if b.isChecked()), "displays")
        if cur not in self.pages:
            self.show_page(cur)


class SystemModule(Module):
    key = "syscontrol"
    title = "System Settings"
    icon = "🖥"

    def __init__(self, ctx):
        super().__init__(ctx)
        self.panel: SystemPanel | None = None
        self.views: list = []
        self.night = sc.NightLight()
        self._reload_hooked = False
        self._profiles: list | None = None          # power modes, read once (powerprofilesctl takes a moment)

    def start(self):
        self.hotkey("hotkey", self.toggle, "system settings")
        self.hotkey("displays_hotkey", self.open_displays, "display settings")
        self.hotkey("night_hotkey", self.toggle_night, "night light")
        if self.s("keep", True):
            QTimer.singleShot(1500, self.restore)
            sync = hypr.sync()
            if sync is not None and not self._reload_hooked:
                self._reload_hooked = True
                sync.reloaded.connect(lambda: QTimer.singleShot(400, self.restore)
                                      if self.running and self.s("keep", True) else None)
        k = int(self.s("night_kelvin", 6500) or 6500)
        if self.s("night_on", False) and k < 6500:
            QTimer.singleShot(2000, lambda: self.night.set(k))

    def stop(self):
        self.night.stop_proc()
        if self.panel:
            self.panel.close()
            self.panel.deleteLater()
            self.panel = None

    def apply_settings(self):
        self.hotkeys.unregister_all(self.key)
        self.hotkey("hotkey", self.toggle, "system settings")
        self.hotkey("displays_hotkey", self.open_displays, "display settings")
        self.hotkey("night_hotkey", self.toggle_night, "night light")
        if self.panel:
            self.panel.set_opacity(float(self.s("bg_opacity", 0.96)))

    def retheme(self):
        if self.panel:
            visible = self.panel.isVisible()
            self.stop()
            if visible:
                self.toggle()

    def actions(self):
        out = [("toggle", "System settings (displays, mouse, keyboard, power…)", self.toggle),
               ("displays", "Display settings", self.open_displays),
               ("identify", "Identify monitors (show their numbers)", lambda: identify(sc.monitors())),
               ("night", "Night light on / off", self.toggle_night)]
        if self._profiles is None:
            self._profiles = [] if IS_WIN else sc.power_profiles()
        for pid, name in self._profiles:
            out.append((f"power_{pid}", f"Power mode: {name}", lambda p=pid: self._power(p)))
        return out

    def tray_actions(self):
        return [("System settings", self.toggle), ("Display settings", self.open_displays)]

    def make_page(self):
        with self.themed():
            view = SystemPanel(self)
        self.views.append(view)
        return widgets.embed(view)

    def toggle(self):
        if self.panel is None:
            with self.themed():
                self.panel = SystemPanel(self)
        if self.panel.isVisible():
            self.panel.hide()
            return
        self.panel.show_and_raise()

    def open_displays(self):
        if self.panel is None or not self.panel.isVisible():
            self.toggle()
        self.panel.show_page("displays")

    def panel_reload_options(self):
        for p in [self.panel] + self.views:
            if alive(p) and isinstance(p, SystemPanel):
                p.load_options()

    # ── keeping changes (Hyprland forgets them on a reload) ─────────────────────
    def remember(self, key: str, value):
        saved = dict(self.s("saved", {}) or {})
        saved[key] = value
        self.settings.set(f"{self.key}.saved", saved)
        self.settings.save_quiet()

    def forget_options(self):
        self.settings.set(f"{self.key}.saved", {})
        self.settings.save_quiet()

    def remember_monitors(self, mons: list | None):
        keep = None if mons is None else [{k: v for k, v in m.items() if k not in ("modes", "label", "focused",
                                                                                   "vrr_now")} for m in mons]
        if not IS_WIN:                                      # Windows keeps them itself
            self.settings.set(f"{self.key}.monitors", keep or [])
            self.settings.save_quiet()

    def restore(self):
        """Put back what was changed here (Hyprland only forgets; Windows and gsettings keep their own)."""
        saved = self.s("saved", {}) or {}
        mons = self.s("monitors", []) or []
        if not saved and not mons:
            return

        def work():
            errors = []
            if saved:
                by_key = {o.key: o for o in sc.options() if o.keep}
                for key, value in saved.items():
                    o = by_key.get(key)
                    if o is not None:
                        err = o.set(value)
                        if err:
                            errors.append(f"{o.label}: {err}")
            if mons and sc.backend() == "hyprland":
                plugged = {m["name"]: m for m in sc.monitors()}
                if all(m.get("name") in plugged for m in mons):      # only the same set of screens
                    want = []
                    for m in mons:
                        full = dict(plugged[m["name"]])
                        full.update(m)
                        want.append(full)
                    err = sc.apply_monitors(want)
                    if err:
                        errors.append(f"displays: {err}")
            return errors
        in_thread(work, lambda errs: [log.warning("restoring a system setting: %s", e) for e in errs or []])

    # ── night light, power ────────────────────────────────────────────────────────
    def set_night(self, kelvin: int) -> str:
        err = self.night.set(kelvin)
        if not err:
            if kelvin < 6500:
                self.settings.set(f"{self.key}.night_kelvin", int(kelvin))
            self.settings.set(f"{self.key}.night_on", kelvin < 6500)
            self.settings.save_quiet()
        return err

    def toggle_night(self):
        k = 6500 if self.night.kelvin < 6500 else int(self.s("night_kelvin", 4000) or 4000)
        k = 4000 if k >= 6500 else k
        err = self.set_night(6500 if self.night.kelvin < 6500 else k)
        self.ctx.notify("Night light", err or ("On" if self.night.kelvin < 6500 else "Off"), 1500)

    def _power(self, pid: str):
        in_thread(lambda: sc.set_power_profile(pid),
                  lambda err: self.ctx.notify("Power", err or f"Power mode: {pid}", 1500))

    @staticmethod
    def open_uri(uri: str):
        from PyQt6.QtGui import QDesktopServices
        from PyQt6.QtCore import QUrl
        QDesktopServices.openUrl(QUrl(uri))
