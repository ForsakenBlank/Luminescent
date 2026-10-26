"""
Small system controls and readings for the Automation nodes: the volume, lock / sleep / screen off,
temperatures, GPU load, the focused window and whether the internet is reachable.

Everything here either returns quickly or runs its slow part on a worker thread (nvidia-smi, the
internet check), because the automation watchers tick on the UI thread.
"""
import os
import glob
import time
import shutil
import socket
import logging
import threading
import subprocess

from core import IS_WIN, hypr

log = logging.getLogger("system")


def _run(cmd: list[str], timeout: float = 5) -> str | None:
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        return r.stdout if r.returncode == 0 else None
    except (OSError, subprocess.SubprocessError):
        return None


# ── volume ────────────────────────────────────────────────────────────────────
def _win_volume():
    from comtypes import CLSCTX_ALL
    from pycaw.pycaw import AudioUtilities, IAudioEndpointVolume
    dev = AudioUtilities.GetSpeakers()
    ep = getattr(dev, "EndpointVolume", None)
    if ep is not None:
        return ep
    return dev.Activate(IAudioEndpointVolume._iid_, CLSCTX_ALL, None).QueryInterface(IAudioEndpointVolume)


def get_volume() -> tuple[float, bool] | None:
    """(percent, muted) of the default output, None when it can not be read."""
    if IS_WIN:
        try:
            v = _win_volume()
            return float(v.GetMasterVolumeLevelScalar()) * 100, bool(v.GetMute())
        except Exception:
            log.debug("volume read failed", exc_info=True)
            return None
    out = _run(["wpctl", "get-volume", "@DEFAULT_AUDIO_SINK@"]) if shutil.which("wpctl") else None
    if out:                                   # "Volume: 0.45 [MUTED]"
        try:
            return float(out.split()[1]) * 100, "MUTED" in out
        except (IndexError, ValueError):
            pass
    if shutil.which("pactl"):
        vol = _run(["pactl", "get-sink-volume", "@DEFAULT_SINK@"]) or ""
        mute = _run(["pactl", "get-sink-mute", "@DEFAULT_SINK@"]) or ""
        pct = next((p for p in vol.split() if p.endswith("%")), "")
        if pct[:-1].isdigit():
            return float(pct[:-1]), "yes" in mute
    return None


def set_volume(mode: str, amount: float = 0) -> str:
    """mode: set (to amount %), up / down (by amount %), mute, unmute, toggle_mute. Returns "" or an error."""
    amount = max(0.0, min(150.0, float(amount)))
    if IS_WIN:
        try:
            v = _win_volume()
            if mode in ("mute", "unmute", "toggle_mute"):
                v.SetMute(not v.GetMute() if mode == "toggle_mute" else mode == "mute", None)
            else:
                cur = float(v.GetMasterVolumeLevelScalar()) * 100
                new = amount if mode == "set" else cur + amount if mode == "up" else cur - amount
                v.SetMasterVolumeLevelScalar(max(0.0, min(100.0, new)) / 100, None)
            return ""
        except Exception as e:
            return f"{type(e).__name__}: {e}"
    if shutil.which("wpctl"):
        sink = "@DEFAULT_AUDIO_SINK@"
        if mode in ("mute", "unmute", "toggle_mute"):
            arg = {"mute": "1", "unmute": "0", "toggle_mute": "toggle"}[mode]
            cmd = ["wpctl", "set-mute", sink, arg]
        else:
            arg = f"{amount / 100:.2f}" + {"set": "", "up": "+", "down": "-"}[mode]
            cmd = ["wpctl", "set-volume", "-l", "1.5", sink, arg]
    elif shutil.which("pactl"):
        sink = "@DEFAULT_SINK@"
        if mode in ("mute", "unmute", "toggle_mute"):
            cmd = ["pactl", "set-sink-mute", sink, {"mute": "1", "unmute": "0", "toggle_mute": "toggle"}[mode]]
        else:
            cmd = ["pactl", "set-sink-volume", sink, {"set": "", "up": "+", "down": "-"}[mode] + f"{amount:g}%"]
    else:
        return "neither wpctl (PipeWire) nor pactl is installed"
    return "" if _run(cmd) is not None else f"{cmd[0]} failed"


# ── power ─────────────────────────────────────────────────────────────────────
POWER_ACTIONS = [("lock", "Lock the screen"), ("screen_off", "Turn the screens off"), ("sleep", "Sleep"),
                 ("hibernate", "Hibernate"), ("logout", "Sign out"), ("restart", "Restart the PC"),
                 ("shutdown", "Shut down the PC")]


