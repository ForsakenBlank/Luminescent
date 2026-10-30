"""
Caelestia looks per Luminescent theme: what a theme changes in the Caelestia shell besides its colours.

Shell settings (how see-through its panels are, fonts, sizes, the dashboard and power menu pictures, the logo,
the lock screen, the desktop clock, the screen border...) are written into ~/.config/caelestia/shell.json when
the theme is applied, a backup first; Caelestia reloads that file by itself. What the file had before is
remembered, so switching to a theme that leaves a setting alone puts the old value back.

Two things Caelestia has no setting for, a picture in place of the lyrics on the dashboard's media page and a
picture behind the lock screen, need two of its files changed. Luminescent never edits Caelestia's own files:
it builds ~/.config/quickshell/caelestia, which Quickshell uses before the system copy, out of links to every
system file except those two, which are patched copies. The patched files read what to show from
~/.local/state/luminescent/caelestia.json, so switching themes needs no restart; turning the extras on or off
needs Caelestia to restart once. If a Caelestia update changes either file so the patch no longer fits, the
extras leave that file as installed.

    theme["caelestia"]["shell"]  = {"appearance.transparency.base": 0.6, ...}
    theme["caelestia"]["extras"] = {"lyrics_image": path, "lock_image": path, "lock_blur": 0..1}
"""
import os
import json
import shutil
import logging
import subprocess
import signal
import time
import threading

from core import IS_WIN

log = logging.getLogger("caelestia")

HOME = os.path.expanduser("~")
CONFIG_HOME = os.environ.get("XDG_CONFIG_HOME") or os.path.join(HOME, ".config")
SHELL_JSON = os.path.join(CONFIG_HOME, "caelestia", "shell.json")
SYSTEM_QS = "/etc/xdg/quickshell/caelestia"
MIRROR = os.path.join(CONFIG_HOME, "quickshell", "caelestia")
MARK = ".luminescent-mirror"                 # only a folder with this file in it is ours to rebuild or remove
STATE = os.path.join(os.environ.get("XDG_STATE_HOME") or os.path.join(HOME, ".local", "state"), "luminescent",
                     "caelestia.json")

# (title, [(key, label, kind, extra)]); kind: bool, real (lo, hi, step), int (lo, hi), font, image
OPTIONS = [
    ("SEE-THROUGH", [
        ("appearance.transparency.enabled", "See-through panels", "bool", None),
        ("appearance.transparency.base", "How solid the panels are (sidebar, dashboard, bar)", "real", (0, 1, 0.05)),
        ("appearance.transparency.layers", "How solid what sits on them is", "real", (0, 1, 0.05)),
    ]),
    ("SIZE AND SHAPE", [
        ("appearance.rounding.scale", "Rounding", "real", (0, 3, 0.1)),
        ("appearance.spacing.scale", "Spacing", "real", (0.5, 2, 0.05)),
        ("appearance.padding.scale", "Padding", "real", (0.5, 2, 0.05)),
        ("appearance.font.scale", "Text size", "real", (0.7, 1.6, 0.05)),
        ("border.thickness", "Screen border thickness", "int", (0, 40)),
        ("border.rounding", "Screen border rounding", "int", (0, 80)),
    ]),
    ("FONTS", [
        ("appearance.font.headline.family", "Big text (the lock screen clock, headings)", "font", None),
        ("appearance.font.title.family", "Titles", "font", None),
        ("appearance.font.body.family", "Body text", "font", None),
        ("appearance.font.label.family", "Labels and buttons", "font", None),
        ("appearance.font.mono.family", "Monospace", "font", None),
        ("appearance.font.clock", "Desktop clock", "font", None),
    ]),
    ("PICTURES", [
        ("paths.mediaGif", "Dashboard picture (next to the music controls)", "image", None),
        ("general.mediaGifSpeedAdjustment", "Its speed (it follows the beat; higher is slower)", "real",
         (50, 600, 10)),
        ("paths.sessionGif", "Power menu picture (shut down, reboot, log out)", "image", None),
        ("general.sessionGifSpeed", "Its speed", "real", (0.1, 3, 0.1)),
        ("general.logo", "Logo (dashboard and lock screen)", "image", None),
        ("paths.noNotifsPic", "Sidebar picture when there are no notifications", "image", None),
    ]),
    ("LOCK SCREEN", [
        ("lock.useWallpaper", "The wallpaper behind it (off: a blur of what was on screen)", "bool", None),
        ("lock.recolourLogo", "Logo in the theme's colours", "bool", None),
        ("lock.hideNotifs", "Hide notifications", "bool", None),
        ("paths.lockNoNotifsPic", "Picture when there are no notifications", "image", None),
    ]),
    ("DASHBOARD", [
        ("dashboard.showMedia", "Media page", "bool", None),
        ("dashboard.showPerformance", "Performance page", "bool", None),
        ("dashboard.showWeather", "Weather page", "bool", None),
    ]),
    ("DESKTOP", [
        ("background.desktopClock.enabled", "Clock on the desktop", "bool", None),
        ("background.desktopClock.scale", "Its size", "real", (0.5, 3, 0.1)),
        ("background.visualiser.enabled", "Audio visualiser on the desktop", "bool", None),
    ]),
]
KINDS = {key: (kind, extra) for _t, rows in OPTIONS for key, _l, kind, extra in rows}

