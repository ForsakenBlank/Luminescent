"""
The Performance module's window and pages: Overview (widgets you arrange and edit), Processes, Boost, Game mode,
Alerts & rules, Startup apps and Options, plus the gaming HUD. The measuring and the logic are in
modules/performance.py, core/perfstats.py and core/perftweaks.py.
"""
import os
import copy
import glob
import logging

from PyQt6.QtWidgets import (QWidget, QVBoxLayout, QHBoxLayout, QGridLayout, QStackedWidget, QFrame, QPushButton,
                             QButtonGroup, QCheckBox, QComboBox, QSlider, QLineEdit, QScrollArea, QApplication,
                             QSizePolicy, QTreeWidget, QTreeWidgetItem, QHeaderView, QSpinBox, QListWidget,
                             QListWidgetItem, QLabel, QAbstractItemView, QFileIconProvider)
from PyQt6.QtCore import Qt, QTimer, QRectF, QPointF, QSize, QFileInfo
from PyQt6.QtGui import QPainter, QColor, QPen, QPainterPath, QIcon, QPixmap

from core import IS_WIN, theme, widgets, perfstats, perftweaks
from core.perfstats import human_bytes, human_rate
from core.guard import alive
from modules.syscontrol import in_thread, calm

log = logging.getLogger("performance")

COLOURS = [("", "Accent"), ("#3DBE96", "Green"), ("#E0894E", "Orange"), ("#5B9BE6", "Blue"), ("#D6409F", "Pink"),
           ("#E5C14B", "Yellow"), ("#9B6CF6", "Purple"), ("#E05555", "Red"), ("#4FC3D9", "Cyan")]

# type: (title, colour, [(element, label, on by default)])
TYPES = {
    "cpu": ("Processor", "", [("usage", "Usage %", True), ("graph", "Graph", True), ("ring", "Ring gauge", False),
                              ("cores", "Every core", True), ("freq", "Clock speed", True),
                              ("temp", "Temperature", True), ("load", "Load average", False),
                              ("name", "Processor name", False)]),
    "memory": ("Memory", "#3DBE96", [("usage", "Used / total", True), ("bar", "Bar", True), ("graph", "Graph", True),
                                     ("ring", "Ring gauge", False), ("available", "Available", False),
                                     ("cached", "Cache", False), ("swap", "Swap", True)]),
    "gpu": ("GPU", "#E0894E", [("load", "Load %", True), ("graph", "Load graph", True), ("ring", "Ring gauge", False),
                               ("vram", "VRAM used / total", True), ("vram_bar", "VRAM bar", True),
                               ("vram_graph", "VRAM graph", False), ("temp", "Temperature", True),
                               ("power", "Power draw", True), ("clock", "Core clock", True),
                               ("mem_clock", "Memory clock", False), ("fan", "Fan", True),
                               ("encoder", "Video engine", False), ("name", "Card name", True)]),
    "vram": ("VRAM", "#D6409F", [("usage", "Used / total", True), ("bar", "Bar", True), ("graph", "Graph", True),
                                 ("ring", "Ring gauge", False), ("apps", "Apps using the most", True)]),
    "apps": ("Top apps", "", [("icon", "Icons", True), ("cpu", "CPU", True), ("mem", "Memory", True),
                              ("gpu", "GPU", True), ("vram", "VRAM", True), ("disk", "Disk", False),
                              ("pid", "PID", False)]),
    "disk": ("Disks", "#E5C14B", [("rates", "Read / write", True), ("graph", "Graph", True),
                                  ("drives", "Space on each drive", True)]),
    "network": ("Network", "#5B9BE6", [("rates", "Download / upload", True), ("graph", "Graph", True),
                                       ("totals", "Since Luminescent started", True)]),
    "temps": ("Temperatures", "#E05555", [("cpu", "Processor", True), ("gpu", "GPUs", True),
                                          ("other", "Every other sensor", True), ("graph", "Graph of the hottest", False)]),
    "battery": ("Battery", "#3DBE96", [("percent", "Charge", True), ("bar", "Bar", True), ("time", "Time left", True)]),
    "system": ("System", "#9B6CF6", [("uptime", "Up for", True), ("procs", "Processes", True),
                                     ("threads", "Threads", True), ("load", "Load average", True)]),
}
HISTORIES = [(30, "30 seconds"), (60, "1 minute"), (120, "2 minutes"), (300, "5 minutes"), (600, "10 minutes"),
             (900, "15 minutes")]
STYLES = [("area", "Filled"), ("line", "Line"), ("bars", "Bars")]
SORTS = [("cpu", "CPU"), ("mem", "Memory"), ("gpu", "GPU"), ("vram", "VRAM"), ("disk", "Disk")]
HUD_ITEMS = [("cpu", "CPU %"), ("cpu_temp", "CPU temperature"), ("gpu", "GPU %"), ("gpu_temp", "GPU temperature"),
             ("vram", "VRAM"), ("ram", "RAM"), ("power", "GPU power"), ("net", "Network"), ("clock", "Clock"),
             ("battery", "Battery")]


def _hint(text: str) -> QLabel:
    lbl = widgets.label(text, 12, tone="muted")
    lbl.setWordWrap(True)
    return lbl


def _heading(text: str) -> QLabel:
    return widgets.label(text.upper(), 9.5, 600, mono=True, spacing=0.14, tone="muted")


def _card(spacing: int = 6, margins=(14, 12, 14, 12)) -> tuple[QFrame, QVBoxLayout]:
    box = QFrame()
    box.setObjectName("lumCard")
    lay = QVBoxLayout(box)
    lay.setContentsMargins(*margins)
    lay.setSpacing(spacing)
    return box, lay


def _short_gpu(name: str) -> str:
    for junk in ("NVIDIA ", "GeForce ", "AMD ", "Advanced Micro Devices, Inc. ", "[AMD/ATI] ", "Intel(R) ",
                 "Corporation "):
        name = name.replace(junk, "")
    if "[" in name and "]" in name:                       # "Raphael [Radeon 610M]" -> "Radeon 610M"
        name = name[name.index("[") + 1:name.index("]")]
    return name.strip() or "GPU"


def _temp_colour(t: float | None) -> str:
    if t is None:
        return theme.P.text_secondary
    return "#3DBE96" if t < 65 else "#E5C14B" if t < 82 else "#E05555"


def _pct(v) -> str:
    return "–" if v is None else f"{v:.0f} %"


# ══ drawing pieces ════════════════════════════════════════════════════════════════
class Graph(QWidget):
    """History as a filled line, a line or bars; more than one series is drawn on top of each other."""

    def __init__(self, height: int = 46):
        super().__init__()
        self.series: list[list[float]] = []
        self.colours: list[str] = []
        self.n = 60
        self.top: float | None = 100.0
        self.style = "area"
        self.setFixedHeight(height)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)

    def set_data(self, series: list[list[float]], colours: list[str], n: int, top: float | None = 100.0,
                 style: str = "area"):
        self.series, self.colours, self.n, self.top, self.style = series, colours, max(2, n), top, style
        self.update()

    def paintEvent(self, e):
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        r = QRectF(self.rect()).adjusted(0.5, 1, -0.5, -1)
        grid = QColor(theme.P.border)
        grid.setAlpha(110)
        p.setPen(QPen(grid, 1, Qt.PenStyle.DotLine))
        for f in (0.25, 0.5, 0.75):
            y = r.top() + r.height() * f
            p.drawLine(QPointF(r.left(), y), QPointF(r.right(), y))
        p.setPen(QPen(QColor(theme.P.border), 1))
        p.drawLine(QPointF(r.left(), r.bottom()), QPointF(r.right(), r.bottom()))
        everything = [v for s in self.series for v in s]
        top = self.top or max(max(everything) if everything else 1.0, 1.0) * 1.1
        step = r.width() / (self.n - 1)
        for vals, colour in zip(self.series, self.colours):
            vals = vals[-self.n:]
            if not vals:
                continue
            c = QColor(colour or theme.P.primary)
            x0 = r.right() - step * (len(vals) - 1)
            pts = [QPointF(x0 + i * step, r.bottom() - min(1.0, max(0.0, v) / top) * r.height())
                   for i, v in enumerate(vals)]
            if self.style == "bars":
                w = max(1.0, step * 0.7)
                fill = QColor(c)
                fill.setAlpha(170)
                for pt in pts:
                    p.fillRect(QRectF(pt.x() - w / 2, pt.y(), w, r.bottom() - pt.y()), fill)
                continue
            if len(pts) < 2:
                continue
            if self.style == "area":
                path = QPainterPath(QPointF(pts[0].x(), r.bottom()))
                for pt in pts:
                    path.lineTo(pt)
                path.lineTo(QPointF(pts[-1].x(), r.bottom()))
                fill = QColor(c)
                fill.setAlpha(45)
                p.fillPath(path, fill)
            line = QPainterPath(pts[0])
            for pt in pts[1:]:
                line.lineTo(pt)
            p.setPen(QPen(c, 1.6))
            p.setBrush(Qt.BrushStyle.NoBrush)
            p.drawPath(line)
        p.end()


class Ring(QWidget):
    """A ring that fills up to a percentage, the number in the middle."""

    def __init__(self, size: int = 74):
        super().__init__()
        self.value, self.colour, self.text, self.sub = 0.0, "", "", ""
        self.setFixedSize(size, size)

    def set(self, value: float | None, colour: str, text: str, sub: str = ""):
        self.value, self.colour, self.text, self.sub = value or 0.0, colour, text, sub
        self.update()

    def paintEvent(self, e):
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        w = 7
        r = QRectF(self.rect()).adjusted(w, w, -w, -w)
        p.setPen(QPen(QColor(theme.P.border), w, cap=Qt.PenCapStyle.RoundCap))
        p.drawArc(r, 0, 360 * 16)
        p.setPen(QPen(QColor(self.colour or theme.P.primary), w, cap=Qt.PenCapStyle.RoundCap))
        p.drawArc(r, 90 * 16, -int(360 * 16 * min(1.0, self.value / 100)))
        p.setPen(QColor(theme.P.text_primary))
        p.setFont(theme.qfont(13 if self.width() < 90 else 16, 600, True))
        p.drawText(r.adjusted(0, -6 if self.sub else 0, 0, 0), Qt.AlignmentFlag.AlignCenter, self.text)
        if self.sub:
            p.setPen(QColor(theme.P.text_muted))
            p.setFont(theme.qfont(8.5, 500, True))
            p.drawText(r.adjusted(0, 22, 0, 0), Qt.AlignmentFlag.AlignCenter, self.sub)
        p.end()


class Meter(QWidget):
    """A thin bar filled to a fraction."""

    def __init__(self, height: int = 6):
        super().__init__()
        self.frac, self.colour = 0.0, ""
        self.setFixedHeight(height)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)

    def set(self, frac: float | None, colour: str = ""):
        self.frac, self.colour = max(0.0, min(1.0, frac or 0.0)), colour
        self.update()

    def paintEvent(self, e):
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        r = QRectF(self.rect())
        rad = r.height() / 2
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor(theme.P.border))
        p.drawRoundedRect(r, rad, rad)
        if self.frac > 0:
            p.setBrush(QColor(self.colour or theme.P.primary))
            p.drawRoundedRect(QRectF(r.left(), r.top(), max(r.height(), r.width() * self.frac), r.height()), rad, rad)
        p.end()


class CoreBars(QWidget):
    """Every core as a small vertical bar."""

    def __init__(self):
        super().__init__()
        self.values: list[float] = []
        self.colour = ""
        self.setFixedHeight(34)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)

    def set(self, values: list[float], colour: str):
        self.values, self.colour = values, colour
        self.update()

    def paintEvent(self, e):
        if not self.values:
            return
        p = QPainter(self)
        n = len(self.values)
        gap = 2 if n < 40 else 1
        w = (self.width() - gap * (n - 1)) / n
        c = QColor(self.colour or theme.P.primary)
        back = QColor(theme.P.border)
        for i, v in enumerate(self.values):
            x = i * (w + gap)
            p.fillRect(QRectF(x, 0, w, self.height()), back)
            h = self.height() * min(1.0, v / 100)
            p.fillRect(QRectF(x, self.height() - h, w, h), c)
        p.end()


def _kv(text: str) -> tuple[QWidget, QLabel]:
    row = QWidget()
    lay = QHBoxLayout(row)
    lay.setContentsMargins(0, 0, 0, 0)
    lay.setSpacing(8)
    lay.addWidget(widgets.label(text, 12, tone="secondary"))
    lay.addStretch()
    val = widgets.label("–", 12, 500, mono=True)
    lay.addWidget(val)
    return row, val


