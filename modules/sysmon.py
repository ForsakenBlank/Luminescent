"""
System monitor: CPU, memory, GPU and network in a small floating panel, with a minute of history.

Everything comes straight from /proc and /sys (no extra packages): CPU from /proc/stat, the CPU
temperature from hwmon (k10temp, zenpower, coretemp), memory from /proc/meminfo, network from
/proc/net/dev. AMD and Intel GPUs report their load in /sys; NVIDIA cards are asked through nvidia-smi
(without blocking). It only measures while the panel is on screen.
"""
import os
import glob
import shutil
from collections import deque

from PyQt6.QtWidgets import QWidget, QVBoxLayout, QHBoxLayout, QSizePolicy
from PyQt6.QtCore import Qt, QTimer, QRectF, QPointF, QProcess
from PyQt6.QtGui import QPainter, QColor, QPen, QPainterPath

from core import DATA_DIR, theme, widgets, save_json, load_json
from core.module import Module

SAVE_PATH = os.path.join(DATA_DIR, "sysmon.json")
HISTORY = 60


def _read(path: str) -> str:
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            return f.read()
    except OSError:
        return ""


def cpu_times() -> tuple[int, int]:
    """(busy, total) jiffies since boot, all cores together."""
    first = _read("/proc/stat").split("\n", 1)[0].split()
    if len(first) < 5 or first[0] != "cpu":
        return 0, 0
    vals = [int(x) for x in first[1:]]
    idle = vals[3] + (vals[4] if len(vals) > 4 else 0)
    return sum(vals) - idle, sum(vals)


def memory() -> tuple[float, float]:
    """(used GiB, total GiB)."""
    info = {}
    for line in _read("/proc/meminfo").splitlines():
        k, _, v = line.partition(":")
        try:
            info[k] = int(v.split()[0])
        except (ValueError, IndexError):
            pass
    total = info.get("MemTotal", 0)
    avail = info.get("MemAvailable", info.get("MemFree", 0))
    return (total - avail) / 1048576, total / 1048576


def net_bytes() -> tuple[int, int]:
    """(received, sent) bytes on every interface but loopback."""
    rx = tx = 0
    for line in _read("/proc/net/dev").splitlines()[2:]:
        name, _, rest = line.partition(":")
        parts = rest.split()
        if name.strip() == "lo" or len(parts) < 9:
            continue
        rx += int(parts[0])
        tx += int(parts[8])
    return rx, tx


def cpu_temp() -> float | None:
    for hw in glob.glob("/sys/class/hwmon/hwmon*"):
        if _read(os.path.join(hw, "name")).strip() in ("k10temp", "zenpower", "coretemp", "cpu_thermal"):
            raw = _read(os.path.join(hw, "temp1_input")).strip()
            if raw.isdigit():
                return int(raw) / 1000
    return None


def sysfs_gpu() -> dict | None:
    """Load (and VRAM, temperature) of an AMD or Intel GPU from /sys, None without one."""
    for dev in sorted(glob.glob("/sys/class/drm/card[0-9]/device")):
        busy = _read(os.path.join(dev, "gpu_busy_percent")).strip()
        if not busy.isdigit():
            continue
        out = {"load": float(busy)}
        used, total = _read(os.path.join(dev, "mem_info_vram_used")).strip(), \
            _read(os.path.join(dev, "mem_info_vram_total")).strip()
        if used.isdigit() and total.isdigit() and int(total):
            out["vram"] = (int(used) / 1073741824, int(total) / 1073741824)
        for hw in glob.glob(os.path.join(dev, "hwmon", "hwmon*")):
            raw = _read(os.path.join(hw, "temp1_input")).strip()
            if raw.isdigit():
                out["temp"] = int(raw) / 1000
                break
        return out
    return None


def _rate(bps: float) -> str:
    for unit in ("B/s", "KB/s", "MB/s", "GB/s"):
        if bps < 1024 or unit == "GB/s":
            return f"{bps:.0f} {unit}" if unit == "B/s" else f"{bps:.1f} {unit}"
        bps /= 1024
    return ""


