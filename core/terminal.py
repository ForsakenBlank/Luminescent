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
"""
import os
import shutil
import logging
import subprocess
from datetime import datetime

from core import theme, APP_NAME

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
