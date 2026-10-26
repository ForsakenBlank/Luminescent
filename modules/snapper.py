"""
Window snapper: hotkeys that put the active window in half, a third or a corner of its screen.

Hyprland tiles windows, so a snapped window is made floating first and then moved and sized to the
spot (the screen minus your bars and dock, with a small gap). Pressing the same direction again steps
through the sizes: left half, left third, left two thirds, back to the half. "Back into the tiling
layout" undoes it. Every spot is also in the command palette.

Windows: the active window is moved with the Win32 API inside its screen's work area (the taskbar stays
clear), a maximised window is restored first, and Windows 11's invisible resize borders are allowed for so
the gaps look even. "Back into the tiling layout" puts the window back where it was before it was snapped.
"""
import os
import logging

from core import IS_WIN, hypr
from core.module import Module

log = logging.getLogger("snapper")

# spot -> (x, y, w, h) as fractions of the usable screen
SPOTS = {
    "left": (0, 0, 1 / 2, 1), "right": (1 / 2, 0, 1 / 2, 1), "top": (0, 0, 1, 1 / 2), "bottom": (0, 1 / 2, 1, 1 / 2),
    "left_third": (0, 0, 1 / 3, 1), "centre_third": (1 / 3, 0, 1 / 3, 1), "right_third": (2 / 3, 0, 1 / 3, 1),
    "left_two_thirds": (0, 0, 2 / 3, 1), "right_two_thirds": (1 / 3, 0, 2 / 3, 1),
    "top_left": (0, 0, 1 / 2, 1 / 2), "top_right": (1 / 2, 0, 1 / 2, 1 / 2),
    "bottom_left": (0, 1 / 2, 1 / 2, 1 / 2), "bottom_right": (1 / 2, 1 / 2, 1 / 2, 1 / 2),
    "maximise": (0, 0, 1, 1), "centre": (0.15, 0.1, 0.7, 0.8),
}
LABELS = {
    "left": "Left half", "right": "Right half", "top": "Top half", "bottom": "Bottom half",
    "left_third": "Left third", "centre_third": "Centre third", "right_third": "Right third",
    "left_two_thirds": "Left two thirds", "right_two_thirds": "Right two thirds",
    "top_left": "Top-left quarter", "top_right": "Top-right quarter", "bottom_left": "Bottom-left quarter",
    "bottom_right": "Bottom-right quarter", "maximise": "Fill the screen", "centre": "Centre, large",
}
# what pressing a direction again moves on to
CYCLES = {
    "left": ["left", "left_third", "left_two_thirds"],
    "right": ["right", "right_third", "right_two_thirds"],
    "up": ["top", "maximise"],
    "down": ["bottom", "centre"],
}


def usable_area(monitor: dict) -> tuple[int, int, int, int]:
    """(x, y, w, h) of a Hyprland monitor in layout pixels, minus what bars and docks reserve."""
    scale = float(monitor.get("scale") or 1.0)
    w, h = int(monitor.get("width", 0) / scale), int(monitor.get("height", 0) / scale)
    if int(monitor.get("transform", 0)) % 2:
        w, h = h, w
    left, top, right, bottom = (int(v) for v in (monitor.get("reserved") or [0, 0, 0, 0])[:4])
    return int(monitor.get("x", 0)) + left, int(monitor.get("y", 0)) + top, w - left - right, h - top - bottom


def spot_rect(area: tuple[int, int, int, int], spot: str, gap: int) -> tuple[int, int, int, int]:
    """Where `spot` is inside `area`, with `gap` pixels around it (half a gap between two snapped windows)."""
    ax, ay, aw, ah = area
    fx, fy, fw, fh = SPOTS[spot]
    x0, y0 = ax + round(aw * fx), ay + round(ah * fy)
    x1, y1 = ax + round(aw * (fx + fw)), ay + round(ah * (fy + fh))
    g, half = gap, gap // 2
    left = g if fx == 0 else half
    top = g if fy == 0 else half
    right = g if abs(fx + fw - 1) < 1e-6 else half
    bottom = g if abs(fy + fh - 1) < 1e-6 else half
    return x0 + left, y0 + top, max(50, x1 - x0 - left - right), max(50, y1 - y0 - top - bottom)