# ══ app icons ═════════════════════════════════════════════════════════════════════
class IconFinder:
    """The icon of a process: its .desktop entry's icon on Linux (by the program, its window class or its Flatpak
    id), the program's own icon on Windows, or a letter in a circle."""

    def __init__(self):
        self.cache: dict[str, QIcon] = {}
        self.index: dict[str, str] | None = None
        self.provider = QFileIconProvider() if IS_WIN else None

    def _build(self):
        self.index = {}
        if IS_WIN:
            return
        dirs = [os.environ.get("XDG_DATA_HOME") or os.path.expanduser("~/.local/share")]
        dirs += (os.environ.get("XDG_DATA_DIRS") or "/usr/local/share:/usr/share").split(":")
        dirs += [os.path.expanduser("~/.local/share/flatpak/exports/share"), "/var/lib/flatpak/exports/share"]
        for d in dirs:
            for path in glob.glob(os.path.join(d, "applications", "**", "*.desktop"), recursive=True):
                e = perftweaks._desktop_entry(path)
                icon = e.get("Icon", "")
                if not icon:
                    continue
                keys = {os.path.basename(path)[:-8].lower()}
                if e.get("StartupWMClass"):
                    keys.add(e["StartupWMClass"].lower())
                tokens = [t for t in e.get("Exec", "").replace('"', "").split() if "=" not in t]
                if tokens:
                    if os.path.basename(tokens[0]) in ("flatpak", "env") and len(tokens) > 1:
                        tokens = [t for t in tokens[1:] if not t.startswith("-") and t != "run"]
                    if tokens:
                        keys.add(os.path.basename(tokens[0]).lower())
                for k in keys:
                    self.index.setdefault(k, icon)
                    self.index.setdefault(k.split(".")[-1], icon)        # org.foo.Bar -> bar

    def _from_name(self, icon: str) -> QIcon | None:
        if os.path.isabs(icon) and os.path.exists(icon):
            return QIcon(icon)
        ic = QIcon.fromTheme(icon)
        return None if ic.isNull() else ic

    @staticmethod
    def letter(name: str, colour: str = "") -> QIcon:
        pm = QPixmap(32, 32)
        pm.fill(Qt.GlobalColor.transparent)
        p = QPainter(pm)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        p.setPen(Qt.PenStyle.NoPen)
        c = QColor(colour or theme.P.primary_muted)
        p.setBrush(c)
        p.drawEllipse(1, 1, 30, 30)
        p.setPen(QColor(theme.P.text_primary))
        p.setFont(theme.qfont(15, 700))
        p.drawText(QRectF(0, 0, 32, 32), Qt.AlignmentFlag.AlignCenter, (name[:1] or "?").upper())
        p.end()
        return QIcon(pm)

    def get(self, proc: dict) -> QIcon:
        name = proc.get("name", "")
        exe = proc.get("exe") or ""
        key = exe or name
        if key in self.cache:
            return self.cache[key]
        icon = None
        if IS_WIN:
            if exe and os.path.exists(exe):
                ic = self.provider.icon(QFileInfo(exe))
                icon = None if ic.isNull() else ic
        else:
            if self.index is None:
                self._build()
            cands = []
            flat = perfstats._read(f"/proc/{proc.get('pid')}/root/.flatpak-info")
            for line in flat.splitlines():
                if line.startswith("name="):
                    cands.append(line[5:].strip().lower())
            base = name.split(" · ")[0]
            for c in (os.path.basename(exe), proc.get("proc", ""), base,
                      os.path.basename((proc.get("cmd") or [""])[0].replace("\\", "/"))):
                c = c.lower()
                if c:
                    cands += [c, c.removesuffix(".exe"), c.removesuffix("-bin"), c.split("-")[0]]
            for c in cands:
                if c in self.index:
                    icon = self._from_name(self.index[c])
                    if icon:
                        break
            if icon is None:
                for c in cands:
                    icon = self._from_name(c)
                    if icon:
                        break
            if icon is None and " · " in name:
                icon = self._from_name("text-x-script")
            if icon is None and base.lower().endswith(".exe"):
                icon = self._from_name("wine") or self._from_name("application-x-ms-dos-executable")
        if icon is None:
            icon = self.letter(name)
        self.cache[key] = icon
        return icon


icons = IconFinder()


