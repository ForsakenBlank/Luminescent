"""
The computer's own settings, for the System Settings module: displays (where each screen is, its resolution,
refresh rate, scale and rotation), the mouse, touchpad and keyboard, the look of the desktop, power, screen
brightness, night light, sound devices and what the computer is.

Linux:
  * Hyprland: monitors and input through its control socket (hl.monitor / hl.config in the Lua config of
    Hyprland 0.55+). These changes only last until Hyprland reloads its config, so the module keeps what you
    changed and puts it back after a reload or a restart (Settings → System Settings → Keep my changes).
  * X11: monitors through xrandr. GNOME: mouse, touchpad and keyboard through gsettings.
  * Everywhere: GTK's dark mode, cursor and text size through gsettings, power profiles through
    powerprofilesctl, brightness through brightnessctl (built-in screens) and ddcutil (monitors over DDC/CI),
    night light through hyprsunset, gammastep or wlsunset, sound devices through pactl.
Windows: SystemParametersInfo, ChangeDisplaySettingsEx, the registry, powercfg and DDC/CI (dxva2). Nothing
needs administrator rights, and Windows keeps every change by itself.

Every slow call (ddcutil, powershell, powercfg) is meant to run on a worker thread; the module does that.
"""
import os
import re
import sys
import json
import glob
import time
import shutil
import socket
import logging
import platform
import subprocess
from dataclasses import dataclass, field
from typing import Callable

from core import IS_WIN, VERSION, hypr

log = logging.getLogger("syscontrol")

_NO_WINDOW = 0x08000000 if IS_WIN else 0         # CREATE_NO_WINDOW: no console flashing up for powercfg


def _run(cmd: list[str], timeout: float = 5) -> str | None:
    """stdout of a command, None when it is missing, fails or takes too long."""
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, creationflags=_NO_WINDOW)
        return r.stdout if r.returncode == 0 else None
    except (OSError, subprocess.SubprocessError):
        return None


def _run_err(cmd: list[str], timeout: float = 10) -> str:
    """'' when the command worked, otherwise what went wrong."""
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, creationflags=_NO_WINDOW)
    except FileNotFoundError:
        return f"{cmd[0]} is not installed"
    except (OSError, subprocess.SubprocessError) as e:
        return f"{type(e).__name__}: {e}"
    if r.returncode == 0:
        return ""
    return (r.stderr or r.stdout or f"{cmd[0]} failed").strip().splitlines()[-1]


def backend() -> str:
    """What changes the displays and input here: 'hyprland', 'x11', 'gnome', 'windows' or ''."""
    if IS_WIN:
        return "windows"
    if hypr.available():
        return "hyprland"
    if os.environ.get("XDG_SESSION_TYPE") == "x11" and shutil.which("xrandr"):
        return "x11"
    if "GNOME" in os.environ.get("XDG_CURRENT_DESKTOP", "").upper() and shutil.which("gsettings"):
        return "gnome"
    return ""


# ══ displays ═══════════════════════════════════════════════════════════════════
# A monitor is a dict:
#   name        the connector ("HDMI-A-2", "\\.\DISPLAY1"): what identifies it
#   label       make and model, for people
#   x, y        where its top-left corner is in the desktop (logical pixels on Hyprland)
#   width, height, refresh   the mode, before rotation
#   scale       1.0, 1.25... (Windows: read-only, see can_scale)
#   transform   0 normal, 1 90°, 2 180°, 3 270°
#   enabled     False: switched off but still plugged in
#   primary     Windows / X11: the main screen (the taskbar, new windows)
#   vrr         None: leave Hyprland's own setting alone, 0 off, 1 on, 2 fullscreen only
#   modes       [[w, h, hz], ...] it can do
ROTATIONS = [(0, "Normal"), (1, "90° (portrait)"), (2, "180° (upside down)"), (3, "270° (portrait, flipped)")]
SCALES = [1.0, 1.25, 1.5, 1.6667, 2.0, 2.5, 3.0]


def can_scale() -> bool:
    return backend() == "hyprland"


def has_primary() -> bool:
    return backend() in ("windows", "x11")


def has_vrr() -> bool:
    return backend() == "hyprland"


def layout_size(m: dict) -> tuple[int, int]:
    """How much room the monitor takes in the desktop: rotated, and divided by the scale on Hyprland."""
    w, h = int(m.get("width") or 0), int(m.get("height") or 0)
    if int(m.get("transform") or 0) % 2:
        w, h = h, w
    if can_scale():
        s = float(m.get("scale") or 1) or 1
        w, h = round(w / s), round(h / s)
    return w, h


def _parse_mode(text: str):
    m = re.match(r"(\d+)x(\d+)@([\d.]+)", text)
    return [int(m.group(1)), int(m.group(2)), round(float(m.group(3)), 2)] if m else None


def monitors() -> list[dict]:
    """Every monitor plugged in, switched on or not. [] when they can not be read here."""
    try:
        b = backend()
        if b == "hyprland":
            return _hypr_monitors()
        if b == "windows":
            return _win_monitors()
        if b == "x11":
            return _xrandr_monitors()
    except Exception:
        log.exception("could not read the monitors")
    return []


def can_change_monitors() -> bool:
    return backend() in ("hyprland", "windows", "x11")


def apply_monitors(mons: list[dict]) -> str:
    """Put every monitor in `mons` as it says. '' or what went wrong."""
    try:
        b = backend()
        if b == "hyprland":
            return _hypr_apply(mons)
        if b == "windows":
            return _win_apply(mons)
        if b == "x11":
            return _xrandr_apply(mons)
    except Exception as e:
        log.exception("could not change the monitors")
        return f"{type(e).__name__}: {e}"
    return "Changing monitors needs Hyprland, an X11 session or Windows."


def config_text(mons: list[dict]) -> str:
    """The monitors as lines for a config file, to keep them for good."""
    b = backend()
    out = []
    for m in mons:
        mode = f"{m['width']}x{m['height']}@{float(m['refresh']):.2f}"
        if b == "hyprland":
            if not m.get("enabled", True):
                out.append(f'hl.monitor({{ output = "{m["name"]}", disabled = true }})')
                continue
            extra = f", transform = {int(m['transform'])}" if m.get("transform") else ""
            if m.get("vrr") is not None:
                extra += f", vrr = {int(m['vrr'])}"
            out.append(f'hl.monitor({{ output = "{m["name"]}", mode = "{mode}", position = "{m["x"]}x{m["y"]}", '
                       f'scale = "{float(m.get("scale") or 1):g}"{extra} }})')
        elif b == "x11":
            out.append(" ".join(_xrandr_args(m)))
        else:
            out.append(f"{m['name']}: {mode} at {m['x']}, {m['y']}" + (" (main)" if m.get("primary") else ""))
    if b == "hyprland":
        out.insert(0, "-- Hyprland Lua config (hyprland.conf: monitor = NAME, MODE, POSITION, SCALE)")
    elif b == "x11":
        out = ["xrandr " + " ".join(line for line in out)]
    return "\n".join(out)


# ── Hyprland ────────────────────────────────────────────────────────────────────
def _hypr_monitors() -> list[dict]:
    data = hypr.query("monitors all") or []
    out = []
    for d in data:
        modes = [m for m in (_parse_mode(t) for t in d.get("availableModes") or []) if m]
        w, h = int(d.get("width") or 0), int(d.get("height") or 0)
        if (not w or not h) and modes:
            w, h = modes[0][0], modes[0][1]
        out.append({
            "name": d.get("name", ""),
            "label": " ".join(x for x in (d.get("make"), d.get("model")) if x) or d.get("description", ""),
            "x": int(d.get("x") or 0), "y": int(d.get("y") or 0),
            "width": w, "height": h,
            "refresh": round(float(d.get("refreshRate") or (modes[0][2] if modes else 60)), 2),
            "scale": float(d.get("scale") or 1),
            "transform": int(d.get("transform") or 0) % 4,
            "enabled": not d.get("disabled", False),
            "primary": False,
            "vrr": None,
            "vrr_now": bool(d.get("vrr")),
            "focused": bool(d.get("focused")),
            "mirror": "" if d.get("mirrorOf") in (None, "none") else str(d.get("mirrorOf")),
            "modes": modes,
        })
    return out


