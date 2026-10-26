"""
"Run at login" via an XDG autostart entry (~/.config/autostart/luminescent.desktop).

UWSM and most desktop sessions start these by themselves.  The entry always
points at the current Launcher.py and Python (or at the built binary), so it
never goes stale when the folder or the virtualenv moves: `sync` rewrites it.
"""
import os
import sys
import logging

from core import APP_DIR, APP_NAME, FROZEN

log = logging.getLogger("autostart")

AUTOSTART_DIR = os.path.join(os.environ.get("XDG_CONFIG_HOME") or os.path.expanduser("~/.config"), "autostart")
ENTRY = os.path.join(AUTOSTART_DIR, "luminescent.desktop")
LAUNCHER = os.path.join(APP_DIR, "Launcher.py")


def command() -> str:
    from core.desktop import _quote          # desktop-entry quoting: the folder may have a space in it
    if FROZEN:
        return _quote(sys.executable)
    return f"{_quote(sys.executable)} {_quote(LAUNCHER)}"


def _entry_text() -> str:
    return "\n".join([
        "[Desktop Entry]",
        "Type=Application",
        f"Name={APP_NAME}",
        "Comment=Hotkey tools hub",
        f"Exec={command()}",
        f"Path={APP_DIR}",
        "Icon=luminescent",
        "Terminal=false",
        "X-GNOME-Autostart-enabled=true",
        "",
    ])


def current() -> str | None:
    """The command currently registered, or None."""
    try:
        with open(ENTRY, "r", encoding="utf-8") as f:
            for line in f:
                if line.startswith("Exec="):
                    return line[5:].strip()
    except FileNotFoundError:
        return None
    except OSError:
        log.exception("could not read %s", ENTRY)
    return None


def is_enabled() -> bool:
    return current() is not None


def set_enabled(on: bool) -> str:
    """Enable/disable. Returns '' or an error message."""
    try:
        if on:
            os.makedirs(AUTOSTART_DIR, exist_ok=True)
            with open(ENTRY, "w", encoding="utf-8") as f:
                f.write(_entry_text())
            log.info("autostart enabled: %s", command())
        else:
            try:
                os.remove(ENTRY)
                log.info("autostart disabled")
            except FileNotFoundError:
                pass
        return ""
    except OSError as e:
        log.exception("could not write %s", ENTRY)
        return str(e)


def sync(want_enabled: bool) -> str:
    """Make the autostart entry match the setting (and repoint it if the folder or Python moved)."""
    cur = current()
    if want_enabled and cur != command():
        return set_enabled(True)
    if not want_enabled and cur is not None:
        return set_enabled(False)
    return ""
