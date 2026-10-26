"""
Terminal integration.

Scripts, "Open a terminal" and the Prompt Log open in your terminal: the one set
in Settings → Dashboard & Scripts, else $TERMINAL, else the first of TERMINALS
that is installed.  With the Luminescent colours on, kitty, alacritty and foot
get the theme's colours as command-line overrides, so no terminal config file is
written or edited for that.

Optionally (Settings → Overlay & Interface, it asks first) your own kitty windows
follow the theme too: kitty.conf gets one include line (a backup is kept) and
luminescent-theme.conf next to it is rewritten on every theme change, then kitty
is told to reload its config.

Windows: Windows Terminal (wt) runs scripts in a new tab, else a console window. The colours are a
Terminal *fragment* (a JSON file Terminal merges in at start-up, so your own settings.json is never
edited) with a "Luminescent" PowerShell profile; the "your own terminal" option makes the fragment also
give your PowerShell and Command Prompt profiles the Luminescent scheme.
"""
import os
import shutil
import logging
import subprocess
from datetime import datetime

from core import IS_WIN, theme, save_json, APP_NAME

log = logging.getLogger("terminal")

TERMINALS = ["kitty", "alacritty", "foot", "wezterm", "ghostty", "konsole", "gnome-terminal", "xfce4-terminal",
             "xterm"]
_colours_on = True
_preferred = ""


def set_preferred(name: str):
    """The terminal chosen in Settings → Dashboard & Scripts ('' = automatic)."""
    global _preferred
    _preferred = (name or "").strip()


def find(configured: str = "") -> str:
    """The terminal program to use ('' if none is installed)."""
    for cand in (configured, _preferred, os.environ.get("TERMINAL", ""), *TERMINALS):
        cand = (cand or "").strip()
        if cand and shutil.which(cand.split()[0]):
            return cand.split()[0]
    return ""


def _mix(a: str, b: str, t: float) -> str:
    a, b = a.lstrip("#"), b.lstrip("#")
    ca = [int(a[i:i + 2], 16) for i in (0, 2, 4)]
    cb = [int(b[i:i + 2], 16) for i in (0, 2, 4)]
    return "#" + "".join(f"{round(x + (y - x) * t):02X}" for x, y in zip(ca, cb))


def scheme() -> dict:
    p = theme.P
    light = theme.is_light()
    # readable ANSI colours for the background, tinted slightly towards the accent
    base = (["#B42318", "#2E7D32", "#9A6700", "#1F5FBF", "#7A3EB1", "#0F7B8A"] if light else
            ["#F07178", "#A6E3A1", "#F9E2AF", "#89B4FA", "#CBA6F7", "#94E2D5"])
    names = ["red", "green", "yellow", "blue", "purple", "cyan"]
    s = {"name": APP_NAME, "background": p.bg_base, "foreground": p.text_primary, "cursorColor": p.primary,
         "selectionBackground": p.primary_muted,
         "black": p.bg_card if not light else "#1F1D18", "white": p.text_secondary if not light else "#E8E6DF",
         "brightBlack": p.text_muted, "brightWhite": p.text_primary if not light else "#FFFFFF"}
    for n, c in zip(names, base):
        s[n] = _mix(c, p.primary, 0.12)
        s["bright" + n[0].upper() + n[1:]] = _mix(c, "#000000" if light else "#FFFFFF", 0.18)
    s["purple"] = p.primary
    return s


def write_fragment(icon_png: str = "") -> str:
    """Turn the Luminescent colours on (they are passed on each terminal's command line)."""
    global _colours_on
    _colours_on = True
    return ""


def remove_fragment():
    global _colours_on
    _colours_on = False


def has_scheme() -> bool:
    return _colours_on


_ANSI = ["black", "red", "green", "yellow", "blue", "purple", "cyan", "white"]


def _ansi(s: dict) -> list[str]:
    """The 16 ANSI colours in terminal order."""
    bright = ["bright" + n[0].upper() + n[1:] for n in _ANSI]
    return [s[n] for n in _ANSI] + [s[n] for n in bright]


def colour_args(term: str) -> list[str]:
    if not _colours_on:
        return []
    try:
        s = scheme()
    except Exception:
        return []
    cols = _ansi(s)
    if term == "kitty":
        opts = {"background": s["background"], "foreground": s["foreground"], "cursor": s["cursorColor"],
                "selection_background": s["selectionBackground"]}
        opts.update({f"color{i}": c for i, c in enumerate(cols)})
        return [a for k, v in opts.items() for a in ("-o", f"{k}={v}")]
    if term == "alacritty":
        opts = {"colors.primary.background": s["background"], "colors.primary.foreground": s["foreground"],
                "colors.cursor.cursor": s["cursorColor"], "colors.selection.background": s["selectionBackground"]}
        names = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"]
        for i, n in enumerate(names):
            opts[f"colors.normal.{n}"] = cols[i]
            opts[f"colors.bright.{n}"] = cols[i + 8]
        return [a for k, v in opts.items() for a in ("-o", f'{k}="{v}"')]
    if term == "foot":
        opts = {"colors.background": s["background"], "colors.foreground": s["foreground"]}
        opts.update({f"colors.regular{i}": c for i, c in enumerate(cols[:8])})
        opts.update({f"colors.bright{i}": c for i, c in enumerate(cols[8:])})
        return [a for k, v in opts.items() for a in ("-o", f"{k}={v.lstrip('#')}")]
    return []


