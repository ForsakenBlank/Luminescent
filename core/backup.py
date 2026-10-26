"""
Settings backup: settings.json plus the data files that hold your setup, in one zip.

The vault (its blobs live elsewhere anyway) and launcher.log are never included.
Before an import the current files are zipped to data/backups/auto-<time>.zip.
"""
import os
import json
import zipfile
from datetime import datetime

from core import HOME_DIR, DATA_DIR

FILES = ["settings.json", "data/notes.json", "data/timers.json", "data/reminders_state.json", "data/hearing.json",
         "data/dashboard.json", "data/themes.json", "data/calculator.json", "data/colours.json",
         "data/palette.json", "data/profiles.json", "data/automation.json"]
BACKUP_DIR = os.path.join(DATA_DIR, "backups")


def _members() -> list[str]:
    out = [f for f in FILES if os.path.isfile(os.path.join(HOME_DIR, f))]
    prof = os.path.join(DATA_DIR, "profiles")
    if os.path.isdir(prof):
        out += [f"data/profiles/{n}" for n in os.listdir(prof) if n.endswith(".json")]
    return out


def export(zip_path: str) -> int:
    """Write the backup. Returns the number of files in it."""
    members = _members()
    with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as z:
        for m in members:
            z.write(os.path.join(HOME_DIR, m), m)
    return len(members)


def _allowed(name: str) -> bool:
    name = name.replace("\\", "/")
    if name in FILES:
        return True
    return name.startswith("data/profiles/") and name.endswith(".json") and name.count("/") == 2 and ".." not in name


def check(zip_path: str) -> list[str]:
    """Return the files an import would restore; raises ValueError if the zip is not a usable backup."""
    with zipfile.ZipFile(zip_path) as z:
        names = [n for n in z.namelist() if _allowed(n)]
        if "settings.json" not in names:
            raise ValueError("This zip has no settings.json, so it is not a Luminescent backup.")
        for n in names:
            try:
                json.loads(z.read(n).decode("utf-8"))
            except (ValueError, UnicodeDecodeError) as e:
                raise ValueError(f"{n} in the backup is damaged: {e}")
    return names


def restore(zip_path: str) -> str:
    """Back up the current files, then write the backup's files atomically. Returns the auto-backup path."""
    names = check(zip_path)
    os.makedirs(BACKUP_DIR, exist_ok=True)
    auto = os.path.join(BACKUP_DIR, f"auto-{datetime.now():%Y%m%d-%H%M%S}.zip")
    export(auto)
    with zipfile.ZipFile(zip_path) as z:
        for n in names:
            dst = os.path.join(HOME_DIR, *n.split("/"))
            os.makedirs(os.path.dirname(dst), exist_ok=True)
            tmp = dst + ".tmp"
            with open(tmp, "wb") as f:
                f.write(z.read(n))
                f.flush()
                os.fsync(f.fileno())
            os.replace(tmp, dst)
    return auto
