"""
The desktop side of Luminescent: its app launcher entry and icon, the icon theme,
and "show this file in the file manager".

  * ~/.local/share/applications/luminescent.desktop, so app launchers and docks (Noctalia...)
    list Luminescent with its icon. The Wayland app id is "luminescent", which is how a dock
    matches the running window to this entry (StartupWMClass is there for XWayland docks).
  * the icon in ~/.local/share/icons/hicolor/<size>/apps/luminescent.png at the usual sizes,
    made from assets/luminescent-mark.png.
  * Qt from pip has no qt6ct plugin, so QIcon.fromTheme() only sees hicolor; the theme qt6ct,
    KDE or GNOME is set to is applied by hand.

Everything is rewritten only when it changed, so this costs nothing on a normal start.
"""
import os
import sys
import shlex
import shutil
import logging
import configparser
import subprocess

from core import APP_DIR, APP_NAME, FROZEN

log = logging.getLogger("desktop")

_DATA = os.environ.get("XDG_DATA_HOME") or os.path.expanduser("~/.local/share")
_CONFIG = os.environ.get("XDG_CONFIG_HOME") or os.path.expanduser("~/.config")
ENTRY = os.path.join(_DATA, "applications", "luminescent.desktop")
ICON_DIR = os.path.join(_DATA, "icons", "hicolor")
ICON_NAME = "luminescent"
ICON_SIZES = (16, 22, 24, 32, 48, 64, 128, 256, 512)
MARK = os.path.join(APP_DIR, "assets", "luminescent-mark.png")


def _quote(arg: str) -> str:
    """Quote one Exec= argument the way the desktop entry spec wants it (double quotes, not the shell's single
    ones, which launchers do not understand): the project folder can have a space in it."""
    if arg and not any(c in arg for c in ' \t\n"\'\\><~|&;$*?#()`'):
        return arg
    return '"' + "".join("\\" + c if c in '"`$\\' else c for c in arg) + '"'


def exec_line() -> str:
    if FROZEN:
        return _quote(os.path.abspath(sys.executable))
    return _quote(os.path.join(APP_DIR, "start.sh"))


def _entry_text() -> str:
    return "\n".join([
        "[Desktop Entry]",
        "Type=Application",
        f"Name={APP_NAME}",
        "GenericName=Hotkey tools hub",
        "Comment=Overlay, notes, doodle, timers, calculator, clipboard and more, a hotkey away",
        f"Exec={exec_line()}",
        f"Path={APP_DIR}",
        f"Icon={ICON_NAME}",
        "Terminal=false",
        "Categories=Utility;",
        "Keywords=overlay;hotkeys;notes;timer;calculator;clipboard;doodle;",
        "StartupWMClass=luminescent",
        "StartupNotify=false",
        "",
    ])


def _write_if_changed(path: str, text: str) -> bool:
    try:
        with open(path, "r", encoding="utf-8") as f:
            if f.read() == text:
                return False
    except OSError:
        pass
    os.makedirs(os.path.dirname(path), exist_ok=True)
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        f.write(text)
    os.replace(tmp, path)
    return True


def install_entry() -> str:
    """Write the launcher entry and the icons (only what changed). Returns '' or an error."""
    try:
        changed = _install_icons()
        if _write_if_changed(ENTRY, _entry_text()):
            changed = True
            log.info("launcher entry written: %s", ENTRY)
        if changed:
            for cmd in (["update-desktop-database", os.path.dirname(ENTRY)],
                        ["gtk-update-icon-cache", "-q", "-t", ICON_DIR]):
                if shutil.which(cmd[0]):
                    subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)
        return ""
    except (OSError, subprocess.SubprocessError) as e:
        log.warning("could not install the launcher entry: %s", e)
        return str(e)


SERVICE_MENU = os.path.join(_DATA, "kio", "servicemenus", "luminescent-strip-metadata.desktop")