# the extras that need the patched files
EXTRAS = [
    ("lyrics_image", "Picture in place of the lyrics (dashboard → media)", "image"),
    ("lock_image", "Picture behind the lock screen (a GIF moves)", "image"),
    ("lock_blur", "How much the lock screen background is blurred", "blur"),
]
EXTRA_DEFAULTS = {"lyrics_image": "", "lock_image": "", "lock_blur": 1.0}


def available() -> bool:
    return not IS_WIN and os.path.isfile(SHELL_JSON)


# ── shell.json ────────────────────────────────────────────────────────────────
_MISSING = object()


def read_shell() -> dict:
    try:
        with open(SHELL_JSON, "r", encoding="utf-8") as f:
            d = json.load(f)
        return d if isinstance(d, dict) else {}
    except (OSError, ValueError):
        return {}


def get(d: dict, key: str, default=_MISSING):
    for part in key.split("."):
        if not isinstance(d, dict) or part not in d:
            return default
        d = d[part]
    return d


def _set(d: dict, key: str, value):
    parts = key.split(".")
    for part in parts[:-1]:
        if not isinstance(d.get(part), dict):
            d[part] = {}
        d = d[part]
    d[parts[-1]] = value


def _drop(d: dict, key: str):
    parts = key.split(".")
    trail = []
    for part in parts[:-1]:
        if not isinstance(d.get(part), dict):
            return
        trail.append((d, part))
        d = d[part]
    d.pop(parts[-1], None)
    for parent, part in reversed(trail):          # no empty groups left behind
        if parent[part]:
            break
        parent.pop(part)


def clean(values: dict) -> dict:
    """Only keys Luminescent knows, with values of the right type."""
    out = {}
    for k, v in (values or {}).items():
        kind = KINDS.get(k, (None, None))[0]
        if kind == "bool" and isinstance(v, bool):
            out[k] = v
        elif kind in ("real", "int") and isinstance(v, (int, float)) and not isinstance(v, bool):
            out[k] = int(v) if kind == "int" else round(float(v), 3)
        elif kind in ("font", "image") and isinstance(v, str) and v.strip():
            out[k] = v.strip()
    return out


def write_shell(changes: dict, backup) -> str:
    """Put `changes` ({dotted key: value, or None to take the key out}) into shell.json. '' or an error."""
    if not changes:
        return ""
    d = read_shell()
    if not d and not os.path.isfile(SHELL_JSON):
        return "Caelestia's shell.json was not found."
    for k, v in changes.items():
        if v is None:
            _drop(d, k)
        else:
            _set(d, k, v)
    try:
        backup(SHELL_JSON)
        tmp = SHELL_JSON + ".lum-tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(d, f, indent=4)
            f.write("\n")
        os.replace(tmp, SHELL_JSON)               # in one step, so the shell never reads half a file
    except OSError as e:
        return f"Could not write shell.json: {e}"
    return ""


def plan_shell(wanted: dict, before: dict) -> tuple[dict, dict]:
    """What to write for a theme that wants `wanted`, given what was in shell.json before themes changed it
    (`before`: {key: old value, None when it was not there}). Returns (changes, new before)."""
    now = read_shell()
    changes = {k: v for k, v in before.items() if k not in wanted}      # put back what this theme leaves alone
    keep = {k: v for k, v in before.items() if k in wanted}
    for k, v in wanted.items():
        if k not in keep:
            old = get(now, k)
            keep[k] = None if old is _MISSING else old
        changes[k] = v
    changes = {k: v for k, v in changes.items() if get(now, k, None) != v}
    return changes, keep


# ── the extras ────────────────────────────────────────────────────────────────
def clean_extras(values: dict) -> dict:
    out = {}
    for k, v in (values or {}).items():
        if k in ("lyrics_image", "lock_image") and isinstance(v, str) and v.strip():
            out[k] = v.strip()
        elif k == "lock_blur" and isinstance(v, (int, float)) and not isinstance(v, bool):
            out[k] = max(0.0, min(1.0, float(v)))
    return out


