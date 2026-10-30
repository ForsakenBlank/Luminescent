"""Shared infrastructure for the Luminescent launcher (settings, hotkeys, theme, widgets)."""
import os
import sys
import json
import shutil
import logging

APP_NAME = "Luminescent"          # shown in the tray, menus, dialogs and the startup entry
VERSION = "1.8.0"                 # the release tag is v<VERSION>; core/updates.py compares against it
REPO = "ForsakenBlank/Luminescent"

FROZEN = bool(getattr(sys, "frozen", False))
IS_WIN = sys.platform == "win32"

# Where the code lives (the bundle's _internal folder in the exe).
APP_DIR = getattr(sys, "_MEIPASS", None) or os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Where settings.json and data/ live. Running from source that is the project folder, as before.
# A built binary keeps them in ~/.local/share/Luminescent (%LOCALAPPDATA%\Luminescent on Windows) so a rebuilt,
# moved or updated build never loses them.
# LUMINESCENT_HOME overrides both (tests, a second portable copy).
if os.environ.get("LUMINESCENT_HOME"):
    HOME_DIR = os.path.abspath(os.environ["LUMINESCENT_HOME"])
elif FROZEN and IS_WIN:
    HOME_DIR = os.path.join(os.environ.get("LOCALAPPDATA") or os.path.expanduser("~/AppData/Local"), APP_NAME)
elif FROZEN:
    HOME_DIR = os.path.join(os.environ.get("XDG_DATA_HOME") or os.path.expanduser("~/.local/share"), APP_NAME)
else:
    HOME_DIR = APP_DIR
DATA_DIR = os.path.join(HOME_DIR, "data")
os.makedirs(DATA_DIR, exist_ok=True)


def _first_frozen_run():
    """The first time the exe runs, bring over settings and data from the source folder it was built from."""
    if not FROZEN or os.path.exists(os.path.join(HOME_DIR, "settings.json")):
        return
    try:
        with open(os.path.join(APP_DIR, "build_info.json"), "r", encoding="utf-8") as f:
            src = json.load(f).get("source_dir", "")
    except (OSError, ValueError):
        return
    if not src or not os.path.isfile(os.path.join(src, "settings.json")):
        return
    try:
        shutil.copy2(os.path.join(src, "settings.json"), os.path.join(HOME_DIR, "settings.json"))
        src_data = os.path.join(src, "data")
        for root, _dirs, files in os.walk(src_data):
            rel = os.path.relpath(root, src_data)
            if rel.split(os.sep)[0] in ("backups",):
                continue
            os.makedirs(os.path.join(DATA_DIR, rel), exist_ok=True)
            for name in files:
                if name.endswith((".json", ".txt")) and not name.startswith("launcher.log"):
                    shutil.copy2(os.path.join(root, name), os.path.join(DATA_DIR, rel, name))
    except OSError:
        logging.getLogger("storage").exception("could not copy the data from %s", src)


_first_frozen_run()


def save_json(path: str, data, indent: int | None = None):
    """Write JSON atomically: a crash or power cut mid-write leaves the previous file, never an empty one."""
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=indent)
        f.flush()
        os.fsync(f.fileno())
    os.replace(tmp, path)


def load_json(path: str, default=None):
    """Read JSON. Missing file -> `default`; unreadable file -> kept as <name>.bad (so it is not overwritten) -> `default`."""
    try:
        with open(path, "r", encoding="utf-8") as f:
            return json.load(f)
    except FileNotFoundError:
        return default
    except (ValueError, OSError):
        logging.getLogger("storage").exception("could not read %s", path)
        try:
            os.replace(path, path + ".bad")
        except OSError:
            pass
        return default
