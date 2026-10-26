"""
Hearing Guard on Windows: the default output device through Core Audio (pycaw / comtypes).

modules/hearing.py uses AudioMonitor and AppsWatcher from here on Windows; the classes have the same
interface as the PipeWire ones.

  * the level comes from a shared-mode WASAPI loopback capture ("what you hear"), A-weighted like a
    sound-level meter; while it starts up (or an exclusive-mode app bypasses it) the endpoint's peak
    meter stands in, corrected by the peak-to-RMS gap it has learnt
  * dips are made on the app sessions (ISimpleAudioVolume), not the master volume: the volume flyout
    stays put, and an app is never turned above the volume you gave it
  * the APPS view reads every session's own peak meter, which costs nothing (no extra capture)
"""
import math
import time
import ctypes
import logging
from collections import deque
from ctypes import POINTER, c_ubyte, c_uint32, c_uint64
from ctypes.wintypes import DWORD
from types import SimpleNamespace

from PyQt6.QtCore import QThread, pyqtSignal

from modules.hearing import AWeightingFilter, Reading, SINE_RMS_DB

log = logging.getLogger("hearing")

AUDCLNT_STREAMFLAGS_LOOPBACK = 0x00020000
AUDCLNT_BUFFERFLAGS_SILENT = 0x2
ENDPOINT_HARDWARE_SUPPORT_VOLUME = 0x1
E_RENDER, E_MULTIMEDIA = 0, 1
DEFAULT_CREST_DB = 12.0          # peak-to-RMS gap of typical music, until the loopback has measured it
_HRESULT_TEXT = {
    0x88890004: "the output device was removed or reconfigured",
    0x88890010: "the Windows Audio service is not running",
    0x80070490: "no audio output device",
}

_api: SimpleNamespace | None = None


def _audio_api() -> SimpleNamespace:
    """Import pycaw/comtypes on first use, so a missing package only disables the meter."""
    global _api
    if _api is None:
        import comtypes
        from comtypes import COMMETHOD, GUID, HRESULT, IUnknown
        from pycaw.api.audioclient import IAudioClient, ISimpleAudioVolume
        from pycaw.api.endpointvolume import IAudioEndpointVolume, IAudioMeterInformation
        from pycaw.api.mmdeviceapi import IMMDeviceEnumerator, PROPERTYKEY
        from pycaw.constants import CLSID_MMDeviceEnumerator
        from pycaw.utils import AudioUtilities

        class IAudioCaptureClient(IUnknown):        # not wrapped by pycaw
            _iid_ = GUID("{C8ADBD64-E71E-48A0-A4DE-185C395CD317}")
            _methods_ = (
                COMMETHOD([], HRESULT, "GetBuffer",
                          (["out"], POINTER(POINTER(c_ubyte)), "ppData"),
                          (["out"], POINTER(c_uint32), "pNumFramesToRead"),
                          (["out"], POINTER(DWORD), "pdwFlags"),
                          (["out"], POINTER(c_uint64), "pu64DevicePosition"),
                          (["out"], POINTER(c_uint64), "pu64QPCPosition")),
                COMMETHOD([], HRESULT, "ReleaseBuffer", (["in"], c_uint32, "NumFramesRead")),
                COMMETHOD([], HRESULT, "GetNextPacketSize", (["out"], POINTER(c_uint32), "pNumFramesInNextPacket")),
            )

        name_key = PROPERTYKEY()
        name_key.fmtid = GUID("{A45C254E-DF1C-4EFD-8020-67D146A850E0}")   # PKEY_Device_FriendlyName
        name_key.pid = 14
        _api = SimpleNamespace(
            comtypes=comtypes, IAudioClient=IAudioClient, IAudioCaptureClient=IAudioCaptureClient,
            ISimpleAudioVolume=ISimpleAudioVolume, IAudioEndpointVolume=IAudioEndpointVolume,
            IAudioMeterInformation=IAudioMeterInformation, IMMDeviceEnumerator=IMMDeviceEnumerator,
            CLSID_MMDeviceEnumerator=CLSID_MMDeviceEnumerator, AudioUtilities=AudioUtilities, name_key=name_key)
    return _api