def _hypr_monitor_lua(m: dict) -> str:
    name = hypr.lstr(m["name"])
    if not m.get("enabled", True):
        return f"hl.monitor({{ output = {name}, disabled = true }})"
    mode = hypr.lstr(f"{int(m['width'])}x{int(m['height'])}@{float(m['refresh']):.2f}")
    pos = hypr.lstr(f"{int(m['x'])}x{int(m['y'])}")
    scale = hypr.lstr(f"{float(m.get('scale') or 1):g}")
    extra = ""
    if m.get("vrr") is not None:
        extra += f", vrr = {int(m['vrr'])}"
    if m.get("mirror"):
        extra += f", mirror = {hypr.lstr(m['mirror'])}"
    return (f"hl.monitor({{ output = {name}, disabled = false, mode = {mode}, position = {pos}, scale = {scale}, "
            f"transform = {int(m.get('transform') or 0)}{extra} }})")


def _hypr_apply(mons: list[dict]) -> str:
    if mons and not any(m.get("enabled", True) for m in mons):
        return "At least one monitor has to stay on."
    # switched-on monitors first, so the desktop never ends up with no screen at all for a moment
    order = sorted(mons, key=lambda m: not m.get("enabled", True))
    return hypr.lua(" ".join(_hypr_monitor_lua(m) for m in order))


# ── X11 (xrandr) ─────────────────────────────────────────────────────────────────
_XR_ROT = {"normal": 0, "left": 1, "inverted": 2, "right": 3}


def _xrandr_monitors() -> list[dict]:
    out, cur = [], None
    for line in (_run(["xrandr", "--query"]) or "").splitlines():
        head = re.match(r"(\S+) connected( primary)?(?: (\d+)x(\d+)\+(-?\d+)\+(-?\d+))?(?: (left|right|inverted))?",
                        line)
        if head:
            name, prim = head.group(1), bool(head.group(2))
            on = head.group(3) is not None
            rot = _XR_ROT.get(head.group(7) or "normal", 0)
            w, h = (int(head.group(3)), int(head.group(4))) if on else (0, 0)
            if rot % 2:
                w, h = h, w
            cur = {"name": name, "label": name, "x": int(head.group(5) or 0), "y": int(head.group(6) or 0),
                   "width": w, "height": h, "refresh": 60.0, "scale": 1.0, "transform": rot, "enabled": on,
                   "primary": prim, "vrr": None, "mirror": "", "modes": []}
            out.append(cur)
            continue
        if not line.startswith(" ") or cur is None:
            cur = None if not line.startswith(" ") else cur
            continue
        parts = line.split()
        size = re.match(r"(\d+)x(\d+)", parts[0]) if parts else None
        if not size:
            continue
        w, h = int(size.group(1)), int(size.group(2))
        for rate in parts[1:]:
            try:
                hz = round(float(rate.rstrip("*+")), 2)
            except ValueError:
                continue
            cur["modes"].append([w, h, hz])
            if "*" in rate:
                cur["width"], cur["height"], cur["refresh"] = w, h, hz
    for m in out:
        if not m["width"] and m["modes"]:
            m["width"], m["height"], m["refresh"] = m["modes"][0]
    return out


def _xrandr_args(m: dict) -> list[str]:
    if not m.get("enabled", True):
        return ["--output", m["name"], "--off"]
    rot = {v: k for k, v in _XR_ROT.items()}[int(m.get("transform") or 0) % 4]
    args = ["--output", m["name"], "--mode", f"{m['width']}x{m['height']}", "--rate", f"{float(m['refresh']):.2f}",
            "--pos", f"{m['x']}x{m['y']}", "--rotate", rot]
    if m.get("primary"):
        args.append("--primary")
    return args


def _xrandr_apply(mons: list[dict]) -> str:
    args = ["xrandr"]
    for m in mons:
        args += _xrandr_args(m)
    return _run_err(args)


# ── Windows ─────────────────────────────────────────────────────────────────────
DM_POSITION = 0x20
DM_DISPLAYORIENTATION = 0x80
DM_PELSWIDTH = 0x80000
DM_PELSHEIGHT = 0x100000
DM_DISPLAYFREQUENCY = 0x400000
ENUM_CURRENT_SETTINGS = 0xFFFFFFFF
CDS_UPDATEREGISTRY = 0x1
CDS_SET_PRIMARY = 0x10
CDS_NORESET = 0x10000000
_DISP_ERRORS = {1: "Windows needs a restart for this", -1: "the display driver refused it",
                -2: "that mode is not supported", -3: "it could not be saved", -4: "bad flags",
                -5: "bad settings", -6: "this computer can not show two desktops at once"}


def _win_types():
    import ctypes
    from ctypes import wintypes

    class DEVMODEW(ctypes.Structure):
        _fields_ = [("dmDeviceName", wintypes.WCHAR * 32), ("dmSpecVersion", wintypes.WORD),
                    ("dmDriverVersion", wintypes.WORD), ("dmSize", wintypes.WORD),
                    ("dmDriverExtra", wintypes.WORD), ("dmFields", wintypes.DWORD),
                    ("dmPositionX", wintypes.LONG), ("dmPositionY", wintypes.LONG),
                    ("dmDisplayOrientation", wintypes.DWORD), ("dmDisplayFixedOutput", wintypes.DWORD),
                    ("dmColor", ctypes.c_short), ("dmDuplex", ctypes.c_short), ("dmYResolution", ctypes.c_short),
                    ("dmTTOption", ctypes.c_short), ("dmCollate", ctypes.c_short),
                    ("dmFormName", wintypes.WCHAR * 32), ("dmLogPixels", wintypes.WORD),
                    ("dmBitsPerPel", wintypes.DWORD), ("dmPelsWidth", wintypes.DWORD),
                    ("dmPelsHeight", wintypes.DWORD), ("dmDisplayFlags", wintypes.DWORD),
                    ("dmDisplayFrequency", wintypes.DWORD), ("dmICMMethod", wintypes.DWORD),
                    ("dmICMIntent", wintypes.DWORD), ("dmMediaType", wintypes.DWORD),
                    ("dmDitherType", wintypes.DWORD), ("dmReserved1", wintypes.DWORD),
                    ("dmReserved2", wintypes.DWORD), ("dmPanningWidth", wintypes.DWORD),
                    ("dmPanningHeight", wintypes.DWORD)]

    class DISPLAY_DEVICEW(ctypes.Structure):
        _fields_ = [("cb", wintypes.DWORD), ("DeviceName", wintypes.WCHAR * 32),
                    ("DeviceString", wintypes.WCHAR * 128), ("StateFlags", wintypes.DWORD),
                    ("DeviceID", wintypes.WCHAR * 128), ("DeviceKey", wintypes.WCHAR * 128)]

    user32 = ctypes.windll.user32
    user32.EnumDisplaySettingsExW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, ctypes.POINTER(DEVMODEW),
                                              wintypes.DWORD]
    user32.EnumDisplayDevicesW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, ctypes.POINTER(DISPLAY_DEVICEW),
                                           wintypes.DWORD]
    user32.ChangeDisplaySettingsExW.argtypes = [wintypes.LPCWSTR, ctypes.POINTER(DEVMODEW), wintypes.HWND,
                                                wintypes.DWORD, ctypes.c_void_p]
    user32.ChangeDisplaySettingsExW.restype = ctypes.c_long
    return ctypes, user32, DEVMODEW, DISPLAY_DEVICEW


def _win_devices():
    """[(device name, adapter text, monitor text, state flags)] for every display output."""
    ctypes, user32, _DM, DD = _win_types()
    out, i = [], 0
    while True:
        dd = DD()
        dd.cb = ctypes.sizeof(DD)
        if not user32.EnumDisplayDevicesW(None, i, ctypes.byref(dd), 0):
            break
        i += 1
        if dd.StateFlags & 0x8:                      # a mirroring driver, not a screen
            continue
        mon = DD()
        mon.cb = ctypes.sizeof(DD)
        has_monitor = bool(user32.EnumDisplayDevicesW(dd.DeviceName, 0, ctypes.byref(mon), 0))
        if not has_monitor:
            continue
        out.append((dd.DeviceName, dd.DeviceString, mon.DeviceString, dd.StateFlags))
    return out