def write_state(extras: dict) -> str:
    """What the patched files show now. '' or an error."""
    d = {**EXTRA_DEFAULTS, **clean_extras(extras)}
    for k in ("lyrics_image", "lock_image"):
        if d[k] and not os.path.isabs(d[k]):
            d[k] = os.path.abspath(os.path.expanduser(d[k]))
    try:
        os.makedirs(os.path.dirname(STATE), exist_ok=True)
        tmp = STATE + ".tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(d, f, indent=2)
        os.replace(tmp, STATE)
    except OSError as e:
        return f"Could not write {STATE}: {e}"
    return ""


# the object every patched file adds: it reads the state file and follows it when it changes
_READER = """
    FileView {
        path: "%s"
        watchChanges: true
        onFileChanged: reload()

        JsonAdapter {
            id: lum

            property string lyrics_image: ""
            property string lock_image: ""
            property real lock_blur: 1
        }
    }
"""


def _patch_lyrics(text: str) -> str | None:
    old = """        LyricList {
            Layout.fillWidth: true
            Layout.fillHeight: true
        }
"""
    new = """        LyricList {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: !lum.lyrics_image
        }

        AnimatedImage {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: !!lum.lyrics_image
            source: lum.lyrics_image ? "file://" + lum.lyrics_image : ""
            fillMode: Image.PreserveAspectFit
            asynchronous: true
            playing: visible
        }
"""
    anchor = "Item {\n"
    if old not in text or anchor not in text or "import Quickshell\n" not in text:
        return None
    text = text.replace(old, new, 1)
    text = text.replace("import Quickshell\n", "import Quickshell\nimport Quickshell.Io\n", 1)
    return text.replace(anchor, anchor + _READER % STATE, 1)


def _patch_lock(text: str) -> str | None:
    loader = "            sourceComponent: Config.lock.useWallpaper ? wallpaperBackground : screencopyBackground\n"
    blur = "            blur: 1\n"
    comp = "    Component {\n        id: screencopyBackground\n"
    holder = "        id: background\n"
    if any(s not in text for s in (loader, blur, comp, holder, "import Quickshell.Wayland\n")):
        return None
    text = text.replace(loader, "            sourceComponent: lum.lock_image ? lumBackground : "
                                "(Config.lock.useWallpaper ? wallpaperBackground : screencopyBackground)\n", 1)
    text = text.replace(blur, "            blur: lum.lock_blur\n", 1)
    text = text.replace(comp, """    Component {
        id: lumBackground

        AnimatedImage {
            source: "file://" + lum.lock_image
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
        }
    }

""" + comp, 1)
    text = text.replace("import Quickshell.Wayland\n", "import Quickshell.Io\nimport Quickshell.Wayland\n", 1)
    return text.replace(holder, holder + _READER.replace("\n    ", "\n        ") % STATE, 1)


PATCHES = {
    "modules/dashboard/media/LyricsAndSelector.qml": _patch_lyrics,
    "modules/lock/LockSurface.qml": _patch_lock,
}


def mirror_state() -> str:
    """'off' (Caelestia runs as installed), 'on' (our mirror is in place) or 'foreign' (someone else's
    ~/.config/quickshell/caelestia, which is never touched)."""
    if not os.path.lexists(MIRROR):
        return "off"
    return "on" if os.path.isfile(os.path.join(MIRROR, MARK)) else "foreign"