def _activate(device, iface):
    # QueryInterface, never ctypes.cast(): a cast copy Releases the COM object a second time when it is
    # garbage-collected (that was the "access violation" in old logs)
    return device.Activate(iface._iid_, _audio_api().comtypes.CLSCTX_ALL, None).QueryInterface(iface)


def _friendly_name(device) -> str:
    try:
        value = device.OpenPropertyStore(0).GetValue(_audio_api().name_key)     # STGM_READ
        name = value.GetValue()
        value.clear()
        return name if isinstance(name, str) and name else "Default output"
    except Exception:
        return "Default output"


def _describe(e: Exception) -> str:
    hr = getattr(e, "hresult", None)
    if hr is None and e.args and isinstance(e.args[0], int):
        hr = e.args[0]
    if isinstance(hr, int) and (hr & 0xFFFFFFFF) in _HRESULT_TEXT:
        return _HRESULT_TEXT[hr & 0xFFFFFFFF]
    return f"{type(e).__name__}: {e}"


def _sessions() -> list:
    """The app sessions on the default output (pycaw AudioSession objects)."""
    return [s for s in _audio_api().AudioUtilities.GetAllSessions() if s.SimpleAudioVolume is not None]


def _session_key(s) -> str:
    try:
        return s._ctl.GetSessionInstanceIdentifier()
    except Exception:
        return f"pid{s.ProcessId}"


def _session_name(s) -> str:
    try:
        if s.Process is not None:
            name = s.Process.name()
            return name[:-4] if name.lower().endswith(".exe") else name
    except Exception:
        pass
    try:
        name = s._ctl.GetDisplayName()
        if name and not name.startswith("@"):
            return name
    except Exception:
        pass
    return "System sounds" if not s.ProcessId else f"Process {s.ProcessId}"


class Loopback:
    """Shared-mode WASAPI loopback capture of one output device ("what you hear")."""
    BUFFER_HNS = 20_000_000          # 2 s, so a briefly busy UI thread loses nothing

    def __init__(self, device):
        import numpy as np
        api = _audio_api()
        self.np = np
        self.peak = 0.0
        self.client = _activate(device, api.IAudioClient)
        fmt = self.client.GetMixFormat()
        try:
            wf = fmt.contents
            self.rate, self.channels, self.align = int(wf.nSamplesPerSec), int(wf.nChannels), int(wf.nBlockAlign)
            tag = wf.wFormatTag
            if tag == 0xFFFE and wf.cbSize >= 22:                 # WAVEFORMATEXTENSIBLE: SubFormat.Data1
                tag = c_uint32.from_address(ctypes.addressof(wf) + 24).value
            kinds = {(3, 32): (np.float32, 1.0), (1, 16): (np.int16, 1 / 32768), (1, 32): (np.int32, 1 / 2 ** 31)}
            if (tag, wf.wBitsPerSample) not in kinds:
                raise RuntimeError(f"unsupported mix format (tag {tag:#x}, {wf.wBitsPerSample} bit)")
            self.dtype, self.scale = kinds[(tag, wf.wBitsPerSample)]
            self.client.Initialize(0, AUDCLNT_STREAMFLAGS_LOOPBACK, self.BUFFER_HNS, 0, fmt, None)
        finally:
            ctypes.windll.ole32.CoTaskMemFree(fmt)
        self.capture = self.client.GetService(api.IAudioCaptureClient._iid_).QueryInterface(api.IAudioCaptureClient)
        self.weighting = AWeightingFilter(self.rate, self.channels)
        self.client.Start()

    def read(self) -> tuple[float, int]:
        """Drain what was captured since the last call -> (A-weighted energy of the loudest channel, frames)."""
        np = self.np
        chunks, frames = [], 0
        while self.capture.GetNextPacketSize():
            data, n, flags, _, _ = self.capture.GetBuffer()
            if n:
                if flags & AUDCLNT_BUFFERFLAGS_SILENT:
                    chunks.append(np.zeros((n, self.channels), np.float32))
                else:
                    raw = np.ctypeslib.as_array(data, shape=(n * self.align,)).view(self.dtype).reshape(n, self.channels)
                    chunks.append(raw.copy() if self.scale == 1.0 else raw.astype(np.float32) * self.scale)
                frames += n
            self.capture.ReleaseBuffer(n)
        if not frames:
            self.peak = 0.0
            return 0.0, 0
        x = np.concatenate(chunks) if len(chunks) > 1 else chunks[0]
        self.peak = float(np.abs(x).max())
        return float(self.weighting.energy(x).max()), frames

    def close(self):
        try:
            self.client.Stop()
        except Exception:
            pass
        self.capture = self.client = None


