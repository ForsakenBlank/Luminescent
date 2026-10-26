"""
Terminal integration.

Scripts, "Open a terminal" and the Prompt Log open in your terminal: the one set
in Settings → Dashboard & Scripts, else $TERMINAL, else the first of TERMINALS
that is installed.  With the Luminescent colours on, kitty, alacritty and foot
get the theme's colours as command-line overrides, so no terminal config file is
ever written or edited.
"""
import os
import shutil
import logging

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
