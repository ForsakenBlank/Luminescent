"""
"Run at login" on Windows, via the per-user Run registry key (core/autostart.py uses this on Windows).

More reliable than a .bat/shortcut in shell:startup: it always points at the
current Launcher.py and pythonw.exe (or at Luminescent.exe when running the
built exe), no console window, no working-directory surprises.
"""
import os
import sys
import logging
import winreg

from core import APP_DIR, APP_NAME, FROZEN

log = logging.getLogger("autostart")

RUN_KEY = r"Software\Microsoft\Windows\CurrentVersion\Run"
VALUE_NAME = APP_NAME
OLD_VALUE_NAMES = ["UsefulScripts"]    # entries written before the rename; removed so it never starts twice
LAUNCHER = os.path.join(APP_DIR, "Launcher.py")


def _pythonw() -> str:
    exe = sys.executable
    cand = os.path.join(os.path.dirname(exe), "pythonw.exe")
    return cand if os.path.exists(cand) else exe


def command() -> str:
    if FROZEN:
        return f'"{sys.executable}"'           # the exe itself
    return f'"{_pythonw()}" "{LAUNCHER}"'


def current() -> str | None:
    """The command currently registered, or None."""
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY, 0, winreg.KEY_READ) as k:
            value, _ = winreg.QueryValueEx(k, VALUE_NAME)
            return value
    except FileNotFoundError:
        return None
    except OSError:
        log.exception("could not read Run key")
        return None


def is_enabled() -> bool:
    return current() is not None


def set_enabled(on: bool) -> str:
    """Enable/disable. Returns '' or an error message."""
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY, 0, winreg.KEY_SET_VALUE) as k:
            if on:
                winreg.SetValueEx(k, VALUE_NAME, 0, winreg.REG_SZ, command())
                log.info("autostart enabled: %s", command())
            else:
                try:
                    winreg.DeleteValue(k, VALUE_NAME)
                    log.info("autostart disabled")
                except FileNotFoundError:
                    pass
        return ""
    except OSError as e:
        log.exception("could not write Run key")
        return str(e)


def _remove_old_names():
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY, 0, winreg.KEY_SET_VALUE) as k:
            for name in OLD_VALUE_NAMES:
                try:
                    winreg.DeleteValue(k, name)
                    log.info("removed old startup entry %r", name)
                except FileNotFoundError:
                    pass
    except OSError:
        log.exception("could not clean old Run entries")


def sync(want_enabled: bool) -> str:
    """Make the registry match the setting (and repoint it if the folder or Python moved)."""
    _remove_old_names()
    cur = current()
    if want_enabled and cur != command():
        return set_enabled(True)
    if not want_enabled and cur is not None:
        return set_enabled(False)
    return ""
