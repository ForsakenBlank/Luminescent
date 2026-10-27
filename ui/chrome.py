"""
The overlay's own elements (the logo, the LUMINESCENT title, the clock, the date, the status chips), as a layer
you can rearrange: ✎ Edit layout (top right) lets you drag them anywhere on any screen, add more, duplicate them,
make them bigger or smaller and freeze them where they are (right-click). Outside edit mode they ignore the mouse,
so clicks reach the dashboard underneath.

    settings ui.chrome_items: [{"id", "kind", "screen", "x", "y", "scale", "frozen"}]
    screen "" = the screen the overlay opens on; x / y are pixels on that screen. An empty list = the default layout.
"""
import uuid
import logging
from datetime import datetime

from PyQt6.QtWidgets import QWidget, QFrame, QLabel, QVBoxLayout, QHBoxLayout, QPushButton
from PyQt6.QtCore import Qt, QPoint

from core import theme, widgets, anim, APP_NAME
from core.guard import alive

log = logging.getLogger("chrome")

KINDS = [("logo", "Logo and title"), ("mark", "Sparkle"), ("title", "LUMINESCENT title"),
         ("clock", "Clock and date"), ("time", "Time"), ("date", "Date"),
         ("status", "Status chips (timers, Hearing Guard, next reminder)")]
NAMES = dict(KINDS)
SIZES = [(0.75, "Small"), (1.0, "Normal"), (1.5, "Large"), (2.0, "Huge")]
CLOCKISH = ("clock", "time", "date")
PAD = 6                                     # the room around an element for its edit outline


def _id() -> str:
    return uuid.uuid4().hex[:8]


def default_items(top: int) -> list[dict]:
    """The layout Luminescent always had: logo top left, the clock under it, the chips under that."""
    return [{"id": _id(), "kind": "logo", "screen": "", "x": 36 - PAD, "y": top + 30 - PAD, "scale": 1.0,
             "frozen": False},
            {"id": _id(), "kind": "clock", "screen": "", "x": 38 - PAD, "y": top + 82 - PAD, "scale": 1.0,
             "frozen": False},
            {"id": _id(), "kind": "status", "screen": "", "x": 36 - PAD, "y": top + 164 - PAD, "scale": 1.0,
             "frozen": False}]


# ── what an element shows ─────────────────────────────────────────────────────
class _Content(QWidget):
    def __init__(self, kind: str, scale: float, launcher):
        super().__init__()
        self.kind = kind
        self.launcher = launcher
        self._last = None
        lay = QVBoxLayout(self) if kind in ("clock", "status") else QHBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(int((6 if kind == "status" else 2 if kind == "clock" else 14) * scale))
        self.lay = lay
        self.time = self.date = None
        if kind in ("logo", "mark"):
            mark = QLabel()
            size = int(30 * scale)
            mark.setPixmap(theme.sparkle_pixmap(size))
            mark.setFixedSize(size, size)
            lay.addWidget(mark)
        if kind in ("logo", "title"):
            lay.addWidget(widgets.label(APP_NAME.upper(), 15 * scale, 600, mono=True, spacing=0.3))
        if kind in ("clock", "time"):
            self.time = widgets.label("", 26 * scale, 500, mono=True, spacing=-0.02)
            lay.addWidget(self.time)
        if kind in ("clock", "date"):
            self.date = widgets.label("", 10.5 * scale, 500, mono=True, spacing=0.12, tone="muted")
            lay.addWidget(self.date)
        self.scale = scale
        self.tick()

    def has_content(self) -> bool:
        return self.kind != "status" or bool(self._last)

    def tick(self):
        if self.time is not None or self.date is not None:
            now = datetime.now()
            if self.time is not None:
                self.time.setText(now.strftime("%H:%M"))
            if self.date is not None:
                self.date.setText(now.strftime("%A %d %B").upper().replace(" 0", " "))
        if self.kind == "status":
            lines = status_lines(self.launcher)
            if lines != self._last:
                self._last = lines
                widgets.clear_layout(self.lay)
                for icon, text in lines:
                    chip = QFrame()
                    chip.setObjectName("lumChip")
                    cl = QHBoxLayout(chip)
                    cl.setContentsMargins(10, 5, 12, 5)
                    cl.setSpacing(8)
                    cl.addWidget(widgets.label(icon, 11 * self.scale))
                    cl.addWidget(widgets.label(text, 11.5 * self.scale, 500, mono=True, tone="secondary"))
                    self.lay.addWidget(chip, 0, Qt.AlignmentFlag.AlignLeft)
        self.adjustSize()