class Endpoint:
    """The default output device: master volume, peak meter, the app sessions and (while audio plays) a
    loopback capture. Same interface as the PipeWire Endpoint in hearing.py."""
    DIP_LATENCY_S = 0.05         # audio being read now was played this long ago (at that moment's dip)

    def __init__(self, device, dev_id: str):
        api = _audio_api()
        self.device, self.id = device, dev_id
        self.name = _friendly_name(device)
        self.volume = _activate(device, api.IAudioEndpointVolume)
        self.meter = _activate(device, api.IAudioMeterInformation)
        lo, hi, _ = self.volume.GetVolumeRange()
        self.min_db, self.max_db = float(lo), float(hi)
        # hardware volume is applied after anything software can tap, so the signal is certainly pre-volume
        self.hw_volume = bool(self.volume.QueryHardwareSupport() & ENDPOINT_HARDWARE_SUPPORT_VOLUME)
        self.loopback: Loopback | None = None
        self.dip_db = 0.0
        self._streams: dict[str, tuple[float, float]] = {}    # session -> (your volume, what we set)
        self._dips: deque = deque([(0.0, 0.0)], maxlen=64)

    def refresh(self):
        if self.dip_db < 0:
            self._apply_dip(self.dip_db)        # apps that started since the last tick get the dip too

    @property
    def volume_linear(self) -> float:
        return float(self.volume.GetMasterVolumeLevelScalar())

    def master_db(self) -> float:
        return float(self.volume.GetMasterVolumeLevel())

    def volume_db(self) -> float:
        return self.master_db() + self.dip_db

    def dip_at(self, t: float) -> float:
        for when, dip in reversed(self._dips):
            if when <= t:
                return dip
        return self._dips[0][1]

    @property
    def muted(self) -> bool:
        return bool(self.volume.GetMute())

    @property
    def running(self) -> bool:
        return float(self.meter.GetPeakValue()) > 1e-5

    def open_loopback(self) -> Loopback:
        return Loopback(self.device)

    def _apply_dip(self, dip: float):
        factor = 10 ** (dip / 20.0)              # session volumes are linear gains
        alive = set()
        for s in _sessions():
            key = _session_key(s)
            alive.add(key)
            vol = s.SimpleAudioVolume
            cur = float(vol.GetMasterVolume())
            mine = self._streams.get(key)
            if mine is None:
                base = cur                       # a session we have not touched: its volume is yours
            else:
                base, applied = mine
                if abs(cur - applied) > 0.003:
                    # you moved this app's volume during a dip: keep that, relative to the dip
                    old = self.dip_at(time.monotonic())
                    base = min(1.0, cur / (10 ** (old / 20.0) if old < 0 else 1.0))
            target = base if dip >= 0 else base * factor
            if abs(cur - target) > 1e-4:
                vol.SetMasterVolume(max(0.0, min(1.0, target)), None)
            if dip >= 0:
                self._streams.pop(key, None)
            else:
                self._streams[key] = (base, target)
        for key in list(self._streams):
            if key not in alive:
                del self._streams[key]

    def set_volume_db(self, db: float, dip: bool = True) -> float:
        """Set the effective volume and return what was actually set (see hearing.py's Endpoint)."""
        db = max(self.min_db, min(self.max_db, db))
        if dip:
            new = max(-60.0, min(0.0, db - self.master_db()))
            if abs(new - self.dip_db) > 0.01:
                self._apply_dip(new)
                self.dip_db = new if new < -0.01 else 0.0
                self._dips.append((time.monotonic(), self.dip_db))
            return self.volume_db()
        self.volume.SetMasterVolumeLevel(db, None)
        return self.volume_db()

    def reset_dip(self):
        if self.dip_db < 0 or self._streams:
            self._apply_dip(0.0)
            self.dip_db = 0.0
            self._dips.append((time.monotonic(), 0.0))

    def set_volume_percent(self, pct: float):
        """The hard cap: this one moves the master volume (only ever down)."""
        self.volume.SetMasterVolumeLevelScalar(max(0.0, min(1.0, pct / 100.0)), None)

    def set_app_volume(self, index: int, value: float):
        """`index` is the app's process id (what AppsWatcher reports)."""
        for s in _sessions():
            if s.ProcessId == index:
                s.SimpleAudioVolume.SetMasterVolume(max(0.0, min(1.0, value)), None)

    def close(self):
        try:
            self.reset_dip()                     # never leave the apps turned down
        except Exception:
            log.warning("could not restore the app volumes")
        if self.loopback:
            self.loopback.close()
        self.loopback = self.volume = self.meter = self.device = None