def command(title: str, cwd: str, argv: list[str], configured: str = "") -> list[str]:
    """The command line that opens a new terminal window running `argv` in `cwd` ([] = no terminal found)."""
    term = find(configured)
    if not term:
        return []
    cwd = cwd or os.path.expanduser("~")
    name = os.path.basename(term)
    col = colour_args(name)
    if name == "kitty":
        return [term, "--title", title, "--directory", cwd, *col, *argv]
    if name == "alacritty":
        return [term, "--title", title, "--working-directory", cwd, *col, "-e", *argv]
    if name == "foot":
        return [term, f"--title={title}", f"--working-directory={cwd}", *col, *argv]
    if name == "wezterm":
        return [term, "start", "--cwd", cwd, "--", *argv]
    if name == "ghostty":
        return [term, f"--title={title}", f"--working-directory={cwd}", "-e", *argv]
    if name == "konsole":
        return [term, "--workdir", cwd, "-p", f"tabtitle={title}", "-e", *argv]
    if name == "gnome-terminal":
        return [term, "--title", title, f"--working-directory={cwd}", "--", *argv]
    if name == "xfce4-terminal":
        return [term, "--title", title, "--working-directory", cwd, "-x", *argv]
    return [term, "-T", title, "-e", *argv]


# ── your own kitty windows ────────────────────────────────────────────────────
KITTY_DIR = os.path.join(os.environ.get("XDG_CONFIG_HOME") or os.path.expanduser("~/.config"), "kitty")
KITTY_CONF = os.path.join(KITTY_DIR, "kitty.conf")
KITTY_THEME = os.path.join(KITTY_DIR, "luminescent-theme.conf")
_INCLUDE = "include luminescent-theme.conf"
_MARK = "# added by Luminescent: its theme colours (Settings > Overlay & Interface)"


def kitty_installed() -> bool:
    return bool(shutil.which("kitty"))


def kitty_linked() -> bool:
    try:
        with open(KITTY_CONF, "r", encoding="utf-8") as f:
            return any(line.strip() == _INCLUDE for line in f)
    except OSError:
        return False