def _win_monitors() -> list[dict]:
    ctypes, user32, DM, _DD = _win_types()
    out = []
    for n, (name, adapter, monitor, flags) in enumerate(_win_devices(), 1):
        dm = DM()
        dm.dmSize = ctypes.sizeof(DM)
        on = bool(flags & 0x1) and bool(user32.EnumDisplaySettingsExW(name, ENUM_CURRENT_SETTINGS,
                                                                      ctypes.byref(dm), 0))
        if not on:
            dm = DM()
            dm.dmSize = ctypes.sizeof(DM)
            user32.EnumDisplaySettingsExW(name, 0xFFFFFFFE, ctypes.byref(dm), 0)     # the registry's settings
        modes, seen, i = [], set(), 0
        while True:
            md = DM()
            md.dmSize = ctypes.sizeof(DM)
            if not user32.EnumDisplaySettingsExW(name, i, ctypes.byref(md), 0):
                break
            i += 1
            key = (md.dmPelsWidth, md.dmPelsHeight, md.dmDisplayFrequency)
            if md.dmBitsPerPel >= 24 and key not in seen and md.dmDisplayOrientation == 0:
                seen.add(key)
                modes.append([int(md.dmPelsWidth), int(md.dmPelsHeight), float(md.dmDisplayFrequency)])
        modes.sort(key=lambda m: (-m[0] * m[1], -m[2]))
        rot = int(dm.dmDisplayOrientation) % 4
        w, h = int(dm.dmPelsWidth), int(dm.dmPelsHeight)
        if rot % 2:
            w, h = h, w
        if (not w or not h) and modes:
            w, h = modes[0][0], modes[0][1]
        out.append({
            "name": name, "label": f"{monitor} ({adapter})" if monitor else adapter or f"Display {n}",
            "x": int(dm.dmPositionX), "y": int(dm.dmPositionY), "width": w, "height": h,
            "refresh": float(dm.dmDisplayFrequency or (modes[0][2] if modes else 60)),
            "scale": 1.0, "transform": rot, "enabled": on, "primary": bool(flags & 0x4),
            "vrr": None, "mirror": "", "modes": modes,
        })
    return out


def _win_apply(mons: list[dict]) -> str:
    ctypes, user32, DM, _DD = _win_types()
    on = [m for m in mons if m.get("enabled", True)]
    if not on:
        return "At least one monitor has to stay on."
    main = next((m for m in on if m.get("primary")), on[0])
    ox, oy = int(main["x"]), int(main["y"])          # Windows wants the main screen at 0, 0
    for m in sorted(mons, key=lambda m: m is not main):
        dm = DM()
        dm.dmSize = ctypes.sizeof(DM)
        user32.EnumDisplaySettingsExW(m["name"], ENUM_CURRENT_SETTINGS, ctypes.byref(dm), 0)
        flags = CDS_UPDATEREGISTRY | CDS_NORESET
        if not m.get("enabled", True):
            dm.dmFields = DM_POSITION | DM_PELSWIDTH | DM_PELSHEIGHT
            dm.dmPelsWidth = dm.dmPelsHeight = 0
            dm.dmPositionX = dm.dmPositionY = 0
        else:
            rot = int(m.get("transform") or 0) % 4
            w, h = int(m["width"]), int(m["height"])
            if rot % 2:
                w, h = h, w
            dm.dmFields = DM_POSITION | DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY | DM_DISPLAYORIENTATION
            dm.dmPelsWidth, dm.dmPelsHeight = w, h
            dm.dmDisplayFrequency = int(round(float(m["refresh"])))
            dm.dmDisplayOrientation = rot
            dm.dmPositionX, dm.dmPositionY = int(m["x"]) - ox, int(m["y"]) - oy
            if m is main:
                flags |= CDS_SET_PRIMARY
        r = user32.ChangeDisplaySettingsExW(m["name"], ctypes.byref(dm), None, flags, None)
        if r != 0:
            return f"{m['label']}: {_DISP_ERRORS.get(r, f'error {r}')}"
    r = user32.ChangeDisplaySettingsExW(None, None, None, 0, None)
    return "" if r == 0 else _DISP_ERRORS.get(r, f"error {r}")


# ══ settings as a list of options ══════════════════════════════════════════════
GROUPS = [("mouse", "Mouse", "🖱"), ("touchpad", "Touchpad", "▭"), ("keyboard", "Keyboard", "⌨"),
          ("desktop", "Windows & desktop", "▣"), ("appearance", "Appearance", "◐"), ("explorer", "Explorer & taskbar", "🗂"),
          ("power", "Power", "⚡")]


@dataclass
class Option:
    group: str
    key: str               # unique: "hypr:input:sensitivity", "win:mouse_speed"...
    label: str
    kind: str              # bool, int, float, choice, text
    get: Callable          # () -> value, None when it can not be read
    set: Callable          # (value) -> '' or the error
    min: float = 0
    max: float = 100
    step: float = 1
    choices: list = field(default_factory=list)       # [(value, text)]
    help: str = ""
    suffix: str = ""
    keep: bool = False     # Luminescent has to put it back itself after a restart (Hyprland)


# ── Hyprland options ──────────────────────────────────────────────────────────────
_FOLLOW = [(0, "Click to focus"), (1, "Focus follows the mouse"), (2, "Follows the mouse, keyboard stays"),
           (3, "Keyboard follows, the mouse doesn't")]
_VRR = [(0, "Off"), (1, "Always on"), (2, "Fullscreen only"), (3, "Fullscreen games only")]

# (group, option path, label, kind, min, max, step, choices, help, suffix)
HYPR_OPTIONS = [
    ("mouse", "input:sensitivity", "Pointer speed", "float", -1, 1, 0.05, None,
     "-1 slowest, 0 as the mouse reports it, 1 fastest.", ""),
    ("mouse", "input:accel_profile", "Acceleration", "choice", 0, 0, 0,
     [("", "Default (libinput's)"), ("flat", "Off: the same speed however fast you move"),
      ("adaptive", "On: faster when you move quickly")], "Gamers usually want it off.", ""),
    ("mouse", "input:natural_scroll", "Natural scrolling (content follows your finger)", "bool", 0, 0, 0, None, "", ""),
    ("mouse", "input:left_handed", "Left-handed (swap the buttons)", "bool", 0, 0, 0, None, "", ""),
    ("mouse", "input:scroll_factor", "Scroll speed", "float", 0.1, 5, 0.1, None, "", "×"),
    ("mouse", "input:follow_mouse", "Focus", "choice", 0, 0, 0, _FOLLOW, "", ""),
    ("mouse", "cursor:inactive_timeout", "Hide the cursor after (0 never)", "float", 0, 60, 1, None, "", " s"),
    ("mouse", "cursor:hide_on_key_press", "Hide the cursor while typing", "bool", 0, 0, 0, None, "", ""),
    ("mouse", "cursor:no_warps", "Never move the cursor by itself", "bool", 0, 0, 0, None,
     "Hyprland moves the cursor to a window when you switch to it with the keyboard.", ""),
    ("touchpad", "input:touchpad:natural_scroll", "Natural scrolling", "bool", 0, 0, 0, None, "", ""),
    ("touchpad", "input:touchpad:tap_to_click", "Tap to click", "bool", 0, 0, 0, None, "", ""),
    ("touchpad", "input:touchpad:disable_while_typing", "Off while typing", "bool", 0, 0, 0, None, "", ""),
    ("touchpad", "input:touchpad:clickfinger_behavior", "Two fingers = right click, three = middle", "bool",
     0, 0, 0, None, "Instead of clicking in the corners of the pad.", ""),
    ("touchpad", "input:touchpad:drag_lock", "Drag lock (lift your finger mid-drag)", "bool", 0, 0, 0, None, "", ""),
    ("touchpad", "input:touchpad:scroll_factor", "Scroll speed", "float", 0.1, 5, 0.1, None, "", "×"),
    ("keyboard", "input:repeat_rate", "Key repeat speed", "int", 1, 100, 1, None,
     "How many times a second a held key repeats.", " /s"),
    ("keyboard", "input:repeat_delay", "Delay before a key repeats", "int", 100, 2000, 25, None, "", " ms"),
    ("keyboard", "input:numlock_by_default", "Num Lock on when you log in", "bool", 0, 0, 0, None, "", ""),
    ("keyboard", "input:kb_layout", "Layout (us, gb, de, fr…; several: us,de)", "text", 0, 0, 0, None,
     "XKB layout names. A comma between them lets you switch.", ""),
    ("keyboard", "input:kb_variant", "Variant (dvorak, colemak, intl…)", "text", 0, 0, 0, None, "", ""),
    ("keyboard", "input:kb_options", "Options (caps:escape, grp:alt_shift_toggle…)", "text", 0, 0, 0, None,
     "XKB options, comma separated.", ""),
    ("desktop", "general:gaps_in", "Gap between windows", "int", 0, 60, 1, None, "", " px"),
    ("desktop", "general:gaps_out", "Gap around the edge of the screen", "int", 0, 100, 1, None, "", " px"),
    ("desktop", "general:border_size", "Border thickness", "int", 0, 20, 1, None, "", " px"),
    ("desktop", "decoration:rounding", "Rounded corners", "int", 0, 40, 1, None, "", " px"),
    ("desktop", "decoration:active_opacity", "Window you're using: opacity", "float", 0.3, 1, 0.01, None, "", ""),
    ("desktop", "decoration:inactive_opacity", "Other windows: opacity", "float", 0.3, 1, 0.01, None, "", ""),
    ("desktop", "decoration:dim_inactive", "Dim the windows you're not using", "bool", 0, 0, 0, None, "", ""),
    ("desktop", "decoration:dim_strength", "How much to dim them", "float", 0, 1, 0.05, None, "", ""),
    ("desktop", "decoration:blur:enabled", "Blur behind see-through windows", "bool", 0, 0, 0, None, "", ""),
    ("desktop", "decoration:blur:size", "Blur size", "int", 1, 20, 1, None, "", ""),
    ("desktop", "decoration:blur:passes", "Blur passes (smoother, slower)", "int", 1, 6, 1, None, "", ""),
    ("desktop", "decoration:shadow:enabled", "Shadows under windows", "bool", 0, 0, 0, None, "", ""),
    ("desktop", "animations:enabled", "Animations", "bool", 0, 0, 0, None, "", ""),
    ("desktop", "general:layout", "Tiling layout", "choice", 0, 0, 0,
     [("dwindle", "Dwindle (split in halves)"), ("master", "Master (one big window and a stack)")], "", ""),
    ("desktop", "misc:vrr", "Variable refresh rate (FreeSync / G-Sync)", "choice", 0, 0, 0, _VRR, "", ""),
    ("desktop", "misc:focus_on_activate", "Windows that ask for attention get focus", "bool", 0, 0, 0, None, "", ""),
    ("desktop", "misc:animate_manual_resizes", "Animate resizing with the mouse", "bool", 0, 0, 0, None, "", ""),
]