class AudioMonitor:
    """Follows the default output device and turns each tick into a Reading. Everything runs on the UI thread."""
    DEVICE_CHECK_S = 2.0         # how often to ask Windows which device is the default (~1 ms)
    LOOPBACK_IDLE_S = 10.0       # stop capturing after this long without sound
    PLAYING_PEAK = 1e-5          # -100 dBFS

    def __init__(self):
        self.endpoint: Endpoint | None = None
        self.error = ""
        self.loopback_error = ""
        self.crest_db = DEFAULT_CREST_DB
        self._enum = None
        self._check_at = 0.0
        self._retry_at = 0.0
        self._backoff = 1.0
        self._loopback_retry_at = 0.0
        self._last_sound = 0.0

    def close(self):
        if self.endpoint:
            self.endpoint.close()
        self.endpoint = None
        self._enum = None

    def poll(self, now: float, dt: float) -> Reading | None:
        if not self._ensure_device(now):
            return None
        ep = self.endpoint
        try:
            ep.refresh()
            volume_db = ep.volume_db()
            volume_pct = ep.volume_linear * 100.0
            muted = ep.muted
            peak = float(ep.meter.GetPeakValue())
            level, secs, source = self._measure(ep, now, dt, peak, muted)
        except Exception as e:
            self._fail(e, now)
            return None
        return Reading(level, secs, source, volume_db, volume_pct, muted, ep.dip_db)

    def _ensure_device(self, now: float) -> bool:
        if self.endpoint is not None and now < self._check_at:
            return True
        if self.endpoint is None and now < self._retry_at:
            return False
        self._check_at = now + self.DEVICE_CHECK_S
        try:
            api = _audio_api()
            if self._enum is None:
                self._enum = api.comtypes.CoCreateInstance(api.CLSID_MMDeviceEnumerator, api.IMMDeviceEnumerator,
                                                           api.comtypes.CLSCTX_INPROC_SERVER)
            device = self._enum.GetDefaultAudioEndpoint(E_RENDER, E_MULTIMEDIA)
            dev_id = device.GetId()
            if self.endpoint is None or self.endpoint.id != dev_id:
                if self.endpoint:
                    self.endpoint.close()
                self.endpoint = None
                self.endpoint = Endpoint(device, dev_id)
                self._loopback_retry_at = 0.0
                log.info("output device: %s%s", self.endpoint.name,
                         " (hardware volume)" if self.endpoint.hw_volume else "")
            self.error = ""
            self._backoff = 1.0
            return True
        except ImportError as e:
            self.error = f"{e} (pip install pycaw numpy)"
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
        if self.endpoint:
            self.endpoint.close()
        self.endpoint = None
        self._enum = None                              # the audio service may have restarted
        self._retry_at = now + self._backoff
        self._backoff = min(10.0, self._backoff * 2)

    def _measure(self, ep: Endpoint, now: float, dt: float, peak: float, muted: bool):
        playing = peak > self.PLAYING_PEAK and not muted
        if playing:
            self._last_sound = now
        lb = ep.loopback
        if lb is None and playing and now >= self._loopback_retry_at:
            try:
                ep.loopback = lb = ep.open_loopback()
                self.loopback_error = ""
            except Exception as e:
                msg = _describe(e)
                if msg != self.loopback_error:
                    log.warning("loopback capture unavailable (%s); using the peak meter", msg)
                self.loopback_error = msg
                self._loopback_retry_at = now + 60.0

        level, secs, source = None, dt, ""
        if lb is not None:
            energy, frames = lb.read()
            if frames:
                secs = max(dt, frames / lb.rate)           # fewer frames than wall time: the rest was silence
                ms = energy / (secs * lb.rate)
                if ms > 1e-11 and not muted:               # above -110 dBFS and actually audible
                    # the capture already has our dip in it; the guard wants the level before it
                    dip = ep.dip_at(now - ep.DIP_LATENCY_S)
                    level, source = 10.0 * math.log10(ms) + SINE_RMS_DB - dip, "loopback"
                    self._last_sound = now
                    if playing and level > -60:            # learn the peak-to-RMS gap for the fallback
                        self.crest_db += 0.02 * (20.0 * math.log10(peak) - level - self.crest_db)
            if now - self._last_sound > self.LOOPBACK_IDLE_S:
                lb.close()
                ep.loopback = None
        if level is None and playing:
            # loopback starting up, unavailable, or bypassed by an exclusive-mode app
            level, source = 20.0 * math.log10(peak) - self.crest_db - ep.dip_db, "peak"
        return level, secs, source


