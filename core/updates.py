"""
"Is there a newer Luminescent?" Asks GitHub for the latest release once a day (a few seconds after
start, on a worker thread) and compares its tag with VERSION. Nothing else is sent.

Updater.install() does the update in one click: a downloaded build fetches the release archive for
this system, unpacks it next to the program folder and swaps the two once Luminescent has quit (a
small helper script does the swap on Windows, where a running program's files can not be moved),
then starts the new version. A git checkout runs `git pull` and installs the requirements instead.

Updating never touches your notes and settings: a downloaded build keeps them in its own data folder
(~/.local/share/Luminescent, or %LOCALAPPDATA%\\Luminescent on Windows), and a git checkout keeps
settings.json and data/ out of git, so `git pull` leaves them alone.
"""
import os
import sys
import json
import time
import shutil
import logging
import tarfile
import zipfile
import tempfile
import threading
import subprocess
import urllib.request

from PyQt6.QtCore import QObject, pyqtSignal

from core import VERSION, REPO, APP_DIR, FROZEN, IS_WIN

log = logging.getLogger("updates")
API = f"https://api.github.com/repos/{REPO}/releases/latest"
ALL = f"https://api.github.com/repos/{REPO}/releases?per_page=30"
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
        self.asset = ""                   # its download for this system
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
            self.asset = asset_url(d)
            self.found.emit(tag, str(d.get("html_url") or PAGE))
        elif force:
            self.found.emit("", "")


ASSET = "Luminescent-windows.zip" if IS_WIN else "Luminescent-linux.tar.gz"


def asset_url(release: dict) -> str:
    for a in release.get("assets") or []:
        if a.get("name") == ASSET:
            return str(a.get("browser_download_url") or "")
    return ""


def list_releases() -> list[dict]:
    """Every published release, newest first: [{"tag", "name", "date", "notes", "asset", "url"}].
    Blocks for up to 15 s, so call it from a worker thread."""
    req = urllib.request.Request(ALL, headers={"User-Agent": "Luminescent", "Accept": "application/vnd.github+json"})
    with urllib.request.urlopen(req, timeout=15) as r:
        data = json.loads(r.read().decode("utf-8"))
    out = []
    for d in data if isinstance(data, list) else []:
        if d.get("draft") or not d.get("tag_name"):
            continue
        out.append({"tag": str(d["tag_name"]), "name": str(d.get("name") or d["tag_name"]),
                    "date": str(d.get("published_at") or "")[:10], "notes": str(d.get("body") or ""),
                    "asset": asset_url(d), "url": str(d.get("html_url") or PAGE),
                    "pre": bool(d.get("prerelease"))})
    out.sort(key=lambda r: _parts(r["tag"]), reverse=True)
    return out


def program_dir() -> str:
    """The folder a downloaded build runs from (the one with Luminescent / Luminescent.exe in it)."""
    return os.path.dirname(os.path.abspath(sys.executable))


def is_git() -> bool:
    return not FROZEN and os.path.isdir(os.path.join(APP_DIR, ".git"))


def can_install() -> bool:
    """True when a one-click update can work here: a git checkout, or a build in a folder we may write to."""
    if is_git():
        return bool(shutil.which("git"))
    if not FROZEN:
        return False
    return os.access(os.path.dirname(program_dir()), os.W_OK)