def install_service_menu() -> str:
    """Dolphin's right-click "Strip metadata" for images, videos and folders. Returns '' or an error."""
    text = "\n".join([
        "[Desktop Entry]",
        "Type=Service",
        "MimeType=image/*;video/*;inode/directory;",
        "X-KDE-ServiceTypes=KonqPopupMenu/Plugin",
        "Actions=strip;",
        "",
        "[Desktop Action strip]",
        "Name=Strip metadata (Luminescent)",
        "Icon=edit-clear-all",
        f"Exec={exec_line()} --strip-metadata %F",
        "",
    ])
    try:
        if _write_if_changed(SERVICE_MENU, text):
            log.info("Dolphin menu written: %s", SERVICE_MENU)
        os.chmod(SERVICE_MENU, 0o755)            # Plasma 6 only runs service menus that are executable
        return ""
    except OSError as e:
        return str(e)


def remove_service_menu():
    try:
        os.remove(SERVICE_MENU)
    except FileNotFoundError:
        pass


def remove_entry():
    """Take Luminescent out of the app launchers again (the icons stay: the autostart entry uses them)."""
    try:
        os.remove(ENTRY)
        log.info("launcher entry removed")
    except FileNotFoundError:
        pass
    except OSError as e:
        log.warning("could not remove %s: %s", ENTRY, e)


def _install_icons() -> bool:
    if not os.path.isfile(MARK):
        return False
    from PyQt6.QtGui import QImage
    from PyQt6.QtCore import Qt
    src = None
    changed = False
    stamp = os.path.getmtime(MARK)
    for size in ICON_SIZES:
        dst = os.path.join(ICON_DIR, f"{size}x{size}", "apps", f"{ICON_NAME}.png")
        if os.path.isfile(dst) and os.path.getmtime(dst) >= stamp:
            continue
        if src is None:
            src = QImage(MARK)
            if src.isNull():
                return False
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        src.scaled(size, size, Qt.AspectRatioMode.KeepAspectRatio,
                   Qt.TransformationMode.SmoothTransformation).save(dst, "PNG")
        changed = True
    return changed


# ── icon theme ────────────────────────────────────────────────────────────────
def configured_icon_theme() -> str:
    """The icon theme your desktop uses: qt6ct, then KDE, then GNOME/GTK."""
    for path, section, key in ((os.path.join(_CONFIG, "qt6ct", "qt6ct.conf"), "Appearance", "icon_theme"),
                               (os.path.join(_CONFIG, "qt5ct", "qt5ct.conf"), "Appearance", "icon_theme"),
                               (os.path.join(_CONFIG, "kdeglobals"), "Icons", "Theme")):
        cp = configparser.ConfigParser(interpolation=None, strict=False)
        try:
            cp.read(path, encoding="utf-8")
            v = cp.get(section, key, fallback="").strip()
            if v:
                return v
        except (configparser.Error, OSError):
            pass
    try:
        out = subprocess.run(["gsettings", "get", "org.gnome.desktop.interface", "icon-theme"],
                             capture_output=True, text=True, timeout=3).stdout.strip().strip("'")
        if out:
            return out
    except (OSError, subprocess.SubprocessError):
        pass
    return ""


def apply_icon_theme():
    """Make QIcon.fromTheme() see your icon theme (call once, after QApplication exists)."""
    from PyQt6.QtGui import QIcon
    if QIcon.themeName() not in ("", "hicolor"):
        return
    name = configured_icon_theme()
    if name:
        QIcon.setThemeName(name)
        QIcon.setFallbackThemeName("hicolor")
        log.info("icon theme: %s", name)


# ── file manager ──────────────────────────────────────────────────────────────
def reveal(path: str) -> str:
    """Open the file manager with `path` selected (its folder for a folder). Returns '' or an error."""
    path = os.path.abspath(os.path.expanduser(os.path.expandvars(path)))
    if not os.path.exists(path):
        return f"{path} does not exist."
    try:
        from PyQt6.QtDBus import QDBusConnection, QDBusMessage
        from PyQt6.QtCore import QUrl
        msg = QDBusMessage.createMethodCall("org.freedesktop.FileManager1", "/org/freedesktop/FileManager1",
                                            "org.freedesktop.FileManager1", "ShowItems")
        msg.setArguments([[QUrl.fromLocalFile(path).toString()], ""])
        reply = QDBusConnection.sessionBus().call(msg, timeout=2000)
        if reply.type() != QDBusMessage.MessageType.ErrorMessage:
            return ""
    except Exception:
        pass
    for fm, args in (("dolphin", ["--select", path]), ("nautilus", ["--select", path]),
                     ("nemo", [path]), ("thunar", [os.path.dirname(path)])):
        if shutil.which(fm):
            try:
                subprocess.Popen([fm, *args], start_new_session=True, stdout=subprocess.DEVNULL,
                                 stderr=subprocess.DEVNULL)
                return ""
            except OSError:
                continue
    from core import runner
    return runner.open_path(path if os.path.isdir(path) else os.path.dirname(path))