# ══ one widget on the Overview ════════════════════════════════════════════════════
class WidgetCard(QFrame):
    def __init__(self, page: "OverviewPage", cfg: dict):
        super().__init__()
        self.page, self.m, self.cfg = page, page.m, cfg
        self.setObjectName("lumCard")
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Maximum)
        t = TYPES.get(cfg["type"], TYPES["cpu"])
        self.colour = cfg.get("colour") or t[1] or theme.P.primary
        els = cfg.get("elements")
        self.els = set(els) if isinstance(els, list) else {k for k, _l, on in t[2] if on}
        lay = QVBoxLayout(self)
        lay.setContentsMargins(14, 10, 10, 12)
        lay.setSpacing(6)
        head = QHBoxLayout()
        head.setSpacing(6)
        dot = QFrame()
        dot.setFixedSize(8, 8)
        dot.setStyleSheet(f"background: {self.colour}; border-radius: 4px;")
        head.addWidget(dot)
        self.title = widgets.elided((cfg.get("title") or t[0]).upper(), 9.5, 600, mono=True, spacing=0.12,
                                    tone="muted")
        head.addWidget(self.title, 1)
        self.big = widgets.label("", 15, 600, mono=True)
        head.addWidget(self.big)
        self.arrange = []
        for txt, tip, fn in (("◀", "Move earlier", lambda: page.move(self, -1)),
                             ("▶", "Move later", lambda: page.move(self, 1)),
                             ("✕", "Remove", lambda: page.remove(self))):
            b = widgets.icon_button(txt, tip, 22, 10, ghost=True)
            b.clicked.connect(fn)
            b.setVisible(page.arranging)
            head.addWidget(b)
            self.arrange.append(b)
        more = widgets.icon_button("⋯", "Edit this widget", 24, 13, ghost=True)
        more.clicked.connect(lambda: page.menu_for(self, more))
        head.addWidget(more)
        lay.addLayout(head)
        self.body = QVBoxLayout()
        self.body.setSpacing(5)
        lay.addLayout(self.body)
        self.parts: dict = {}
        getattr(self, f"_build_{cfg['type']}", self._build_cpu)()
        self.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.customContextMenuRequested.connect(lambda pos: page.menu_for(self, None, self.mapToGlobal(pos)))

    def mouseDoubleClickEvent(self, e):
        self.page.edit(self)

    def on(self, el: str) -> bool:
        return el in self.els

    def wants_processes(self) -> bool:
        return self.cfg["type"] == "apps" or (self.cfg["type"] == "vram" and self.on("apps"))

    # helpers
    def _n(self) -> int:
        interval = max(0.5, float(self.m.data.get("interval", 1.0) or 1.0))
        return max(10, int(int(self.cfg.get("history", 60)) / interval))

    def _graph(self, key: str = "graph", height: int | None = None) -> Graph:
        g = Graph(height or {"small": 32, "medium": 50, "large": 84}.get(self.cfg.get("height", "medium"), 50))
        self.parts[key] = g
        self.body.addWidget(g)
        return g

    def _row(self, key: str, text: str) -> QLabel:
        row, val = _kv(text)
        self.parts[key] = val
        self.body.addWidget(row)
        return val

    def _ring_row(self) -> QHBoxLayout | None:
        if not self.on("ring"):
            return None
        row = QHBoxLayout()
        ring = Ring()
        self.parts["ring"] = ring
        row.addWidget(ring)
        col = QVBoxLayout()
        col.setSpacing(4)
        row.addLayout(col, 1)
        self.body.addLayout(row)
        self._ring_col = col
        return row

    def _add(self, w):
        (getattr(self, "_ring_col", None) or self.body).addWidget(w)

    def _row_in_ring(self, key: str, text: str) -> QLabel:
        row, val = _kv(text)
        self.parts[key] = val
        self._add(row)
        return val

    def _gpu(self, d: dict) -> dict | None:
        gpus = d.get("gpus") or []
        src = self.cfg.get("source", "main")
        return next((g for g in gpus if g["id"] == src), gpus[0] if gpus else None)

    def _set_graph(self, key: str, series: list[list[float]], colours: list[str], top: float | None = 100.0):
        g = self.parts.get(key)
        if g is not None:
            g.set_data(series, colours, self._n(), top, self.cfg.get("style", "area"))

    # ── processor ───────────────────────────────────────────────────────────────
    def _build_cpu(self):
        self._ring_row()
        if self.on("name"):
            self._add(widgets.elided("", 11, tone="muted"))
            self.parts["name"] = self._ring_col.itemAt(self._ring_col.count() - 1).widget() \
                if getattr(self, "_ring_col", None) else self.body.itemAt(self.body.count() - 1).widget()
        for key, text in (("freq", "Clock"), ("temp", "Temperature"), ("load", "Load average")):
            if self.on(key):
                self._row_in_ring(key, text)
        self._ring_col = None
        if self.on("graph"):
            self._graph()
        if self.on("cores"):
            self.parts["cores"] = CoreBars()
            self.body.addWidget(self.parts["cores"])

    def _update_cpu(self, d):
        c = d["cpu"]
        self.big.setText(f"{c['usage']:.0f} %" if self.on("usage") else "")
        if "ring" in self.parts:
            self.parts["ring"].set(c["usage"], self.colour, f"{c['usage']:.0f}%", "CPU")
        if "name" in self.parts:
            self.parts["name"].setText(c.get("name") or "")
        if "freq" in self.parts:
            f = c.get("freq")
            self.parts["freq"].setText(f"{f / 1000:.2f} GHz" if f else "–")
        if "temp" in self.parts:
            t = c.get("temp")
            self.parts["temp"].setText(f"{t:.0f} °C" if t is not None else "–")
            self.parts["temp"].setStyleSheet(f"color: {_temp_colour(t)};")
        if "load" in self.parts:
            la = c.get("load_avg")
            self.parts["load"].setText("  ".join(f"{x:.2f}" for x in la) if la else "–")
        self._set_graph("graph", [self.m.series("cpu", self._n())], [self.colour])
        if "cores" in self.parts:
            self.parts["cores"].set(c["cores"], self.colour)

    # ── memory ──────────────────────────────────────────────────────────────────
    def _build_memory(self):
        self._ring_row()
        for key, text in (("usage", "In use"), ("available", "Available"), ("cached", "Cache"), ("swap", "Swap")):
            if self.on(key):
                self._row_in_ring(key, text)
        self._ring_col = None
        if self.on("bar"):
            self.parts["bar"] = Meter()
            self.body.addWidget(self.parts["bar"])
        if self.on("graph"):
            self._graph()

    def _update_memory(self, d):
        mm = d["mem"]
        self.big.setText(f"{mm['percent']:.0f} %")
        if "ring" in self.parts:
            self.parts["ring"].set(mm["percent"], self.colour, f"{mm['percent']:.0f}%", "RAM")
        if "usage" in self.parts:
            self.parts["usage"].setText(f"{human_bytes(mm['used'])} / {human_bytes(mm['total'])}")
        if "available" in self.parts:
            self.parts["available"].setText(human_bytes(mm["available"]))
        if "cached" in self.parts:
            self.parts["cached"].setText(human_bytes(mm["cached"]))
        if "swap" in self.parts:
            self.parts["swap"].setText(f"{human_bytes(mm['swap_used'])} / {human_bytes(mm['swap_total'])}"
                                       if mm["swap_total"] else "none")
        if "bar" in self.parts:
            self.parts["bar"].set(mm["percent"] / 100, self.colour)
        self._set_graph("graph", [self.m.series("mem", self._n())], [self.colour])

    # ── GPU ─────────────────────────────────────────────────────────────────────
    def _build_gpu(self):
        self._ring_row()
        if self.on("name"):
            lbl = widgets.elided("", 11, tone="muted")
            self.parts["name"] = lbl
            self._add(lbl)
        for key, text in (("vram", "VRAM"), ("temp", "Temperature"), ("power", "Power"), ("clock", "Core clock"),
                          ("mem_clock", "Memory clock"), ("fan", "Fan"), ("encoder", "Video engine")):
            if self.on(key):
                self._row_in_ring(key, text)
        self._ring_col = None
        if self.on("vram_bar"):
            self.parts["vram_bar"] = Meter()
            self.body.addWidget(self.parts["vram_bar"])
        if self.on("graph"):
            self._graph()
        if self.on("vram_graph"):
            self._graph("vram_graph")

    def _update_gpu(self, d):
        g = self._gpu(d)
        if g is None:
            self.big.setText("")
            if "name" in self.parts:
                self.parts["name"].setText("No GPU found")
            return
        if not self.cfg.get("title"):
            self.title.setText(_short_gpu(g["name"]).upper())
        self.big.setText(_pct(g.get("load")) if self.on("load") else "")
        if "ring" in self.parts:
            self.parts["ring"].set(g.get("load"), self.colour, _pct(g.get("load")).replace(" ", ""), "GPU")
        if "name" in self.parts:
            self.parts["name"].setText(g["name"] + ("  ·  built-in" if g.get("integrated") else ""))
        used, total = g.get("vram_used"), g.get("vram_total")
        if "vram" in self.parts:
            self.parts["vram"].setText(f"{human_bytes(used)} / {human_bytes(total)}" if total else
                                       human_bytes(used) if used else "–")
        if "vram_bar" in self.parts:
            self.parts["vram_bar"].set((used or 0) / total if total else 0, self.colour)
        if "temp" in self.parts:
            t = g.get("temp")
            self.parts["temp"].setText(f"{t:.0f} °C" if t is not None else "–")
            self.parts["temp"].setStyleSheet(f"color: {_temp_colour(t)};")
        if "power" in self.parts:
            pw, lim = g.get("power"), g.get("power_limit")
            self.parts["power"].setText((f"{pw:.0f} W" if pw is not None else "–") + (f" / {lim:.0f} W" if lim else ""))
        if "clock" in self.parts:
            self.parts["clock"].setText(f"{g['clock']:.0f} MHz" if g.get("clock") else "–")
        if "mem_clock" in self.parts:
            self.parts["mem_clock"].setText(f"{g['mem_clock']:.0f} MHz" if g.get("mem_clock") else "–")
        if "fan" in self.parts:
            self.parts["fan"].setText(_pct(g.get("fan")))
        if "encoder" in self.parts:
            self.parts["encoder"].setText(_pct(g.get("encoder")))
        self._set_graph("graph", [self.m.series(f"gpu:{g['id']}:load", self._n())], [self.colour])
        self._set_graph("vram_graph", [self.m.series(f"gpu:{g['id']}:vram", self._n())], [self.colour])

    # ── VRAM ────────────────────────────────────────────────────────────────────
    def _build_vram(self):
        self._ring_row()
        if self.on("usage"):
            self._row_in_ring("usage", "In use")
        self._row_in_ring("free", "Free")
        self._ring_col = None
        if self.on("bar"):
            self.parts["bar"] = Meter(8)
            self.body.addWidget(self.parts["bar"])
        if self.on("graph"):
            self._graph()
        if self.on("apps"):
            self.body.addSpacing(2)
            self.body.addWidget(_heading("Using the most"))
            self.app_rows = [AppRow(compact=True) for _ in range(int(self.cfg.get("count", 5)))]
            for r in self.app_rows:
                self.body.addWidget(r)

    def _update_vram(self, d):
        g = self._gpu(d)
        if g is None:
            return
        if not self.cfg.get("title"):
            self.title.setText(f"VRAM · {_short_gpu(g['name'])}".upper())
        used, total = g.get("vram_used") or 0, g.get("vram_total")
        pct = 100.0 * used / total if total else None
        self.big.setText(_pct(pct))
        if "ring" in self.parts:
            self.parts["ring"].set(pct, self.colour, _pct(pct).replace(" ", ""), "VRAM")
        if "usage" in self.parts:
            self.parts["usage"].setText(f"{human_bytes(used)} / {human_bytes(total)}" if total else human_bytes(used))
        self.parts["free"].setText(human_bytes(total - used) if total else "–")
        if "bar" in self.parts:
            self.parts["bar"].set(used / total if total else 0, self.colour)
        self._set_graph("graph", [self.m.series(f"gpu:{g['id']}:vram", self._n())], [self.colour])
        if self.on("apps") and "procs" in d:
            top = sorted(group_apps(d["procs"], self.m), key=lambda a: -a["vram"])
            for row, a in zip(self.app_rows, top + [None] * len(self.app_rows)):
                row.show_app(a if a and a["vram"] > 0 else None, [("vram", human_bytes(a["vram"]) if a else "")],
                             (a["vram"] / total) if a and total else 0, self.colour)

    # ── top apps ────────────────────────────────────────────────────────────────
    def _build_apps(self):
        cols = [k for k in ("cpu", "mem", "gpu", "vram", "disk", "pid") if self.on(k)]
        head = QHBoxLayout()
        head.setContentsMargins(0, 0, 0, 0)
        head.addWidget(_heading(f"by {dict(SORTS).get(self.cfg.get('sort', 'cpu'), 'CPU')}"), 1)
        for c in cols:
            lbl = _heading({"mem": "RAM", "pid": "PID"}.get(c, c.upper()))
            lbl.setFixedWidth(64)
            lbl.setAlignment(Qt.AlignmentFlag.AlignRight)
            head.addWidget(lbl)
        self.body.addLayout(head)
        self.cols = cols
        self.app_rows = [AppRow(icon=self.on("icon")) for _ in range(int(self.cfg.get("count", 8)))]
        for r in self.app_rows:
            self.body.addWidget(r)

    def _update_apps(self, d):
        if "procs" not in d:
            return
        sort = self.cfg.get("sort", "cpu")
        apps = group_apps(d["procs"], self.m)
        apps.sort(key=lambda a: -(a.get(sort) or 0))
        total_mem = d["mem"]["total"] or 1
        self.big.setText(f"{len(apps)} apps")
        for row, a in zip(self.app_rows, apps + [None] * len(self.app_rows)):
            if a is None:
                row.show_app(None, [], 0, self.colour)
                continue
            vals = []
            for c in self.cols:
                vals.append((c, {"cpu": f"{a['cpu']:.1f} %", "mem": human_bytes(a["mem"]),
                                 "gpu": f"{a['gpu']:.0f} %", "vram": human_bytes(a["vram"]) if a["vram"] else "–",
                                 "disk": human_rate(a["disk"]) if a["disk"] else "–",
                                 "pid": str(a["pids"][0]) + ("+" if len(a["pids"]) > 1 else "")}[c]))
            frac = {"cpu": a["cpu"] / 100, "mem": a["mem"] / total_mem, "gpu": a["gpu"] / 100}.get(sort, 0)
            if sort == "vram":
                tot = max((g.get("vram_total") or 0) for g in d["gpus"]) if d["gpus"] else 0
                frac = a["vram"] / tot if tot else 0
            row.show_app(a, vals, frac, self.colour)

    # ── disks, network ──────────────────────────────────────────────────────────
    def _build_disk(self):
        if self.on("rates"):
            self._row("read", "Read")
            self._row("write", "Write")
        if self.on("graph"):
            self._graph()
        if self.on("drives"):
            self.drive_box = QVBoxLayout()
            self.drive_box.setSpacing(4)
            self.body.addLayout(self.drive_box)
            self.drive_rows = {}

    def _update_disk(self, d):
        dk = d["disk"]
        self.big.setText(human_rate(dk["read"] + dk["write"]))
        if "read" in self.parts:
            self.parts["read"].setText(human_rate(dk["read"]))
            self.parts["write"].setText(human_rate(dk["write"]))
        self._set_graph("graph", [self.m.series("disk:read", self._n()), self.m.series("disk:write", self._n())],
                        [self.colour, "#E05555"], None)
        if self.on("drives"):
            for dr in dk["drives"]:
                row = self.drive_rows.get(dr["mount"])
                if row is None:
                    w = QWidget()
                    lay = QVBoxLayout(w)
                    lay.setContentsMargins(0, 2, 0, 0)
                    lay.setSpacing(3)
                    top = QHBoxLayout()
                    name = widgets.elided(dr["mount"], 11.5, 500)
                    top.addWidget(name, 1)
                    val = widgets.label("", 11, mono=True, tone="secondary")
                    top.addWidget(val)
                    lay.addLayout(top)
                    meter = Meter(5)
                    lay.addWidget(meter)
                    self.drive_box.addWidget(w)
                    row = self.drive_rows[dr["mount"]] = (val, meter)
                free = dr["total"] - dr["used"]
                row[0].setText(f"{human_bytes(free)} free of {human_bytes(dr['total'], 0)}")
                row[1].set(dr["percent"] / 100, "#E05555" if dr["percent"] >= 90 else self.colour)

    def _build_network(self):
        if self.on("rates"):
            self._row("down", "↓ Download")
            self._row("up", "↑ Upload")
        if self.on("graph"):
            self._graph()
        if self.on("totals"):
            self._row("recv", "Downloaded")
            self._row("sent", "Uploaded")

    def _update_network(self, d):
        n = d["net"]
        self.big.setText(human_rate(n["down"]))
        if "down" in self.parts:
            self.parts["down"].setText(human_rate(n["down"]))
            self.parts["up"].setText(human_rate(n["up"]))
        if "recv" in self.parts:
            self.parts["recv"].setText(human_bytes(n["recv"]))
            self.parts["sent"].setText(human_bytes(n["sent"]))
        self._set_graph("graph", [self.m.series("net:down", self._n()), self.m.series("net:up", self._n())],
                        [self.colour, "#D6409F"], None)

    # ── temperatures, battery, system ───────────────────────────────────────────
    def _build_temps(self):
        self.temp_box = QVBoxLayout()
        self.temp_box.setSpacing(4)
        self.body.addLayout(self.temp_box)
        self.temp_rows = {}
        if self.on("graph"):
            self._graph()

    def _temp_row(self, key: str, text: str, t: float | None):
        val = self.temp_rows.get(key)
        if val is None:
            row, val = _kv(text)
            self.temp_box.addWidget(row)
            self.temp_rows[key] = val
        val.setText(f"{t:.0f} °C" if t is not None else "–")
        val.setStyleSheet(f"color: {_temp_colour(t)};")

    def _update_temps(self, d):
        hottest = []
        if self.on("cpu"):
            t = d["cpu"].get("temp")
            self._temp_row("cpu", "Processor", t)
            hottest.append(t)
        if self.on("gpu"):
            for g in d["gpus"]:
                self._temp_row(g["id"], _short_gpu(g["name"]), g.get("temp"))
                hottest.append(g.get("temp"))
        if self.on("other"):
            skip = {"k10temp", "zenpower", "coretemp", "amdgpu", "nouveau", "i915", "xe"}
            names = {"nvme": "SSD", "spd5118": "RAM", "r8169": "Network", "iwlwifi": "Wi-Fi", "acpitz": "Board",
                     "prom21_xhci": "USB controller", "mt7921": "Wi-Fi", "asus": "Board", "nct": "Board"}
            for chip, label, t in d["sensors"]:
                if chip in skip:
                    continue
                nice = next((v for k, v in names.items() if chip.startswith(k)), chip)
                text = nice if label in (chip, "", "Composite") else f"{nice} · {label}"
                self._temp_row(f"{chip}:{label}", text, t)
        hot = [t for t in hottest if t is not None] + [t for _c, _l, t in d["sensors"]]
        self.big.setText(f"{max(hot):.0f} °C" if hot else "")
        self._set_graph("graph", [self.m.series("hottest", self._n())], [self.colour])

    def _build_battery(self):
        if self.on("percent"):
            self._row("state", "State")
        if self.on("time"):
            self._row("time", "Time left")
        if self.on("bar"):
            self.parts["bar"] = Meter(8)
            self.body.addWidget(self.parts["bar"])

    def _update_battery(self, d):
        b = d.get("battery")
        if b is None:
            self.big.setText("")
            if "state" in self.parts:
                self.parts["state"].setText("No battery")
            return
        self.big.setText(f"{b['percent']:.0f} %")
        if "state" in self.parts:
            self.parts["state"].setText("Charging" if b["plugged"] else "On battery")
        if "time" in self.parts:
            self.parts["time"].setText(perfstats.duration(b["secs_left"]) if b["secs_left"] else "–")
        if "bar" in self.parts:
            self.parts["bar"].set(b["percent"] / 100, "#E05555" if b["percent"] < 20 else self.colour)

    def _build_system(self):
        for key, text in (("uptime", "Up for"), ("procs", "Processes"), ("threads", "Threads"),
                          ("load", "Load average")):
            if self.on(key) and not (key == "load" and IS_WIN):
                self._row(key, text)

    def _update_system(self, d):
        s = d["system"]
        if "uptime" in self.parts:
            self.parts["uptime"].setText(perfstats.duration(s["uptime"]) if s.get("uptime") else "–")
        if "procs" in self.parts:
            self.parts["procs"].setText(str(s.get("procs") or "–"))
        if "threads" in self.parts:
            self.parts["threads"].setText(str(s.get("threads") or "–"))
        if "load" in self.parts:
            la = d["cpu"].get("load_avg")
            self.parts["load"].setText("  ".join(f"{x:.2f}" for x in la) if la else "–")

    def update_from(self, d: dict):
        getattr(self, f"_update_{self.cfg['type']}", lambda _d: None)(d)