class Updater(QObject):
    """Downloads and installs an update on a worker thread. `progress` says what it is doing, `finished`
    is (ok, message); after ok the caller restarts (git) or quits so the swap can happen (a build)."""
    progress = pyqtSignal(str)
    finished = pyqtSignal(bool, str)

    def __init__(self):
        super().__init__()
        self.busy = False

    def install(self, asset: str = "", tag: str = ""):
        """`tag` picks a version (also an older one); empty = the newest."""
        if self.busy:
            return
        self.busy = True
        target = self._git if is_git() else self._build
        threading.Thread(target=self._run, args=(target, asset, tag), daemon=True, name="updater").start()

    def _run(self, target, asset, tag):
        try:
            ok, msg = target(asset, tag)
        except Exception as e:
            log.exception("update failed")
            ok, msg = False, f"{type(e).__name__}: {e}"
        self.busy = False
        self.finished.emit(ok, msg)

    # ── a git checkout ─────────────────────────────────────────────────────
    def _git(self, _asset, tag=""):
        def git(*args, timeout=300):
            return subprocess.run(["git", *args], cwd=APP_DIR, capture_output=True, text=True, timeout=timeout)
        if tag:
            # a chosen version (newer or older): check that tag out; "Back to the newest" undoes it
            self.progress.emit(f"Getting {tag} (git)…")
            r = git("fetch", "--tags", "origin")
            if r.returncode == 0:
                r = git("checkout", "--detach", tag)
        else:
            self.progress.emit("Getting the new code (git pull)…")
            head = git("symbolic-ref", "-q", "HEAD")
            if head.returncode != 0:                 # still on an older version from the list: back to main
                r = git("checkout", "main")
                if r.returncode != 0:
                    return False, (r.stderr or r.stdout).strip()[-400:] or "git checkout main failed"
            r = git("pull", "--ff-only")
        if r.returncode != 0:
            return False, (r.stderr or r.stdout).strip()[-400:] or "git failed"
        self.progress.emit("Installing the Python packages…")
        r = subprocess.run([sys.executable, "-m", "pip", "install", "-q", "-r", "requirements.txt"], cwd=APP_DIR,
                           capture_output=True, text=True, timeout=900)
        if r.returncode != 0:
            log.warning("pip install after update: %s", r.stderr[-2000:])
            return True, "The code is updated, but installing the packages reported a problem (see the log)."
        return True, ""

    # ── a downloaded build ─────────────────────────────────────────────────
    def _build(self, asset, _tag=""):
        if not asset:
            return False, f"This release has no {ASSET} to download."
        here = program_dir()
        parent = os.path.dirname(here)
        stage = os.path.join(parent, ".luminescent-update")
        shutil.rmtree(stage, ignore_errors=True)
        os.makedirs(stage)
        archive = os.path.join(stage, ASSET)
        self._download(asset, archive)
        self.progress.emit("Unpacking…")
        out = os.path.join(stage, "new")
        if archive.endswith(".zip"):
            with zipfile.ZipFile(archive) as z:
                z.extractall(out)
        else:
            with tarfile.open(archive) as t:
                if hasattr(tarfile, "data_filter"):
                    t.extractall(out, filter="data")
                else:
                    t.extractall(out)
        os.remove(archive)
        new = os.path.join(out, "Luminescent")
        exe = "Luminescent.exe" if IS_WIN else "Luminescent"
        if not os.path.isfile(os.path.join(new, exe)):
            return False, "The download did not have the program in it."
        if IS_WIN:
            self._write_swap_script(stage, here, new, exe)
        else:
            # on Linux a running program's folder can be renamed, so swap now; the restart starts the new one
            old = here + ".old"
            shutil.rmtree(old, ignore_errors=True)
            os.rename(here, old)
            os.rename(new, here)
            shutil.rmtree(stage, ignore_errors=True)
        return True, ""

    def _download(self, url: str, path: str):
        req = urllib.request.Request(url, headers={"User-Agent": "Luminescent"})
        with urllib.request.urlopen(req, timeout=30) as r, open(path, "wb") as f:
            total = int(r.headers.get("Content-Length") or 0)
            got, shown = 0, -1
            while True:
                chunk = r.read(256 * 1024)
                if not chunk:
                    break
                f.write(chunk)
                got += len(chunk)
                pct = int(got * 100 / total) if total else -1
                if pct // 10 != shown // 10:
                    shown = pct
                    self.progress.emit(f"Downloading… {pct}%" if pct >= 0 else f"Downloading… {got // 1_000_000} MB")

    @staticmethod
    def _write_swap_script(stage: str, here: str, new: str, exe: str):
        # waits for this process to end, swaps the folders (retrying while Windows still holds a file),
        # starts the new version and cleans up after itself
        bat = os.path.join(tempfile.gettempdir(), "luminescent-update.bat")
        old = here + ".old"
        with open(bat, "w", encoding="utf-8") as f:
            f.write(f"""@echo off
:wait
tasklist /FI "PID eq {os.getpid()}" 2>nul | find "{os.getpid()}" >nul && (timeout /t 1 /nobreak >nul & goto wait)
if exist "{old}" rmdir /s /q "{old}"
set tries=0
:swap
move "{here}" "{old}" >nul 2>&1 || (set /a tries+=1 & timeout /t 1 /nobreak >nul & if %tries% lss 30 goto swap)
move "{new}" "{here}" >nul 2>&1 || (move "{old}" "{here}" >nul 2>&1 & goto start)
rmdir /s /q "{stage}"
:start
start "" "{os.path.join(here, exe)}"
rmdir /s /q "{old}" >nul 2>&1
del "%~f0"
""")
        flags = getattr(subprocess, "CREATE_NO_WINDOW", 0) | getattr(subprocess, "DETACHED_PROCESS", 0)
        subprocess.Popen(["cmd", "/c", bat], creationflags=flags, close_fds=True)


def clean_old():
    """After a Linux build swapped itself, the old folder is left for the restart; remove it now."""
    if FROZEN:
        shutil.rmtree(program_dir() + ".old", ignore_errors=True)


def changes_since(old: str) -> str:
    """The CHANGELOG.md entries newer than `old`, as plain text for the What is new message."""
    try:
        with open(os.path.join(APP_DIR, "CHANGELOG.md"), "r", encoding="utf-8") as f:
            text = f.read()
    except OSError:
        return ""
    out, keep = [], False
    for line in text.splitlines():
        if line.startswith("## "):
            keep = newer(line[3:].strip(), old)
            if keep:
                out.append(f"Version {line[3:].strip()}")
            continue
        if keep:
            out.append(line.replace("**", ""))
    return "\n".join(out).strip()