def _hypr_read(path: str):
    try:
        d = json.loads(hypr.request(f"j/getoption {path}") or "{}")
    except ValueError:
        return None
    for k in ("bool", "int", "float", "str"):
        if k in d:
            v = d[k]
            if k == "str" and v == "[[EMPTY]]":
                return ""
            return round(v, 3) if k == "float" else v
    if "css" in d:                              # gaps: "top right bottom left", shown as one number
        try:
            return int(str(d["css"]).split()[0])
        except (ValueError, IndexError):
            return None
    if "custom" in d:
        return d["custom"]
    return None


def _lua_value(v) -> str:
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, (int, float)):
        return repr(v)
    return hypr.lstr(str(v))


def hypr_set(path: str, value) -> str:
    """hl.config({ a = { b = { c = value } } }) for 'a:b:c'."""
    parts = path.split(":")
    body = f"{parts[-1]} = {_lua_value(value)}"
    for p in reversed(parts[:-1]):
        body = f"{p} = {{ {body} }}"
    return hypr.lua(f"hl.config({{ {body} }})")


def _hypr_options() -> list[Option]:
    out = []
    for group, path, label, kind, lo, hi, step, choices, help_, suffix in HYPR_OPTIONS:
        now = _hypr_read(path)
        if now is None:                          # not in this Hyprland
            continue
        if kind == "choice" and choices and isinstance(choices[0][0], int):
            conv = int
        elif kind == "bool" and type(now) is int:     # a few switches are 0 / 1 numbers in Hyprland
            conv = lambda v: int(bool(v))
        else:
            conv = {"int": int, "float": float, "bool": bool}.get(kind, str)
        out.append(Option(group, "hypr:" + path, label, kind,
                          get=lambda p=path: _hypr_read(p),
                          set=lambda v, p=path, c=conv: hypr_set(p, c(v)),
                          min=lo, max=hi, step=step, choices=choices or [], help=help_, suffix=suffix, keep=True))
    return out


# ── cursor ─────────────────────────────────────────────────────────────────────────
def cursor_themes() -> list[str]:
    seen = set()
    roots = [os.path.expanduser("~/.local/share/icons"), os.path.expanduser("~/.icons"), "/usr/share/icons"]
    for root in roots:
        for d in glob.glob(os.path.join(root, "*", "cursors")) + glob.glob(os.path.join(root, "*", "manifest.hl")):
            seen.add(os.path.basename(os.path.dirname(d)))
    return sorted(seen, key=str.lower)


def _cursor_now() -> tuple[str, int]:
    theme = _gs_get("org.gnome.desktop.interface", "cursor-theme") if _gs_has("org.gnome.desktop.interface") else None
    size = _gs_get("org.gnome.desktop.interface", "cursor-size") if _gs_has("org.gnome.desktop.interface") else None
    theme = os.environ.get("HYPRCURSOR_THEME") or os.environ.get("XCURSOR_THEME") or theme or "default"
    try:
        size = int(os.environ.get("HYPRCURSOR_SIZE") or os.environ.get("XCURSOR_SIZE") or size or 24)
    except ValueError:
        size = 24
    return str(theme), size


_cursor = {}                                   # what was last set, so theme and size go together


def set_cursor(theme: str | None = None, size: int | None = None) -> str:
    now_theme, now_size = _cursor.get("theme") or _cursor_now()[0], _cursor.get("size") or _cursor_now()[1]
    theme, size = theme or now_theme, int(size or now_size)
    _cursor.update(theme=theme, size=size)
    err = ""
    if hypr.available():
        out = hypr.request(f"setcursor {theme} {size}").strip()
        err = "" if out in ("ok", "") else out
    if _gs_has("org.gnome.desktop.interface"):
        _gs_set("org.gnome.desktop.interface", "cursor-theme", theme)
        _gs_set("org.gnome.desktop.interface", "cursor-size", size)
    return err


def _cursor_options() -> list[Option]:
    themes = cursor_themes()
    if not themes or not (hypr.available() or _gs_has("org.gnome.desktop.interface")):
        return []
    keep = hypr.available()
    return [
        Option("mouse", "cursor:theme", "Cursor style", "choice", get=lambda: _cursor.get("theme") or _cursor_now()[0],
               set=lambda v: set_cursor(theme=str(v)), choices=[(t, t) for t in themes], keep=keep),
        Option("mouse", "cursor:size", "Cursor size", "int", get=lambda: _cursor.get("size") or _cursor_now()[1],
               set=lambda v: set_cursor(size=int(v)), min=12, max=128, step=4, suffix=" px", keep=keep),
    ]


# ── gsettings (GNOME, and GTK apps everywhere) ───────────────────────────────────────
_schemas: set | None = None


def _gs_has(schema: str) -> bool:
    global _schemas
    if IS_WIN or not shutil.which("gsettings"):
        return False
    if _schemas is None:
        _schemas = set((_run(["gsettings", "list-schemas"]) or "").split())
    return schema in _schemas


def _gs_get(schema: str, key: str):
    out = _run(["gsettings", "get", schema, key])
    if out is None:
        return None
    t = out.strip()
    if t.startswith("'") and t.endswith("'"):
        return t[1:-1]
    if t in ("true", "false"):
        return t == "true"
    for prefix in ("uint32 ", "int32 ", "uint64 ", "int64 "):
        if t.startswith(prefix):
            t = t[len(prefix):]
    try:
        return int(t)
    except ValueError:
        try:
            return round(float(t), 3)
        except ValueError:
            return t


def _gs_set(schema: str, key: str, value) -> str:
    if isinstance(value, bool):
        v = "true" if value else "false"
    elif isinstance(value, (int, float)):
        v = repr(value)
    else:
        v = "'" + str(value).replace("'", "\\'") + "'"
    return _run_err(["gsettings", "set", schema, key, v])


def _gs_option(group, schema, key, label, kind, conv=None, **kw) -> Option | None:
    if not _gs_has(schema) or _gs_get(schema, key) is None:
        return None
    conv = conv or {"int": int, "float": float, "bool": bool}.get(kind, str)
    return Option(group, f"gs:{schema}:{key}", label, kind, get=lambda: _gs_get(schema, key),
                  set=lambda v: _gs_set(schema, key, conv(v)), **kw)