class AppRow(QWidget):
    """One app in a Top apps / VRAM widget: icon, name, a faint bar behind it for its share, its numbers."""

    def __init__(self, icon: bool = True, compact: bool = False):
        super().__init__()
        self.frac, self.colour = 0.0, ""
        lay = QHBoxLayout(self)
        lay.setContentsMargins(4, 2, 4, 2)
        lay.setSpacing(8)
        self.icon = QLabel()
        self.icon.setFixedSize(18, 18)
        self.icon.setVisible(icon)
        lay.addWidget(self.icon)
        self.name = widgets.elided("", 12)
        lay.addWidget(self.name, 1)
        self.vals: list[QLabel] = []
        self.lay = lay
        self.setFixedHeight(22 if compact else 26)

    def show_app(self, a: dict | None, vals: list[tuple[str, str]], frac: float, colour: str):
        self.setVisible(a is not None)
        if a is None:
            return
        self.frac, self.colour = max(0.0, min(1.0, frac)), colour
        if self.icon.isVisible():
            self.icon.setPixmap(icons.get(a).pixmap(18, 18))
        self.name.setText(a["name"] + (f"  ({len(a['pids'])})" if len(a["pids"]) > 1 else ""))
        self.name.setToolTip(a.get("exe") or a["name"])
        while len(self.vals) < len(vals):
            lbl = widgets.label("", 11.5, 500, mono=True)
            lbl.setFixedWidth(64)
            lbl.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
            self.lay.addWidget(lbl)
            self.vals.append(lbl)
        for lbl, (_k, text) in zip(self.vals, vals):
            lbl.setText(text)
        self.update()

    def paintEvent(self, e):
        if self.frac <= 0:
            return
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        c = QColor(self.colour or theme.P.primary)
        c.setAlpha(38)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(c)
        p.drawRoundedRect(QRectF(0, 1, self.width() * self.frac, self.height() - 2), 5, 5)
        p.end()


def group_apps(procs: list[dict], m) -> list[dict]:
    """Processes with the same name added up into one app."""
    out: dict[str, dict] = {}
    for p in procs:
        a = out.get(p["name"])
        if a is None:
            a = out[p["name"]] = {"name": p["name"], "proc": p["proc"], "exe": p["exe"], "cmd": p["cmd"],
                                  "pid": p["pid"], "pids": [], "cpu": 0.0, "mem": 0, "gpu": 0.0, "vram": 0,
                                  "disk": 0.0, "threads": 0, "user": p["user"]}
        a["pids"].append(p["pid"])
        a["cpu"] += m.proc_cpu(p)
        a["mem"] += p["mem"]
        a["gpu"] = min(100.0, a["gpu"] + (p["gpu"] or 0))
        a["vram"] += p["vram"] or 0
        a["disk"] += p["disk"] or 0
        a["threads"] += p["threads"] or 0
    return list(out.values())


# ══ Overview ══════════════════════════════════════════════════════════════════════
class OverviewPage(QWidget):
    def __init__(self, m):
        super().__init__()
        self.m = m
        self.arranging = False
        self.cards: list[WidgetCard] = []
        lay = QVBoxLayout(self)
        lay.setContentsMargins(16, 12, 16, 0)
        lay.setSpacing(8)
        bar = QHBoxLayout()
        bar.setSpacing(6)
        bar.addWidget(widgets.label("Overview", 18, 600))
        self.game_chip = widgets.label("", 10.5, 600, mono=True, tone="accent")
        bar.addWidget(self.game_chip)
        bar.addStretch()
        add = widgets.button("+ Add widget", "primary")
        add.clicked.connect(lambda: self._add_menu(add))
        bar.addWidget(add)
        self.arr = widgets.button("Arrange")
        self.arr.setCheckable(True)
        self.arr.setToolTip("Show ◀ ▶ ✕ on every widget to move and remove them")
        self.arr.toggled.connect(self._arrange)
        bar.addWidget(self.arr)
        more = widgets.button("Layout ▾")
        more.clicked.connect(lambda: self._layout_menu(more))
        bar.addWidget(more)
        lay.addLayout(bar)
        self.scroll = QScrollArea()
        self.scroll.setWidgetResizable(True)
        self.scroll.setFrameShape(QFrame.Shape.NoFrame)
        inner = QWidget()
        outer = QVBoxLayout(inner)
        outer.setContentsMargins(0, 4, 0, 16)
        self.grid = QGridLayout()
        self.grid.setSpacing(10)
        outer.addLayout(self.grid)
        outer.addStretch()
        self.scroll.setWidget(inner)
        lay.addWidget(self.scroll, 1)
        self.build()
        self.game_changed()

    def game_changed(self):
        self.game_chip.setText("  🎮 GAME MODE ON" if self.m.game_on else "")

    def wants_processes(self) -> bool:
        return any(c.wants_processes() for c in self.cards)

    def build(self):
        for c in self.cards:
            self.grid.removeWidget(c)
            c.deleteLater()
        self.cards = []
        cols = max(1, min(4, int(self.m.data.get("columns", 3))))
        for i in range(4):
            self.grid.setColumnStretch(i, 1 if i < cols else 0)
        r = c = 0
        for cfg in self.m.data["widgets"]:
            span = max(1, min(cols, int(cfg.get("span", 1))))
            if c + span > cols:
                r, c = r + 1, 0
            try:
                card = WidgetCard(self, cfg)
            except Exception:
                log.exception("could not build a %s widget", cfg.get("type"))
                continue
            self.grid.addWidget(card, r, c, 1, span, Qt.AlignmentFlag.AlignTop)
            self.cards.append(card)
            c += span
            if c >= cols:
                r, c = r + 1, 0
        if self.m.last:
            self.update_from(self.m.last)
        self.m._refresh_wants()

    def update_from(self, d: dict):
        for c in self.cards:
            try:
                c.update_from(d)
            except Exception:
                log.exception("the %s widget could not show the numbers", c.cfg.get("type"))

    def _arrange(self, on: bool):
        self.arranging = on
        for c in self.cards:
            for b in c.arrange:
                b.setVisible(on)

    # ── changing the widgets ────────────────────────────────────────────────────
    def _index(self, card) -> int:
        return next(i for i, w in enumerate(self.m.data["widgets"]) if w is card.cfg)

    def _changed(self):
        self.m.save()
        self.build()

    def move(self, card, by: int):
        ws = self.m.data["widgets"]
        i = self._index(card)
        j = max(0, min(len(ws) - 1, i + by))
        ws.insert(j, ws.pop(i))
        self._changed()

    def remove(self, card):
        self.m.data["widgets"].pop(self._index(card))
        self._changed()

    def resize_card(self, card, by: int):
        cols = int(self.m.data.get("columns", 3))
        card.cfg["span"] = max(1, min(cols, int(card.cfg.get("span", 1)) + by))
        self._changed()

    def duplicate(self, card):
        cfg = copy.deepcopy(card.cfg)
        cfg["id"] = self.m.new_widget_id()
        self.m.data["widgets"].insert(self._index(card) + 1, cfg)
        self._changed()

    def edit(self, card):
        dlg = WidgetEditor(self, self.m, card.cfg)
        if dlg.exec():
            card.cfg.clear()
            card.cfg.update(dlg.result_cfg())
            self._changed()

    def add(self, kind: str, source: str = ""):
        cfg = {"id": self.m.new_widget_id(), "type": kind, "span": 2 if kind == "apps" else 1}
        if source:
            cfg["source"] = source
        if kind == "apps":
            cfg.update({"sort": "cpu", "count": 8})
        self.m.data["widgets"].append(cfg)
        self._changed()
        QTimer.singleShot(50, lambda: self.scroll.verticalScrollBar().setValue(
            self.scroll.verticalScrollBar().maximum()) if alive(self.scroll) else None)

    def menu_for(self, card, button=None, pos=None):
        menu = widgets.menu(self)
        menu.addAction("Edit…", lambda: self.edit(card))
        menu.addSeparator()
        menu.addAction("Move earlier", lambda: self.move(card, -1))
        menu.addAction("Move later", lambda: self.move(card, 1))
        menu.addAction("Wider", lambda: self.resize_card(card, 1))
        menu.addAction("Narrower", lambda: self.resize_card(card, -1))
        menu.addAction("Duplicate", lambda: self.duplicate(card))
        menu.addSeparator()
        menu.addAction("Remove", lambda: self.remove(card))
        menu.exec(pos or button.mapToGlobal(button.rect().bottomLeft()))

    def _add_menu(self, button):
        menu = widgets.menu(self)
        gpus = (self.m.last or {}).get("gpus") or []
        for kind, (title, _c, _e) in TYPES.items():
            if kind in ("gpu", "vram") and len(gpus) > 1:
                sub = menu.addMenu(title)
                sub.addAction("The main graphics card", lambda k=kind: self.add(k, "main"))
                for g in gpus:
                    sub.addAction(_short_gpu(g["name"]) + ("  (built-in)" if g.get("integrated") else ""),
                                  lambda k=kind, gid=g["id"]: self.add(k, gid))
            else:
                menu.addAction(title, lambda k=kind: self.add(k, "main" if k in ("gpu", "vram") else ""))
        menu.exec(button.mapToGlobal(button.rect().bottomLeft()))

    def _layout_menu(self, button):
        menu = widgets.menu(self)
        cols = int(self.m.data.get("columns", 3))
        for n in (1, 2, 3, 4):
            a = menu.addAction(f"{n} column{'s' if n > 1 else ''}", lambda n=n: self._columns(n))
            a.setCheckable(True)
            a.setChecked(n == cols)
        menu.addSeparator()
        menu.addAction("Back to the standard widgets", self._reset)
        menu.exec(button.mapToGlobal(button.rect().bottomLeft()))

    def _columns(self, n: int):
        self.m.data["columns"] = n
        self._changed()

    def _reset(self):
        if widgets.confirm(self, "Performance", "Put the standard widgets back? Yours are removed.", "Reset"):
            from modules.performance import DEFAULT_WIDGETS
            self.m.data["widgets"] = copy.deepcopy(DEFAULT_WIDGETS)
            for i, w in enumerate(self.m.data["widgets"]):
                w["id"] = f"w{i}"
            self._changed()


