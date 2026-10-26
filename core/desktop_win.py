"""
The Windows side of core/desktop.py: a Start menu shortcut, Explorer's right-click "Strip metadata",
"show this file in Explorer" and real file and program icons.

  * %APPDATA%\\Microsoft\\Windows\\Start Menu\\Programs\\Luminescent.lnk, so Start and search find it
    (made through the WScript.Shell COM object, by PowerShell).
  * HKCU\\Software\\Classes\\…\\shell\\LuminescentStripMetadata for pictures, videos and folders: right-click
    → Strip metadata (Luminescent) runs `Luminescent --strip-metadata <file>`. Only the current user's keys are
    touched, so no administrator rights are needed.
"""
import os
import sys
import json
import logging
import subprocess

from core import APP_DIR, APP_NAME, FROZEN, DATA_DIR

log = logging.getLogger("desktop")

START_MENU = os.path.join(os.environ.get("APPDATA", ""), "Microsoft", "Windows", "Start Menu", "Programs")
SHORTCUT = os.path.join(START_MENU, f"{APP_NAME}.lnk")
STATE = os.path.join(DATA_DIR, "desktop_win.json")
MENU_KEY = "LuminescentStripMetadata"
MENU_CLASSES = [r"Software\Classes\SystemFileAssociations\image", r"Software\Classes\SystemFileAssociations\video",
                r"Software\Classes\Directory"]


def _program() -> tuple[str, str]:
    """(program, its arguments) that start Luminescent: the exe, or pythonw + Launcher.py from source."""
    if FROZEN:
        return sys.executable, ""
    exe = sys.executable
    w = os.path.join(os.path.dirname(exe), "pythonw.exe")
    return (w if os.path.isfile(w) else exe), f'"{os.path.join(APP_DIR, "Launcher.py")}"'


def _icon_file() -> str:
    """An .ico for the shortcut (the exe has its own; from source one is made from the PNG mark)."""
    if FROZEN:
        return sys.executable
    ico = os.path.join(DATA_DIR, "luminescent.ico")
    if not os.path.isfile(ico):
        try:
            from PIL import Image
            src = os.path.join(APP_DIR, "assets", "luminescent-256.png")
            Image.open(src).save(ico, sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (256, 256)])
        except Exception:
            return ""
    return ico


def _ps(script: str) -> bool:
    try:
        r = subprocess.run(["powershell.exe", "-NoProfile", "-NonInteractive", "-Command", script],
                           capture_output=True, timeout=20, creationflags=subprocess.CREATE_NO_WINDOW)
        return r.returncode == 0
    except (OSError, subprocess.SubprocessError):
        return False


def _q(s: str) -> str:
    return "'" + s.replace("'", "''") + "'"


def install_entry() -> str:
    """The Start menu shortcut (rewritten only when where Luminescent lives changed). Returns '' or an error."""
    exe, args = _program()
    want = {"exe": exe, "args": args}
    try:
        with open(STATE, "r", encoding="utf-8") as f:
            if json.load(f) == want and os.path.isfile(SHORTCUT):
                return ""
    except (OSError, ValueError):
        pass
    script = (f"$s = (New-Object -ComObject WScript.Shell).CreateShortcut({_q(SHORTCUT)}); "
              f"$s.TargetPath = {_q(exe)}; $s.Arguments = {_q(args)}; $s.WorkingDirectory = {_q(APP_DIR)}; "
              f"$s.Description = {_q('Hotkey tools hub')}; "
              + (f"$s.IconLocation = {_q(_icon_file())}; " if _icon_file() else "") + "$s.Save()")
    if not _ps(script):
        return "could not make the Start menu shortcut"
    try:
        with open(STATE, "w", encoding="utf-8") as f:
            json.dump(want, f)
    except OSError:
        pass
    log.info("Start menu shortcut written: %s", SHORTCUT)
    return ""


def remove_entry():
    for p in (SHORTCUT, STATE):
        try:
            os.remove(p)
        except OSError:
            pass


def install_service_menu() -> str:
    """Explorer's right-click → Strip metadata (Luminescent). Returns '' or an error."""
    import winreg
    exe, args = _program()
    command = f'"{exe}" {args} --strip-metadata "%1"'.replace("  ", " ")
    try:
        for cls in MENU_CLASSES:
            with winreg.CreateKey(winreg.HKEY_CURRENT_USER, rf"{cls}\shell\{MENU_KEY}") as k:
                winreg.SetValueEx(k, "", 0, winreg.REG_SZ, "Strip metadata (Luminescent)")
                icon = _icon_file()
                if icon:
                    winreg.SetValueEx(k, "Icon", 0, winreg.REG_SZ, icon)
            with winreg.CreateKey(winreg.HKEY_CURRENT_USER, rf"{cls}\shell\{MENU_KEY}\command") as k:
                winreg.SetValueEx(k, "", 0, winreg.REG_SZ, command)
        return ""
    except OSError as e:
        return str(e)


def remove_service_menu():
    import winreg
    for cls in MENU_CLASSES:
        for sub in (rf"{cls}\shell\{MENU_KEY}\command", rf"{cls}\shell\{MENU_KEY}"):
            try:
                winreg.DeleteKey(winreg.HKEY_CURRENT_USER, sub)
            except OSError:
                pass


def apply_icon_theme():
    """Windows draws its own icons: nothing to set up."""


def reveal(path: str) -> str:
    """Open Explorer with `path` selected. Returns '' or an error."""
    path = os.path.abspath(os.path.expanduser(os.path.expandvars(path)))
    if not os.path.exists(path):
        return f"{path} does not exist."
    try:
        subprocess.Popen(["explorer", "/select,", path])
        return ""
    except OSError as e:
        return str(e)


def icon_for(target: str, kind: str):
    """The real icon of a dashboard item's file, folder or program (what Explorer shows), or None."""
    from PyQt6.QtCore import QFileInfo
    from PyQt6.QtWidgets import QFileIconProvider
    import shutil
    t = (target or "").strip().strip('"')
    if not t:
        return None
    provider = QFileIconProvider()
    if kind == "url":
        return None
    path = os.path.expandvars(os.path.expanduser(t))
    if kind == "command":
        found = shutil.which(t.split()[0]) if t.split() else None
        path = found or ""
    if path and os.path.exists(path):
        icon = provider.icon(QFileInfo(path))
        return icon if not icon.isNull() else None
    return None