class Spark(QWidget):
    """A minute of one value as a filled line (0..top)."""

    def __init__(self, colour: str = ""):
        super().__init__()
        self.values: deque = deque(maxlen=HISTORY)
        self.colour = colour or theme.P.primary
        self.top: float | None = 100.0              # None: scale to the largest value shown
        self.setFixedHeight(28)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)

    def push(self, v: float):
        self.values.append(max(0.0, v))
        self.update()

    def paintEvent(self, e):
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        r = QRectF(self.rect()).adjusted(0, 1, 0, -1)
        p.setPen(QPen(QColor(theme.P.border), 1))
        p.drawLine(QPointF(r.left(), r.bottom()), QPointF(r.right(), r.bottom()))
        vals = list(self.values)
        if len(vals) >= 2:
            top = self.top or max(max(vals), 1.0)
            step = r.width() / (HISTORY - 1)
            x0 = r.right() - step * (len(vals) - 1)
            pts = [QPointF(x0 + i * step, r.bottom() - min(1.0, v / top) * r.height()) for i, v in enumerate(vals)]
            path = QPainterPath(QPointF(pts[0].x(), r.bottom()))
            for pt in pts:
                path.lineTo(pt)
            path.lineTo(QPointF(pts[-1].x(), r.bottom()))
            fill = QColor(self.colour)
            fill.setAlpha(50)
            p.fillPath(path, fill)
            p.setPen(QPen(QColor(self.colour), 1.5))
            for a, b in zip(pts, pts[1:]):
                p.drawLine(a, b)
        p.end()


class SysPanel(widgets.FloatingPanel):
    def __init__(self, m: "SysmonModule"):
        self.m = m
        super().__init__("System", min_size=(240, 260), opacity=float(m.s("bg_opacity", 0.88)),
                         drag_needs_edit_mode=False, close_hides=True, activate_on_show=False)
        lay = QVBoxLayout()
        lay.setContentsMargins(14, 10, 14, 14)
        lay.setSpacing(4)
        self.rows = {}
        for key, name, colour in (("cpu", "CPU", ""), ("mem", "MEMORY", "#3DBE96"), ("gpu", "GPU", "#E0894E"),
                                  ("down", "DOWNLOAD", "#5B9BE6"), ("up", "UPLOAD", "#D6409F")):
            head = QHBoxLayout()
            head.addWidget(widgets.label(name, 9.5, 600, mono=True, spacing=0.12, tone="muted"))
            head.addStretch()
            value = widgets.label("…", 11, 500, mono=True)
            head.addWidget(value)
            spark = Spark(colour)
            if key in ("down", "up"):
                spark.top = None
            box = QWidget()
            bl = QVBoxLayout(box)
            bl.setContentsMargins(0, 4, 0, 4)
            bl.setSpacing(3)
            bl.addLayout(head)
            bl.addWidget(spark)
            lay.addWidget(box)
            self.rows[key] = (box, value, spark)
        lay.addStretch()
        self.body.addLayout(lay)
        st = load_json(SAVE_PATH, {})
        st = st if isinstance(st, dict) else {}
        self.resize(int(st.get("w", 280)), int(st.get("h", 330)))
        if "x" in st and "y" in st:
            self.move(int(st["x"]), int(st["y"]))
            self.clamp_to_screens()
        else:
            self.snap_to("tr", 16)
        self.geometry_changed.connect(self._remember)
        self.escape_pressed.connect(self.hide)

    def _remember(self):
        g = self.geometry()
        try:
            save_json(SAVE_PATH, {"x": g.x(), "y": g.y(), "w": g.width(), "h": g.height()})
        except OSError:
            pass

    def set_row(self, key: str, text: str, value: float | None):
        box, label, spark = self.rows[key]
        box.setVisible(value is not None or key != "gpu")
        label.setText(text)
        if value is not None:
            spark.push(value)