class AppsWatcher(QThread):
    """Every app playing on the default output with its volume and its own peak level, once a second."""
    ready = pyqtSignal(list)                 # [{"index" (process id), "name", "volume", "peak"}]
    failed = pyqtSignal(str)

    def __init__(self):
        super().__init__()
        self._stop = False

    def stop(self):
        self._stop = True

    def run(self):
        try:
            api = _audio_api()
            api.comtypes.CoInitialize()
        except Exception as e:
            self.failed.emit(_describe(e))
            return
        try:
            while not self._stop:
                out = {}
                for s in _sessions():
                    try:
                        peak = float(s._ctl.QueryInterface(api.IAudioMeterInformation).GetPeakValue())
                        vol = float(s.SimpleAudioVolume.GetMasterVolume())
                    except Exception:
                        continue
                    pid = int(s.ProcessId or 0)
                    prev = out.get(pid)
                    if prev is None or peak > prev["peak"]:       # an app with several sessions: the loudest
                        out[pid] = {"index": pid, "name": _session_name(s), "volume": vol, "peak": peak}
                self.ready.emit(list(out.values()))
                for _ in range(8):
                    if self._stop:
                        break
                    self.msleep(100)
        except Exception as e:
            self.failed.emit(_describe(e))
        finally:
            try:
                api.comtypes.CoUninitialize()
            except Exception:
                pass
