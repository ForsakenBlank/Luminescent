"""
Desktop wallpaper switching for themes.

A theme's `wallpaper` can be a picture (.jpg/.png/.webp...) or a Wallpaper
Engine project (project.json, its folder, or a .pkg/.mp4 inside it).

Pictures go to whatever draws your wallpaper, the first one that answers:
Noctalia (`noctalia msg wallpaper-set`), swww, hyprpaper, swaybg.
Wallpaper Engine projects need linux-wallpaperengine (AUR); it is given the
project folder and started once per screen.  Both run on a worker thread so a
slow wallpaper tool never stalls the UI.
"""
import os
import re
import json
import shutil
import logging
import threading
import subprocess

from core import IS_WIN

log = logging.getLogger("wallpaper")

IMAGE_EXT = {".jpg", ".jpeg", ".png", ".bmp", ".webp", ".gif"}
_engine_procs: list[subprocess.Popen] = []
_swaybg: subprocess.Popen | None = None


def _steam_libraries() -> list[str]:
    home = os.path.expanduser("~")
    roots = [os.path.join(home, ".local", "share", "Steam"), os.path.join(home, ".steam", "steam"),
             os.path.join(home, ".var", "app", "com.valvesoftware.Steam", ".local", "share", "Steam")]
    libs = []
    for root in roots:
        if not os.path.isdir(root):
            continue
        libs.append(root)
        vdf = os.path.join(root, "steamapps", "libraryfolders.vdf")
        try:
            with open(vdf, "r", encoding="utf-8", errors="ignore") as f:
                libs += re.findall(r'"path"\s+"([^"]+)"', f.read())
        except OSError:
            pass
    return list(dict.fromkeys(os.path.realpath(p) for p in libs))


def find_wallpaper_engine(configured: str = "") -> str:
    """linux-wallpaperengine (the program that plays Wallpaper Engine projects on Linux)."""
    if configured and os.path.isfile(configured):
        return configured
    return shutil.which("linux-wallpaperengine") or ""


def _assets_dir() -> str:
    for lib in _steam_libraries():
        p = os.path.join(lib, "steamapps", "common", "wallpaper_engine", "assets")
        if os.path.isdir(p):
            return p
    return ""


def _project_dir(path: str) -> str:
    if os.path.isdir(path):
        return path
    return os.path.dirname(path)


def is_image(path: str) -> bool:
    return os.path.splitext(path)[1].lower() in IMAGE_EXT


def _monitors() -> list[str]:
    from core import hypr
    return [m.get("name", "") for m in (hypr.query("monitors") or []) if m.get("name")]


def _run(cmd: list[str], timeout: float = 10) -> bool:
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        if r.returncode != 0:
            log.info("%s: %s", cmd[0], (r.stderr or r.stdout).strip()[:200])
        return r.returncode == 0
    except (OSError, subprocess.SubprocessError) as e:
        log.info("%s: %s", cmd[0], e)
        return False


def _set_picture(path: str):
    global _swaybg
    path = os.path.abspath(path)
    if shutil.which("noctalia") and _run(["noctalia", "msg", "wallpaper-set", path]):
        return "Noctalia"
    if shutil.which("swww") and _run(["swww", "img", path]):
        return "swww"
    if shutil.which("hyprctl") and _run(["hyprctl", "hyprpaper", "preload", path]):
        if _run(["hyprctl", "hyprpaper", "wallpaper", f",{path}"]):
            return "hyprpaper"
    if shutil.which("swaybg"):
        if _swaybg and _swaybg.poll() is None:
            _swaybg.terminate()
        _swaybg = subprocess.Popen(["swaybg", "-m", "fill", "-i", path], start_new_session=True,
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        return "swaybg"
    raise OSError("No wallpaper program answered (Noctalia, swww, hyprpaper or swaybg).")


def _open_engine(exe: str, path: str):
    proj = _project_dir(path)
    if not os.path.isfile(os.path.join(proj, "project.json")):
        raise FileNotFoundError(f"{proj} has no project.json")
    for p in _engine_procs:
        if p.poll() is None:
            p.terminate()
    _engine_procs.clear()
    args = [exe]
    assets = _assets_dir()
    if assets:
        args += ["--assets-dir", assets]
    for name in _monitors() or [""]:
        cmd = args + (["--screen-root", name] if name else []) + ["--bg", proj]
        _engine_procs.append(subprocess.Popen(cmd, start_new_session=True,
                                              stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
    try:
        with open(os.path.join(proj, "project.json"), "r", encoding="utf-8") as f:
            log.info("Wallpaper Engine: %s", json.load(f).get("title", proj))
    except (OSError, ValueError):
        log.info("Wallpaper Engine: %s", proj)


def apply(path: str, engine_exe: str = "", on_error=None):
    """Switch the desktop wallpaper in the background. `on_error(text)` is called from the worker thread."""
    path = os.path.expandvars(os.path.expanduser((path or "").strip().strip('"')))
    if not path:
        return

    def work():
        try:
            if not os.path.exists(path):
                raise FileNotFoundError(f"{path} does not exist")
            if is_image(path):
                for p in _engine_procs:          # a picture replaces a running Wallpaper Engine project
                    if p.poll() is None:
                        p.terminate()
                log.info("wallpaper set to %s (%s)", path, _set_picture(path))
                return
            exe = find_wallpaper_engine(engine_exe)
            if not exe:
                raise FileNotFoundError("linux-wallpaperengine was not found. Install it (AUR) or set its path in "
                                        "Settings → Overlay & Interface.")
            _open_engine(exe, path)
        except Exception as e:
            log.warning("could not switch wallpaper: %s", e)
            if on_error:
                on_error(str(e))

    threading.Thread(target=work, name="wallpaper", daemon=True).start()


if IS_WIN:
    from core.wallpaper_win import apply, find_wallpaper_engine, is_image  # noqa: E402,F811