class SysmonModule(Module):
    key = "sysmon"
    title = "System Monitor"
    icon = "📈"

    def __init__(self, ctx):
        super().__init__(ctx)
        self.panel: SysPanel | None = None
        self._last_cpu = (0, 0)
        self._last_net = (0, 0)
        self._nvidia: dict | None = None
        self._nv_proc: QProcess | None = None
        self._timer = QTimer(self)
        self._timer.setInterval(1000)
        self._timer.timeout.connect(self.sample)

    def start(self):
        self.hotkey("hotkey", self.toggle, "system monitor")
        if self.ctx.shows_on_start("sysmon"):
            self.toggle()

    def stop(self):
        self._timer.stop()
        if self.panel:
            self.panel.close()
            self.panel.deleteLater()
            self.panel = None

    def apply_settings(self):
        self.hotkeys.unregister_all(self.key)
        self.hotkey("hotkey", self.toggle, "system monitor")
        if self.panel:
            self.panel.set_opacity(float(self.s("bg_opacity", 0.88)))

    def retheme(self):
        if self.panel:
            visible = self.panel.isVisible()
            self.stop()
            if visible:
                self.toggle()

    def actions(self):
        return [("toggle", "System monitor (CPU, RAM, GPU, network)", self.toggle)]

    def tray_actions(self):
        return [("System monitor", self.toggle)]

    def toggle(self):
        if self.panel is None:
            with self.themed():
                self.panel = SysPanel(self)
            self.panel.hidden.connect(self._timer.stop)
        if self.panel.isVisible():
            self.panel.hide()
            return
        self._last_cpu, self._last_net = cpu_times(), net_bytes()
        self.panel.show_and_raise(activate=False)
        self.sample()
        self._timer.start()

    # ── sampling ───────────────────────────────────────────────────────────
    def sample(self):
        if self.panel is None:
            return
        busy, total = cpu_times()
        db, dt = busy - self._last_cpu[0], total - self._last_cpu[1]
        self._last_cpu = (busy, total)
        cpu = 100.0 * db / dt if dt > 0 else 0.0
        temp = cpu_temp()
        self.panel.set_row("cpu", f"{cpu:.0f} %" + (f"  ·  {temp:.0f}°C" if temp is not None else ""), cpu)
        used, tot = memory()
        self.panel.set_row("mem", f"{used:.1f} / {tot:.1f} GiB", 100.0 * used / tot if tot else 0.0)
        gpu = sysfs_gpu() or self._nvidia_reading()
        if gpu is None:
            self.panel.set_row("gpu", "", None)
        else:
            parts = [f"{gpu['load']:.0f} %"]
            if gpu.get("vram"):
                parts.append(f"{gpu['vram'][0]:.1f} / {gpu['vram'][1]:.0f} GiB")
            if gpu.get("temp") is not None:
                parts.append(f"{gpu['temp']:.0f}°C")
            self.panel.set_row("gpu", "  ·  ".join(parts), gpu["load"])
        rx, tx = net_bytes()
        down, up = max(0, rx - self._last_net[0]), max(0, tx - self._last_net[1])
        self._last_net = (rx, tx)
        self.panel.set_row("down", _rate(down), down)
        self.panel.set_row("up", _rate(up), up)

    def _nvidia_reading(self) -> dict | None:
        """The last nvidia-smi answer; a new question goes out (it takes a moment, so it is never waited for)."""
        if not shutil.which("nvidia-smi"):
            return None
        if self._nv_proc is None:
            self._nv_proc = QProcess(self)
            self._nv_proc.finished.connect(self._nvidia_answered)
            self._nv_proc.start("nvidia-smi", ["--query-gpu=utilization.gpu,memory.used,memory.total,temperature.gpu",
                                               "--format=csv,noheader,nounits"])
        return self._nvidia

    def _nvidia_answered(self, *_):
        proc, self._nv_proc = self._nv_proc, None
        if proc is None:
            return
        line = bytes(proc.readAllStandardOutput()).decode("utf-8", "replace").strip().splitlines()[:1]
        proc.deleteLater()
        try:
            load, used, total, temp = (float(x) for x in line[0].split(","))
            self._nvidia = {"load": load, "vram": (used / 1024, total / 1024), "temp": temp}
        except (ValueError, IndexError):
            self._nvidia = None