class SnapperModule(Module):
    key = "snapper"
    title = "Window Snapper"
    icon = "▣"

    def __init__(self, ctx):
        super().__init__(ctx)
        self._last: dict[str, tuple[str, int]] = {}      # window address -> (direction, step in its cycle)
        self._before: dict[int, tuple] = {}             # Windows: hwnd -> its rect before the first snap

    def start(self):
        for d in CYCLES:
            self.hotkey(f"{d}_hotkey", lambda d=d: self.step(d), f"snap {d}")
        for spot in ("top_left", "top_right", "bottom_left", "bottom_right", "centre_third"):
            self.hotkey(f"{spot}_hotkey", lambda s=spot: self.snap(s), f"snap {LABELS[spot].lower()}")
        self.hotkey("tile_hotkey", self.tile, "back into tiling")

    def apply_settings(self):
        self.hotkeys.unregister_all(self.key)
        self.start()

    def actions(self):
        out = [(f"snap_{k}", f"Snap window: {v}", lambda k=k: self.snap(k)) for k, v in LABELS.items()]
        out.append(("tile", "Snap window: put it back where it was" if IS_WIN else
                     "Snap window: back into the tiling layout", self.tile))
        return out

    # ── snapping ───────────────────────────────────────────────────────────
    def _active(self) -> tuple[dict, dict] | None:
        """(the active window, its monitor) from Hyprland, or None (no Hyprland, nothing focused, Luminescent)."""
        if IS_WIN:
            return _win_active()
        if not hypr.available():
            self.ctx.notify(self.title, "Snapping windows needs Hyprland.")
            return None
        win = hypr.query("activewindow")
        if not isinstance(win, dict) or not win.get("address"):
            return None
        if win.get("class") == hypr.APP_ID:
            return None                          # Luminescent's own panels move with their own snap buttons
        mons = hypr.query("monitors") or []
        mon = next((m for m in mons if m.get("id") == win.get("monitor")), None) or \
            next((m for m in mons if m.get("focused")), None)
        return (win, mon) if mon else None

    def step(self, direction: str):
        """A direction hotkey: the first press snaps, pressing it again steps through the sizes."""
        got = self._active()
        if got is None:
            return
        win, _mon = got
        addr = win["address"]
        last_dir, n = self._last.get(addr, ("", -1))
        n = (n + 1) % len(CYCLES[direction]) if last_dir == direction else 0
        self._last[addr] = (direction, n)
        self.snap(CYCLES[direction][n], got)

    def snap(self, spot: str, got=None):
        got = got or self._active()
        if got is None:
            return
        win, mon = got
        if IS_WIN:
            self._before.setdefault(win["hwnd"], _win_rect(win["hwnd"]))
            _win_snap(win["hwnd"], spot_rect(mon["area"], spot, round(max(0, int(self.s("gap", 8))) * mon["scale"])))
            return
        target = hypr.lstr("address:" + win["address"])
        if not win.get("floating"):
            hypr.dispatch(f"hl.dsp.window.float({{ action = \"enable\", window = {target} }})")
        x, y, w, h = spot_rect(usable_area(mon), spot, max(0, int(self.s("gap", 8))))
        hypr.dispatch(f"hl.dsp.window.resize({{ x = {w}, y = {h}, window = {target} }})")
        hypr.dispatch(f"hl.dsp.window.move({{ x = {x}, y = {y}, window = {target} }})")
        log.info("snapped %s to %s", win.get("class"), spot)

    def tile(self):
        got = self._active()
        if got is None:
            return
        win, _mon = got
        self._last.pop(win["address"], None)
        if IS_WIN:
            before = self._before.pop(win["hwnd"], None)
            if before:
                _win_move(win["hwnd"], *before)
            return
        if win.get("floating"):
            hypr.dispatch(f"hl.dsp.window.float({{ action = \"disable\", window = {hypr.lstr('address:' + win['address'])} }})")