def status_lines(launcher) -> list[tuple[str, str]]:
    """Running timers, the Hearing Guard level, the next reminder."""
    out = []
    mods = launcher.modules
    t = mods.get("timer")
    if t is not None and t.running:
        out += [("⏱", line) for line in t.tooltip_lines()[:4]]
    h = mods.get("hearing")
    if h is not None and h.running and h.display_level is not None and h.display_level >= 40:
        out.append(("🎧", f"{h.display_level:.0f} dB · {h.dose * 100:.0f} % of today's dose"))
    r = mods.get("reminders")
    nxt = r.next_reminder() if r is not None and r.running and hasattr(r, "next_reminder") else None
    if nxt:
        when, title = nxt
        day = "" if when.date() == datetime.now().date() else when.strftime("%a ")
        out.append(("⏰", f"{day}{when:%H:%M}  {title}"))
    return out


# ── one element on the screen ─────────────────────────────────────────────────
class ChromeItem(QFrame):
    def __init__(self, layer: "ChromeLayer", data: dict, host: QWidget):
        super().__init__(host)
        self.layer = layer
        self.data = data
        self.setObjectName("chromeItem")
        lay = QVBoxLayout(self)
        lay.setContentsMargins(PAD, PAD, PAD, PAD)
        self.content = _Content(data["kind"], float(data.get("scale", 1.0) or 1.0), layer.launcher)
        lay.addWidget(self.content)
        self.hint = widgets.label(NAMES.get(data["kind"], data["kind"]), 11, tone="muted")
        self.hint.hide()
        lay.addWidget(self.hint)
        self._press = None
        self.move(int(data.get("x", 0)), int(data.get("y", 0)))
        self.set_editing(layer.editing)

    def set_editing(self, on: bool):
        self.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents, not on)
        self.setProperty("editing", on)
        self.setProperty("frozen", bool(self.data.get("frozen")))
        self.style().unpolish(self)
        self.style().polish(self)
        self.setCursor(Qt.CursorShape.ArrowCursor if not on or self.data.get("frozen")
                       else Qt.CursorShape.OpenHandCursor)
        frozen = " · frozen (right-click → Unfreeze)" if self.data.get("frozen") else ""
        self.setToolTip(f"{NAMES.get(self.data['kind'], '')}{frozen}\nDrag to move · right-click for more"
                        if on else "")
        self.refresh()

    def refresh(self):
        # an empty status strip shows a placeholder while editing, and nothing otherwise
        empty = not self.content.has_content()
        self.content.setVisible(not empty)
        self.hint.setVisible(empty and self.layer.editing)
        self.adjustSize()
        self.setVisible(self.layer.wanted(self) and (not empty or self.layer.editing))

    def tick(self):
        self.content.tick()
        self.refresh()

    # ── moving ─────────────────────────────────────────────────────────────
    def mousePressEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton and not self.data.get("frozen"):
            self._press = (e.globalPosition().toPoint(), self.pos())
            self.setCursor(Qt.CursorShape.ClosedHandCursor)
            self.raise_()
        e.accept()

    def mouseMoveEvent(self, e):
        if self._press is None:
            return
        start, origin = self._press
        p = origin + e.globalPosition().toPoint() - start
        g = max(1, int(self.layer.launcher.settings.get("dashboard.snap", 8) or 1))
        x = round(p.x() / g) * g if g > 1 else p.x()
        y = round(p.y() / g) * g if g > 1 else p.y()
        host = self.parentWidget()
        self.move(max(0, min(x, host.width() - self.width())), max(0, min(y, host.height() - self.height())))

    def mouseReleaseEvent(self, e):
        if self._press is None:
            return
        self._press = None
        self.setCursor(Qt.CursorShape.OpenHandCursor)
        self.data["x"], self.data["y"] = self.x(), self.y()
        self.layer.save()

    def contextMenuEvent(self, e):
        self.layer.item_menu(self, e.globalPos())
        e.accept()