def _gnome_input_options() -> list[Option]:
    m, t, k = ("org.gnome.desktop.peripherals.mouse", "org.gnome.desktop.peripherals.touchpad",
               "org.gnome.desktop.peripherals.keyboard")
    opts = [
        _gs_option("mouse", m, "speed", "Pointer speed", "float", min=-1, max=1, step=0.05),
        _gs_option("mouse", m, "accel-profile", "Acceleration", "choice",
                   choices=[("default", "Default"), ("flat", "Off"), ("adaptive", "On")]),
        _gs_option("mouse", m, "natural-scroll", "Natural scrolling", "bool"),
        _gs_option("mouse", m, "left-handed", "Left-handed (swap the buttons)", "bool"),
        _gs_option("touchpad", t, "speed", "Pointer speed", "float", min=-1, max=1, step=0.05),
        _gs_option("touchpad", t, "tap-to-click", "Tap to click", "bool"),
        _gs_option("touchpad", t, "natural-scroll", "Natural scrolling", "bool"),
        _gs_option("touchpad", t, "disable-while-typing", "Off while typing", "bool"),
        _gs_option("keyboard", k, "repeat", "Keys repeat when held", "bool"),
        _gs_option("keyboard", k, "delay", "Delay before a key repeats", "int", min=100, max=2000, step=25,
                   suffix=" ms"),
        _gs_option("keyboard", k, "repeat-interval", "Time between repeats", "int", min=5, max=200, step=1,
                   suffix=" ms"),
    ]
    return [o for o in opts if o]


def _gtk_appearance_options() -> list[Option]:
    i = "org.gnome.desktop.interface"
    opts = [
        _gs_option("appearance", i, "color-scheme", "Apps: light or dark", "choice",
                   choices=[("prefer-dark", "Dark"), ("prefer-light", "Light"), ("default", "Let each app choose")],
                   help="GTK, libadwaita and most Qt and Electron apps follow this, and websites read it."),
        _gs_option("appearance", i, "text-scaling-factor", "Text size in apps", "float", min=0.5, max=3, step=0.05,
                   suffix="×"),
        _gs_option("appearance", i, "enable-animations", "Animations in apps", "bool"),
        _gs_option("appearance", i, "clock-format", "Clock", "choice", choices=[("24h", "24-hour"), ("12h", "12-hour")]),
        _gs_option("appearance", i, "font-antialiasing", "Font smoothing", "choice",
                   choices=[("grayscale", "Greyscale"), ("rgba", "Subpixel (LCD)"), ("none", "None")]),
        _gs_option("appearance", i, "font-hinting", "Font hinting", "choice",
                   choices=[("none", "None"), ("slight", "Slight"), ("medium", "Medium"), ("full", "Full")]),
        _gs_option("appearance", "org.gnome.desktop.wm.preferences", "button-layout", "Title bar buttons", "choice",
                   choices=[("appmenu:minimize,maximize,close", "Right: minimise, maximise, close"),
                            ("appmenu:close", "Right: close only"),
                            ("close,minimize,maximize:appmenu", "Left (like macOS)")]),
    ]
    return [o for o in opts if o]


# ── Windows options ─────────────────────────────────────────────────────────────────
SPIF = 0x1 | 0x2                                # update the user's profile and tell every program


def _user32():
    import ctypes
    from ctypes import wintypes
    u = ctypes.windll.user32
    u.SystemParametersInfoW.argtypes = [wintypes.UINT, wintypes.UINT, ctypes.c_void_p, wintypes.UINT]
    u.SystemParametersInfoW.restype = wintypes.BOOL
    return ctypes, u


def _spi_get(action: int, kind: str = "uint"):
    ctypes, u = _user32()
    v = ctypes.c_uint(0) if kind == "uint" else ctypes.c_int(0)
    if not u.SystemParametersInfoW(action, 0, ctypes.byref(v), 0):
        return None
    return bool(v.value) if kind == "bool" else int(v.value)


def _spi_set(action: int, ui: int = 0, pv=None) -> str:
    ctypes, u = _user32()
    if not u.SystemParametersInfoW(action, int(ui), pv, SPIF):
        return f"Windows refused it (error {ctypes.GetLastError()})"
    return ""


def _spi_uint(action_get: int, action_set: int, via_pv: bool = False):
    import ctypes
    return (lambda: _spi_get(action_get),
            lambda v: _spi_set(action_set, 0, ctypes.c_void_p(int(v))) if via_pv else _spi_set(action_set, int(v)))


def _spi_bool(action_get: int, action_set: int, via_pv: bool = True):
    import ctypes
    return (lambda: _spi_get(action_get, "bool"),
            lambda v: _spi_set(action_set, 0, ctypes.c_void_p(1 if v else 0)) if via_pv
            else _spi_set(action_set, 1 if v else 0))


def _precision_get():
    ctypes, u = _user32()
    arr = (ctypes.c_int * 3)()
    return bool(arr[2]) if u.SystemParametersInfoW(0x3, 0, arr, 0) else None


def _precision_set(on) -> str:
    import ctypes
    arr = (ctypes.c_int * 3)(*((6, 10, 1) if on else (0, 0, 0)))
    return _spi_set(0x4, 0, ctypes.cast(arr, ctypes.c_void_p))


def _swap_get():
    ctypes, u = _user32()
    return bool(u.GetSystemMetrics(23))


def _dblclick_get():
    ctypes, u = _user32()
    return int(u.GetDoubleClickTime())


def _caret_get():
    ctypes, u = _user32()
    return int(u.GetCaretBlinkTime())


def _caret_set(ms) -> str:
    ctypes, u = _user32()
    ms = int(ms)
    if not u.SetCaretBlinkTime(ms):
        return "Windows refused it"
    _reg_set(r"Control Panel\Desktop", "CursorBlinkRate", str(ms), "sz")
    return ""


def _reg_get(path: str, name: str, default=None, hive: str = "HKCU"):
    import winreg
    root = winreg.HKEY_CURRENT_USER if hive == "HKCU" else winreg.HKEY_LOCAL_MACHINE
    try:
        with winreg.OpenKey(root, path) as k:
            return winreg.QueryValueEx(k, name)[0]
    except OSError:
        return default


def _reg_set(path: str, name: str, value, kind: str = "dword") -> str:
    import winreg
    try:
        with winreg.CreateKeyEx(winreg.HKEY_CURRENT_USER, path, 0, winreg.KEY_SET_VALUE) as k:
            winreg.SetValueEx(k, name, 0, winreg.REG_DWORD if kind == "dword" else winreg.REG_SZ,
                              int(value) if kind == "dword" else str(value))
        return ""
    except OSError as e:
        return f"could not write the setting: {e}"


def _broadcast(area: str):
    """Tell every window a setting changed (Explorer, the taskbar and apps pick it up)."""
    import ctypes
    from ctypes import wintypes
    res = wintypes.DWORD()
    ctypes.windll.user32.SendMessageTimeoutW(0xFFFF, 0x1A, 0, area, 0x2, 1000, ctypes.byref(res))


def _explorer_refresh():
    import ctypes
    ctypes.windll.shell32.SHChangeNotify(0x08000000, 0, None, None)
    _broadcast("TraySettings")
    _broadcast("ShellState")


_PERSONALIZE = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"
_ADVANCED = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced"


def _reg_option(group, key, label, path, name, on=1, off=0, default=None, after=None, **kw) -> Option:
    def get():
        v = _reg_get(path, name, default)
        return None if v is None else v == on

    def set_(v):
        err = _reg_set(path, name, on if v else off)
        if not err and after:
            after()
        return err
    return Option(group, key, label, "bool", get=get, set=set_, **kw)


def _reg_choice(group, key, label, path, name, choices, default=None, after=None, **kw) -> Option:
    def set_(v):
        err = _reg_set(path, name, int(v))
        if not err and after:
            after()
        return err
    return Option(group, key, label, "choice", get=lambda: _reg_get(path, name, default), set=set_,
                  choices=choices, **kw)


