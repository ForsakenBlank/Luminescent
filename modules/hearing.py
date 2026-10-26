"""
Hearing Guard: an estimated-dB volume limiter with daily exposure tracking.

How the estimate works
  A PC cannot measure sound pressure at your ear, but it knows two things:
    * what is being played    -> a recording of the default output's monitor (parec), A-weighted
                                 like a sound-level meter and reduced to an RMS level in dBFS
    * how loud the master volume is -> the PipeWire/PulseAudio sink volume, in dB (0 = 100 %)
  estimated dB(A) = signal dBFS + master volume dB + calibration
  "calibration" is what the device produces at 100 % volume with a full-scale tone (~100-115 dB
  for most headphones). It is stored per output device; calibrate once against a phone sound-meter
  app and the number becomes reasonably trustworthy.

  The monitor is only recorded while the output is actually playing something (the sink is
  "running"), and stops again after a few seconds of silence.

Safe listening (NIOSH): 85 dB(A) for 8 h per day, and every +3 dB halves the time
(88 dB -> 4 h, 91 dB -> 2 h, 94 dB -> 1 h, 100 dB -> 15 min...). The daily "dose" adds up how
much of that allowance you have used.

Limit mode dips the master volume whenever the level over the last second goes above your limit and
brings it back up a second or two after the loud part is over, like a slow compressor. It can only
move the master volume (the monitor is a read-only copy), so the whole mix dips, not just the loud sound.
A hard "never above X %" cap needs no calibration at all.

A volume control on the headphones themselves (an analog wheel after the jack) is invisible to
the PC. It can only attenuate, so calibrate with it at max and the estimate is the worst case.
"""
import os
import math
import time
import shutil
import logging
import subprocess
from collections import deque
from dataclasses import dataclass
from datetime import date, datetime

from PyQt6.QtWidgets import (
    QWidget, QLabel, QVBoxLayout, QHBoxLayout, QDialog, QDoubleSpinBox, QSizePolicy, QFrame, QPushButton
)
from PyQt6.QtCore import Qt, QTimer, QRectF, QPointF
from PyQt6.QtGui import QPainter, QColor, QPen, QPainterPath

from core import DATA_DIR, theme, widgets, save_json, load_json
from core.module import Module

log = logging.getLogger("hearing")
DATA_PATH = os.path.join(DATA_DIR, "hearing.json")

CRITERION_DB = 85.0        # NIOSH recommended exposure limit
CRITERION_HOURS = 8.0
EXCHANGE_DB = 3.0          # every +3 dB halves the allowed time
AUDIBLE_DB = 40.0          # estimates below this count as silence
SINE_RMS_DB = 3.0103       # reference RMS levels to a full-scale sine, so "calibration" = that sine's dB
GREEN, AMBER, RED = "#3FBF74", "#E0AE3F", "#E8645E"


def safe_seconds(db: float) -> float:
    """Allowed daily exposure at a constant level `db`, in seconds (NIOSH)."""
    return CRITERION_HOURS * 3600.0 * 2.0 ** ((CRITERION_DB - db) / EXCHANGE_DB)


