"""
"Is there a newer Luminescent?" Asks GitHub for the latest release once a day (a few seconds after
start, on a worker thread) and compares its tag with VERSION. Nothing else is sent.

Updating never touches your notes and settings: a downloaded build keeps them in its own data folder
(~/.local/share/Luminescent, or %LOCALAPPDATA%\\Luminescent on Windows), and a git checkout keeps
settings.json and data/ out of git, so `git pull` leaves them alone.
"""
import json
import time
import logging
import threading
import urllib.request

from PyQt6.QtCore import QObject, pyqtSignal

from core import VERSION, REPO

log = logging.getLogger("updates")
API = f"https://api.github.com/repos/{REPO}/releases/latest"
PAGE = f"https://github.com/{REPO}/releases/latest"


def _parts(tag: str) -> tuple:
    out = []
    for p in tag.strip().lstrip("vV").split("."):
        digits = "".join(c for c in p if c.isdigit())
        out.append(int(digits) if digits else 0)
    return tuple(out + [0] * (3 - len(out)))


def newer(tag: str, than: str = VERSION) -> bool:
    return _parts(tag) > _parts(than)


class UpdateChecker(QObject):
    found = pyqtSignal(str, str)          # tag, release page

    def __init__(self):
        super().__init__()
        self.latest = ""                  # the newer tag, once one was found
        self._last = 0.0

    def check(self, force: bool = False):
        if not force and time.time() - self._last < 86400:
            return
        self._last = time.time()
        threading.Thread(target=self._ask, args=(force,), daemon=True, name="updates").start()

    def _ask(self, force: bool):
        try:
            req = urllib.request.Request(API, headers={"User-Agent": "Luminescent",
                                                       "Accept": "application/vnd.github+json"})
            with urllib.request.urlopen(req, timeout=15) as r:
                d = json.loads(r.read().decode("utf-8"))
            tag = str(d.get("tag_name") or "")
        except Exception as e:
            log.info("update check failed: %s", e)
            if force:
                self.found.emit("", "")
            return
        if tag and newer(tag):
            self.latest = tag
            self.found.emit(tag, str(d.get("html_url") or PAGE))
        elif force:
            self.found.emit("", "")