def _win_options() -> list[Option]:
    import ctypes
    sp_get, sp_set = _spi_uint(0x70, 0x71, via_pv=True)
    rep_get, rep_set = _spi_uint(0x0A, 0x0B)
    del_get, del_set = _spi_uint(0x16, 0x17)
    wheel_get, wheel_set = _spi_uint(0x68, 0x69)
    trail_get, trail_set = _spi_uint(0x5E, 0x5D)
    sonar_get, sonar_set = _spi_bool(0x101C, 0x101D)
    snap_get, snap_set = _spi_bool(0x5F, 0x60, via_pv=False)
    anim_get, anim_set = _spi_bool(0x1042, 0x1043)
    smooth_get, smooth_set = _spi_bool(0x4A, 0x4B, via_pv=False)
    shadow_get, shadow_set = _spi_bool(0x1024, 0x1025)
    light = lambda: _broadcast("ImmersiveColorSet")
    return [
        Option("mouse", "win:mouse_speed", "Pointer speed", "int", get=sp_get, set=sp_set, min=1, max=20,
               help="10 is Windows' default."),
        Option("mouse", "win:precision", "Enhance pointer precision (acceleration)", "bool", get=_precision_get,
               set=_precision_set, help="Gamers usually want it off."),
        Option("mouse", "win:swap", "Left-handed (swap the buttons)", "bool", get=_swap_get,
               set=lambda v: _spi_set(0x21, 1 if v else 0)),
        Option("mouse", "win:wheel", "Lines to scroll per wheel notch", "int", get=wheel_get, set=wheel_set,
               min=1, max=30),
        Option("mouse", "win:dblclick", "Double-click speed", "int", get=_dblclick_get,
               set=lambda v: _spi_set(0x20, int(v)), min=200, max=900, step=10, suffix=" ms",
               help="How quickly the two clicks have to come. Lower is faster."),
        Option("mouse", "win:trails", "Pointer trails (0 off)", "int", get=trail_get,
               set=lambda v: trail_set(0 if int(v) < 2 else v), min=0, max=7),
        Option("mouse", "win:sonar", "Show where the pointer is when you press Ctrl", "bool", get=sonar_get,
               set=sonar_set),
        Option("mouse", "win:snap", "Jump to the default button in dialogs", "bool", get=snap_get, set=snap_set),
        Option("keyboard", "win:repeat", "Key repeat speed", "int", get=rep_get, set=rep_set, min=0, max=31,
               help="0 slowest (about 2.5 a second), 31 fastest (about 30)."),
        Option("keyboard", "win:delay", "Delay before a key repeats", "choice", get=del_get, set=del_set,
               choices=[(0, "Short (250 ms)"), (1, "Medium (500 ms)"), (2, "Long (750 ms)"), (3, "Longest (1 s)")]),
        Option("keyboard", "win:caret", "Text cursor blink time", "int", get=_caret_get, set=_caret_set,
               min=100, max=1200, step=50, suffix=" ms"),
        _reg_option("appearance", "win:apps_light", "Apps: dark mode", _PERSONALIZE, "AppsUseLightTheme",
                    on=0, off=1, default=1, after=light),
        _reg_option("appearance", "win:system_light", "Windows: dark mode (taskbar, Start)", _PERSONALIZE,
                    "SystemUsesLightTheme", on=0, off=1, default=1, after=light),
        _reg_option("appearance", "win:transparency", "Transparency effects", _PERSONALIZE, "EnableTransparency",
                    default=1, after=light),
        _reg_option("appearance", "win:accent_bars", "Accent colour on the taskbar and Start", _PERSONALIZE,
                    "ColorPrevalence", default=0, after=light),
        Option("appearance", "win:animations", "Animations", "bool", get=anim_get, set=anim_set),
        Option("appearance", "win:smoothing", "Smooth the edges of fonts", "bool", get=smooth_get, set=smooth_set),
        Option("appearance", "win:shadow", "Shadow under the pointer", "bool", get=shadow_get, set=shadow_set),
        _reg_option("explorer", "win:extensions", "Show file extensions (.txt, .exe)", _ADVANCED, "HideFileExt",
                    on=0, off=1, default=1, after=_explorer_refresh),
        _reg_option("explorer", "win:hidden", "Show hidden files", _ADVANCED, "Hidden", on=1, off=2, default=2,
                    after=_explorer_refresh),
        _reg_option("explorer", "win:super_hidden", "Show protected system files", _ADVANCED, "ShowSuperHidden",
                    default=0, after=_explorer_refresh),
        _reg_choice("explorer", "win:launch_to", "File Explorer opens on", _ADVANCED, "LaunchTo",
                    [(1, "This PC"), (2, "Home / Quick access"), (3, "Downloads")], default=2),
        _reg_choice("explorer", "win:taskbar_align", "Taskbar icons", _ADVANCED, "TaskbarAl",
                    [(0, "On the left"), (1, "In the centre")], default=1, after=_explorer_refresh),
        _reg_choice("explorer", "win:search", "Search on the taskbar",
                    r"Software\Microsoft\Windows\CurrentVersion\Search", "SearchboxTaskbarMode",
                    [(0, "Hidden"), (1, "Icon only"), (2, "Search box"), (3, "Icon and label")], default=2,
                    after=_explorer_refresh),
        _reg_option("explorer", "win:taskview", "Task view button", _ADVANCED, "ShowTaskViewButton", default=1,
                    after=_explorer_refresh),
        _reg_option("explorer", "win:seconds", "Seconds on the taskbar clock", _ADVANCED, "ShowSecondsInSystemClock",
                    default=0, after=_explorer_refresh),
        _reg_option("explorer", "win:end_task", "End task in the taskbar's right-click menu",
                    _ADVANCED + r"\TaskbarDeveloperSettings", "TaskbarEndTask", default=0, after=_explorer_refresh),
        _reg_option("explorer", "win:compact", "Compact view in File Explorer", _ADVANCED, "UseCompactMode",
                    default=0, after=_explorer_refresh),
    ]


# ── power ─────────────────────────────────────────────────────────────────────────────
_TIMEOUTS = [(0, "Never"), (1, "1 minute"), (2, "2 minutes"), (3, "3 minutes"), (5, "5 minutes"),
             (10, "10 minutes"), (15, "15 minutes"), (20, "20 minutes"), (30, "30 minutes"), (45, "45 minutes"),
             (60, "1 hour"), (120, "2 hours"), (180, "3 hours"), (300, "5 hours")]


def power_profiles() -> list[tuple[str, str]]:
    """[(id, name)] of the power modes / plans this computer has."""
    if IS_WIN:
        out = []
        for line in (_run(["powercfg", "/list"]) or "").splitlines():
            m = re.search(r"([0-9a-fA-F-]{36})\s+\((.+?)\)", line)
            if m:
                out.append((m.group(1).lower(), m.group(2)))
        return out
    if not shutil.which("powerprofilesctl"):
        return []
    names = {"performance": "Performance", "balanced": "Balanced", "power-saver": "Power saver"}
    found = re.findall(r"^[ *]+([a-z-]+):\s*$", _run(["powerprofilesctl", "list"]) or "", re.M)
    return [(p, names.get(p, p.capitalize())) for p in found]


def power_profile() -> str | None:
    if IS_WIN:
        m = re.search(r"([0-9a-fA-F-]{36})", _run(["powercfg", "/getactivescheme"]) or "")
        return m.group(1).lower() if m else None
    out = _run(["powerprofilesctl", "get"]) if shutil.which("powerprofilesctl") else None
    return out.strip() if out else None


def set_power_profile(pid: str) -> str:
    if IS_WIN:
        return _run_err(["powercfg", "/setactive", pid])
    return _run_err(["powerprofilesctl", "set", pid])


def _powercfg_minutes(sub: str, setting: str):
    out = _run(["powercfg", "/query", "SCHEME_CURRENT", sub, setting]) or ""
    vals = re.findall(r":\s*0x([0-9a-fA-F]+)\s*$", out, re.M)
    return int(vals[-2], 16) // 60 if len(vals) >= 2 else None       # the AC value (DC comes after it)


def _powercfg_set(name: str, minutes) -> str:
    err = _run_err(["powercfg", "/change", f"{name}-ac", str(int(minutes))])
    _run_err(["powercfg", "/change", f"{name}-dc", str(int(minutes))])      # no battery: harmless error
    return err


def _power_options() -> list[Option]:
    out = []
    profiles = power_profiles()
    if profiles:
        out.append(Option("power", "power:profile", "Power mode" if not IS_WIN else "Power plan", "choice",
                          get=power_profile, set=lambda v: set_power_profile(str(v)), choices=profiles,
                          help="Performance runs the processor faster and warmer, power saver the other way."))
    if IS_WIN:
        out += [
            Option("power", "power:screen", "Turn the screen off after", "choice",
                   get=lambda: _powercfg_minutes("SUB_VIDEO", "VIDEOIDLE"),
                   set=lambda v: _powercfg_set("monitor-timeout", v), choices=_TIMEOUTS),
            Option("power", "power:sleep", "Go to sleep after", "choice",
                   get=lambda: _powercfg_minutes("SUB_SLEEP", "STANDBYIDLE"),
                   set=lambda v: _powercfg_set("standby-timeout", v), choices=_TIMEOUTS),
        ]
    return out


def options() -> list[Option]:
    """Every setting this computer lets Luminescent change. Reads the system, so call it off the UI thread
    (powercfg and gsettings take a moment)."""
    out = []
    try:
        if IS_WIN:
            out += _win_options()
        else:
            if hypr.available():
                out += _hypr_options()
            elif backend() == "gnome":
                out += _gnome_input_options()
            out += _cursor_options()
            out += _gtk_appearance_options()
        out += _power_options()
    except Exception:
        log.exception("could not list the system settings")
    return out