def build_mirror() -> tuple[bool, str]:
    """Build (or rebuild, after a Caelestia update) the mirror. Returns (ok, message)."""
    if IS_WIN or not os.path.isdir(SYSTEM_QS):
        return False, f"Caelestia's shell files were not found in {SYSTEM_QS}."
    if mirror_state() == "foreign":
        return False, (f"{MIRROR} already exists and is not Luminescent's (your own copy of the shell?), so it "
                       "was left alone.")
    patched = {}
    for rel, fn in PATCHES.items():
        try:
            with open(os.path.join(SYSTEM_QS, rel), "r", encoding="utf-8") as f:
                out = fn(f.read())
        except OSError:
            out = None
        if out is None:
            log.warning("caelestia extras: %s did not match, it stays as installed", rel)
        else:
            patched[rel] = out
    if not patched:
        return False, "This version of Caelestia's files did not match, so the extras can not be added."
    tmp = MIRROR + ".lum-new"
    shutil.rmtree(tmp, ignore_errors=True)
    try:
        # real folders only on the way down to a patched file, links for everything else
        dirs = set()
        for rel in patched:
            parts = rel.split("/")[:-1]
            for i in range(len(parts) + 1):
                dirs.add("/".join(parts[:i]))

        def fill(rel_dir: str):
            dst = os.path.join(tmp, rel_dir) if rel_dir else tmp
            os.makedirs(dst, exist_ok=True)
            src = os.path.join(SYSTEM_QS, rel_dir) if rel_dir else SYSTEM_QS
            for name in os.listdir(src):
                rel = f"{rel_dir}/{name}" if rel_dir else name
                if rel in patched:
                    with open(os.path.join(dst, name), "w", encoding="utf-8") as f:
                        f.write(patched[rel])
                elif rel in dirs:
                    fill(rel)
                else:
                    os.symlink(os.path.join(src, name), os.path.join(dst, name))

        fill("")
        with open(os.path.join(tmp, MARK), "w", encoding="utf-8") as f:
            f.write("Made by Luminescent (Caelestia extras). Links to /etc/xdg/quickshell/caelestia plus a patched "
                    "copy of:\n" + "\n".join(patched) + "\nDelete this folder (or turn the extras off in "
                    "Luminescent) to run Caelestia as installed.\n")
        if mirror_state() == "on":
            shutil.rmtree(MIRROR)
        os.makedirs(os.path.dirname(MIRROR), exist_ok=True)
        os.replace(tmp, MIRROR)
    except OSError as e:
        shutil.rmtree(tmp, ignore_errors=True)
        return False, f"Could not build {MIRROR}: {e}"
    left = [rel for rel in PATCHES if rel not in patched]
    return True, ("" if not left else "Not added (Caelestia's file changed): " + ", ".join(left))


def remove_mirror() -> str:
    if mirror_state() != "on":
        return ""
    try:
        shutil.rmtree(MIRROR)
    except OSError as e:
        return f"Could not remove {MIRROR}: {e}"
    return ""


def mirror_outdated() -> bool:
    """True when Caelestia was updated since the mirror was built (a file was added or removed up top, or a
    patched file changed), so it should be built again."""
    if mirror_state() != "on":
        return False
    try:
        mark = os.path.getmtime(os.path.join(MIRROR, MARK))
        if any(os.path.getmtime(os.path.join(SYSTEM_QS, rel)) > mark for rel in PATCHES
               if os.path.exists(os.path.join(SYSTEM_QS, rel))):
            return True
        return sorted(os.listdir(SYSTEM_QS)) != sorted(n for n in os.listdir(MIRROR) if n != MARK)
    except OSError:
        return True


def restart_shell() -> str:
    """Caelestia picks up a new (or no) mirror when it starts. '' or an error."""
    exe = shutil.which("caelestia")
    if not exe:
        return "The caelestia command was not found."
    # "caelestia shell -r" only stops the shells started from the folder "-c caelestia" points at now, and the
    # mirror changes that folder: the old shell kept running next to the new one and every gap was doubled.
    # so every caelestia shell is stopped here, whichever folder it runs from, before one is started again.
    def go():
        stop_all_shells()
        try:
            subprocess.Popen([exe, "shell", "-d"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                             start_new_session=True)
        except OSError as e:
            log.warning("could not start Caelestia again: %s", e)
    # stopping can take a few seconds, so not on the screen's thread
    threading.Thread(target=go, daemon=True).start()
    return ""


def shell_pids() -> list[int]:
    """Every running Caelestia shell (quickshell instances whose config is a caelestia folder)."""
    if not shutil.which("qs"):
        return []
    try:
        out = subprocess.run(["qs", "list", "--all", "-j"], capture_output=True, text=True, timeout=5).stdout
        items = json.loads(out) if out.strip().startswith("[") else []
    except (OSError, subprocess.SubprocessError, ValueError):
        return []
    return [int(i["pid"]) for i in items if isinstance(i, dict) and isinstance(i.get("pid"), int)
            and os.path.basename(os.path.dirname(str(i.get("config_path", "")))) == "caelestia"]


def stop_all_shells(wait: float = 5.0):
    """Ask every Caelestia shell to quit and wait until they have (a stuck one is ended after `wait` seconds)."""
    pids = shell_pids()
    for pid in pids:
        subprocess.run(["qs", "kill", "--pid", str(pid)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                       timeout=5)
    end = time.monotonic() + wait
    while pids and time.monotonic() < end:
        time.sleep(0.1)
        pids = [p for p in pids if os.path.exists(f"/proc/{p}")]
    for pid in pids:
        try:
            os.kill(pid, signal.SIGKILL)
        except OSError:
            pass


def lock_now() -> str:
    """Caelestia's lock screen, the one it shows after a while away."""
    exe = shutil.which("caelestia")
    if not exe:
        return "The caelestia command was not found."
    try:
        subprocess.Popen([exe, "shell", "lock", "lock"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                         start_new_session=True)
    except OSError as e:
        return str(e)
    return ""