def fmt_duration(sec: float) -> str:
    if sec >= 48 * 3600:
        return "> 48 h"
    if sec >= 3600:
        h, m = int(sec // 3600), int((sec % 3600) // 60)
        return f"{h} h {m:02d} m" if m else f"{h} h"
    if sec >= 60:
        return f"{int(sec // 60)} m {int(sec % 60):02d} s"
    return f"{int(sec)} s"


def _short_duration(sec: float) -> str:
    if sec >= 3600:
        return f"{sec / 3600:.0f} h"
    if sec >= 60:
        return f"{sec / 60:.0f} m" if sec >= 600 else f"{sec / 60:.1f} m"
    return f"{sec:.0f} s"


def level_colour(db: float, limit: float) -> str:
    return GREEN if db < limit - 5 else (AMBER if db < limit else RED)


def _db(energy: float, seconds: float) -> float | None:
    """Energy-average level from summed 10^(dB/10)*seconds."""
    return 10.0 * math.log10(energy / seconds) if energy > 0 and seconds > 0 else None


# ── PipeWire / PulseAudio (pulsectl + parec) ──────────────────────────────────
def _pulsectl():
    """Import pulsectl on first use, so a missing package only disables the meter."""
    import pulsectl
    return pulsectl


def _describe(e: Exception) -> str:
    name = type(e).__name__
    if name == "PulseError" or "connect" in str(e).lower():
        return "the sound server (PipeWire / PulseAudio) is not reachable"
    return f"{name}: {e}"


def a_weighting(freqs):
    """IEC 61672 A-weighting as a linear amplitude gain (1.0 at 1 kHz)."""
    import numpy as np
    f2 = np.asarray(freqs, dtype=float) ** 2
    ra = (12194.0 ** 2 * f2 * f2) / ((f2 + 20.6 ** 2) * np.sqrt((f2 + 107.7 ** 2) * (f2 + 737.9 ** 2))
                                     * (f2 + 12194.0 ** 2))
    return ra / 0.7943282347242815


class AWeightingFilter:
    """
    ~85 ms linear-phase FIR with the A-weighting response (within ~1 dB down to 31.5 Hz), applied by
    FFT overlap-save so the level is exact sample by sample. numpy only: scipy would add ~3 s and ~60 MB
    to the launcher's start-up just for a filter.
    """

    def __init__(self, rate: int, channels: int):
        import numpy as np
        self.np = np
        taps = int(rate * 0.085) | 1
        grid = 1 << max(16, (4 * taps).bit_length())
        h = np.fft.irfft(a_weighting(np.fft.rfftfreq(grid, 1.0 / rate)), grid)
        self.h = np.roll(h, taps // 2)[:taps] * np.blackman(taps)
        self.hist = np.zeros((taps - 1, channels), np.float32)
        self._spectra: dict[int, object] = {}

    def energy(self, x):
        """Sum of squares of the weighted signal for each channel of `x` (frames x channels)."""
        np = self.np
        keep = len(self.hist)
        buf = np.concatenate((self.hist, x))
        self.hist = buf[-keep:]
        n = 1 << (len(buf) - 1).bit_length()
        spec = self._spectra.get(n)
        if spec is None:
            spec = self._spectra[n] = np.fft.rfft(self.h, n)[:, None]
        y = np.fft.irfft(np.fft.rfft(buf, n, axis=0) * spec, n, axis=0)[keep:len(buf)]
        return np.einsum("ij,ij->j", y, y)


class Loopback:
    """A parec recording of one output's monitor source ("what you hear"), read without blocking."""
    RATE, CHANNELS = 48000, 2

    def __init__(self, monitor_source: str):
        import numpy as np
        self.np = np
        exe = shutil.which("parec")
        if not exe:
            raise RuntimeError("parec is missing (install pipewire-pulse or libpulse)")
        self.rate, self.channels = self.RATE, self.CHANNELS
        self.proc = subprocess.Popen(
            [exe, "-d", monitor_source, "--raw", "--format=float32le", f"--rate={self.rate}",
             f"--channels={self.channels}", "--latency-msec=50", "--client-name=Luminescent",
             "--stream-name=Hearing Guard meter"],
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, stdin=subprocess.DEVNULL)
        os.set_blocking(self.proc.stdout.fileno(), False)
        self._rest = b""
        self.peak = 0.0
        self.weighting = AWeightingFilter(self.rate, self.channels)

    def read(self) -> tuple[float, int]:
        """Drain what was captured since the last call -> (A-weighted energy of the loudest channel, frames)."""
        np = self.np
        if self.proc.poll() is not None:
            raise RuntimeError(f"parec stopped (exit code {self.proc.returncode})")
        chunks = [self._rest]
        while True:
            try:
                b = os.read(self.proc.stdout.fileno(), 1 << 20)
            except BlockingIOError:
                break
            if not b:
                break
            chunks.append(b)
        data = b"".join(chunks)
        frame = 4 * self.channels
        n = len(data) // frame
        self._rest = data[n * frame:]
        if not n:
            self.peak = 0.0
            return 0.0, 0
        x = np.frombuffer(data[:n * frame], np.float32).reshape(n, self.channels)
        self.peak = float(np.abs(x).max())
        return float(self.weighting.energy(x).max()), n

    def close(self):
        try:
            self.proc.terminate()
            self.proc.wait(1)
        except Exception:
            pass


class Endpoint:
    """
    The default output (sink): master volume, the app streams playing on it and, while audio plays, a
    recording of its monitor.

    Dips are made on the app streams, not the master volume: the master slider (and the volume pop-up
    your bar shows when it changes) stays where you put it, and a dip can only ever bring the apps back to
    the volume you gave them, never above it. The guard works with one "effective" volume = master + dip.
    """
    DIP_LATENCY_S = 0.12         # audio being read now was played this long ago (at that moment's dip)

    def __init__(self, pulse, sink):
        self.pulse = pulse
        self.id = sink.name
        self.name = sink.description or sink.name
        self.monitor = sink.monitor_source_name
        self.min_db, self.max_db = -90.0, 0.0
        # PipeWire records the monitor before the sink volume, like Windows loopback did
        self.hw_volume = False
        self.loopback: Loopback | None = None
        self.sink = sink
        self.dip_db = 0.0
        self._streams: dict[int, tuple[list, list]] = {}   # stream index -> (your volumes, what we set)
        self._dips: deque = deque([(0.0, 0.0)], maxlen=64)  # (time, dip) for matching recorded audio

    def refresh(self):
        self.sink = self.pulse.get_sink_by_name(self.id)
        if self.dip_db < 0:
            self._apply_dip(self.dip_db)        # streams that started since the last tick get the dip too

    @property
    def volume_linear(self) -> float:
        return float(self.sink.volume.value_flat)

    def master_db(self) -> float:
        v = self.volume_linear
        return max(self.min_db, 60.0 * math.log10(v)) if v > 0 else self.min_db     # PulseAudio's cubic scale

    def volume_db(self) -> float:
        """The effective volume: master plus the dip on the app streams."""
        return self.master_db() + self.dip_db

    def dip_at(self, t: float) -> float:
        """The dip that was in force at monotonic time `t`."""
        for when, dip in reversed(self._dips):
            if when <= t:
                return dip
        return self._dips[0][1]

    @property
    def muted(self) -> bool:
        return bool(self.sink.mute)

    @property
    def running(self) -> bool:
        return "running" in str(self.sink.state).lower()

    def _apply_dip(self, dip: float):
        factor = 10 ** (dip / 60.0)              # cubic volume scale: -60 dB = 0.1
        alive = set()
        for si in self.pulse.sink_input_list():
            if si.sink != self.sink.index:
                continue
            alive.add(si.index)
            cur = list(si.volume.values)
            mine = self._streams.get(si.index)
            if mine is None:
                base = cur                       # a stream we have not touched: its volume is yours
            else:
                base, applied = mine
                if any(abs(c - a) > 0.003 for c, a in zip(cur, applied)):
                    # you moved this app's volume during a dip: keep that, relative to the dip
                    old = self.dip_at(time.monotonic())
                    old_factor = 10 ** (old / 60.0) if old < 0 else 1.0
                    base = [min(1.0, c / old_factor) for c in cur]
            if dip >= 0:
                target = base
            else:
                target = [round(b * factor * 65536) / 65536 for b in base]
            if any(abs(c - t) > 1e-4 for c, t in zip(cur, target)):
                vol = si.volume
                vol.values = target
                self.pulse.volume_set(si, vol)
            if dip >= 0:
                self._streams.pop(si.index, None)
            else:
                self._streams[si.index] = (base, target)
        for idx in list(self._streams):
            if idx not in alive:
                del self._streams[idx]

    def set_volume_db(self, db: float, dip: bool = True) -> float:
        """Set the effective volume and return what was actually set.

        dip=True: by dipping the app streams (never above master + 0). dip=False: by moving the master volume.
        """
        db = max(self.min_db, min(self.max_db, db))
        if dip:
            new = max(-60.0, min(0.0, db - self.master_db()))
            if abs(new - self.dip_db) > 0.01:
                self._apply_dip(new)
                self.dip_db = new if new < -0.01 else 0.0
                self._dips.append((time.monotonic(), self.dip_db))
            return self.volume_db()
        self.pulse.volume_set_all_chans(self.sink, round((10 ** (db / 60.0) if db > self.min_db else 0.0)
                                                         * 65536) / 65536)
        self.sink = self.pulse.get_sink_by_name(self.id)
        return self.volume_db()

    def reset_dip(self):
        if self.dip_db < 0 or self._streams:
            self._apply_dip(0.0)
            self.dip_db = 0.0
            self._dips.append((time.monotonic(), 0.0))

    def set_volume_percent(self, pct: float):
        """The hard cap: this one moves the master volume (only ever down)."""
        self.pulse.volume_set_all_chans(self.sink, round(max(0.0, min(1.0, pct / 100.0)) * 65536) / 65536)
        self.sink = self.pulse.get_sink_by_name(self.id)

    def close(self):
        try:
            self.reset_dip()                     # never leave the apps turned down
        except Exception:
            log.warning("could not restore the app volumes")
        if self.loopback:
            self.loopback.close()
        self.loopback = None


@dataclass
class Reading:
    level: float | None      # A-weighted signal level over `secs`, dBFS; None = silence
    secs: float              # how much time this reading covers
    source: str              # "loopback" or "" (silence)
    volume_db: float         # effective: master + the dip on the app streams
    volume_pct: float        # master
    muted: bool
    dip_db: float = 0.0


class AudioMonitor:
    """Follows the default output device and turns each tick into a Reading. Everything runs on the UI thread."""
    DEVICE_CHECK_S = 2.0         # how often to ask the sound server which output is the default
    LOOPBACK_IDLE_S = 10.0       # stop recording after this long without sound
    PLAYING_PEAK = 1e-5          # -100 dBFS

    def __init__(self):
        self.endpoint: Endpoint | None = None
        self.error = ""
        self.loopback_error = ""
        self._pulse = None
        self._check_at = 0.0
        self._retry_at = 0.0
        self._backoff = 1.0
        self._loopback_retry_at = 0.0
        self._last_sound = 0.0

    def close(self):
        if self.endpoint:
            self.endpoint.close()
        self.endpoint = None
        if self._pulse is not None:
            try:
                self._pulse.close()
            except Exception:
                pass
        self._pulse = None

    def poll(self, now: float, dt: float) -> Reading | None:
        if not self._ensure_device(now):
            return None
        ep = self.endpoint
        try:
            ep.refresh()
            volume_db = ep.volume_db()
            volume_pct = ep.volume_linear * 100.0
            muted = ep.muted
            level, secs, source = self._measure(ep, now, dt, muted)
        except Exception as e:
            self._fail(e, now)
            return None
        return Reading(level, secs, source, volume_db, volume_pct, muted, ep.dip_db)

    # ── device tracking ────────────────────────────────────────────────────
    def _ensure_device(self, now: float) -> bool:
        if self.endpoint is not None and now < self._check_at:
            return True
        if self.endpoint is None and now < self._retry_at:
            return False
        self._check_at = now + self.DEVICE_CHECK_S
        try:
            if self._pulse is None:
                self._pulse = _pulsectl().Pulse("Luminescent Hearing Guard")
            name = self._pulse.server_info().default_sink_name
            if self.endpoint is None or self.endpoint.id != name:
                if self.endpoint:
                    self.endpoint.close()
                self.endpoint = None
                self.endpoint = Endpoint(self._pulse, self._pulse.get_sink_by_name(name))
                self._loopback_retry_at = 0.0
                log.info("output device: %s", self.endpoint.name)
            self.error = ""
            self._backoff = 1.0
            return True
        except ImportError as e:
            self.error = f"{e} (pip install pulsectl numpy)"
            self._retry_at = now + 3600
            return False
        except Exception as e:
            self._fail(e, now)
            return False

    def _fail(self, e: Exception, now: float):
        msg = _describe(e)
        if msg != self.error:                          # log each new problem once, not five times a second
            log.warning("audio unavailable: %s", msg)
        self.error = msg
        self.close()                                   # the sound server may have restarted
        self._retry_at = now + self._backoff
        self._backoff = min(10.0, self._backoff * 2)

    # ── measuring ──────────────────────────────────────────────────────────
    def _measure(self, ep: Endpoint, now: float, dt: float, muted: bool):
        lb = ep.loopback
        if lb is None and ep.running and not muted and now >= self._loopback_retry_at:
            try:
                ep.loopback = lb = Loopback(ep.monitor)
                self._last_sound = now
                self.loopback_error = ""
            except Exception as e:
                msg = _describe(e)
                if msg != self.loopback_error:
                    log.warning("monitor recording unavailable: %s", msg)
                self.loopback_error = msg
                self._loopback_retry_at = now + 60.0

        level, secs, source = None, dt, ""
        if lb is not None:
            try:
                energy, frames = lb.read()
            except RuntimeError as e:
                log.warning("%s", e)
                lb.close()
                ep.loopback = None
                self._loopback_retry_at = now + 5.0
                return level, secs, source
            if frames:
                secs = max(dt, frames / lb.rate)           # fewer frames than wall time: the rest was silence
                ms = energy / (secs * lb.rate)
                if ms > 1e-11 and not muted:               # above -110 dBFS and actually audible
                    # the recording already has our dip in it; the guard wants the level before it
                    dip = ep.dip_at(now - ep.DIP_LATENCY_S)
                    level, source = 10.0 * math.log10(ms) + SINE_RMS_DB - dip, "loopback"
                if lb.peak > self.PLAYING_PEAK and not muted:
                    self._last_sound = now
            if now - self._last_sound > self.LOOPBACK_IDLE_S:
                lb.close()                                 # lets the sink go idle; restarted when it plays
                ep.loopback = None
        return level, secs, source


# ── panel widgets ─────────────────────────────────────────────────────────────
class Bar(QWidget):
    """Thin rounded bar, painted directly (restyling a QFrame five times a second is not free)."""

    def __init__(self, height: int):
        super().__init__()
        self.setFixedHeight(height)
        self._frac, self._colour = 0.0, GREEN

    def set_value(self, frac: float, colour: str):
        frac = max(0.0, min(1.0, frac))
        if abs(frac - self._frac) > 0.002 or colour != self._colour:
            self._frac, self._colour = frac, colour
            self.update()

    def paintEvent(self, e):
        qp = QPainter(self)
        qp.setRenderHint(QPainter.RenderHint.Antialiasing)
        qp.setPen(Qt.PenStyle.NoPen)
        r = QRectF(self.rect())
        rad = r.height() / 2
        qp.setBrush(QColor(theme.P.bg_frame))
        qp.drawRoundedRect(r, rad, rad)
        if self._frac > 0:
            qp.setBrush(QColor(self._colour))
            qp.drawRoundedRect(QRectF(r.left(), r.top(), max(r.height(), r.width() * self._frac), r.height()), rad, rad)
        qp.end()


class SegmentMeter(QWidget):
    """The live level as 30 segments from 40 to 100 dB: green, then amber, then red; unlit ones faint."""
    N, LO, HI = 30, 40.0, 100.0

    def __init__(self):
        super().__init__()
        self.setFixedHeight(8)
        self._lit = 0
        self._limit = 85.0

    def set_level(self, db: float | None, limit: float):
        lit = 0 if db is None else int(round((max(self.LO, min(self.HI, db)) - self.LO) / (self.HI - self.LO) * self.N))
        if (lit, limit) != (self._lit, self._limit):
            self._lit, self._limit = lit, limit
            self.update()

    def _colour(self, i: int) -> str:
        db = self.LO + (i + 0.5) / self.N * (self.HI - self.LO)
        return level_colour(db, self._limit)

    def paintEvent(self, e):
        qp = QPainter(self)
        qp.setRenderHint(QPainter.RenderHint.Antialiasing)
        qp.setPen(Qt.PenStyle.NoPen)
        gap = 2.0
        w = (self.width() - gap * (self.N - 1)) / self.N
        for i in range(self.N):
            c = QColor(self._colour(i))
            c.setAlphaF(1.0 if i < self._lit else 0.18)
            qp.setBrush(c)
            qp.drawRoundedRect(QRectF(i * (w + gap), 0, w, self.height()), 1, 1)
        qp.end()


def _scale(*labels: str) -> QHBoxLayout:
    """A row of mono tick labels spread edge to edge."""
    row = QHBoxLayout()
    row.setContentsMargins(0, 0, 0, 0)
    for i, t in enumerate(labels):
        if i:
            row.addStretch()
        row.addWidget(widgets.label(t, 9.5, mono=True, tone="muted"))
    return row


class SafeTimeChart(QWidget):
    """Allowed listening time vs. level, with the limit and the current level marked."""
    X0, X1 = 70.0, 115.0
    Y0, Y1 = math.log10(60.0), math.log10(24 * 3600.0)
    GRID = ((24 * 3600, "24 h"), (8 * 3600, "8 h"), (4 * 3600, "4 h"), (2 * 3600, "2 h"), (3600, "1 h"),
            (1800, "30 m"), (900, "15 m"), (300, "5 m"), (60, "1 m"))

    def __init__(self):
        super().__init__()
        self.setMinimumHeight(190)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        self.current: float | None = None
        self.limit = 85.0
        self._paths: tuple | None = None       # (rect, curve, unsafe area, safe area), rebuilt on resize

    def set_state(self, current: float | None, limit: float):
        current = None if current is None else round(current * 2) / 2
        if (current, limit) != (self.current, self.limit):
            self.current, self.limit = current, limit
            self.update()

    def _map(self, db: float, secs: float, r: QRectF) -> QPointF:
        x = r.left() + (db - self.X0) / (self.X1 - self.X0) * r.width()
        ly = max(self.Y0, min(self.Y1, math.log10(max(secs, 1.0))))
        y = r.bottom() - (ly - self.Y0) / (self.Y1 - self.Y0) * r.height()
        return QPointF(x, y)

    def _curve(self, r: QRectF):
        if self._paths is None or self._paths[0] != r:
            pts = [self._map(self.X0 + i * 0.5, safe_seconds(self.X0 + i * 0.5), r)
                   for i in range(int((self.X1 - self.X0) * 2) + 1)]
            path = QPainterPath(pts[0])
            for pt in pts[1:]:
                path.lineTo(pt)
            unsafe = QPainterPath(path)
            unsafe.lineTo(QPointF(r.right(), r.top()))
            unsafe.lineTo(QPointF(r.left(), r.top()))
            unsafe.closeSubpath()
            safe = QPainterPath(path)
            safe.lineTo(QPointF(r.right(), r.bottom()))
            safe.lineTo(QPointF(r.left(), r.bottom()))
            safe.closeSubpath()
            self._paths = (QRectF(r), path, unsafe, safe)
        return self._paths[1:]

    def paintEvent(self, e):
        p = theme.P
        qp = QPainter(self)
        qp.setRenderHint(QPainter.RenderHint.Antialiasing)
        r = QRectF(self.rect()).adjusted(46, 8, -10, -22)
        qp.setFont(theme.qfont(9.5, mono=True))
        grid_pen, text_col = QPen(QColor(p.border), 1), QColor(p.text_muted)

        for secs, label in self.GRID:
            y = self._map(self.X0, secs, r).y()
            qp.setPen(grid_pen)
            qp.drawLine(QPointF(r.left(), y), QPointF(r.right(), y))
            qp.setPen(text_col)
            qp.drawText(QRectF(0, y - 7, r.left() - 6, 14), Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter, label)
        for db in range(int(self.X0), int(self.X1) + 1, 5):
            x = self._map(db, 60, r).x()
            qp.setPen(grid_pen)
            qp.drawLine(QPointF(x, r.top()), QPointF(x, r.bottom()))
            qp.setPen(text_col)
            qp.drawText(QRectF(x - 16, r.bottom() + 4, 32, 14), Qt.AlignmentFlag.AlignCenter, f"{db}")

        path, unsafe, safe = self._curve(r)
        bad, good = QColor(RED), QColor(GREEN)
        bad.setAlpha(26)
        good.setAlpha(20)
        qp.fillPath(unsafe, bad)
        qp.fillPath(safe, good)
        qp.setPen(QPen(QColor(p.primary), 2))
        qp.drawPath(path)

        lx = self._map(self.limit, 60, r).x()
        qp.setPen(QPen(QColor(AMBER), 1, Qt.PenStyle.DashLine))
        qp.drawLine(QPointF(lx, r.top()), QPointF(lx, r.bottom()))
        qp.drawText(QRectF(lx + 3, r.bottom() - 16, 90, 14), Qt.AlignmentFlag.AlignLeft, f"limit {self.limit:.0f} dB")

        if self.current is not None and self.X0 <= self.current <= self.X1:
            secs = safe_seconds(self.current)
            pt = self._map(self.current, secs, r)
            col = QColor(GREEN if self.current < self.limit else RED)
            qp.setPen(QPen(col, 2))
            qp.setBrush(col)
            qp.drawEllipse(pt, 4.5, 4.5)
            label = f"{self.current:.0f} dB → {fmt_duration(secs)}/day"
            tw = qp.fontMetrics().horizontalAdvance(label) + 8
            tx = pt.x() + 8 if pt.x() + 8 + tw < r.right() else pt.x() - 8 - tw
            qp.setPen(QColor(p.text_primary))
            qp.drawText(QRectF(tx, pt.y() - 18, tw, 14), Qt.AlignmentFlag.AlignLeft, label)
        qp.end()


class TodayChart(QWidget):
    """Today's level in half-hour bars, from the first sound of the day to now: accent, amber near the limit,
    red over it."""
    BUCKET = 30

    def __init__(self):
        super().__init__()
        self.setMinimumHeight(70)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        self.minutes: dict[int, float] = {}
        self.limit = 85.0
        self.span = (8 * 60, 8 * 60 + 30)

    def set_data(self, minutes: dict, limit: float):
        self.minutes, self.limit = minutes, limit
        now = datetime.now()
        end = now.hour * 60 + now.minute + 1
        first = min((m for m, db in minutes.items() if db >= AUDIBLE_DB), default=8 * 60)
        start = min(first, end - self.BUCKET) // self.BUCKET * self.BUCKET
        self.span = (max(0, start), max(end, start + self.BUCKET))
        self.update()

    def buckets(self) -> list[float | None]:
        start, end = self.span
        out = []
        for b0 in range(start, end, self.BUCKET):
            vals = [db for m, db in self.minutes.items() if b0 <= m < b0 + self.BUCKET and db >= AUDIBLE_DB]
            # energy average, like a sound level meter's Leq
            out.append(10 * math.log10(sum(10 ** (v / 10) for v in vals) / len(vals)) if vals else None)
        return out

    def paintEvent(self, e):
        p = theme.P
        qp = QPainter(self)
        qp.setRenderHint(QPainter.RenderHint.Antialiasing)
        r = QRectF(self.rect()).adjusted(0, 2, 0, -1)
        qp.setPen(QPen(QColor(p.border), 1))
        qp.drawLine(QPointF(r.left(), r.bottom() + 0.5), QPointF(r.right(), r.bottom() + 0.5))
        bars = self.buckets()
        n = max(1, len(bars))
        gap = 3.0 if n < 40 else 1.0
        w = max(1.0, (r.width() - gap * (n - 1)) / n)
        lo, hi = 30.0, 100.0
        qp.setPen(Qt.PenStyle.NoPen)
        for i, db in enumerate(bars):
            if db is None:
                continue
            h = max(2.0, (min(hi, db) - lo) / (hi - lo) * r.height())
            col = RED if db >= self.limit else (AMBER if db >= self.limit - 10 else p.primary)
            qp.setBrush(QColor(col))
            x = r.left() + i * (w + gap)
            path = QPainterPath()
            path.addRoundedRect(QRectF(x, r.bottom() - h, w, h + 2), 2, 2)
            qp.setClipRect(QRectF(x, r.top(), w, r.height()))
            qp.drawPath(path)
            qp.setClipping(False)
        qp.end()


# ── calibration dialog ────────────────────────────────────────────────────────
class CalibrateDialog(QDialog):
    def __init__(self, module: "HearingModule", parent=None):
        super().__init__(parent)
        self.m = module
        self.setWindowTitle("Calibrate Hearing Guard")
        self.setWindowFlags(Qt.WindowType.Dialog | Qt.WindowType.WindowStaysOnTopHint)
        self.setMinimumWidth(480)
        lay = QVBoxLayout(self)
        lay.setContentsMargins(20, 20, 20, 20)
        lay.setSpacing(10)
        info = QLabel(
            "1. Play music at the volume you normally use.\n"
            "2. Open a sound-meter app on your phone set to dB(A) (e.g. NIOSH SLM, Decibel X) and hold its "
            "microphone where your ear is. For headphones, press one ear cup against the phone.\n"
            "3. Reset the app's average (LAeq / AVG) and press Restart here at the same moment.\n"
            "4. After 20-30 seconds, type the app's average below and press Apply.\n\n"
            "This sets an offset for this output device only. It is a rough calibration, not a lab measurement.")
        info.setWordWrap(True)
        lay.addWidget(info)
        self.device = QLabel("")
        self.device.setWordWrap(True)
        self.device.setStyleSheet(f"color: {theme.P.text_secondary};")
        lay.addWidget(self.device)
        self.live = QLabel("")
        self.live.setStyleSheet(f"color: {theme.P.text_secondary};")
        lay.addWidget(self.live)
        row = QHBoxLayout()
        row.addWidget(QLabel("Phone average:"))
        self.reading = QDoubleSpinBox()
        self.reading.setRange(30, 130)
        self.reading.setDecimals(1)
        self.reading.setSuffix(" dB")
        self.reading.setValue(max(30.0, min(130.0, module.display_level or 75.0)))
        row.addWidget(self.reading)
        row.addStretch()
        lay.addLayout(row)
        btns = QHBoxLayout()
        reset = widgets.button("Use the default for this device")
        reset.clicked.connect(self._reset)
        restart = widgets.button("Restart")
        restart.clicked.connect(module.start_calibration_measure)
        cancel = widgets.button("Cancel")
        cancel.clicked.connect(self.reject)
        apply = widgets.button("Apply", "primary")
        apply.clicked.connect(self._apply)
        btns.addWidget(reset)
        btns.addStretch()
        btns.addWidget(restart)
        btns.addWidget(cancel)
        btns.addWidget(apply)
        lay.addLayout(btns)
        module.start_calibration_measure()
        self._t = QTimer(self)
        self._t.timeout.connect(self._tick)
        self._t.start(250)
        self._tick()

    def done(self, r):
        self._t.stop()
        self.m.stop_calibration_measure()
        super().done(r)

    def _tick(self):
        ep = self.m.monitor.endpoint
        if ep is None:
            self.device.setText("No output device.")
            self.live.setText("")
            return
        cal, custom = self.m.calibration_for(ep)
        self.device.setText(f"Device: {ep.name} (currently {cal:.1f} dB, {'calibrated' if custom else 'default'})")
        raw, secs = self.m.calibration_measure()
        now = self.m.display_level
        if raw is None:
            self.live.setText("No sound yet. Play some audio.")
        else:
            self.live.setText(f"Now {now:.0f} dB · PC average since Restart {raw + cal:.1f} dB over {secs:.0f} s"
                              if now is not None else
                              f"PC average since Restart {raw + cal:.1f} dB over {secs:.0f} s")

    def _apply(self):
        raw, secs = self.m.calibration_measure()
        if raw is None or secs < 5:
            widgets.error(self, "Calibrate", "Let it measure at least 5 seconds of audio first.")
            return
        value = round(self.reading.value() - raw, 1)
        if not 80 <= value <= 135 and not widgets.confirm(
                self, "Calibrate", f"That means {value:.0f} dB at full volume, which is unusual (headphones are "
                                   f"typically 100-120). Use it anyway?"):
            return
        self.m.set_calibration(value)
        self.accept()

    def _reset(self):
        self.m.set_calibration(None)
        self.accept()


# ── panel ─────────────────────────────────────────────────────────────────────
class HearingPanel(widgets.FloatingPanel):
    TODAY_REFRESH_S = 5.0

    def __init__(self, m: "HearingModule"):
        self.m = m
        super().__init__("Hearing Guard", min_size=(380, 540), opacity=0.96)
        self.resize(440, 660)
        self.center_on()
        self.escape_pressed.connect(self.hide)
        self._big_colour = ""
        self._today_at = 0.0
        p = theme.P

        def section(top_rule: bool = True) -> QVBoxLayout:
            f = QFrame()
            widgets.style(f, f"background: transparent; border: none;"
                             f" border-top: 1px solid {p.border if top_rule else 'transparent'};")
            lay = QVBoxLayout(f)
            lay.setContentsMargins(18, 12, 18, 14)
            lay.setSpacing(8)
            body.addWidget(f)
            return lay

        body = QVBoxLayout()
        body.setContentsMargins(0, 0, 0, 0)
        body.setSpacing(0)

        # the reading: a big mono number, dB(A), how long is still safe
        now = section(False)
        now.setContentsMargins(18, 18, 18, 16)
        top = QHBoxLayout()
        top.setSpacing(14)
        self.big = widgets.label("—", 56, 500, mono=True, spacing=-0.03)
        top.addWidget(self.big, 0, Qt.AlignmentFlag.AlignBottom)
        col = QVBoxLayout()
        col.setSpacing(6)
        col.addStretch()
        col.addWidget(widgets.label("dB(A)", 11, 500, mono=True, tone="muted"))
        self.safe_label = widgets.label("", 13, tone="secondary")
        self.safe_label.setTextFormat(Qt.TextFormat.RichText)
        self.safe_label.setWordWrap(True)
        col.addWidget(self.safe_label)
        top.addLayout(col, 1)
        now.addLayout(top)
        now.addSpacing(6)
        self.bar = SegmentMeter()
        now.addWidget(self.bar)
        now.addLayout(_scale("40", "70", "85", "100"))
        self.vol_label = widgets.label("", 12, tone="muted")
        self.device_label = widgets.label("", 11, tone="muted")
        self.device_label.setWordWrap(True)
        now.addSpacing(4)
        now.addWidget(self.vol_label)
        now.addWidget(self.device_label)

        # today's dose
        dose = section()
        head = QHBoxLayout()
        head.addWidget(widgets.label("DOSE", 9.5, 600, mono=True, spacing=0.12, tone="muted"))
        head.addStretch()
        self.listen_label = widgets.label("", 9.5, mono=True, tone="muted")
        head.addWidget(self.listen_label)
        dose.addLayout(head)
        self.dose_label = widgets.label("", 13, tone="secondary")
        self.dose_label.setWordWrap(True)
        dose.addWidget(self.dose_label)
        self.dose_bar = Bar(4)
        dose.addWidget(self.dose_bar)

        # today's levels
        today = section()
        head = QHBoxLayout()
        head.addWidget(widgets.label("TODAY", 9.5, 600, mono=True, spacing=0.12, tone="muted"))
        head.addStretch()
        self.stats_label = widgets.label("", 9.5, mono=True, tone="muted")
        head.addWidget(self.stats_label)
        today.addLayout(head)
        self.today = TodayChart()
        today.addWidget(self.today, 1)
        self.today_scale = QHBoxLayout()
        today.addLayout(self.today_scale)

        # how long each level is safe
        safe = section()
        head = QHBoxLayout()
        head.addWidget(widgets.label("SAFE TIME PER DAY", 9.5, 600, mono=True, spacing=0.12, tone="muted"))
        head.addStretch()
        head.addWidget(widgets.label("NIOSH · 85 dB FOR 8 H, HALF PER +3 dB", 9.5, mono=True, tone="muted"))
        safe.addLayout(head)
        self.chart = SafeTimeChart()
        safe.addWidget(self.chart, 3)
        table = widgets.label("", 10, mono=True, tone="muted")
        table.setTextFormat(Qt.TextFormat.RichText)
        table.setText("&nbsp;&nbsp;&nbsp; ".join(
            f"{db}&nbsp;dB&nbsp;<span style='color:{p.text_primary}'>"
            + _short_duration(safe_seconds(db)).replace(" ", "&nbsp;") + "</span>" for db in range(85, 113, 3)))
        table.setWordWrap(True)
        safe.addWidget(table)

        # mode switch, limit, calibrate
        ctl = QHBoxLayout()
        ctl.setContentsMargins(18, 4, 18, 16)
        ctl.setSpacing(10)
        seg = QFrame()
        seg.setObjectName("segBox")
        seg.setStyleSheet(f"#segBox {{ background: {p.bg_frame}; border: 1px solid {p.border}; border-radius: 15px; }}"
                          f"#segBox QPushButton {{ background: transparent; border: none; border-radius: 12px;"
                          f" padding: 5px 12px; color: {p.text_muted}; font-size: 12px; }}"
                          f"#segBox QPushButton:checked {{ background: {p.bg_card}; color: {p.text_primary}; }}")
        sl = QHBoxLayout(seg)
        sl.setContentsMargins(2, 2, 2, 2)
        sl.setSpacing(0)
        self._mode_btns = {}
        for key, label, tip in (("limit", "Limit", "Dip the master volume while it is over the limit"),
                                ("monitor", "Monitor", "Only warn")):
            b_ = QPushButton(label)
            b_.setCheckable(True)
            b_.setAutoExclusive(True)
            b_.setToolTip(tip)
            b_.setCursor(Qt.CursorShape.PointingHandCursor)
            b_.clicked.connect(lambda _, k=key: m.set_mode(k))
            sl.addWidget(b_)
            self._mode_btns[key] = b_
        cur = m.s("mode", "limit")
        self._mode_btns.get(cur, self._mode_btns["limit"]).setChecked(True)
        for b_ in self._mode_btns.values():
            b_.setMinimumWidth(b_.sizeHint().width() + 8)
        ctl.addWidget(seg)
        self.limit_label = widgets.label("", 11, mono=True, tone="muted")
        ctl.addWidget(self.limit_label)
        ctl.addStretch()
        cal = widgets.button("Calibrate…", "ghost")
        cal.setStyleSheet("QPushButton { font-size: 12px; padding: 4px 6px; }")
        cal.clicked.connect(lambda: CalibrateDialog(m, self).exec())
        ctl.addWidget(cal)
        body.addLayout(ctl)
        more = widgets.icon_button("⋯", "More", 26, 12, ghost=True)
        more.clicked.connect(lambda: self._more_menu(more.mapToGlobal(more.rect().bottomLeft())))
        self.header_right.addWidget(more)
        self.body.addLayout(body)
        # FloatingPanel's fixed minimum overrides the layout's, which let the charts get squashed under the text
        need = self.layout().minimumSize()
        self.setMinimumSize(max(self.minimumWidth(), need.width()), max(self.minimumHeight(), need.height()))
        self.resize(max(self.width(), need.width()), max(self.height(), need.height() + 40))
        self.center_on()
        self.refresh(full=True)

    def showEvent(self, e):
        super().showEvent(e)
        self.refresh(full=True)

    def _set_big(self, text: str, colour: str):
        self.big.setText(text)
        if colour != self._big_colour:          # setStyleSheet re-polishes the widget: only when it changes
            self._big_colour = colour
            self.big.setStyleSheet(f"color: {colour};")

    def _more_menu(self, pos):
        menu = widgets.menu(self)
        menu.addAction("Calibrate…", lambda: CalibrateDialog(self.m, self).exec())
        menu.addAction("Reset today", self.m.reset_today)
        menu.addSeparator()
        menu.addAction("Hearing Guard settings…", lambda: self.m.ctx.open_settings("hearing"))
        menu.exec(pos)

    def _today_scale(self):
        widgets.clear_layout(self.today_scale)
        start, end = self.today.span
        mid1, mid2 = start + (end - start) // 3, start + 2 * (end - start) // 3
        for i, t in enumerate((f"{start // 60:02d}:{start % 60:02d}", f"{mid1 // 60:02d}:00",
                               f"{mid2 // 60:02d}:00", "NOW")):
            if i:
                self.today_scale.addStretch()
            self.today_scale.addWidget(widgets.label(t, 9.5, mono=True, tone="muted"))

    def refresh(self, full: bool = False):
        m, p = self.m, theme.P
        limit = float(m.s("limit_db", 85))
        est = m.display_level
        if est is None or est < AUDIBLE_DB:
            est = None
            self._set_big("—", p.text_muted)
        else:
            self._set_big(f"{est:.0f}", p.text_primary if est < limit else RED)
        self.bar.set_level(est, limit)

        ep, rd = m.monitor.endpoint, m.reading
        if ep is None or rd is None:
            self.vol_label.setText("No audio output device")
            self.device_label.setText(m.monitor.error)
        else:
            lowered = f" · apps lowered {-rd.dip_db:.0f} dB" if rd.dip_db < -0.5 else ""
            self.vol_label.setText("Master volume muted" if rd.muted else
                                   f"Master volume {rd.volume_pct:.0f} % ({rd.volume_db - rd.dip_db:+.1f} dB)"
                                   f"{lowered}")
            cal, custom = m.calibration_for(ep)
            text = f"{ep.name} · {'calibrated' if custom else 'not calibrated'} ({cal:.0f} dB)"
            self.device_label.setText(text)
        self.limit_label.setText(f"LIMIT {limit:.0f} dB")
        self.limit_label.setToolTip(f"Limit {limit:.0f} dB · volume cap {int(m.s('max_volume', 100))} %")

        dose = m.dose
        if est:
            left = fmt_duration(max(0.0, 1.0 - dose) * safe_seconds(est))
            self.safe_label.setText(f"Safe for <span style='color:{p.text_primary}'>{left}</span> more today")
        else:
            self.safe_label.setText("Quiet. Nothing is playing.")
        self.dose_label.setText(f"{dose * 100:.0f} % of today's allowance used")
        self.dose_bar.set_value(dose, GREEN if dose < 0.5 else (AMBER if dose < 1.0 else RED))
        self.listen_label.setText(f"LISTENED {fmt_duration(m.listen_seconds).upper()}")
        self.chart.set_state(est, limit)

        now = time.monotonic()
        if full or now - self._today_at >= self.TODAY_REFRESH_S:
            self._today_at = now
            minutes = m.today_minutes()
            self.today.set_data(minutes, limit)
            self._today_scale()
            avg = m.today_average()
            peak = max(minutes.values(), default=None)
            self.stats_label.setText(" · ".join(x for x in (f"AVG {avg:.0f}" if avg is not None else "",
                                                             f"PEAK {peak:.0f}" if peak is not None else "") if x))


# ── module ────────────────────────────────────────────────────────────────────
class HearingModule(Module):
    key = "hearing"
    title = "Hearing Guard"
    icon = "🎧"

    TICK_MS = 200
    WINDOW_S = 1.0               # "slow" sound-level-meter averaging for the display and the limiter
    MIN_WINDOW_S = 0.6           # the limiter waits for this much data before acting
    RELEASE_HOLD_S = 1.0         # after a cut: wait this long before turning back up
    RELEASE_DB_PER_S = 5.0       # ...then rise this fast
    RELEASE_HEADROOM_DB = 2.0    # ...as far as keeps the level this much under the limit
    DIP_NOTIFY_S = 600.0         # dips are routine: say so at most this often
    SAVE_S = 60.0

    def __init__(self, ctx):
        super().__init__(ctx)
        self.panel: HearingPanel | None = None
        self.monitor = AudioMonitor()
        self.reading: Reading | None = None
        self.display_level: float | None = None     # dB(A) over the last second at the current volume
        self.dose = 0.0
        self.listen_seconds = 0.0
        self._day = date.today().isoformat()
        self._minutes: dict[int, list] = {}        # minute of day -> [sum 10^(dB/10)*s, s]
        self._data: dict = {}
        self._calibrations: dict = {}
        self._window: deque = deque()               # (t, seconds, pre-volume energy*seconds, audible seconds)
        self._window_key: tuple | None = None        # (device id, signal includes volume) the window was measured with
        self._dirty = False
        self._last_tick = time.monotonic()
        self._last_save = time.monotonic()
        self._last_warn = -1e9
        self._over_since: float | None = None
        self.restore_to: float | None = None        # the volume (dB) you had before the limiter cut in
        self._restore_dev = ""
        self._last_cut = -1e9
        self._expected_vol: float | None = None     # last volume seen or set by us; anything else = you changed it
        self._warned_levels: set[int] = set()
        self._cal: list | None = None               # [device id, energy*s, s] while calibrating
        self._tick_error = ""
        self._timer = QTimer(self)
        self._timer.setInterval(self.TICK_MS)
        self._timer.timeout.connect(self.tick)

    # ── lifecycle ──────────────────────────────────────────────────────────
    def start(self):
        self._load()
        self._last_tick = time.monotonic()
        self._timer.start()
        self.hotkey("hotkey", self.toggle, "hearing guard panel")
        if self.ctx.shows_on_start("hearing"):
            self.toggle()
        self.log.info("ready")

    def stop(self):
        self._timer.stop()
        ep = self.monitor.endpoint
        if self.restore_to is not None and self._expected_vol is not None and ep is not None and ep.id == self._restore_dev:
            try:
                # don't leave a dip behind when the guard goes away, unless you have moved the volume since
                ep.refresh()
                if abs(ep.volume_db() - self._expected_vol) <= 0.25:
                    ep.set_volume_db(self.restore_to)
            except Exception as e:
                self.log.warning("could not put the volume back: %s", _describe(e))
        self._save()
        self.monitor.close()
        self._window.clear()
        self.display_level = self.reading = self.restore_to = None
        if self.panel:
            self.panel.close()
            self.panel.deleteLater()
            self.panel = None

    def apply_settings(self):
        self.hotkeys.unregister_all(self.key)
        self.hotkey("hotkey", self.toggle, "hearing guard panel")
        ep = self.monitor.endpoint
        if ep is not None and (self.s("mode", "limit") != "limit" or not self.s("restore_volume", True)):
            try:
                ep.reset_dip()                   # dips only exist while they come back up by themselves
            except Exception as e:
                self.log.warning("could not restore the app volumes: %s", _describe(e))
            self.restore_to = None
        if self.panel:
            btn = self.panel._mode_btns.get(self.s("mode", "limit"))
            if btn is not None:
                btn.setChecked(True)
            self.panel.refresh(full=True)

    def tray_actions(self):
        return [("Hearing Guard panel", self.toggle)]

    def retheme(self):
        if self.panel:
            visible = self.panel.isVisible()
            self.panel.close()
            self.panel.deleteLater()
            self.panel = None
            if visible:
                self.toggle()

    def toggle(self):
        if self.panel is None:
            self.panel = HearingPanel(self)
        self.panel.toggle()

    # ── settings helpers ───────────────────────────────────────────────────
    def set_mode(self, mode: str):
        self.settings.set("hearing.mode", mode)
        self.settings.save(emit=False)

    def calibration_for(self, ep: Endpoint) -> tuple[float, bool]:
        """(calibration dB, True if it was measured for this device rather than the default)."""
        c = self._calibrations.get(ep.id)
        if isinstance(c, dict) and isinstance(c.get("db"), (int, float)):
            return float(c["db"]), True
        return float(self.s("calibration", 105)), False

    def set_calibration(self, value: float | None):
        ep = self.monitor.endpoint
        if ep is None:
            return
        if value is None:
            self._calibrations.pop(ep.id, None)
            msg = f"{ep.name} uses the default calibration again."
        else:
            self._calibrations[ep.id] = {"db": value, "name": ep.name, "date": date.today().isoformat()}
            msg = f"{ep.name}: {value:.1f} dB at 100 % volume with a full-scale tone."
        self._save()
        self.ctx.notify(self.title, msg)

    def start_calibration_measure(self):
        ep = self.monitor.endpoint
        self._cal = [ep.id if ep else None, 0.0, 0.0]

    def stop_calibration_measure(self):
        self._cal = None

    def calibration_measure(self) -> tuple[float | None, float]:
        """(average signal + volume in dB since the measurement started, seconds measured)."""
        if not self._cal:
            return None, 0.0
        return _db(self._cal[1], self._cal[2]), self._cal[2]

    # ── the tick ───────────────────────────────────────────────────────────
    def tick(self):
        try:
            self._tick()
            self._tick_error = ""
        except Exception as e:
            # log each distinct failure once; an exception every 200 ms would flood the log and the tray
            if str(e) != self._tick_error:
                self._tick_error = str(e)
                self.log.exception("tick failed")

    def _tick(self):
        now = time.monotonic()
        dt = min(1.0, max(0.0, now - self._last_tick))   # capped so sleep/resume is not counted as listening
        self._last_tick = now
        now_dt = datetime.now()
        self._roll_day(now_dt)

        rd = self.reading = self.monitor.poll(now, dt)
        ep = self.monitor.endpoint
        if rd is None or ep is None:
            self._window.clear()
            self.display_level = None
            self._over_since = None
        else:
            includes_volume = self.s("meter_includes_volume", False) and not ep.hw_volume
            if (ep.id, includes_volume) != self._window_key:     # samples measured another way: start over
                self._window_key = (ep.id, includes_volume)
                self._window.clear()
            cal, _ = self.calibration_for(ep)
            # the signal before the master volume; the volume is added on top when a level is needed
            pre = None if rd.level is None else rd.level - (rd.volume_db if includes_volume else 0.0)
            est = None if pre is None else pre + rd.volume_db + cal

            if self._cal is not None:
                if self._cal[0] != ep.id:
                    self._cal = [ep.id, 0.0, 0.0]          # device changed mid-calibration: start over
                if pre is not None:
                    self._cal[1] += 10 ** ((pre + rd.volume_db) / 10.0) * rd.secs
                self._cal[2] += rd.secs

            if est is not None and est >= AUDIBLE_DB:
                self.dose += rd.secs / safe_seconds(est)
                self.listen_seconds += rd.secs
                bucket = self._minutes.setdefault(now_dt.hour * 60 + now_dt.minute, [0.0, 0.0])
                bucket[0] += 10 ** (est / 10.0) * rd.secs
                bucket[1] += rd.secs
                self._dirty = True

            # one-second window of the signal alone; the current volume is added when it is read, so the
            # display follows the volume knob at once and the limiter never counts the same excess twice
            self._window.append((now, rd.secs, 0.0 if pre is None else 10 ** (pre / 10.0) * rd.secs,
                                 0.0 if pre is None else rd.secs))
            while self._window and self._window[0][0] <= now - self.WINDOW_S:
                self._window.popleft()
            energy = sum(w[2] for w in self._window)
            sig = _db(energy, sum(w[1] for w in self._window))
            # the same over just the audible part: a song ending in silence is not a quiet passage
            heard = _db(energy, sum(w[3] for w in self._window))
            self.display_level = None if sig is None else sig + rd.volume_db + cal
            self._enforce(now, ep, rd, None if heard is None else heard + rd.volume_db + cal)
            self._dose_warnings()

        if self._dirty and now - self._last_save > self.SAVE_S:
            self._save()
        if self.panel and self.panel.isVisible():
            self.panel.refresh()

    def _enforce(self, now: float, ep: Endpoint, rd: Reading, heard: float | None):
        limit = float(self.s("limit_db", 85))
        cap = float(self.s("max_volume", 100))
        level = self.display_level
        limiting = self.s("mode", "limit") == "limit"
        try:
            # a volume change we did not make means you took over: your new volume is the one to come back to
            if self.restore_to is not None and (
                    not limiting or not self.s("restore_volume", True) or self._restore_dev != ep.id
                    or self._expected_vol is None or abs(rd.volume_db - self._expected_vol) > 0.25):
                self.restore_to = None
                ep.reset_dip()                  # you set a volume yourself: the apps go back to theirs
                rd.volume_db = ep.volume_db()
            self._expected_vol = rd.volume_db
            if rd.volume_pct > cap + 0.5:
                ep.set_volume_percent(cap)
                self.restore_to = self._expected_vol = None
                self.log.info("volume capped to %.0f%%", cap)
                return                          # `level` was measured at the old volume
            if level is None or level - limit <= 0.5:
                self._over_since = None
                self._release(now, ep, rd, heard, limit)
                return
            if self._over_since is None:
                self._over_since = now
            if limiting:
                if sum(w[1] for w in self._window) < self.MIN_WINDOW_S:
                    return
                target = rd.volume_db - (level - limit) - 0.5
                dipping = self.s("restore_volume", True)
                if self.restore_to is None and dipping:
                    self.restore_to, self._restore_dev = rd.volume_db, ep.id
                self._expected_vol = ep.set_volume_db(target, dip=dipping)
                self._last_cut = now
                self.log.debug("limited: %.1f dB -> volume %.1f dB", level, target)
                self._over_since = None
                if now - self._last_warn > (self.DIP_NOTIFY_S if dipping else 30):
                    self._last_warn = now
                    self.ctx.notify(self.title, f"Dipping the volume while it is over {limit:.0f} dB. It comes back "
                                                f"up after the loud part." if dipping else
                                                f"Volume turned down to stay under {limit:.0f} dB.")
            elif now - self._over_since >= 3 and now - self._last_warn > 30:
                self._last_warn = now
                self.ctx.notify(self.title, f"About {level:.0f} dB, above your {limit:.0f} dB limit. "
                                            f"Safe for {fmt_duration(safe_seconds(level))} per day at this level.")
        except Exception as e:
            self.log.warning("could not adjust volume: %s", _describe(e))

    def _release(self, now: float, ep: Endpoint, rd: Reading, heard: float | None, limit: float):
        """
        After a dip, turn back up towards your volume as soon as the loud part is over. `heard` is the level of
        the audible part of the last second at the current volume; the window holds the signal before the
        volume, so it follows each step at once and the rise stops where the music would come within
        RELEASE_HEADROOM_DB of the limit (the gap to the +0.5 dB cut is the hysteresis).
        """
        if self.restore_to is None or now - self._last_cut < self.RELEASE_HOLD_S:
            return
        if heard is None:                              # silence says nothing about how loud the next part is
            return
        step = min(self.RELEASE_DB_PER_S * rd.secs, self.restore_to - rd.volume_db,
                   limit - self.RELEASE_HEADROOM_DB - heard)
        if step > 0.05:
            self._expected_vol = ep.set_volume_db(rd.volume_db + step)
        if self._expected_vol >= self.restore_to - 0.05:
            self.log.debug("dip over: volume back to %.1f dB", self.restore_to)
            self.restore_to = None

    def _dose_warnings(self):
        if not self.s("warn_dose", True):
            return
        for level in (50, 80, 100):
            if self.dose * 100 >= level and level not in self._warned_levels:
                self._warned_levels.add(level)
                msg = ("You have used all of today's safe listening allowance. Give your ears a rest."
                       if level == 100 else f"You have used {level} % of today's safe listening allowance.")
                self.ctx.toast(self.title, msg)

    # ── data ───────────────────────────────────────────────────────────────
    def _roll_day(self, now_dt: datetime):
        today = now_dt.date().isoformat()
        if today != self._day:
            self._save()
            self._day = today
            self.dose = self.listen_seconds = 0.0
            self._minutes = {}
            self._warned_levels = set()

    def today_minutes(self) -> dict[int, float]:
        return {m: _db(e, s) for m, (e, s) in self._minutes.items() if e > 0 and s > 0}

    def today_average(self) -> float | None:
        return _db(sum(e for e, _ in self._minutes.values()), sum(s for _, s in self._minutes.values()))

    def reset_today(self):
        self.dose = self.listen_seconds = 0.0
        self._minutes = {}
        self._warned_levels = set()
        self._save()
        if self.panel:
            self.panel.refresh(full=True)

    def _load(self):
        data = load_json(DATA_PATH, {})
        self._data = data if isinstance(data, dict) else {}
        days = self._data.setdefault("days", {})
        self._calibrations = self._data.setdefault("calibrations", {})
        self._day = date.today().isoformat()
        # Days recorded while the PC clock had jumped ahead would otherwise never age out (they always sort
        # newest), and once that date really comes their phantom dose would be added to the real day.
        future = [k for k in days if k > self._day]
        if future:
            for k in future:
                del days[k]
            self.log.info("dropped %d day(s) dated in the future (the clock was ahead): %s", len(future),
                          ", ".join(sorted(future)))
        self.dose, self.listen_seconds, self._minutes = 0.0, 0.0, {}
        try:
            day = days.get(self._day, {})
            for k, v in day.get("minutes", {}).items():
                db, secs = v if isinstance(v, list) else (v, 60.0)     # files before this version stored only the level
                self._minutes[int(k)] = [10 ** (float(db) / 10.0) * float(secs), float(secs)]
            self.dose = float(day.get("dose", 0.0))
            self.listen_seconds = float(day.get("listen", sum(s for _, s in self._minutes.values())))
        except (TypeError, ValueError, AttributeError):
            self.log.exception("today's entry in hearing.json is damaged; starting the day fresh")
            self._minutes = {}
        self._warned_levels = {lv for lv in (50, 80, 100) if self.dose * 100 >= lv}

    def _save(self):
        self._last_save = time.monotonic()
        self._dirty = False
        days = self._data.setdefault("days", {})
        days[self._day] = {
            "dose": round(self.dose, 6),
            "listen": round(self.listen_seconds, 1),
            "minutes": {str(m): [round(_db(e, s), 1), round(s, 1)] for m, (e, s) in self._minutes.items() if e > 0 and s > 0},
        }
        for k in sorted(days)[:-60]:
            del days[k]
        self._data["calibrations"] = self._calibrations
        try:
            save_json(DATA_PATH, self._data)
        except OSError:
            self.log.exception("could not save hearing.json")