# ── the layer: every element on every screen, and edit mode ───────────────────
class ChromeLayer:
    def __init__(self, overlay):
        self.overlay = overlay
        self.launcher = overlay.launcher
        self.settings = overlay.settings
        self.editing = False
        self.items: list[ChromeItem] = []
        self.hosts: dict[str, QWidget] = {}         # screen name -> its backdrop
        self.host_name = ""
        self.top = 0
        self.bar: QFrame | None = None
        self.page = "dashboard"

    # ── data ───────────────────────────────────────────────────────────────
    def _data(self) -> list[dict]:
        raw = self.settings.get("ui.chrome_items") or []
        items = [d for d in raw if isinstance(d, dict) and d.get("kind") in NAMES]
        return items or default_items(self.top)

    def save(self):
        self.settings.set("ui.chrome_items", [dict(i.data) for i in self.items if alive(i)])
        self.settings.save_quiet()

    # ── building ───────────────────────────────────────────────────────────
    def build(self, hosts: dict[str, QWidget], host_name: str, top: int):
        self.hosts, self.host_name, self.top = hosts, host_name, top
        self.items = []
        for d in self._data():
            self._make(d)

    def _host_for(self, d: dict) -> QWidget:
        return self.hosts.get(d.get("screen") or "") or self.hosts[self.host_name]

    def _make(self, d: dict) -> ChromeItem:
        item = ChromeItem(self, d, self._host_for(d))
        self.items.append(item)
        item.raise_()
        return item

    def widgets(self) -> list[QWidget]:
        out = [i for i in self.items if alive(i)]
        if alive(self.bar):
            out.append(self.bar)
        return out

    def wanted(self, item: ChromeItem) -> bool:
        """Whether the settings and the page let this element show right now."""
        kind = item.data["kind"]
        if self.editing:
            return True
        if kind in CLOCKISH and not self.settings.get("ui.show_clock", True):
            return False
        if kind == "status" and not self.settings.get("ui.show_status", True):
            return False
        # the node editor uses the whole main screen: the clock and chips would sit on top of the nodes
        on_host = (item.data.get("screen") or self.host_name) == self.host_name
        return not (on_host and self.page == "automation" and (kind in CLOCKISH or kind == "status"))

    def tick(self):
        for i in self.items:
            if alive(i):
                i.tick()

    def page_changed(self, key: str):
        self.page = key
        for i in self.items:
            if alive(i):
                i.refresh()
                i.raise_()
        if alive(self.bar):
            self.bar.raise_()

    def apply_settings(self):
        for i in self.items:
            if alive(i):
                i.refresh()

    def bottom_on_host(self) -> int:
        """The lowest point of the elements in the top-left part of the main screen (dashboard boxes start
        below it). The status chips get room for two more."""
        y = 0
        for i in self.items:
            if not alive(i) or (i.data.get("screen") or self.host_name) != self.host_name or i.x() > 520:
                continue
            if not i.isVisible() and i.data["kind"] != "status":
                continue
            h = i.height() if i.isVisible() else 0
            y = max(y, i.y() + h + (2 * 34 if i.data["kind"] == "status" else 0))
        return y

    # ── edit mode ──────────────────────────────────────────────────────────
    def toggle_editing(self):
        self.set_editing(not self.editing)

    def set_editing(self, on: bool):
        if on == self.editing:
            return
        self.editing = on
        if on and not self.settings.get("ui.chrome_items"):
            self.save()                              # the default layout becomes yours to change
        for i in self.items:
            if alive(i):
                i.set_editing(on)
                i.raise_()
        if on:
            self._show_bar()
        elif alive(self.bar):
            bar = self.bar
            self.bar = None
            anim.fade(bar, 1.0, 0.0, "overlay", 120, then=bar.deleteLater)

    def _show_bar(self):
        host = self.hosts.get(self.host_name)
        if host is None:
            return
        bar = QFrame(host)
        bar.setObjectName("lumDock")
        bl = QHBoxLayout(bar)
        bl.setContentsMargins(14, 5, 5, 5)
        bl.setSpacing(6)
        bl.addWidget(widgets.label("EDITING THE LAYOUT", 10.5, 600, mono=True, spacing=0.1, tone="accent"))
        bl.addWidget(widgets.label("drag to move · right-click to freeze, copy, resize", 12, tone="muted"))
        add = QPushButton("＋ Add")
        add.setCursor(Qt.CursorShape.PointingHandCursor)
        add.clicked.connect(lambda: self.add_menu(add.mapToGlobal(add.rect().bottomLeft())))
        reset = QPushButton("Reset")
        reset.setProperty("variant", "ghost")
        reset.setToolTip("Back to the logo, clock and chips in the top-left corner")
        reset.clicked.connect(self.reset)
        done = QPushButton("Done")
        done.setProperty("variant", "primary")
        done.setCursor(Qt.CursorShape.PointingHandCursor)
        done.clicked.connect(lambda: self.set_editing(False))
        for b in (add, reset, done):
            b.setFixedHeight(32)
            bl.addWidget(b)
        bar.mousePressEvent = lambda e: e.accept()
        bar.adjustSize()
        bar.move((host.width() - bar.width()) // 2, self.top + 72)
        bar.show()
        bar.raise_()
        anim.fade(bar, 0.0, 1.0, "overlay", 150)
        self.bar = bar

    # ── changes ────────────────────────────────────────────────────────────
    def _menu_parent(self, item: ChromeItem | None = None) -> QWidget:
        return item if item is not None and alive(item) else self.hosts.get(self.host_name)

    def item_menu(self, item: ChromeItem, pos: QPoint):
        m = widgets.menu(self._menu_parent(item))
        d = item.data
        frozen = bool(d.get("frozen"))
        m.addAction("Unfreeze (can be moved again)" if frozen else "Freeze (stays where it is)",
                    lambda: self._set(item, frozen=not frozen))
        m.addAction("Duplicate", lambda: self.duplicate(item))
        sizes = m.addMenu("Size")
        for v, label in SIZES:
            a = sizes.addAction(label, lambda v=v: self._set(item, scale=v, rebuild=True))
            a.setCheckable(True)
            a.setChecked(abs(float(d.get("scale", 1.0)) - v) < 0.01)
        if len(self.hosts) > 1:
            scr = m.addMenu("Move to screen")
            cur = d.get("screen") or ""
            a = scr.addAction(("●  " if not cur else "     ") + "The screen the overlay opens on",
                              lambda: self._set(item, screen="", rebuild=True))
            for name in self.hosts:
                scr.addAction(("●  " if cur == name else "     ") + name,
                              lambda n=name: self._set(item, screen=n, rebuild=True))
        m.addSeparator()
        m.addAction("Remove", lambda: self.remove(item))
        m.addSeparator()
        if not self.editing:
            m.addAction("Edit the layout…", lambda: self.set_editing(True))
        else:
            m.addAction("Done editing", lambda: self.set_editing(False))
        m.exec(pos)

    def add_menu(self, pos: QPoint):
        m = widgets.menu(self._menu_parent())
        for kind, label in KINDS:
            m.addAction(label, lambda k=kind: self.add(k))
        m.exec(pos)

    def add(self, kind: str, screen: str = ""):
        host = self.hosts.get(screen or self.host_name)
        x, y = (host.width() // 2 - 80, self.top + 140) if host is not None else (200, 200)
        d = {"id": _id(), "kind": kind, "screen": screen, "x": x, "y": y, "scale": 1.0, "frozen": False}
        item = self._make(d)
        item.show()
        anim.fade(item, 0.0, 1.0, "overlay", 160)
        self.save()

    def duplicate(self, item: ChromeItem):
        d = dict(item.data, id=_id(), frozen=False, x=item.x() + 24, y=item.y() + 24)
        new = self._make(d)
        new.show()
        anim.fade(new, 0.0, 1.0, "overlay", 160)
        self.save()

    def remove(self, item: ChromeItem):
        if item in self.items:
            self.items.remove(item)
        self.save()
        anim.fade(item, 1.0, 0.0, "overlay", 140, then=item.deleteLater)

    def reset(self):
        for i in self.items:
            if alive(i):
                i.deleteLater()
        self.items = []
        self.settings.set("ui.chrome_items", [])
        for d in default_items(self.top):
            self._make(d).show()
        self.save()

    def _set(self, item: ChromeItem, rebuild: bool = False, **kw):
        item.data.update(kw)
        if rebuild:
            # a new size or screen: build it again in its new place
            i = self.items.index(item) if item in self.items else -1
            if "screen" in kw:
                host = self._host_for(item.data)
                item.data["x"] = max(0, min(item.data["x"], host.width() - 80))
                item.data["y"] = max(0, min(item.data["y"], host.height() - 40))
            new = ChromeItem(self, item.data, self._host_for(item.data))
            if i >= 0:
                self.items[i] = new
            item.deleteLater()
            new.show()
            new.raise_()
        else:
            item.set_editing(self.editing)
        self.save()

    def clear(self):
        self.items = []
        self.bar = None
        self.editing = False