# ── real icons for files, folders, programs ───────────────────────────────────
_app_icons: dict[str, str] | None = None


def _desktop_dirs() -> list[str]:
    dirs = [os.path.join(_DATA, "applications")]
    for d in (os.environ.get("XDG_DATA_DIRS") or "/usr/local/share:/usr/share").split(":"):
        if d:
            dirs.append(os.path.join(d, "applications"))
    dirs.append(os.path.expanduser("~/.local/share/flatpak/exports/share/applications"))
    dirs.append("/var/lib/flatpak/exports/share/applications")
    return dirs


def _read_entry(path: str) -> dict:
    out = {}
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            in_main = False
            for line in f:
                line = line.strip()
                if line.startswith("["):
                    in_main = line == "[Desktop Entry]"
                    continue
                if in_main and "=" in line:
                    k, _, v = line.partition("=")
                    out.setdefault(k.strip(), v.strip())
    except OSError:
        pass
    return out


def _index_apps() -> dict[str, str]:
    """{program name (and desktop id): icon name} from every installed .desktop file."""
    global _app_icons
    if _app_icons is not None:
        return _app_icons
    _app_icons = {}
    for d in _desktop_dirs():
        try:
            names = os.listdir(d)
        except OSError:
            continue
        for n in names:
            if not n.endswith(".desktop"):
                continue
            e = _read_entry(os.path.join(d, n))
            icon = e.get("Icon", "")
            if not icon:
                continue
            _app_icons.setdefault(n[:-len(".desktop")].lower(), icon)
            try:
                argv = shlex.split(e.get("Exec", ""))
            except ValueError:
                argv = e.get("Exec", "").split()
            argv = [a for a in argv if a != "env" and "=" not in a]
            if argv:
                _app_icons.setdefault(os.path.basename(argv[0]).lower(), icon)
    return _app_icons


def _theme_icon(name: str):
    from PyQt6.QtGui import QIcon
    if not name:
        return None
    if os.path.isabs(name) and os.path.isfile(name):
        icon = QIcon(name)
        return icon if not icon.isNull() else None
    icon = QIcon.fromTheme(name)
    return icon if not icon.isNull() else None


def icon_for(target: str, kind: str):
    """A QIcon for a dashboard item's target (a file, folder, program, .desktop, URL or command), or None."""
    from PyQt6.QtCore import QMimeDatabase, QFileInfo
    t = (target or "").strip().strip('"')
    if not t:
        return None
    if kind == "url":
        return _theme_icon("internet-web-browser") or _theme_icon("applications-internet")
    path = os.path.expandvars(os.path.expanduser(t))
    if kind == "folder":
        home = os.path.expanduser("~")
        special = {home: "user-home", os.path.join(home, "Desktop"): "user-desktop",
                   os.path.join(home, "Downloads"): "folder-download", os.path.join(home, "Documents"): "folder-documents",
                   os.path.join(home, "Pictures"): "folder-pictures", os.path.join(home, "Music"): "folder-music",
                   os.path.join(home, "Videos"): "folder-videos"}
        return _theme_icon(special.get(os.path.normpath(path), "")) or _theme_icon("folder")
    if path.endswith(".desktop") and os.path.isfile(path):
        return _theme_icon(_read_entry(path).get("Icon", ""))
    if kind in ("program", "python", "file") and os.path.isfile(path):
        name = os.path.basename(path).lower()
        app = _index_apps().get(name) or _index_apps().get(os.path.splitext(name)[0])
        if app:
            icon = _theme_icon(app)
            if icon:
                return icon
        mime = QMimeDatabase().mimeTypeForFile(QFileInfo(path))
        return _theme_icon(mime.iconName()) or _theme_icon(mime.genericIconName())
    if kind == "command":
        try:
            prog = os.path.basename(shlex.split(t)[0]).lower()
        except (ValueError, IndexError):
            return None
        return _theme_icon(_index_apps().get(prog, "")) or _theme_icon(prog) or _theme_icon("utilities-terminal")
    return None