# ══ brightness ═════════════════════════════════════════════════════════════════════
# A screen is {"id", "label", "monitor" (connector / \\.\DISPLAYn or ""), "value" (0..100 or None)}

def brightness_list() -> list[dict]:
    """Every screen whose brightness can be changed. Slow (DDC/CI asks the monitor itself): use a thread."""
    out = []
    try:
        out = _win_brightness_list() if IS_WIN else _linux_brightness_list()
    except Exception:
        log.exception("could not read the brightness")
    return out


def set_brightness(sid: str, value: int) -> str:
    value = max(0, min(100, int(value)))
    try:
        if sid.startswith("backlight:"):
            return _run_err(["brightnessctl", "-d", sid.split(":", 1)[1], "set", f"{value}%"])
        if sid.startswith("ddcutil:"):
            return _run_err(["ddcutil", "--noverify", "--display", sid.split(":", 1)[1], "setvcp", "10", str(value)],
                            timeout=15)
        if sid == "wmi":
            return _run_err(["powershell", "-NoProfile", "-Command",
                             "(Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightnessMethods)"
                             f" | Invoke-CimMethod -MethodName WmiSetBrightness -Arguments @{{Timeout=1; Brightness={value}}}"],
                            timeout=15)
        if sid.startswith("ddc:"):
            return _win_ddc_set(sid, value)
    except Exception as e:
        log.exception("could not set the brightness")
        return f"{type(e).__name__}: {e}"
    return "unknown screen"


def _linux_brightness_list() -> list[dict]:
    out = []
    if shutil.which("brightnessctl"):
        for line in (_run(["brightnessctl", "-m", "-c", "backlight", "-l"]) or "").splitlines():
            parts = line.split(",")
            if len(parts) >= 4 and parts[1] == "backlight":
                out.append({"id": "backlight:" + parts[0], "label": "Built-in screen", "monitor": "eDP",
                            "value": int(parts[3].rstrip("%") or 0)})
    if shutil.which("ddcutil"):
        detected = _run(["ddcutil", "detect", "--brief"], timeout=20) or ""
        for block in re.split(r"\n(?=Display \d+)", detected):
            num = re.match(r"Display (\d+)", block.strip())
            if not num:
                continue
            conn = re.search(r"DRM connector:\s*card\d+-(\S+)", block)
            model = re.search(r"Monitor:\s*\S*?:(\S+?):", block)
            n = num.group(1)
            vcp = _run(["ddcutil", "--display", n, "getvcp", "10", "--brief"], timeout=15) or ""
            m = re.search(r"VCP 10 C (\d+) (\d+)", vcp)
            value = round(100 * int(m.group(1)) / max(1, int(m.group(2)))) if m else None
            out.append({"id": "ddcutil:" + n, "label": (model.group(1) if model else f"Monitor {n}"),
                        "monitor": conn.group(1) if conn else "", "value": value})
    return out


def _win_physical(fn):
    """Call fn(device name, index, handle, description) for every DDC/CI-capable monitor; collect what it returns."""
    import ctypes
    from ctypes import wintypes

    class MONITORINFOEXW(ctypes.Structure):
        _fields_ = [("cbSize", wintypes.DWORD), ("rcMonitor", wintypes.RECT), ("rcWork", wintypes.RECT),
                    ("dwFlags", wintypes.DWORD), ("szDevice", wintypes.WCHAR * 32)]

    class PHYSICAL_MONITOR(ctypes.Structure):
        _fields_ = [("hPhysicalMonitor", wintypes.HANDLE), ("szPhysicalMonitorDescription", wintypes.WCHAR * 128)]

    user32, dxva2 = ctypes.windll.user32, ctypes.windll.dxva2
    handles, out = [], []
    proc = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HMONITOR, wintypes.HDC, ctypes.POINTER(wintypes.RECT),
                              wintypes.LPARAM)

    def each(hmon, _hdc, _rect, _lp):
        handles.append(hmon)
        return True
    cb = proc(each)
    user32.EnumDisplayMonitors(None, None, cb, 0)
    for hmon in handles:
        info = MONITORINFOEXW()
        info.cbSize = ctypes.sizeof(MONITORINFOEXW)
        user32.GetMonitorInfoW(hmon, ctypes.byref(info))
        n = wintypes.DWORD()
        if not dxva2.GetNumberOfPhysicalMonitorsFromHMONITOR(hmon, ctypes.byref(n)) or not n.value:
            continue
        arr = (PHYSICAL_MONITOR * n.value)()
        if not dxva2.GetPhysicalMonitorsFromHMONITOR(hmon, n.value, arr):
            continue
        try:
            for i in range(n.value):
                r = fn(info.szDevice, i, arr[i].hPhysicalMonitor, arr[i].szPhysicalMonitorDescription)
                if r is not None:
                    out.append(r)
        finally:
            dxva2.DestroyPhysicalMonitors(n.value, arr)
    return out


def _win_brightness_list() -> list[dict]:
    import ctypes
    from ctypes import wintypes

    def read(dev, i, h, desc):
        lo, cur, hi = wintypes.DWORD(), wintypes.DWORD(), wintypes.DWORD()
        if not ctypes.windll.dxva2.GetMonitorBrightness(h, ctypes.byref(lo), ctypes.byref(cur), ctypes.byref(hi)):
            return None
        span = max(1, hi.value - lo.value)
        return {"id": f"ddc:{dev}:{i}", "label": desc or dev, "monitor": dev,
                "value": round(100 * (cur.value - lo.value) / span)}
    out = _win_physical(read)
    laptop = _run(["powershell", "-NoProfile", "-Command",
                   "(Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightness).CurrentBrightness"],
                  timeout=15)
    if laptop and laptop.strip().splitlines()[0].strip().isdigit():
        out.insert(0, {"id": "wmi", "label": "Built-in screen", "monitor": "",
                       "value": int(laptop.strip().splitlines()[0])})
    return out


def _win_ddc_set(sid: str, value: int) -> str:
    import ctypes
    from ctypes import wintypes
    dev, _, idx = sid[4:].rpartition(":")

    def write(d, i, h, _desc):
        if d != dev or str(i) != idx:
            return None
        lo, cur, hi = wintypes.DWORD(), wintypes.DWORD(), wintypes.DWORD()
        ctypes.windll.dxva2.GetMonitorBrightness(h, ctypes.byref(lo), ctypes.byref(cur), ctypes.byref(hi))
        raw = lo.value + round((max(1, hi.value - lo.value)) * value / 100)
        return "" if ctypes.windll.dxva2.SetMonitorBrightness(h, raw) else "the monitor refused it"
    res = _win_physical(write)
    return res[0] if res else "that monitor is gone"


# ══ night light ════════════════════════════════════════════════════════════════════
class NightLight:
    """Warmer colours in the evening: hyprsunset, gammastep or wlsunset on Linux, the gamma ramp on Windows.
    `set(kelvin)`: 6500 is normal, 2500 very warm. 0 or 6500 switches it off."""

    def __init__(self):
        self.kelvin = 6500
        self._proc: subprocess.Popen | None = None

    @staticmethod
    def tool() -> str:
        if IS_WIN:
            return "windows"
        for t in ("hyprsunset", "gammastep", "wlsunset"):
            if shutil.which(t):
                return t
        return ""

    def set(self, kelvin: int) -> str:
        kelvin = max(1500, min(6500, int(kelvin or 6500)))
        self.kelvin = kelvin
        if IS_WIN:
            return _win_gamma(kelvin)
        self.stop_proc()
        if kelvin >= 6500:
            return ""
        tool = self.tool()
        cmd = {"hyprsunset": ["hyprsunset", "-t", str(kelvin)],
               "gammastep": ["gammastep", "-P", "-O", str(kelvin)],
               "wlsunset": ["wlsunset", "-t", str(kelvin), "-T", str(kelvin + 1)]}.get(tool)
        if not cmd:
            return "Needs hyprsunset, gammastep or wlsunset (sudo pacman -S hyprsunset)."
        try:
            self._proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                          start_new_session=True)
        except OSError as e:
            return str(e)
        return ""

    def stop_proc(self):
        if self._proc is not None:
            try:
                self._proc.terminate()
                self._proc.wait(2)
            except Exception:
                try:
                    self._proc.kill()
                except Exception:
                    pass
            self._proc = None

    def off(self):
        self.set(6500)