def power(action: str) -> str:
    """Returns "" or an error text."""
    if IS_WIN:
        import ctypes
        try:
            if action == "lock":
                ok = ctypes.windll.user32.LockWorkStation()
                return "" if ok else "Windows refused to lock"
            if action == "screen_off":
                # SendMessage would wait for every window; Post does not
                ctypes.windll.user32.PostMessageW(0xFFFF, 0x0112, 0xF170, 2)     # WM_SYSCOMMAND, SC_MONITORPOWER
                return ""
            if action in ("sleep", "hibernate"):
                ctypes.windll.powrprof.SetSuspendState(action == "hibernate", False, False)
                return ""
            cmd = {"logout": ["shutdown", "/l"], "restart": ["shutdown", "/r", "/t", "0"],
                   "shutdown": ["shutdown", "/s", "/t", "0"]}.get(action)
            if cmd:
                subprocess.Popen(cmd, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
                return ""
        except Exception as e:
            return f"{type(e).__name__}: {e}"
        return f"unknown action {action}"
    if action == "lock":
        if shutil.which("hyprlock") and hypr.available():
            subprocess.Popen(["hyprlock"], start_new_session=True, stdout=subprocess.DEVNULL,
                             stderr=subprocess.DEVNULL)
            return ""
        return "" if _run(["loginctl", "lock-session"]) is not None else "loginctl lock-session failed"
    if action == "screen_off":
        if hypr.available():
            # the classic dispatcher first, then the Lua config's name for it
            if _run(["hyprctl", "dispatch", "dpms", "off"]) is not None:
                return ""
            return hypr.dispatch('hl.dsp.dpms({ action = "off" })')
        if shutil.which("xset") and os.environ.get("DISPLAY"):
            return "" if _run(["xset", "dpms", "force", "off"]) is not None else "xset failed"
        return "turning the screens off needs Hyprland or X11"
    cmd = {"sleep": ["systemctl", "suspend"], "hibernate": ["systemctl", "hibernate"],
           "restart": ["systemctl", "reboot"], "shutdown": ["systemctl", "poweroff"]}.get(action)
    if action == "logout":
        if shutil.which("uwsm") and os.environ.get("UWSM_FINALIZE_VARNAMES") is not None:
            cmd = ["uwsm", "stop"]
        elif os.environ.get("XDG_SESSION_ID"):
            cmd = ["loginctl", "terminate-session", os.environ["XDG_SESSION_ID"]]
        else:
            cmd = ["hyprctl", "dispatch", "exit"]
    if not cmd:
        return f"unknown action {action}"
    return "" if _run(cmd, timeout=15) is not None else f"{' '.join(cmd)} failed"


# ── readings ──────────────────────────────────────────────────────────────────
def _read(path: str) -> str:
    try:
        with open(path, "r") as f:
            return f.read().strip()
    except OSError:
        return ""


def cpu_temp() -> float | None:
    """The CPU package temperature in °C (Linux; Windows has none without admin rights)."""
    if IS_WIN:
        return None
    for hw in glob.glob("/sys/class/hwmon/hwmon*"):
        if _read(os.path.join(hw, "name")) in ("k10temp", "zenpower", "coretemp", "cpu_thermal", "acpitz"):
            raw = _read(os.path.join(hw, "temp1_input"))
            if raw.isdigit():
                return int(raw) / 1000
    for zone in glob.glob("/sys/class/thermal/thermal_zone*"):
        if _read(os.path.join(zone, "type")) in ("x86_pkg_temp", "cpu-thermal"):
            raw = _read(os.path.join(zone, "temp"))
            if raw.isdigit():
                return int(raw) / 1000
    return None


class _Gpu:
    """GPU load and temperature: AMD / Intel from /sys straight away, NVIDIA through nvidia-smi on a worker
    thread (the answer of the last question is used, so a reading never waits)."""

    def __init__(self):
        self.last: dict | None = None
        self._asking = False

    def read(self) -> dict | None:
        if not IS_WIN:
            for dev in sorted(glob.glob("/sys/class/drm/card[0-9]/device")):
                busy = _read(os.path.join(dev, "gpu_busy_percent"))
                if not busy.isdigit():
                    continue
                out = {"load": float(busy)}
                for hw in glob.glob(os.path.join(dev, "hwmon", "hwmon*")):
                    raw = _read(os.path.join(hw, "temp1_input"))
                    if raw.isdigit():
                        out["temp"] = int(raw) / 1000
                        break
                return out
        if shutil.which("nvidia-smi") and not self._asking:
            self._asking = True
            threading.Thread(target=self._ask, daemon=True, name="nvidia-smi").start()
        return self.last

    def _ask(self):
        try:
            out = _run(["nvidia-smi", "--query-gpu=utilization.gpu,temperature.gpu",
                        "--format=csv,noheader,nounits"], timeout=8)
            if out:
                load, temp = (float(x) for x in out.splitlines()[0].split(",")[:2])
                self.last = {"load": load, "temp": temp}
        except (ValueError, IndexError):
            pass
        finally:
            self._asking = False


gpu = _Gpu()


def focused_window() -> tuple[str, str]:
    """(app / class, title) of the window you are in."""
    if IS_WIN:
        import ctypes
        from ctypes import wintypes
        u = ctypes.windll.user32
        hwnd = u.GetForegroundWindow()
        if not hwnd:
            return "", ""
        n = u.GetWindowTextLengthW(hwnd)
        buf = ctypes.create_unicode_buffer(n + 1)
        u.GetWindowTextW(hwnd, buf, n + 1)
        pid = wintypes.DWORD()
        u.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
        name = ""
        try:
            import psutil
            name = psutil.Process(pid.value).name()
        except Exception:
            pass
        return name, buf.value
    w = hypr.query("activewindow") if hypr.available() else None
    if isinstance(w, dict):
        return str(w.get("class") or ""), str(w.get("title") or "")
    return "", ""


class _Online:
    """Is the internet reachable? Checked on a worker thread at most every 10 s."""

    def __init__(self):
        self.state: bool | None = None
        self._at = 0.0
        self._asking = False

    def read(self) -> bool | None:
        if not self._asking and time.monotonic() - self._at > 10:
            self._asking = True
            threading.Thread(target=self._ask, daemon=True, name="online").start()
        return self.state

    def _ask(self):
        ok = False
        for host in (("1.1.1.1", 53), ("8.8.8.8", 53)):
            try:
                socket.create_connection(host, timeout=3).close()
                ok = True
                break
            except OSError:
                continue
        self.state = ok
        self._at = time.monotonic()
        self._asking = False


online = _Online()