class WidgetEditor(widgets.RoundedDialog):
    """Everything about one widget: its title, what it shows, which GPU / how many apps, graph, colour, width."""

    def __init__(self, parent, m, cfg: dict):
        t = TYPES.get(cfg["type"], TYPES["cpu"])
        super().__init__(parent, f"Edit {t[0]}", resizable=False)
        self.m, self.cfg, self.t = m, copy.deepcopy(cfg), t
        b = self.body
        b.addWidget(widgets.label("Title", 12, 500))
        self.title = QLineEdit(cfg.get("title", ""))
        self.title.setPlaceholderText(t[0])
        b.addWidget(self.title)
        self.source = None
        gpus = (m.last or {}).get("gpus") or []
        if cfg["type"] in ("gpu", "vram"):
            b.addWidget(widgets.label("Graphics card", 12, 500))
            self.source = QComboBox()
            self.source.addItem("The main graphics card", "main")
            for g in gpus:
                self.source.addItem(g["name"] + ("  (built-in)" if g.get("integrated") else ""), g["id"])
            self.source.setCurrentIndex(max(0, self.source.findData(cfg.get("source", "main"))))
            b.addWidget(self.source)
        self.sort = self.count = None
        if cfg["type"] in ("apps", "vram"):
            row = QHBoxLayout()
            if cfg["type"] == "apps":
                row.addWidget(widgets.label("Sort by", 12))
                self.sort = QComboBox()
                for v, txt in SORTS:
                    self.sort.addItem(txt, v)
                self.sort.setCurrentIndex(max(0, self.sort.findData(cfg.get("sort", "cpu"))))
                row.addWidget(self.sort)
            row.addWidget(widgets.label("How many", 12))
            self.count = QSpinBox()
            self.count.setRange(1, 25)
            self.count.setValue(int(cfg.get("count", 8 if cfg["type"] == "apps" else 5)))
            row.addWidget(self.count)
            row.addStretch()
            b.addLayout(row)
        b.addWidget(widgets.label("Shows", 12, 500))
        grid = QGridLayout()
        grid.setHorizontalSpacing(16)
        els = cfg.get("elements")
        on = set(els) if isinstance(els, list) else {k for k, _l, d in t[2] if d}
        self.checks = {}
        for i, (k, label, _d) in enumerate(t[2]):
            cb = QCheckBox(label)
            cb.setChecked(k in on)
            grid.addWidget(cb, i // 2, i % 2)
            self.checks[k] = cb
        b.addLayout(grid)
        form = QGridLayout()
        form.setHorizontalSpacing(10)
        form.setVerticalSpacing(8)
        self.style = QComboBox()
        for v, txt in STYLES:
            self.style.addItem(txt, v)
        self.style.setCurrentIndex(max(0, self.style.findData(cfg.get("style", "area"))))
        self.history = QComboBox()
        for v, txt in HISTORIES:
            self.history.addItem(txt, v)
        self.history.setCurrentIndex(max(0, self.history.findData(int(cfg.get("history", 60)))))
        self.height_ = QComboBox()
        for v, txt in (("small", "Small"), ("medium", "Medium"), ("large", "Large")):
            self.height_.addItem(txt, v)
        self.height_.setCurrentIndex(max(0, self.height_.findData(cfg.get("height", "medium"))))
        self.span = QComboBox()
        for n in (1, 2, 3, 4):
            self.span.addItem(f"{n} column{'s' if n > 1 else ''}", n)
        self.span.setCurrentIndex(max(0, self.span.findData(int(cfg.get("span", 1)))))
        self.colour = QComboBox()
        for v, txt in COLOURS:
            pm = QPixmap(14, 14)
            pm.fill(QColor(v or t[1] or theme.P.primary))
            self.colour.addItem(QIcon(pm), txt if v else f"{txt} (standard)", v)
        self.colour.setCurrentIndex(max(0, self.colour.findData(cfg.get("colour", ""))))
        for i, (label, w) in enumerate((("Graph", self.style), ("History", self.history), ("Graph size", self.height_),
                                        ("Width", self.span), ("Colour", self.colour))):
            form.addWidget(widgets.label(label, 12), i // 2, (i % 2) * 2)
            form.addWidget(w, i // 2, (i % 2) * 2 + 1)
        b.addLayout(form)
        row = QHBoxLayout()
        row.addStretch()
        cancel = widgets.button("Cancel")
        cancel.clicked.connect(self.reject)
        ok = widgets.button("Save", "primary")
        ok.clicked.connect(self.accept)
        row.addWidget(cancel)
        row.addWidget(ok)
        b.addLayout(row)
        self.resize(480, self.sizeHint().height())

    def result_cfg(self) -> dict:
        c = self.cfg
        c["title"] = self.title.text().strip()
        if not c["title"]:
            c.pop("title")
        c["elements"] = [k for k, cb in self.checks.items() if cb.isChecked()]
        if self.source is not None:
            c["source"] = self.source.currentData()
        if self.sort is not None:
            c["sort"] = self.sort.currentData()
        if self.count is not None:
            c["count"] = self.count.value()
        c["style"] = self.style.currentData()
        c["history"] = self.history.currentData()
        c["height"] = self.height_.currentData()
        c["span"] = self.span.currentData()
        c["colour"] = self.colour.currentData()
        return c


# ══ Processes ═════════════════════════════════════════════════════════════════════
PROC_COLUMNS = [("name", "Name", 240), ("pid", "PID", 70), ("cpu", "CPU", 70), ("mem", "Memory", 90),
                ("gpu", "GPU", 60), ("vram", "VRAM", 84), ("disk", "Disk", 84), ("threads", "Threads", 66),
                ("priority", "Priority", 96), ("user", "User", 90), ("cmd", "Command line", 300)]
NUMERIC = {"pid", "cpu", "mem", "gpu", "vram", "disk", "threads"}


class _Item(QTreeWidgetItem):
    def __lt__(self, other):
        tree = self.treeWidget()
        col = tree.sortColumn() if tree else 0
        a, b = self.data(col, Qt.ItemDataRole.UserRole), other.data(col, Qt.ItemDataRole.UserRole)
        if isinstance(a, (int, float)) and isinstance(b, (int, float)):
            return a < b
        return (self.text(col) or "").lower() < (other.text(col) or "").lower()


class ProcessesPage(QWidget):
    def __init__(self, m):
        super().__init__()
        self.m = m
        self.items: dict[str, _Item] = {}
        self.paused = False
        lay = QVBoxLayout(self)
        lay.setContentsMargins(16, 12, 16, 12)
        lay.setSpacing(8)
        top = QHBoxLayout()
        top.addWidget(widgets.label("Processes", 18, 600))
        self.summary = widgets.label("", 11, mono=True, tone="muted")
        top.addWidget(self.summary)
        top.addStretch()
        lay.addLayout(top)
        bar = QHBoxLayout()
        bar.setSpacing(6)
        self.search = QLineEdit()
        self.search.setPlaceholderText("Search apps, scripts, PIDs, command lines…")
        self.search.setClearButtonEnabled(True)
        self.search.textChanged.connect(lambda _t: self._refresh())
        bar.addWidget(self.search, 1)
        self.group = QCheckBox("Group by app")
        self.group.setChecked(bool(m.data.get("group_apps", True)))
        self.group.toggled.connect(self._group_toggled)
        bar.addWidget(self.group)
        self.mine = QCheckBox("Only mine")
        self.mine.setToolTip("Hide processes that belong to the system and other users")
        self.mine.toggled.connect(lambda _v: self._refresh())
        bar.addWidget(self.mine)
        cols = widgets.button("Columns ▾")
        cols.clicked.connect(lambda: self._columns_menu(cols))
        bar.addWidget(cols)
        self.end_btn = widgets.button("End task", "danger" if True else "")
        self.end_btn.clicked.connect(lambda: self._end(False))
        bar.addWidget(self.end_btn)
        lay.addLayout(bar)
        self.tree = QTreeWidget()
        self.tree.setColumnCount(len(PROC_COLUMNS))
        self.tree.setHeaderLabels([t for _k, t, _w in PROC_COLUMNS])
        self.tree.setRootIsDecorated(True)
        self.tree.setUniformRowHeights(True)
        self.tree.setAlternatingRowColors(False)
        self.tree.setIconSize(QSize(18, 18))
        self.tree.setSelectionMode(QAbstractItemView.SelectionMode.ExtendedSelection)
        self.tree.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.tree.customContextMenuRequested.connect(self._menu)
        self.tree.setSortingEnabled(True)
        self.tree.sortByColumn(2, Qt.SortOrder.DescendingOrder)
        h = self.tree.header()
        h.setStretchLastSection(False)
        h.setSectionResizeMode(0, QHeaderView.ResizeMode.Stretch)
        for i, (_k, _t, w) in enumerate(PROC_COLUMNS):
            if i:
                self.tree.setColumnWidth(i, w)
        for i, (_k, _t, _w) in enumerate(PROC_COLUMNS):
            h.setDefaultAlignment(Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter)
        self._apply_columns()
        lay.addWidget(self.tree, 1)
        foot = QHBoxLayout()
        self.selected_lbl = _hint("Right-click an app to end it, change its priority or the cores it may use.")
        foot.addWidget(self.selected_lbl, 1)
        lay.addLayout(foot)
        self.last_procs: list = []

    def wants_processes(self) -> bool:
        return True

    def game_changed(self):
        pass

    def _apply_columns(self):
        shown = set(self.m.data.get("proc_columns") or [])
        for i, (k, _t, _w) in enumerate(PROC_COLUMNS):
            self.tree.setColumnHidden(i, i != 0 and k not in shown)

    def _columns_menu(self, button):
        menu = widgets.menu(self)
        shown = list(self.m.data.get("proc_columns") or [])
        for k, t, _w in PROC_COLUMNS[1:]:
            a = menu.addAction(t)
            a.setCheckable(True)
            a.setChecked(k in shown)
            a.toggled.connect(lambda on, k=k: self._toggle_column(k, on))
        menu.exec(button.mapToGlobal(button.rect().bottomLeft()))

    def _toggle_column(self, k: str, on: bool):
        cols = [c for c in (self.m.data.get("proc_columns") or []) if c != k]
        if on:
            cols.append(k)
        self.m.data["proc_columns"] = cols
        self.m.save()
        self._apply_columns()

    def _group_toggled(self, on: bool):
        self.m.data["group_apps"] = on
        self.m.save()
        self.tree.clear()
        self.items = {}
        self._refresh()

    # ── filling the tree ────────────────────────────────────────────────────────
    def update_from(self, d: dict):
        if "procs" not in d:
            return
        self.last_procs = d["procs"]
        self.total_mem = d["mem"]["total"] or 1
        self.vram_total = max([g.get("vram_total") or 0 for g in d["gpus"]] or [0]) or 1
        if not self.paused:
            self._refresh()

    def _visible(self, p: dict) -> bool:
        if self.mine.isChecked():
            me = os.environ.get("USER") or os.environ.get("USERNAME") or ""
            if me and not (p["user"] or "").split("\\")[-1] == me:
                return False
        q = self.search.text().strip().lower()
        if not q:
            return True
        return q in p["name"].lower() or q == str(p["pid"]) or q in " ".join(p.get("cmd") or []).lower() \
            or q in (p.get("exe") or "").lower()

    def _values(self, p: dict) -> dict:
        cpu = self.m.proc_cpu(p) if "pids" not in p else p["cpu"]
        return {"name": p["name"], "pid": p["pid"], "cpu": cpu, "mem": p["mem"], "gpu": p["gpu"] or 0.0,
                "vram": p["vram"] or 0, "disk": p["disk"] or 0.0, "threads": p["threads"] or 0,
                "priority": dict(perftweaks.PRIORITIES).get(perftweaks.priority_of(p.get("nice")), ""),
                "user": (p.get("user") or "").split("\\")[-1], "cmd": " ".join(p.get("cmd") or [])}

    def _fill(self, item: _Item, p: dict, group: bool = False):
        v = self._values(p)
        texts = {"name": v["name"] + (f"  ({len(p['pids'])})" if group and len(p["pids"]) > 1 else ""),
                 "pid": "" if group and len(p["pids"]) > 1 else str(v["pid"]),
                 "cpu": f"{v['cpu']:.1f} %", "mem": human_bytes(v["mem"]),
                 "gpu": f"{v['gpu']:.0f} %" if v["gpu"] else "", "vram": human_bytes(v["vram"]) if v["vram"] else "",
                 "disk": human_rate(v["disk"]) if v["disk"] else "", "threads": str(v["threads"]),
                 "priority": "" if group and len(p["pids"]) > 1 else v["priority"], "user": v["user"],
                 "cmd": v["cmd"] if not group or len(p["pids"]) == 1 else ""}
        heat = {"cpu": v["cpu"] / 100, "mem": v["mem"] / self.total_mem * 4, "gpu": v["gpu"] / 100,
                "vram": v["vram"] / self.vram_total * 2}
        accent = QColor(theme.P.primary)
        for i, (k, _t, _w) in enumerate(PROC_COLUMNS):
            if item.text(i) != texts[k]:
                item.setText(i, texts[k])
            item.setData(i, Qt.ItemDataRole.UserRole, v[k] if k in NUMERIC else texts[k])
            if k in NUMERIC and k != "pid":
                item.setTextAlignment(i, Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
            if k in heat:
                c = QColor(accent)
                c.setAlpha(int(min(1.0, heat[k]) * 150))
                item.setBackground(i, c if heat[k] > 0.02 else QColor(0, 0, 0, 0))
        if item.icon(0).isNull():
            item.setIcon(0, icons.get(p))
        item.setData(0, Qt.ItemDataRole.UserRole + 1, p["pids"] if "pids" in p else [p["pid"]])
        item.setToolTip(0, p.get("exe") or p["name"])

    def _refresh(self):
        procs = [p for p in self.last_procs if self._visible(p)]
        self.tree.setSortingEnabled(False)
        keep = set()
        if self.group.isChecked():
            by: dict[str, list] = {}
            for p in procs:
                by.setdefault(p["name"], []).append(p)
            apps = {a["name"]: a for a in group_apps(procs, self.m)}
            for name, members in by.items():
                key = f"g:{name}"
                keep.add(key)
                it = self.items.get(key)
                if it is None:
                    it = self.items[key] = _Item()
                    self.tree.addTopLevelItem(it)
                self._fill(it, apps[name], group=True)
                if len(members) > 1:
                    for p in members:
                        ck = f"p:{p['pid']}"
                        keep.add(ck)
                        child = self.items.get(ck)
                        if child is None or child.parent() is not it:
                            if child is not None:
                                (child.parent() or self.tree.invisibleRootItem()).removeChild(child)
                            child = self.items[ck] = _Item()
                            it.addChild(child)
                        self._fill(child, p)
        else:
            for p in procs:
                key = f"p:{p['pid']}"
                keep.add(key)
                it = self.items.get(key)
                if it is None:
                    it = self.items[key] = _Item()
                    self.tree.addTopLevelItem(it)
                self._fill(it, p)
        for key in [k for k in self.items if k not in keep]:
            it = self.items.pop(key)
            parent = it.parent()
            if parent is not None:
                parent.removeChild(it)
            else:
                idx = self.tree.indexOfTopLevelItem(it)
                if idx >= 0:
                    self.tree.takeTopLevelItem(idx)
        self.tree.setSortingEnabled(True)
        total_cpu = sum(self.m.proc_cpu(p) for p in procs)
        self.summary.setText(f"  {len(procs)} processes  ·  CPU {total_cpu:.0f} %  ·  "
                             f"RAM {human_bytes(sum(p['mem'] for p in procs))}  ·  "
                             f"VRAM {human_bytes(sum(p['vram'] or 0 for p in procs))}")

    # ── acting on processes ─────────────────────────────────────────────────────
    def _selected(self) -> list[tuple[int, dict]]:
        out, seen = [], set()
        by_pid = {p["pid"]: p for p in self.last_procs}
        for it in self.tree.selectedItems():
            for pid in it.data(0, Qt.ItemDataRole.UserRole + 1) or []:
                if pid not in seen and pid in by_pid:
                    seen.add(pid)
                    out.append((pid, by_pid[pid]))
        return out

    def _end(self, force: bool):
        sel = self._selected()
        if not sel:
            return
        names = sorted({p["name"] for _pid, p in sel})
        what = names[0] if len(names) == 1 else f"{len(names)} apps"
        count = f" ({len(sel)} processes)" if len(sel) > 1 else ""
        if not widgets.confirm(self, "End task" if not force else "Force quit",
                               f"{'Force quit' if force else 'End'} {what}{count}? Anything unsaved in it is lost.",
                               "Force quit" if force else "End task", danger=True):
            return
        errors = []
        for pid, _p in sel:
            err = perftweaks.end_process(pid, force)
            if err and "AccessDenied" in err:
                errors.append(pid)
        if errors and not IS_WIN:
            if widgets.confirm(self, "End task", f"{len(errors)} of them belong to the system or another user. "
                               "End them as administrator (asks for your password)?", "End as administrator",
                               danger=True):
                for pid in errors:
                    err = perftweaks.end_process(pid, force, root_ok=True)
                    if err:
                        self.m.ctx.notify("Performance", f"Could not end {pid}: {err}")
                        break
        elif errors:
            self.m.ctx.notify("Performance", "Windows did not let Luminescent end that. Run it as administrator "
                                             "to end system processes.")

    def _priority(self, level: str):
        for pid, p in self._selected():
            err = perftweaks.set_priority(pid, level)
            if err:
                self.m.ctx.notify("Performance", f"Priority of {p['name']}: {err}")
                break

    def _cores(self):
        sel = self._selected()
        if not sel:
            return
        n = os.cpu_count() or 1
        try:
            import psutil
            current = set(psutil.Process(sel[0][0]).cpu_affinity())
        except Exception:
            current = set(range(n))
        dlg = widgets.RoundedDialog(self, "CPU cores")
        dlg.body.addWidget(_hint(f"Which cores {sel[0][1]['name']} may run on. Fewer keeps it out of a game's way."))
        grid = QGridLayout()
        checks = []
        for i in range(n):
            cb = QCheckBox(f"Core {i}")
            cb.setChecked(i in current)
            grid.addWidget(cb, i // 4, i % 4)
            checks.append(cb)
        dlg.body.addLayout(grid)
        row = QHBoxLayout()
        allb = widgets.button("All")
        allb.clicked.connect(lambda: [c.setChecked(True) for c in checks])
        row.addWidget(allb)
        row.addStretch()
        ok = widgets.button("Apply", "primary")
        ok.clicked.connect(dlg.accept)
        row.addWidget(ok)
        dlg.body.addLayout(row)
        if dlg.exec():
            cores = [i for i, c in enumerate(checks) if c.isChecked()] or list(range(n))
            for pid, p in sel:
                err = perftweaks.set_affinity(pid, cores)
                if err:
                    self.m.ctx.notify("Performance", f"Cores of {p['name']}: {err}")
                    break

    def _add_rule(self, name: str, level: str):
        rules = [r for r in self.m.data.get("rules") or [] if r.get("match", "").lower() != name.lower()]
        rules.append({"match": name, "priority": level, "cores": []})
        self.m.data["rules"] = rules
        self.m.save()
        self.m._refresh_wants()
        self.m.ctx.notify("Performance", f"{name} now always starts with {dict(perftweaks.PRIORITIES)[level]} "
                                         "priority (Alerts & rules).")

    def _add_to_game(self, name: str, key: str):
        lst = self.m.data["game"].setdefault(key, [])
        if name.lower() not in [n.lower() for n in lst]:
            lst.append(name)
            self.m.save()
            self.m._refresh_wants()

    def _menu(self, pos):
        sel = self._selected()
        if not sel:
            return
        self.paused = True
        p = sel[0][1]
        name = p["name"].split(" · ")[-1] if " · " in p["name"] else p["name"]
        menu = widgets.menu(self)
        menu.addAction("End task", lambda: self._end(False))
        menu.addAction("Force quit", lambda: self._end(True))
        pr = menu.addMenu("Priority")
        now = perftweaks.priority_of(p.get("nice"))
        for k, t in perftweaks.PRIORITIES:
            a = pr.addAction(t, lambda k=k: self._priority(k))
            a.setCheckable(True)
            a.setChecked(k == now and len(sel) == 1)
        menu.addAction("CPU cores…", self._cores)
        menu.addSeparator()
        always = menu.addMenu(f"Always give {name}")
        for k, t in perftweaks.PRIORITIES:
            always.addAction(f"{t} priority", lambda k=k: self._add_rule(name, k))
        game = menu.addMenu("Game mode")
        game.addAction(f"Start Game mode when {name} runs", lambda: self._add_to_game(name, "auto"))
        game.addAction(f"Lower {name}'s priority in Game mode", lambda: self._add_to_game(name, "lower"))
        game.addAction(f"Close {name} when Game mode starts", lambda: self._add_to_game(name, "close"))
        menu.addSeparator()
        if p.get("exe"):
            menu.addAction("Open file location", lambda: perftweaks.open_location(p["exe"]))
        menu.addAction("Copy command line", lambda: QApplication.clipboard().setText(" ".join(p.get("cmd") or [])))
        menu.addAction("Copy PID", lambda: QApplication.clipboard().setText(
            " ".join(str(pid) for pid, _p in sel)))
        menu.exec(self.tree.viewport().mapToGlobal(pos))
        self.paused = False
        self._refresh()


# ══ settings rows (Boost) ═════════════════════════════════════════════════════════
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

    def wants_processes(self) -> bool:
        return False

    def update_from(self, d):
        pass

    def game_changed(self):
        pass


def option_row(o, on_set) -> QFrame:
    """A card for one perftweaks / syscontrol Option: a switch, a list or a slider."""
    box, lay = _card(4, (14, 10, 14, 10))
    top = QHBoxLayout()
    top.setSpacing(10)
    value = o.get()
    if o.kind == "bool":
        ctl = QCheckBox(o.label)
        ctl.setChecked(bool(value))
        ctl.toggled.connect(lambda v: on_set(o, bool(v), ctl))
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
            ctl.activated.connect(lambda _i: on_set(o, ctl.currentData(), ctl))
            ctl.setMinimumWidth(220)
            calm(ctl)
            top.addWidget(ctl)
        else:
            step = o.step or 1
            ctl = QSlider(Qt.Orientation.Horizontal)
            ctl.setRange(round(o.min / step), round(o.max / step))
            ctl.setValue(round(float(value or 0) / step))
            readout = widgets.label("", 12, 500, mono=True)
            readout.setText(f"{int(round(ctl.value() * step))}{o.suffix}")
            top.addWidget(readout)
            timer = QTimer(ctl)
            timer.setSingleShot(True)
            timer.setInterval(600)              # a password prompt per slider tick would be awful
            timer.timeout.connect(lambda: on_set(o, int(round(ctl.value() * step)), ctl))
            ctl.valueChanged.connect(lambda v: (readout.setText(f"{int(round(v * step))}{o.suffix}"), timer.start()))
            calm(ctl)
    lay.addLayout(top)
    if o.kind in ("int", "float"):
        lay.addWidget(ctl)
    if o.help:
        lay.addWidget(_hint(o.help))
    if value is None:
        box.setEnabled(False)
        box.setToolTip("Could not be read on this computer.")
    return box


class BoostPage(_Page):
    def __init__(self, m):
        super().__init__("Boost", "Settings that trade speed against heat, noise and battery. " +
                         ("Changes to the processor, memory and graphics card ask for your password (they need "
                          "root) and last until you restart, unless noted." if not IS_WIN else
                          "These change Windows' own settings and stay as you leave them."))
        self.m = m
        self.loading = _hint("Reading this computer's settings…")
        self.lay.addWidget(self.loading)
        self.at = self.lay.count()
        self._tools()
        self.lay.addStretch()
        in_thread(perftweaks.options, self._got)

    def _got(self, opts):
        if not alive(self):
            return
        self.loading.hide()
        if not isinstance(opts, list):
            self.loading.setText(f"Could not read the settings: {opts}")
            self.loading.show()
            return
        at = self.at
        for gid, title, icon in perftweaks.GROUPS:
            mine = [o for o in opts if o.group == gid]
            if not mine:
                continue
            self.lay.insertWidget(at, _heading(f"{icon}  {title}"))
            at += 1
            for o in mine:
                try:
                    self.lay.insertWidget(at, option_row(o, self._set))
                    at += 1
                except Exception:
                    log.exception("could not show %s", o.key)
        if not opts:
            self.lay.insertWidget(at, _hint("No speed settings could be read on this computer."))

    def _set(self, o, value, ctl):
        def done(err):
            if not alive(self):
                return
            if err:
                self.m.ctx.notify("Boost", f"{o.label}: {err}")
                cur = o.get()                         # show what it really is again
                ctl.blockSignals(True)
                if isinstance(ctl, QCheckBox):
                    ctl.setChecked(bool(cur))
                elif isinstance(ctl, QComboBox):
                    ctl.setCurrentIndex(max(0, ctl.findData(cur)))
                ctl.blockSignals(False)
            else:
                self.m.ctx.notify("Boost", f"{o.label}: done.")
        in_thread(lambda: o.set(value), done)

    def _tools(self):
        self.lay.addWidget(_heading("🧹  Clean up"))
        if not IS_WIN:
            box, lay = _card()
            row = QHBoxLayout()
            row.addWidget(widgets.label("Free cached memory", 13), 1)
            b = widgets.button("Free it")
            b.clicked.connect(self._drop)
            row.addWidget(b)
            lay.addLayout(row)
            lay.addWidget(_hint("Gives back the memory Linux keeps as file cache. It refills as you use files, so this "
                                "only helps right before something that needs a lot of RAM. Asks for your password."))
            self.lay.addWidget(box)
        for name, path in perftweaks.temp_places():
            if not path:
                continue
            box, lay = _card()
            row = QHBoxLayout()
            row.addWidget(widgets.label(name, 13), 1)
            size = widgets.label("…", 12, mono=True, tone="secondary")
            row.addWidget(size)
            b = widgets.button("Empty")
            b.clicked.connect(lambda _c=False, p=path, n=name, s=size: self._empty(n, p, s))
            row.addWidget(b)
            lay.addLayout(row)
            lay.addWidget(_hint(path))
            self.lay.addWidget(box)
            in_thread(lambda p=path: perftweaks.folder_size(p),
                      lambda n, s=size: s.setText(human_bytes(n)) if alive(s) and isinstance(n, int) else None)
        if IS_WIN:
            box, lay = _card()
            row = QHBoxLayout()
            row.addWidget(widgets.label("Ultimate Performance power plan", 13), 1)
            b = widgets.button("Add it")
            b.clicked.connect(lambda: in_thread(perftweaks.add_ultimate_plan, lambda err: self.m.ctx.notify(
                "Boost", err or "Added. Pick it in Power plan above (reopen this page).")))
            row.addWidget(b)
            lay.addLayout(row)
            lay.addWidget(_hint("Windows' hidden fastest plan: no power saving at all. Best on desktops."))
            self.lay.addWidget(box)

    def _drop(self):
        in_thread(perftweaks.drop_caches,
                  lambda err: self.m.ctx.notify("Boost", err or "Cached memory freed."))

    def _empty(self, name: str, path: str, size_lbl):
        if not widgets.confirm(self, name, f"Delete everything in {path}? Files that are in use are skipped.",
                               "Empty", danger=True):
            return

        def done(res):
            if isinstance(res, tuple):
                self.m.ctx.notify("Boost", f"{name}: freed {human_bytes(res[0])}" +
                                  (f", {res[1]} files in use were left." if res[1] else "."))
                if alive(size_lbl):
                    size_lbl.setText(human_bytes(perftweaks.folder_size(path)))
        in_thread(lambda: perftweaks.empty_folder(path), done)


# ══ Game mode ═════════════════════════════════════════════════════════════════════
class NameList(QFrame):
    """A list of app names with "add a running app", "type a name" and remove."""

    def __init__(self, m, key: str, title: str, hint: str):
        super().__init__()
        self.m, self.key = m, key
        self.setObjectName("lumCard")
        lay = QVBoxLayout(self)
        lay.setContentsMargins(14, 10, 14, 10)
        lay.setSpacing(6)
        lay.addWidget(widgets.label(title, 13, 500))
        lay.addWidget(_hint(hint))
        self.list = QListWidget()
        self.list.setMaximumHeight(110)
        lay.addWidget(self.list)
        row = QHBoxLayout()
        add = widgets.button("+ Running app ▾")
        add.clicked.connect(lambda: self._pick(add))
        row.addWidget(add)
        typed = widgets.button("Type a name…")
        typed.clicked.connect(self._type)
        row.addWidget(typed)
        row.addStretch()
        rm = widgets.button("Remove")
        rm.clicked.connect(self._remove)
        row.addWidget(rm)
        lay.addLayout(row)
        self._fill()

    def names(self) -> list[str]:
        return self.m.data["game"].setdefault(self.key, [])

    def _fill(self):
        self.list.clear()
        for n in self.names():
            self.list.addItem(QListWidgetItem(icons.get({"name": n, "proc": n, "exe": ""}), n))

    def _add(self, name: str):
        name = name.strip()
        if name and name.lower() not in [n.lower() for n in self.names()]:
            self.names().append(name)
            self.m.save()
            self.m._refresh_wants()
            self._fill()

    def _pick(self, button):
        procs = (self.m.last or {}).get("procs") or []
        if not procs:
            self.m.ctx.notify("Game mode", "The list of running apps is still being read; try again in a second.")
            self.m._want = (1.0, True)
            return
        apps = sorted(group_apps(procs, self.m), key=lambda a: -(a["mem"]))
        menu = widgets.menu(self)
        for a in apps[:40]:
            name = a["name"].split(" · ")[-1] if " · " in a["name"] else a["name"]
            menu.addAction(icons.get(a), name, lambda n=name: self._add(n))
        menu.exec(button.mapToGlobal(button.rect().bottomLeft()))

    def _type(self):
        text, ok = widgets.ask_text(self, "Add an app", "Its name or program (e.g. steam, cs2, minecraft):")
        if ok:
            self._add(text)

    def _remove(self):
        for it in self.list.selectedItems():
            n = it.text()
            lst = self.names()
            if n in lst:
                lst.remove(n)
        self.m.save()
        self.m._refresh_wants()
        self._fill()


class GamePage(_Page):
    def __init__(self, m):
        super().__init__("Game mode", "One switch for the fastest settings while you play. Everything it changes "
                                      "goes back when it stops.")
        self.m = m
        g = m.data["game"]
        box, lay = _card(8, (16, 14, 16, 14))
        row = QHBoxLayout()
        self.state = widgets.label("", 15, 600)
        row.addWidget(self.state, 1)
        self.btn = widgets.button("", "primary")
        self.btn.setMinimumWidth(170)
        self.btn.clicked.connect(m.toggle_game)
        row.addWidget(self.btn)
        lay.addLayout(row)
        self.sub = _hint("")
        lay.addWidget(self.sub)
        self.lay.addWidget(box)
        self.lay.addWidget(_heading("What it does"))
        opts = [("power", "Switch to the fastest power mode"),
                ("effects", "Turn off desktop animations, blur and shadows" if not IS_WIN else
                 "Turn off transparency and animations, turn on Windows Game Mode"),
                ("hud", "Show the gaming HUD")]
        if not IS_WIN:
            opts += [("dnd", "Do not disturb (no notification pop-ups)"),
                     ("free_memory", "Free cached memory first (asks for your password)")]
        card, cl = _card(6)
        for k, text in opts:
            cb = QCheckBox(text)
            cb.setChecked(bool(g.get(k)))
            cb.toggled.connect(lambda v, k=k: self._set(k, v))
            cl.addWidget(cb)
        self.lay.addWidget(card)
        self.lay.addWidget(NameList(m, "auto", "Start by itself when one of these runs",
                                    "Game mode turns on when the app starts and off when the last one closes. "
                                    "Steam games, Minecraft (java), Roblox…"))
        self.lay.addWidget(NameList(m, "lower", "Lower the priority of these",
                                    "Browsers, Discord, launchers: they keep running but give way to the game."))
        self.lay.addWidget(NameList(m, "close", "Close these when Game mode starts",
                                    "Asked politely to quit, like clicking ✕. They don't come back by themselves."))
        self.finish()
        self.game_changed()

    def finish(self):
        self.lay.addStretch()

    def _set(self, k, v):
        self.m.data["game"][k] = bool(v)
        self.m.save()

    def game_changed(self):
        on = self.m.game_on
        self.state.setText("🎮  Game mode is on" if on else "Game mode is off")
        self.btn.setText("Stop Game mode" if on else "Start Game mode")
        self.sub.setText(("Started by itself because a game from the list is running. " if self.m.game_auto else "")
                         + ("It can also be switched with a key: Settings → Performance." if not on else
                            "Stop it to put everything back."))


# ══ Alerts & rules ════════════════════════════════════════════════════════════════
ALERTS = [("cpu", "Processor busy", "%", True), ("mem", "Memory nearly full", "%", True),
          ("vram", "VRAM nearly full", "%", True), ("cpu_temp", "Processor too hot", "°C", True),
          ("gpu_temp", "GPU too hot", "°C", True), ("app_cpu", "One app using a lot of CPU", "%", True),
          ("disk_full", "A drive nearly full", "%", False), ("battery", "Battery low", "%", False)]


class AlertsPage(_Page):
    def __init__(self, m):
        super().__init__("Alerts & rules", "A notification when something stays too high, and priorities that are "
                                           "set for an app every time it starts.")
        self.m = m
        a = m.data["alerts"]
        master = QCheckBox("Watch in the background (also when this window is closed)")
        master.setChecked(bool(a.get("on")))
        master.toggled.connect(self._master)
        self.lay.addWidget(master)
        box, lay = _card(6)
        grid = QGridLayout()
        grid.setHorizontalSpacing(10)
        grid.setVerticalSpacing(6)
        for i, (k, label, unit, has_for) in enumerate(ALERTS):
            r = a[k]
            cb = QCheckBox(label)
            cb.setChecked(bool(r.get("on")))
            cb.toggled.connect(lambda v, k=k: self._set(k, "on", bool(v)))
            grid.addWidget(cb, i, 0)
            limit_key = "under" if k == "battery" else "over"
            grid.addWidget(widgets.label("below" if k == "battery" else "over", 12, tone="muted"), i, 1)
            sp = QSpinBox()
            sp.setRange(1, 150 if unit == "°C" else 100)
            sp.setSuffix(f" {unit}")
            sp.setValue(int(r.get(limit_key, 90)))
            sp.valueChanged.connect(lambda v, k=k, lk=limit_key: self._set(k, lk, v))
            calm(sp)
            grid.addWidget(sp, i, 2)
            if has_for:
                grid.addWidget(widgets.label("for", 12, tone="muted"), i, 3)
                sf = QSpinBox()
                sf.setRange(0, 3600)
                sf.setSuffix(" s")
                sf.setValue(int(r.get("for", 30)))
                sf.valueChanged.connect(lambda v, k=k: self._set(k, "for", v))
                calm(sf)
                grid.addWidget(sf, i, 4)
        grid.setColumnStretch(5, 1)
        lay.addLayout(grid)
        lay.addWidget(_hint("Each alert waits 10 minutes before it tells you again."))
        self.lay.addWidget(box)
        self.lay.addSpacing(6)
        self.lay.addWidget(_heading("Priority rules"))
        self.lay.addWidget(_hint("Every time a matching app starts it gets this priority (and these cores, if you "
                                 "pick any). Raising a priority above normal needs root on Linux, so rules there "
                                 "can only lower it. Right-click an app on the Processes page for a quick rule."))
        self.rules_box = QVBoxLayout()
        self.rules_box.setSpacing(6)
        self.lay.addLayout(self.rules_box)
        add = widgets.button("+ Add a rule")
        add.clicked.connect(self._add_rule)
        self.lay.addWidget(add, 0, Qt.AlignmentFlag.AlignLeft)
        self._rules()
        self.lay.addStretch()

    def _master(self, on):
        self.m.data["alerts"]["on"] = bool(on)
        self.m.save()
        self.m._refresh_wants()

    def _set(self, k, field, v):
        self.m.data["alerts"][k][field] = v
        self.m.save()
        self.m._refresh_wants()

    def _rules(self):
        widgets.clear_layout(self.rules_box)
        rules = self.m.data.get("rules") or []
        if not rules:
            self.rules_box.addWidget(_hint("No rules yet."))
        n = os.cpu_count() or 1
        for i, r in enumerate(rules):
            box, lay = _card(4, (12, 8, 12, 8))
            row = QHBoxLayout()
            name = QLineEdit(r.get("match", ""))
            name.setPlaceholderText("App name")
            name.editingFinished.connect(lambda i=i, w=name: self._rule(i, "match", w.text().strip()))
            row.addWidget(name, 2)
            pr = QComboBox()
            for k, t in perftweaks.PRIORITIES:
                pr.addItem(t, k)
            pr.setCurrentIndex(max(0, pr.findData(r.get("priority", "normal"))))
            pr.activated.connect(lambda _x, i=i, w=pr: self._rule(i, "priority", w.currentData()))
            calm(pr)
            row.addWidget(pr, 1)
            cores = QLineEdit(",".join(str(c) for c in r.get("cores") or []))
            cores.setPlaceholderText(f"cores (all; 0-{n - 1})")
            cores.setToolTip("Leave empty for every core, or list them: 0,1,2,3 or 0-3")
            cores.editingFinished.connect(lambda i=i, w=cores: self._rule(i, "cores", _parse_cores(w.text(), n)))
            row.addWidget(cores, 1)
            rm = widgets.icon_button("✕", "Remove this rule", 26, 11, ghost=True)
            rm.clicked.connect(lambda _c=False, i=i: self._remove(i))
            row.addWidget(rm)
            lay.addLayout(row)
            self.rules_box.addWidget(box)

    def _rule(self, i, field, value):
        rules = self.m.data.get("rules") or []
        if i < len(rules):
            rules[i][field] = value
            self.m._ruled = set()                  # apply again with the new settings
            self.m.save()

    def _remove(self, i):
        rules = self.m.data.get("rules") or []
        if i < len(rules):
            rules.pop(i)
            self.m.save()
            self.m._refresh_wants()
            self._rules()

    def _add_rule(self):
        self.m.data.setdefault("rules", []).append({"match": "", "priority": "below", "cores": []})
        self.m.save()
        self.m._refresh_wants()
        self._rules()


def _parse_cores(text: str, n: int) -> list[int]:
    out = set()
    for part in text.replace(" ", "").split(","):
        if not part:
            continue
        try:
            if "-" in part:
                a, b = (int(x) for x in part.split("-", 1))
                out.update(range(max(0, a), min(n - 1, b) + 1))
            else:
                c = int(part)
                if 0 <= c < n:
                    out.add(c)
        except ValueError:
            continue
    return sorted(out)


# ══ Startup apps ══════════════════════════════════════════════════════════════════
class StartupPage(_Page):
    def __init__(self, m):
        super().__init__("Startup apps", "What starts by itself when you log in. Fewer makes logging in faster and "
                                         "leaves more memory free.")
        self.m = m
        top = QHBoxLayout()
        top.addStretch()
        refresh = widgets.button("Refresh")
        refresh.clicked.connect(self.load)
        top.addWidget(refresh)
        self.lay.addLayout(top)
        self.box = QVBoxLayout()
        self.box.setSpacing(6)
        self.lay.addLayout(self.box)
        self.lay.addStretch()
        self.load()

    def load(self):
        widgets.clear_layout(self.box)
        self.box.addWidget(_hint("Reading…"))
        in_thread(perftweaks.startup_apps, self._got)

    def _got(self, apps):
        if not alive(self):
            return
        widgets.clear_layout(self.box)
        if not isinstance(apps, list) or not apps:
            self.box.addWidget(_hint("Nothing starts by itself here." if isinstance(apps, list) else str(apps)))
            return
        for a in apps:
            box, lay = _card(2, (12, 8, 12, 8))
            row = QHBoxLayout()
            row.setSpacing(10)
            ic = QLabel()
            icon = icons._from_name(a["icon"]) if a.get("icon") and not IS_WIN else None
            if icon is None and IS_WIN:
                exe = a["command"].strip('"').split('"')[0].split(" -")[0].strip()
                if os.path.exists(exe):
                    icon = icons.provider.icon(QFileInfo(exe))
            ic.setPixmap((icon or icons.letter(a["name"])).pixmap(22, 22))
            row.addWidget(ic)
            col = QVBoxLayout()
            col.setSpacing(0)
            col.addWidget(widgets.label(a["name"], 13, 500))
            cmd = widgets.elided(a["command"], 11, mono=True, tone="muted")
            cmd.setToolTip(a["command"])
            col.addWidget(cmd)
            row.addLayout(col, 1)
            row.addWidget(widgets.label(f"for {a['where']}", 11, tone="muted"))
            cb = QCheckBox("Starts")
            cb.setChecked(a["enabled"])
            cb.toggled.connect(lambda v, a=a, cb=cb: self._toggle(a, v, cb))
            row.addWidget(cb)
            lay.addLayout(row)
            self.box.addWidget(box)

    def _toggle(self, a, on, cb):
        err = perftweaks.set_startup(a, on)
        if err:
            self.m.ctx.notify("Startup apps", f"{a['name']}: {err}")
            cb.blockSignals(True)
            cb.setChecked(not on)
            cb.blockSignals(False)


# ══ Options (and the HUD) ═════════════════════════════════════════════════════════
class OptionsPage(_Page):
    def __init__(self, m):
        super().__init__("Options", "How often it measures, and the gaming HUD.")
        self.m = m
        d = m.data
        box, lay = _card(8)
        grid = QGridLayout()
        grid.setHorizontalSpacing(12)
        grid.setVerticalSpacing(8)
        self.interval = QComboBox()
        for v, t in ((0.5, "Twice a second"), (1.0, "Every second"), (2.0, "Every 2 seconds"),
                     (5.0, "Every 5 seconds")):
            self.interval.addItem(t, v)
        self.interval.setCurrentIndex(max(0, self.interval.findData(float(d.get("interval", 1.0)))))
        self.interval.activated.connect(lambda _i: self._put("interval", self.interval.currentData()))
        calm(self.interval)
        grid.addWidget(widgets.label("Measure", 13), 0, 0)
        grid.addWidget(self.interval, 0, 1)
        self.mode = QComboBox()
        self.mode.addItem("Of the whole CPU (like Task Manager)", "total")
        self.mode.addItem("Of one core (like top; can go over 100 %)", "core")
        self.mode.setCurrentIndex(max(0, self.mode.findData(d.get("cpu_mode", "total"))))
        self.mode.activated.connect(lambda _i: self._put("cpu_mode", self.mode.currentData()))
        calm(self.mode)
        grid.addWidget(widgets.label("CPU % of an app", 13), 1, 0)
        grid.addWidget(self.mode, 1, 1)
        grid.setColumnStretch(2, 1)
        lay.addLayout(grid)
        lay.addWidget(_hint("It only measures while a Performance page or the HUD is on screen, or while alerts, "
                            "rules or automatic Game mode need it."))
        self.lay.addWidget(box)

        self.lay.addWidget(_heading("Gaming HUD"))
        h = d["hud"]
        box, lay = _card(8)
        row = QHBoxLayout()
        row.addWidget(_hint("A small readout that stays on top of games in a corner of the screen. Give it a key "
                            "in Settings → Performance."), 1)
        show = widgets.button("Show / hide the HUD", "primary")
        show.clicked.connect(lambda: m.toggle_hud())
        row.addWidget(show)
        lay.addLayout(row)
        grid = QGridLayout()
        grid.setHorizontalSpacing(16)
        for i, (k, t) in enumerate(HUD_ITEMS):
            cb = QCheckBox(t)
            cb.setChecked(k in (h.get("elements") or []))
            cb.toggled.connect(lambda v, k=k: self._hud_item(k, v))
            grid.addWidget(cb, i // 3, i % 3)
        lay.addLayout(grid)
        form = QGridLayout()
        form.setHorizontalSpacing(12)
        form.setVerticalSpacing(8)
        from core.settings import ANCHOR_CHOICES
        pos = QComboBox()
        for v, t in ANCHOR_CHOICES:
            pos.addItem(t, v)
        pos.setCurrentIndex(max(0, pos.findData(h.get("anchor", "tl"))))
        pos.activated.connect(lambda _i: self._hud("anchor", pos.currentData()))
        calm(pos)
        form.addWidget(widgets.label("Where", 13), 0, 0)
        form.addWidget(pos, 0, 1)
        lay_c = QComboBox()
        lay_c.addItem("One under the other", "column")
        lay_c.addItem("In a row", "row")
        lay_c.setCurrentIndex(max(0, lay_c.findData(h.get("layout", "column"))))
        lay_c.activated.connect(lambda _i: self._hud("layout", lay_c.currentData()))
        calm(lay_c)
        form.addWidget(widgets.label("Layout", 13), 1, 0)
        form.addWidget(lay_c, 1, 1)
        size = QSpinBox()
        size.setRange(8, 28)
        size.setSuffix(" px")
        size.setValue(int(h.get("size", 11)))
        size.valueChanged.connect(lambda v: self._hud("size", v))
        calm(size)
        form.addWidget(widgets.label("Text size", 13), 2, 0)
        form.addWidget(size, 2, 1)
        op = QSlider(Qt.Orientation.Horizontal)
        op.setRange(0, 100)
        op.setValue(int(float(h.get("opacity", 0.78)) * 100))
        op.valueChanged.connect(lambda v: self._hud("opacity", v / 100))
        calm(op)
        form.addWidget(widgets.label("Background", 13), 3, 0)
        form.addWidget(op, 3, 1)
        form.setColumnStretch(2, 1)
        lay.addLayout(form)
        ct = QCheckBox("Clicks go through it (to the game underneath)")
        ct.setChecked(bool(h.get("click_through", True)))
        ct.toggled.connect(lambda v: self._hud("click_through", bool(v)))
        lay.addWidget(ct)
        self.lay.addWidget(box)
        self.lay.addStretch()

    def _put(self, k, v):
        self.m.data[k] = v
        self.m.save()
        self.m._refresh_wants()

    def _hud(self, k, v):
        self.m.data["hud"][k] = v
        self.m.save()
        self.m.rebuild_hud()

    def _hud_item(self, k, on):
        els = [e for e in self.m.data["hud"].get("elements") or [] if e != k]
        if on:
            els.append(k)
        order = [key for key, _t in HUD_ITEMS]
        self.m.data["hud"]["elements"] = sorted(els, key=order.index)
        self.m.save()
        self.m.rebuild_hud()


# ══ the window ════════════════════════════════════════════════════════════════════
class PerformancePanel(widgets.FloatingPanel):
    def __init__(self, m):
        self.m = m
        super().__init__("Performance", min_size=(860, 580), opacity=float(m.s("bg_opacity", 0.95)),
                         close_hides=True)
        root = QHBoxLayout()
        root.setContentsMargins(0, 0, 0, 0)
        root.setSpacing(0)
        side = QFrame()
        side.setObjectName("setSidebar")
        side.setFixedWidth(190)
        self.side = QVBoxLayout(side)
        self.side.setContentsMargins(10, 12, 10, 12)
        self.side.setSpacing(2)
        self.group = QButtonGroup(self)
        self.buttons: dict[str, QPushButton] = {}
        self.stack = QStackedWidget()
        self.pages: dict[str, QWidget] = {}
        self.makers = {}
        root.addWidget(side)
        root.addWidget(self.stack, 1)
        self.body.addLayout(root)
        self._add("overview", "▦  Overview", lambda: OverviewPage(m))
        self._add("processes", "☰  Processes", lambda: ProcessesPage(m))
        self._add("boost", "⚡  Boost", lambda: BoostPage(m))
        self._add("game", "🎮  Game mode", lambda: GamePage(m))
        self._add("alerts", "🔔  Alerts & rules", lambda: AlertsPage(m))
        self._add("startup", "⏻  Startup apps", lambda: StartupPage(m))
        self.side.addStretch()
        self.mini = widgets.label("", 10.5, mono=True, tone="muted")
        self.mini.setWordWrap(True)
        self.side.addWidget(self.mini)
        self.side.addSpacing(6)
        self._add("options", "⚙  Options & HUD", lambda: OptionsPage(m))
        self.show_page("overview")
        self.resize(1080, 720)
        self.escape_pressed.connect(self.hide)
        self.hidden.connect(lambda: QTimer.singleShot(0, m._refresh_wants))

    def showEvent(self, e):
        super().showEvent(e)
        QTimer.singleShot(0, self.m._refresh_wants)

    def _add(self, key: str, text: str, make):
        b = QPushButton(text.replace("&", "&&"))
        b.setProperty("side", True)
        b.setCheckable(True)
        b.setFixedHeight(30)
        b.setCursor(Qt.CursorShape.PointingHandCursor)
        b.clicked.connect(lambda _c=False, k=key: self.show_page(k))
        self.group.addButton(b)
        self.buttons[key] = b
        self.makers[key] = make
        self.side.addWidget(b)

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
        if self.m.last:
            self.update_from(self.m.last)
        QTimer.singleShot(0, self.m._refresh_wants)

    def _current(self):
        return self.stack.currentWidget()

    def wants_processes(self) -> bool:
        cur = self._current()
        return bool(cur is not None and hasattr(cur, "wants_processes") and cur.wants_processes())

    def update_from(self, d: dict):
        cur = self._current()
        if cur is not None and hasattr(cur, "update_from"):
            cur.update_from(d)
        g = perfstats.main_gpu(d["gpus"])
        parts = [f"CPU {d['cpu']['usage']:.0f}%", f"RAM {d['mem']['percent']:.0f}%"]
        if g:
            parts.append(f"GPU {_pct(g.get('load')).replace(' ', '')}")
            if g.get("vram_total"):
                parts.append(f"VRAM {human_bytes(g.get('vram_used'))}")
        self.mini.setText("\n".join(parts))

    def game_changed(self):
        for p in self.pages.values():
            if hasattr(p, "game_changed"):
                p.game_changed()


# ══ the gaming HUD ════════════════════════════════════════════════════════════════
class Hud(QWidget):
    """A small always-on-top readout for games; clicks go through it unless that is switched off."""

    def __init__(self, m):
        super().__init__()
        self.m = m
        h = m.data["hud"]
        flags = Qt.WindowType.FramelessWindowHint | Qt.WindowType.WindowStaysOnTopHint | Qt.WindowType.Tool
        if h.get("click_through", True):
            flags |= Qt.WindowType.WindowTransparentForInput
        self.setWindowFlags(flags)
        self.setWindowTitle("Luminescent HUD")
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating)
        self.lines: list[tuple[str, str, str]] = []           # (label, value, colour)
        self.font_ = theme.qfont(int(h.get("size", 11)), 600, True)
        self.setMinimumSize(60, 20)

    def show_hud(self):
        self._lay_out()
        self.show()
        self._place()
        QTimer.singleShot(300, self._place)                   # Hyprland may move a new window once

    def _place(self):
        if not alive(self):
            return
        self.move(widgets.anchor_position(self.width(), self.height(), self.m.data["hud"].get("anchor", "tl"), 12))

    def _lay_out(self):
        from PyQt6.QtGui import QFontMetrics
        fm = QFontMetrics(self.font_)
        lines = self.lines or [("CPU", "…", "")]
        pad = 10
        if self.m.data["hud"].get("layout") == "row":
            w = sum(fm.horizontalAdvance(f"{a} {b}") for a, b, _c in lines) + 16 * (len(lines) - 1) + 2 * pad
            h = fm.height() + 2 * pad - 4
        else:
            lw = max(fm.horizontalAdvance(a) for a, _b, _c in lines)
            vw = max(fm.horizontalAdvance(b) for _a, b, _c in lines)
            w = lw + vw + 14 + 2 * pad
            h = fm.height() * len(lines) + 2 * pad - 4
        size = QSize(int(w), int(h))
        if size != self.size():
            self.setFixedSize(size)
            self._place()

    def update_from(self, d: dict):
        els = self.m.data["hud"].get("elements") or []
        g = perfstats.main_gpu(d["gpus"])
        out = []
        for k in els:
            if k == "cpu":
                out.append(("CPU", f"{d['cpu']['usage']:.0f}%", ""))
            elif k == "cpu_temp" and d["cpu"].get("temp") is not None:
                t = d["cpu"]["temp"]
                out.append(("CPU°", f"{t:.0f}°C", _temp_colour(t)))
            elif k == "gpu" and g:
                out.append(("GPU", _pct(g.get("load")).replace(" ", ""), ""))
            elif k == "gpu_temp" and g and g.get("temp") is not None:
                out.append(("GPU°", f"{g['temp']:.0f}°C", _temp_colour(g["temp"])))
            elif k == "vram" and g and g.get("vram_total"):
                out.append(("VRAM", f"{(g.get('vram_used') or 0) / perfstats.GiB:.1f}/"
                                    f"{g['vram_total'] / perfstats.GiB:.0f}G", ""))
            elif k == "ram":
                out.append(("RAM", f"{d['mem']['used'] / perfstats.GiB:.1f}G", ""))
            elif k == "power" and g and g.get("power") is not None:
                out.append(("PWR", f"{g['power']:.0f}W", ""))
            elif k == "net":
                out.append(("NET", f"↓{human_rate(d['net']['down'])}", ""))
            elif k == "clock":
                from datetime import datetime
                out.append(("TIME", datetime.now().strftime("%H:%M"), ""))
            elif k == "battery" and d.get("battery"):
                out.append(("BAT", f"{d['battery']['percent']:.0f}%", ""))
        self.lines = out or [("HUD", "pick items in Options", "")]
        self._lay_out()
        self.update()

    def paintEvent(self, e):
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        bg = QColor(theme.P.bg_base)
        bg.setAlphaF(max(0.0, min(1.0, float(self.m.data["hud"].get("opacity", 0.78)))))
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(bg)
        p.drawRoundedRect(QRectF(self.rect()), 8, 8)
        p.setFont(self.font_)
        fm = p.fontMetrics()
        pad = 10
        accent = QColor(theme.P.primary_text)
        text = QColor(theme.P.text_primary)
        if self.m.data["hud"].get("layout") == "row":
            x = pad
            y = pad - 2 + fm.ascent()
            for a, b, c in self.lines:
                p.setPen(accent)
                p.drawText(QPointF(x, y), a)
                x += fm.horizontalAdvance(a + " ")
                p.setPen(QColor(c) if c else text)
                p.drawText(QPointF(x, y), b)
                x += fm.horizontalAdvance(b) + 16
        else:
            lw = max((fm.horizontalAdvance(a) for a, _b, _c in self.lines), default=0)
            for i, (a, b, c) in enumerate(self.lines):
                y = pad - 2 + fm.ascent() + i * fm.height()
                p.setPen(accent)
                p.drawText(QPointF(pad, y), a)
                p.setPen(QColor(c) if c else text)
                p.drawText(QPointF(pad + lw + 14, y), b)
        p.end()