def _kelvin_rgb(k: int) -> tuple[float, float, float]:
    """Roughly the colour of light at k kelvin, as 0..1 multipliers (Tanner Helland's fit)."""
    import math
    t = k / 100
    r = 1.0 if t <= 66 else max(0.0, min(1.0, 329.698727446 * ((t - 60) ** -0.1332047592) / 255))
    g = (99.4708025861 * math.log(t) - 161.1195681661) / 255 if t <= 66 else \
        288.1221695283 * ((t - 60) ** -0.0755148492) / 255
    b = 1.0 if t >= 66 else (0.0 if t <= 19 else (138.5177312231 * math.log(t - 10) - 305.0447927307) / 255)
    return r, max(0.0, min(1.0, g)), max(0.0, min(1.0, b))


def _win_gamma(kelvin: int) -> str:
    import ctypes
    r, g, b = _kelvin_rgb(kelvin)
    ramp = (ctypes.c_ushort * 768)()
    for i in range(256):
        for c, mul in enumerate((r, g, b)):
            ramp[c * 256 + i] = min(65535, int(i * 257 * mul))
    hdc = ctypes.windll.user32.GetDC(None)
    try:
        ok = ctypes.windll.gdi32.SetDeviceGammaRamp(hdc, ctypes.byref(ramp))
    finally:
        ctypes.windll.user32.ReleaseDC(None, hdc)
    return "" if ok else "the display driver does not allow colour changes"


# ══ sound devices ══════════════════════════════════════════════════════════════════
def sound_devices(kind: str = "sinks") -> tuple[list[tuple[str, str]], str]:
    """([(id, name)], default id) of outputs ('sinks') or inputs ('sources')."""
    if IS_WIN:
        return _win_sound_devices(kind)
    if not shutil.which("pactl"):
        return [], ""
    devs = []
    try:
        for d in json.loads(_run(["pactl", "-f", "json", "list", kind]) or "[]"):
            name = d.get("name", "")
            if kind == "sources" and (name.endswith(".monitor") or d.get("monitor_of_sink") not in (None, "n/a")):
                continue
            devs.append((name, d.get("description") or name))
    except ValueError:
        for line in (_run(["pactl", "list", "short", kind]) or "").splitlines():
            parts = line.split("\t")
            if len(parts) > 1 and not parts[1].endswith(".monitor"):
                devs.append((parts[1], parts[1]))
    default = (_run(["pactl", "get-default-sink" if kind == "sinks" else "get-default-source"]) or "").strip()
    return devs, default


def set_sound_device(kind: str, dev: str) -> str:
    if IS_WIN:
        return _win_set_default_device(dev)
    return _run_err(["pactl", "set-default-sink" if kind == "sinks" else "set-default-source", dev])


def _win_sound_devices(kind: str):
    try:
        from pycaw.pycaw import AudioUtilities
        flow = 0 if kind == "sinks" else 1                    # eRender, eCapture
        out = []
        for d in AudioUtilities.GetAllDevices():
            if getattr(d, "state", None) is not None and "Active" not in str(d.state):
                continue
            # endpoint ids start {0.0.0.…} for outputs and {0.0.1.…} for inputs
            if not str(d.id or "").startswith("{0.0.%d." % flow):
                continue
            out.append((d.id, d.FriendlyName or d.id))
        default = ""
        try:
            dev = AudioUtilities.GetSpeakers() if kind == "sinks" else AudioUtilities.GetMicrophone()
            default = dev.GetId() if hasattr(dev, "GetId") else getattr(dev, "id", "")
        except Exception:
            pass
        return out, default
    except Exception:
        log.debug("sound devices", exc_info=True)
        return [], ""


def _win_set_default_device(dev: str) -> str:
    """The undocumented IPolicyConfig every sound switcher uses (Windows 7 to 11)."""
    try:
        import ctypes
        import comtypes
        from comtypes import GUID, COMMETHOD, HRESULT, IUnknown
        from ctypes import wintypes

        class IPolicyConfig(IUnknown):
            _iid_ = GUID("{f8679f50-850a-41cf-9c72-430f290290c8}")
            _methods_ = [COMMETHOD([], HRESULT, f"Unused{i}") for i in range(10)] + [
                COMMETHOD([], HRESULT, "SetDefaultEndpoint",
                          (["in"], wintypes.LPCWSTR, "id"), (["in"], ctypes.c_int, "role")),
            ]
        comtypes.CoInitialize()
        pc = comtypes.CoCreateInstance(GUID("{870af99c-171d-4f9e-af0d-e63df40c2bc9}"), IPolicyConfig,
                                       comtypes.CLSCTX_ALL)
        for role in (0, 1, 2):
            pc.SetDefaultEndpoint(dev, role)
        return ""
    except Exception as e:
        log.debug("default device", exc_info=True)
        return f"Windows refused it ({type(e).__name__})"


# ══ about this computer ════════════════════════════════════════════════════════════
def _read(path: str) -> str:
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            return f.read()
    except OSError:
        return ""


def _uptime_seconds() -> float:
    if IS_WIN:
        import ctypes
        ctypes.windll.kernel32.GetTickCount64.restype = ctypes.c_ulonglong
        return ctypes.windll.kernel32.GetTickCount64() / 1000
    try:
        return float(_read("/proc/uptime").split()[0])
    except (IndexError, ValueError):
        return 0.0


def _duration(s: float) -> str:
    d, s = divmod(int(s), 86400)
    h, s = divmod(s, 3600)
    m = s // 60
    return (f"{d} d " if d else "") + f"{h} h {m} min"


def about() -> list[tuple[str, str]]:
    """[(what, value)] describing this computer."""
    from modules.sysmon import memory
    info = []
    if IS_WIN:
        rel, ver, _sp, _t = platform.win32_ver()
        build = int(ver.split(".")[-1]) if ver.split(".")[-1].isdigit() else 0
        name = _reg_get(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "ProductName", "", "HKLM") or "Windows"
        if build >= 22000:
            name = name.replace("Windows 10", "Windows 11")
        disp = _reg_get(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "DisplayVersion", "", "HKLM")
        info.append(("System", f"{name} {disp}".strip() + f" (build {ver})"))
        cpu = _reg_get(r"HARDWARE\DESCRIPTION\System\CentralProcessor\0", "ProcessorNameString", "", "HKLM")
        gpus = sorted({d[1] for d in _win_devices()})
    else:
        osr = dict(re.findall(r'^(\w+)="?(.*?)"?$', _read("/etc/os-release"), re.M))
        info.append(("System", osr.get("PRETTY_NAME") or platform.system()))
        info.append(("Kernel", platform.release()))
        de = os.environ.get("XDG_CURRENT_DESKTOP") or os.environ.get("DESKTOP_SESSION") or "?"
        if hypr.available():
            ver = (_run(["hyprctl", "version", "-j"]) or "{}")
            try:
                de = f"Hyprland {json.loads(ver).get('tag', '').lstrip('v')}".strip()
            except ValueError:
                pass
        info.append(("Desktop", f"{de} ({os.environ.get('XDG_SESSION_TYPE', '?')})"))
        cpu = next((l.split(":", 1)[1].strip() for l in _read("/proc/cpuinfo").splitlines()
                    if l.startswith("model name")), platform.processor())
        gpus = []
        if shutil.which("lspci"):
            for line in (_run(["lspci"]) or "").splitlines():
                if re.search(r"VGA|3D controller|Display controller", line):
                    gpus.append(re.sub(r"^.*?: ", "", line).replace(" Corporation", ""))
    cores = os.cpu_count() or 0
    info.append(("Processor", f"{cpu} ({cores} threads)" if cpu else f"{cores} threads"))
    for g in gpus:
        info.append(("Graphics", g))
    used, total = memory()
    info.append(("Memory", f"{total:.1f} GiB ({used:.1f} GiB in use)"))
    try:
        du = shutil.disk_usage(os.path.expanduser("~"))
        info.append(("Home drive", f"{du.free / 1073741824:.0f} GiB free of {du.total / 1073741824:.0f} GiB"))
    except OSError:
        pass
    info.append(("Computer name", socket.gethostname()))
    info.append(("User", os.environ.get("USER") or os.environ.get("USERNAME") or "?"))
    info.append(("Up for", _duration(_uptime_seconds())))
    from PyQt6.QtCore import QT_VERSION_STR
    info.append(("Luminescent", f"{VERSION} (Python {platform.python_version()}, Qt {QT_VERSION_STR})"))
    return info


def settings_uri(page: str) -> str:
    """A Windows Settings page to open for what Luminescent can not change itself."""
    return {"display": "ms-settings:display", "sound": "ms-settings:sound", "mouse": "ms-settings:mousetouchpad",
            "power": "ms-settings:powersleep", "night": "ms-settings:nightlight",
            "colors": "ms-settings:colors"}.get(page, "ms-settings:")
