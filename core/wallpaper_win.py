"""
Desktop wallpaper switching for themes, on Windows (core/wallpaper.py uses this on Windows).

A theme's `wallpaper` can be a picture (.jpg/.png/.bmp, set as the normal
Windows wallpaper) or a Wallpaper Engine project (project.json, its folder, or
a .pkg/.mp4 inside it), opened with Wallpaper Engine's command-line control:
    wallpaper64.exe -control openWallpaper -file "<project.json>"
Both run on a worker thread: SystemParametersInfo can take a while and must
not stall the keyboard hook.
"""
import os
import re
import ctypes
import logging
import threading
import subprocess
import winreg

log = logging.getLogger("wallpaper")

IMAGE_EXT = {".jpg", ".jpeg", ".png", ".bmp"}
SPI_SETDESKWALLPAPER = 0x0014
SPIF_UPDATEINIFILE, SPIF_SENDCHANGE = 0x01, 0x02


def _steam_libraries() -> list[str]:
    libs = []
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Software\Valve\Steam") as k:
            steam = winreg.QueryValueEx(k, "SteamPath")[0].replace("/", "\\")
        libs.append(steam)
        vdf = os.path.join(steam, "steamapps", "libraryfolders.vdf")
        with open(vdf, "r", encoding="utf-8", errors="ignore") as f:
            libs += [p.replace("\\\\", "\\") for p in re.findall(r'"path"\s+"([^"]+)"', f.read())]
    except OSError:
        pass
    libs += [r"C:\Program Files (x86)\Steam", r"C:\Program Files\Steam"]
    return list(dict.fromkeys(libs))


def find_wallpaper_engine(configured: str = "") -> str:
    if configured and os.path.isfile(configured):
        return configured
    for lib in _steam_libraries():
        for exe in ("wallpaper64.exe", "wallpaper32.exe"):
            p = os.path.join(lib, "steamapps", "common", "wallpaper_engine", exe)
            if os.path.isfile(p):
                return p
    return ""


def _project_file(path: str) -> str:
    if os.path.isdir(path):
        path = os.path.join(path, "project.json")
    elif os.path.basename(path).lower() != "project.json":
        cand = os.path.join(os.path.dirname(path), "project.json")
        if os.path.isfile(cand):
            path = cand
    return path


def is_image(path: str) -> bool:
    return os.path.splitext(path)[1].lower() in IMAGE_EXT


def apply(path: str, engine_exe: str = "", on_error=None):
    """Switch the desktop wallpaper in the background. `on_error(text)` is called from the worker thread."""
    path = os.path.expandvars((path or "").strip().strip('"'))
    if not path:
        return

    def work():
        try:
            if not os.path.exists(path):
                raise FileNotFoundError(f"{path} does not exist")
            if is_image(path):
                ok = ctypes.windll.user32.SystemParametersInfoW(
                    SPI_SETDESKWALLPAPER, 0, os.path.abspath(path), SPIF_UPDATEINIFILE | SPIF_SENDCHANGE)
                if not ok:
                    raise OSError("Windows refused the wallpaper")
                log.info("wallpaper set to %s", path)
                return
            exe = find_wallpaper_engine(engine_exe)
            if not exe:
                raise FileNotFoundError("Wallpaper Engine was not found. Set its path in Settings → Overlay & Interface.")
            subprocess.Popen([exe, "-control", "openWallpaper", "-file", _project_file(path)],
                             creationflags=subprocess.CREATE_NO_WINDOW, close_fds=True)
            log.info("Wallpaper Engine: opened %s", path)
        except Exception as e:
            log.warning("could not switch wallpaper: %s", e)
            if on_error:
                on_error(str(e))

    threading.Thread(target=work, name="wallpaper", daemon=True).start()