def write_kitty_theme() -> str:
    """Write the theme's colours to luminescent-theme.conf and make running kittys reload. '' or an error."""
    try:
        s = scheme()
        lines = [f"# {APP_NAME} theme colours, rewritten whenever the theme changes. Do not edit.",
                 f"background {s['background']}", f"foreground {s['foreground']}",
                 f"cursor {s['cursorColor']}", f"selection_background {s['selectionBackground']}",
                 f"selection_foreground {s['foreground']}", f"active_border_color {theme.P.primary}",
                 f"inactive_border_color {theme.P.border}", f"url_color {theme.P.primary_text}"]
        lines += [f"color{i} {c}" for i, c in enumerate(_ansi(s))]
        os.makedirs(KITTY_DIR, exist_ok=True)
        with open(KITTY_THEME, "w", encoding="utf-8") as f:
            f.write("\n".join(lines) + "\n")
    except OSError as e:
        return str(e)
    if shutil.which("pkill"):
        subprocess.run(["pkill", "-USR1", "-x", "kitty"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return ""


def link_kitty(backup_dir: str) -> str:
    """Add the include line to kitty.conf (a copy of it goes to `backup_dir` first). '' or an error."""
    if kitty_linked():
        return write_kitty_theme()
    try:
        os.makedirs(KITTY_DIR, exist_ok=True)
        if os.path.exists(KITTY_CONF):
            os.makedirs(backup_dir, exist_ok=True)
            shutil.copy2(KITTY_CONF, os.path.join(backup_dir, f"kitty.conf.{datetime.now():%Y%m%d-%H%M%S}"))
            with open(KITTY_CONF, "r", encoding="utf-8") as f:
                text = f.read()
        else:
            text = ""
        # at the end, so the theme wins over colours set earlier in the file
        text = text.rstrip("\n") + ("\n\n" if text.strip() else "") + f"{_MARK}\n{_INCLUDE}\n"
        with open(KITTY_CONF, "w", encoding="utf-8") as f:
            f.write(text)
    except OSError as e:
        return str(e)
    return write_kitty_theme()


def unlink_kitty() -> str:
    """Take the include line out again (luminescent-theme.conf is removed too)."""
    try:
        with open(KITTY_CONF, "r", encoding="utf-8") as f:
            lines = f.read().split("\n")
        keep = [ln for ln in lines if ln.strip() not in (_INCLUDE, _MARK)]
        while len(keep) > 1 and keep[-1] == "" and keep[-2] == "":
            keep.pop()
        with open(KITTY_CONF, "w", encoding="utf-8") as f:
            f.write("\n".join(keep))
        if os.path.exists(KITTY_THEME):
            os.remove(KITTY_THEME)
    except OSError as e:
        return str(e)
    if shutil.which("pkill"):
        subprocess.run(["pkill", "-USR1", "-x", "kitty"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return ""


# ── Windows ───────────────────────────────────────────────────────────────────
if IS_WIN:
    FRAGMENT_DIR = os.path.join(os.environ.get("LOCALAPPDATA", ""), "Microsoft", "Windows Terminal", "Fragments",
                                APP_NAME)
    FRAGMENT = os.path.join(FRAGMENT_DIR, "luminescent.json")
    # the profiles Windows Terminal makes itself: Windows PowerShell, Command Prompt, PowerShell 7
    _DEFAULT_PROFILES = ["{61c54bbd-c2c6-5271-96e7-009a87ff44bf}", "{0caa0dad-35be-5f56-a8ff-afceeeaa6101}",
                         "{574e775e-4f2a-5b96-ac1e-a2962a402336}"]
    _icon_png = ""
    _own = False

    def wt_exe() -> str:
        return shutil.which("wt") or ""

    def find(configured: str = "") -> str:
        for cand in (configured, _preferred):
            cand = (cand or "").strip()
            if cand and shutil.which(cand.split()[0]):
                return cand.split()[0]
        return wt_exe() or "powershell.exe"

    def _write():
        if not os.environ.get("LOCALAPPDATA"):
            return "LOCALAPPDATA is not set"
        try:
            os.makedirs(FRAGMENT_DIR, exist_ok=True)
            profile = {"name": APP_NAME, "commandline": "powershell.exe -NoLogo", "colorScheme": APP_NAME,
                       "startingDirectory": "%USERPROFILE%"}
            if _icon_png and os.path.isfile(_icon_png):
                dst = os.path.join(FRAGMENT_DIR, "luminescent.png")
                shutil.copyfile(_icon_png, dst)
                profile["icon"] = dst
            s = scheme()
            s["name"] = APP_NAME
            profiles = [profile] + ([{"updates": g, "colorScheme": APP_NAME} for g in _DEFAULT_PROFILES] if _own else [])
            save_json(FRAGMENT, {"profiles": profiles, "schemes": [s]}, indent=2)
            return ""
        except OSError as e:
            log.warning("could not write the Terminal fragment: %s", e)
            return str(e)

    def write_fragment(icon_png: str = "") -> str:
        global _colours_on, _icon_png
        _colours_on, _icon_png = True, icon_png or _icon_png
        return _write()

    def remove_fragment():
        global _colours_on
        _colours_on = False
        if not _own:
            shutil.rmtree(FRAGMENT_DIR, ignore_errors=True)

    def has_scheme() -> bool:
        return _colours_on and os.path.isfile(FRAGMENT)

    def command(title: str, cwd: str, argv: list[str], configured: str = "") -> list[str]:
        """A new Windows Terminal tab running `argv` (or `argv` itself, which the runner gives a console)."""
        term = find(configured)
        cwd = cwd or os.path.expanduser("~")
        if os.path.basename(term).lower() in ("wt", "wt.exe"):
            esc = [a.replace(";", "\\;") for a in argv]          # ';' separates wt sub-commands
            args = [term, "-w", "0", "new-tab", "--title", title.replace(";", "\\;")]
            if has_scheme():
                args += ["--colorScheme", APP_NAME]
            return args + ["-d", cwd] + esc
        return list(argv)

    # "your own terminal follows the theme": the fragment also updates your built-in profiles
    def own_installed() -> bool:
        return bool(wt_exe())

    def own_linked() -> bool:
        try:
            with open(FRAGMENT, "r", encoding="utf-8") as f:
                return '"updates"' in f.read()
        except OSError:
            return False

    def link_own(backup_dir: str = "") -> str:
        global _own
        _own = True
        return _write()

    def unlink_own() -> str:
        global _own
        _own = False
        return _write() if _colours_on else (shutil.rmtree(FRAGMENT_DIR, ignore_errors=True) or "")

    def write_own() -> str:
        global _own
        _own = True
        return _write()
else:
    own_installed, own_linked, link_own, unlink_own, write_own = (kitty_installed, kitty_linked, link_kitty,
                                                                  unlink_kitty, write_kitty_theme)
OWN_NAME = "Windows Terminal" if IS_WIN else "kitty"