# ── Windows ───────────────────────────────────────────────────────────────────
def _u32():
    import ctypes
    return ctypes.windll.user32


def _win_active():
    """(window, monitor) for the foreground window, or None (the desktop, Luminescent itself)."""
    import ctypes
    from ctypes import wintypes
    u32 = _u32()
    hwnd = u32.GetForegroundWindow()
    if not hwnd or not u32.IsWindowVisible(hwnd):
        return None
    pid = wintypes.DWORD()
    u32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
    if pid.value == os.getpid():
        return None
    cls = ctypes.create_unicode_buffer(64)
    u32.GetClassNameW(hwnd, cls, 64)
    if cls.value in ("Progman", "WorkerW", "Shell_TrayWnd"):
        return None

    class MONITORINFO(ctypes.Structure):
        _fields_ = [("cbSize", wintypes.DWORD), ("rcMonitor", wintypes.RECT), ("rcWork", wintypes.RECT),
                    ("dwFlags", wintypes.DWORD)]
    info = MONITORINFO()
    info.cbSize = ctypes.sizeof(MONITORINFO)
    if not u32.GetMonitorInfoW(u32.MonitorFromWindow(hwnd, 2), ctypes.byref(info)):     # MONITOR_DEFAULTTONEAREST
        return None
    r = info.rcWork
    try:
        scale = u32.GetDpiForWindow(hwnd) / 96.0
    except Exception:
        scale = 1.0
    return ({"address": str(hwnd), "hwnd": hwnd},
            {"area": (r.left, r.top, r.right - r.left, r.bottom - r.top), "scale": scale})


def _win_rect(hwnd) -> tuple[int, int, int, int]:
    import ctypes
    from ctypes import wintypes
    r = wintypes.RECT()
    _u32().GetWindowRect(hwnd, ctypes.byref(r))
    return r.left, r.top, r.right - r.left, r.bottom - r.top


def _win_borders(hwnd) -> tuple[int, int, int, int]:
    """Windows 10/11 windows have invisible resize borders: (left, top, right, bottom) of them."""
    import ctypes
    from ctypes import wintypes
    outer, frame = wintypes.RECT(), wintypes.RECT()
    _u32().GetWindowRect(hwnd, ctypes.byref(outer))
    try:
        if ctypes.windll.dwmapi.DwmGetWindowAttribute(hwnd, 9, ctypes.byref(frame), ctypes.sizeof(frame)):
            return 0, 0, 0, 0                                      # DWMWA_EXTENDED_FRAME_BOUNDS failed
    except Exception:
        return 0, 0, 0, 0
    return frame.left - outer.left, frame.top - outer.top, outer.right - frame.right, outer.bottom - frame.bottom


def _win_move(hwnd, x: int, y: int, w: int, h: int):
    u32 = _u32()
    if u32.IsZoomed(hwnd) or u32.IsIconic(hwnd):
        u32.ShowWindow(hwnd, 9)                                    # SW_RESTORE
    u32.SetWindowPos(hwnd, 0, int(x), int(y), int(w), int(h), 0x0004 | 0x0010)   # NOZORDER | NOACTIVATE


def _win_snap(hwnd, rect: tuple[int, int, int, int]):
    x, y, w, h = rect
    u32 = _u32()
    if u32.IsZoomed(hwnd):
        u32.ShowWindow(hwnd, 9)                                    # a maximised window ignores SetWindowPos
    left, top, right, bottom = _win_borders(hwnd)
    _win_move(hwnd, x - left, y - top, w + left + right, h + top + bottom)
    log.info("snapped window %s to %s", hwnd, rect)
