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


# what the active theme sets for the terminal itself (see ui/theme_extras.py for every key); set_theme() is
# called whenever a theme is applied
_theme: dict = {}
NAMES = ["background", "foreground", "cursorColor", "selectionBackground"] + \
    [n for c in ["black", "red", "green", "yellow", "blue", "purple", "cyan", "white"]
     for n in (c, "bright" + c[0].upper() + c[1:])] + ["colour16", "colour17", "colour18"]
# where the picture goes, and what happens in a narrow terminal
LOGO_POSITIONS = ("auto", "left", "top", "right")
NARROW = ("hide", "top", "text")
# the info lines Luminescent's own info box can show: (fastfetch module, label)
INFO_MODULES = [("os", "distro"), ("kernel", "kernel"), ("uptime", "uptime"), ("shell", "shell"),
                ("packages", "pkgs"), ("memory", "memory"), ("cpu", "cpu"), ("gpu", "gpu"), ("wm", "wm"),
                ("terminal", "terminal"), ("disk", "disk"), ("battery", "battery"), ("host", "host"),
                ("title", "user"), ("localip", "ip"), ("datetime", "time"), ("colors", "")]
DEFAULT_INFO = ["os", "kernel", "uptime", "shell", "packages", "memory"]
# colour slots a line or the start-up text can use: the 16 terminal colours and Caelestia's extra three
SLOTS = {"colour16": "38;5;16", "colour17": "38;5;17", "colour18": "38;5;18", "foreground": "39",
         **{n: c for n, c in zip(["black", "red", "green", "yellow", "blue", "purple", "cyan", "white"],
                                 ["30", "31", "32", "33", "34", "35", "36", "37"])},
         **{"bright" + n[0].upper() + n[1:]: c for n, c in
            zip(["black", "red", "green", "yellow", "blue", "purple", "cyan", "white"],
                ["90", "91", "92", "93", "94", "95", "96", "97"])}}


def _num(v, lo, hi, default):
    try:
        return max(lo, min(hi, int(v)))
    except (TypeError, ValueError):
        return default


def set_theme(d: dict | None):
    global _theme
    d = d if isinstance(d, dict) else {}
    cols = {k: v for k, v in (d.get("colours") or {}).items() if k in NAMES and isinstance(v, str)
            and v.startswith("#")}
    out = {"colours": cols} if cols else {}
    if isinstance(d.get("opacity"), (int, float)) and not isinstance(d.get("opacity"), bool):
        out["opacity"] = max(0.1, min(1.0, float(d["opacity"])))
    if isinstance(d.get("logo"), str) and d["logo"].strip():
        out["logo"] = d["logo"].strip()
        out["logo_width"] = _num(d.get("logo_width"), 4, 200, 30)
        out["logo_height"] = _num(d.get("logo_height"), 0, 100, 0)
        out["logo_padding"] = _num(d.get("logo_padding"), 0, 20, 2)
        out["logo_padding_top"] = _num(d.get("logo_padding_top"), 0, 20, 0)
        out["logo_position"] = d.get("logo_position") if d.get("logo_position") in LOGO_POSITIONS else "left"
    if isinstance(d.get("narrow"), dict):
        n = d["narrow"]
        out["narrow"] = {"below": _num(n.get("below"), 20, 300, 90),
                         "then": n.get("then") if n.get("then") in NARROW else "hide"}
    g = d.get("greeting")
    if isinstance(g, dict) and g.get("on"):
        info = g.get("info") if g.get("info") in ("own", "luminescent", "none") else "own"
        lines = []
        for ln in g.get("lines") or []:
            if isinstance(ln, dict) and ln.get("type") in dict(INFO_MODULES):
                lines.append({"type": ln["type"], "label": str(ln.get("label", ""))[:24],
                              "colour": ln.get("colour") if ln.get("colour") in SLOTS else "colour16"})
        out["greeting"] = {"text": str(g.get("text") or "")[:4000],
                           "colour": g.get("colour") if g.get("colour") in SLOTS else "colour16",
                           "info": info, "style": "plain" if g.get("style") == "plain" else "box",
                           "lines": lines or [{"type": t, "label": dict(INFO_MODULES)[t], "colour": "colour16"}
                                              for t in DEFAULT_INFO],
                           "args": str(g.get("args") or "")[:200]}
    _theme = out


def theme_settings() -> dict:
    return dict(_theme)


def scheme() -> dict:
    s = _palette_scheme()
    s.update(_theme.get("colours") or {})
    return s


def _palette_scheme() -> dict:
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
    # Caelestia's extra three (fish greetings and fastfetch boxes often use them)
    s["colour16"], s["colour17"], s["colour18"] = p.primary, p.primary_text, s["cyan"]
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
    return kitty_include_state() == "ok"


def _broken_include(line: str) -> bool:
    """A line that was meant to include luminescent-theme.conf but does not (a typo like ".con")."""
    parts = line.strip().split(None, 1)
    return (len(parts) == 2 and parts[0] == "include" and parts[1].startswith("luminescent-theme")
            and parts[1] != "luminescent-theme.conf")


def kitty_include_state() -> str:
    """"ok" (kitty loads Luminescent's colours), "broken" (an include line with a typo) or "missing"."""
    try:
        with open(KITTY_CONF, "r", encoding="utf-8") as f:
            lines = f.read().split("\n")
    except OSError:
        return "missing"
    if any(ln.strip() == _INCLUDE for ln in lines):
        return "ok"
    return "broken" if any(_broken_include(ln) for ln in lines) else "missing"


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
        if "opacity" in _theme:
            # kitty can only change it on a reload when this was on when it started
            lines += ["dynamic_background_opacity yes", f"background_opacity {_theme['opacity']:.2f}"]
        os.makedirs(KITTY_DIR, exist_ok=True)
        with open(KITTY_THEME, "w", encoding="utf-8") as f:
            f.write("\n".join(lines) + "\n")
    except OSError as e:
        return str(e)
    if shutil.which("pkill"):
        subprocess.run(["pkill", "-USR1", "-x", "kitty"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return ""


# ── colours for terminals that are already open ───────────────────────────────
# Caelestia's fish config prints this file at the start of every shell (`cat ~/.local/state/caelestia/
# sequences.txt`), which sets the 16 colours, 16 to 18, the text, background, cursor and selection: whatever
# kitty.conf says is painted over in every new shell. So with "your own terminal follows the theme" on,
# Luminescent's colours go into that file too (and to every terminal that is open, the way Caelestia does it).
CAEL_SEQUENCES = os.path.join(os.environ.get("XDG_STATE_HOME") or os.path.expanduser("~/.local/state"),
                              "caelestia", "sequences.txt")


def _osc(hex_colour: str, *idx: int) -> str:
    h = hex_colour.lstrip("#")
    return f"\x1b]{';'.join(map(str, idx))};rgb:{h[0:2]}/{h[2:4]}/{h[4:6]}\x1b\\"


def sequences(s: dict | None = None) -> str:
    """The escape codes that give a running terminal these colours."""
    s = s or scheme()
    out = _osc(s["foreground"], 10) + _osc(s["background"], 11) + _osc(s["cursorColor"], 12) + \
        _osc(s["selectionBackground"], 17)
    out += "".join(_osc(c, 4, i) for i, c in enumerate(_ansi(s)))
    out += "".join(_osc(s[k], 4, i) for i, k in ((16, "colour16"), (17, "colour17"), (18, "colour18")) if k in s)
    return out


def fish_loads_caelestia() -> bool:
    """Does fish print Caelestia's colours at the start of every shell?"""
    conf = os.path.join(os.environ.get("XDG_CONFIG_HOME") or os.path.expanduser("~/.config"), "fish",
                        "config.fish")
    try:
        with open(conf, "r", encoding="utf-8") as f:
            return "caelestia/sequences.txt" in f.read()
    except OSError:
        return False


def push_colours(backup_dir: str) -> str:
    """The theme's terminal colours into Caelestia's sequences file (when it is used; a copy of Caelestia's own
    is kept) and into every terminal that is open now. '' or an error."""
    seq = sequences()
    if os.path.isdir(os.path.dirname(CAEL_SEQUENCES)) and (os.path.isfile(CAEL_SEQUENCES) or fish_loads_caelestia()):
        try:
            keep = os.path.join(backup_dir, "caelestia-sequences.txt")
            if os.path.isfile(CAEL_SEQUENCES):
                with open(CAEL_SEQUENCES, "r", encoding="utf-8", errors="replace") as f:
                    now = f.read()
                if now != seq:
                    # caelestia wrote it (a new scheme): that is the one to put back later
                    os.makedirs(backup_dir, exist_ok=True)
                    with open(keep, "w", encoding="utf-8") as f:
                        f.write(now)
            tmp = CAEL_SEQUENCES + ".lum-tmp"
            with open(tmp, "w", encoding="utf-8") as f:
                f.write(seq)
            os.replace(tmp, CAEL_SEQUENCES)
        except OSError as e:
            return str(e)
    send_to_open_terminals(seq)
    return ""


def give_back_colours(backup_dir: str):
    """Caelestia's own terminal colours back (when Luminescent had put the theme's there)."""
    keep = os.path.join(backup_dir, "caelestia-sequences.txt")
    try:
        with open(keep, "r", encoding="utf-8") as f:
            seq = f.read()
        with open(CAEL_SEQUENCES, "w", encoding="utf-8") as f:
            f.write(seq)
        os.remove(keep)
    except OSError:
        return
    send_to_open_terminals(seq)


def send_to_open_terminals(seq: str):
    """Write colour codes to every terminal of this user that is open (only colours, nothing is typed)."""
    if IS_WIN or not os.path.isdir("/dev/pts"):
        return
    uid = os.getuid()
    for name in os.listdir("/dev/pts"):
        if not name.isdigit():
            continue
        path = os.path.join("/dev/pts", name)
        try:
            if os.stat(path).st_uid != uid:
                continue
            fd = os.open(path, os.O_WRONLY | os.O_NONBLOCK | os.O_NOCTTY)
            try:
                os.write(fd, seq.encode())
            finally:
                os.close(fd)
        except OSError:
            continue


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
        lines = text.split("\n")
        if any(_broken_include(ln) for ln in lines):
            # the line is there with a typo: put it right where it is
            text = "\n".join(_INCLUDE if _broken_include(ln) else ln for ln in lines)
        else:
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


# ── the picture fastfetch shows when a terminal opens ─────────────────────────
FASTFETCH_CONF = os.path.join(os.environ.get("XDG_CONFIG_HOME") or os.path.expanduser("~/.config"), "fastfetch",
                              "config.jsonc")


def _value_span(text: str, start: int) -> int:
    """Where the JSON value starting at `start` ends (strings, nested {} and [] understood)."""
    i, depth, n = start, 0, len(text)
    while i < n:
        c = text[i]
        if c == '"':
            i += 1
            while i < n and text[i] != '"':
                i += 2 if text[i] == "\\" else 1
        elif c in "{[":
            depth += 1
        elif c in "}]":
            if depth == 0:
                return i
            depth -= 1
            if depth == 0:
                return i + 1
        elif c in ",\n" and depth == 0:
            return i
        i += 1
    return n


def _top_level_key(text: str, key: str) -> tuple[int, int] | None:
    """(start, end) of the value of a top-level key in a JSON(C) object, None when it is not there."""
    i, depth, n = 0, 0, len(text)
    while i < n:
        c = text[i]
        if c == "/" and text[i:i + 2] == "//":
            i = text.find("\n", i)
            i = n if i < 0 else i
            continue
        if c == "/" and text[i:i + 2] == "/*":
            i = text.find("*/", i)
            i = n if i < 0 else i + 2
            continue
        if c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            if depth == 1 and text[i + 1:j] == key:
                k = j + 1
                while k < n and text[k] in " \t\r\n":
                    k += 1
                if k < n and text[k] == ":":
                    k += 1
                    while k < n and text[k] in " \t\r\n":
                        k += 1
                    return k, _value_span(text, k)
            i = j + 1
            continue
        if c in "{[":
            depth += 1
        elif c in "}]":
            depth -= 1
        i += 1
    return None


def fastfetch_logo() -> str | None:
    """The logo value in fastfetch's config as JSON text, None when the file or the key is not there."""
    try:
        with open(FASTFETCH_CONF, "r", encoding="utf-8") as f:
            text = f.read()
    except OSError:
        return None
    span = _top_level_key(text, "logo")
    return None if span is None else text[span[0]:span[1]].strip()


def set_fastfetch_logo(value: str | None, backup_dir: str) -> str:
    """Put `value` (JSON text) in as fastfetch's logo, or take the key out with None. The rest of the file,
    comments included, is left as it is; a copy goes to backup_dir first. '' or an error."""
    try:
        with open(FASTFETCH_CONF, "r", encoding="utf-8") as f:
            text = f.read()
    except OSError as e:
        return f"fastfetch's config was not found ({e})."
    span = _top_level_key(text, "logo")
    if span is not None:
        if value is None:
            # the key's line goes: from the line break before it to the comma after its value
            k = text.rfind('"logo"', 0, span[0])
            end = span[1]
            if text[end:].lstrip(" \t").startswith(","):
                end = text.index(",", end) + 1
            if k >= 0:
                line = text.rfind("\n", 0, k)
                text = text[:line if line >= 0 else k] + text[end:]
        else:
            text = text[:span[0]] + value + text[span[1]:]
    elif value is not None:
        brace = text.find("{")
        if brace < 0:
            return "fastfetch's config is not a JSON object."
        text = text[:brace + 1] + f'\n    "logo": {value},' + text[brace + 1:]
    else:
        return ""
    try:
        os.makedirs(backup_dir, exist_ok=True)
        shutil.copy2(FASTFETCH_CONF, os.path.join(backup_dir, f"fastfetch.config.{datetime.now():%Y%m%d-%H%M%S}"))
        tmp = FASTFETCH_CONF + ".lum-tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            f.write(text)
        os.replace(tmp, FASTFETCH_CONF)
    except OSError as e:
        return str(e)
    return ""


def logo_value(t: dict | None = None) -> str | None:
    """The theme's start-up picture as fastfetch's "logo" (JSON text), None when the theme has none."""
    t = t if t is not None else _theme
    if not t.get("logo"):
        return None
    import json
    logo = {"type": "auto", "source": os.path.expanduser(t["logo"]), "width": t.get("logo_width", 30)}
    if t.get("logo_height"):
        logo["height"] = t["logo_height"]
    pad = t.get("logo_padding", 2)
    logo["padding"] = {"top": t.get("logo_padding_top", 0), "left": pad, "right": pad}
    if t.get("logo_position", "left") != "left":
        logo["position"] = t["logo_position"]
    return json.dumps(logo)


# ── the start-up text and info box (fish) ─────────────────────────────────────
_CONF = os.environ.get("XDG_CONFIG_HOME") or os.path.expanduser("~/.config")
FISH_DIR = os.path.join(_CONF, "fish")
FISH_GREETING = os.path.join(FISH_DIR, "conf.d", "luminescent-greeting.fish")
OWN_GREETING = os.path.join(FISH_DIR, "functions", "fish_greeting.fish")
LUM_FASTFETCH = os.path.join(_CONF, "fastfetch", "luminescent.jsonc")


def own_greeting() -> tuple[str, str]:
    """(the text your own fish greeting prints, the options it gives fastfetch), read from
    functions/fish_greeting.fish: echo lines only, so it is a starting point, not a copy of the script."""
    import re
    try:
        with open(OWN_GREETING, "r", encoding="utf-8") as f:
            src = f.read()
    except OSError:
        return "", ""
    lines, args = [], ""
    for ln in src.split("\n"):
        s = ln.strip()
        m = re.match(r"^echo\s+(-\w+\s+)?'(.*)'\s*(#.*)?$", s) or re.match(r'^echo\s+(-\w+\s+)?"(.*)"\s*(#.*)?$', s)
        if m and not (m.group(1) and "e" in m.group(1) and m.group(2).startswith("\\x1b")):
            lines.append(m.group(2).replace("\\'", "'"))
        m = re.search(r"\bfastfetch\b([^;&|]*)$", s)
        if m and not s.startswith("command -v") or (m and "&&" in s):
            args = m.group(1).strip() if m else args
    return "\n".join(lines), args


def _fish_str(s: str) -> str:
    return "'" + s.replace("\\", "\\\\").replace("'", "\\'") + "'"


def _fastfetch_config(g: dict) -> str:
    """Luminescent's own info box for fastfetch (JSON with a comment on top)."""
    import json
    lines = g.get("lines") or []
    width = max([len(ln["label"]) for ln in lines] + [4])
    mods: list = ["break"]
    box = g.get("style") == "box"
    edge = SLOTS.get(g.get("colour", "colour16"), "38;5;16")
    if box:
        mods.append({"type": "custom", "key": "╭" + "─" * (width + 30), "keyColor": edge, "format": ""})
    for ln in lines:
        key = ln["label"].ljust(width)
        m = {"type": ln["type"], "key": ("│ " if box else "") + key, "keyColor": SLOTS.get(ln["colour"], "38;5;16")}
        if ln["type"] == "colors":
            m = {"type": "colors", "key": ("│ " if box else "") + " " * width, "symbol": "circle"}
        mods.append(m)
    if box:
        mods.append({"type": "custom", "key": "╰" + "─" * (width + 30), "keyColor": edge, "format": ""})
    mods.append("break")
    conf = {"$schema": "https://github.com/fastfetch-cli/fastfetch/raw/dev/doc/json_schema.json",
            "display": {"separator": "  "}, "modules": mods}
    logo = logo_value()
    if logo is None:
        own = fastfetch_logo()
        if own:
            try:
                conf["logo"] = json.loads(own)
            except ValueError:
                pass
    else:
        conf["logo"] = json.loads(logo)
    return ("// Written by Luminescent from the theme's Terminal page (the info box). It is rewritten when the "
            "theme changes;\n// your own config.jsonc is not used for it.\n" + json.dumps(conf, indent=4, ensure_ascii=False) + "\n")


def greeting_script(t: dict | None = None) -> str:
    """The fish greeting for the theme: its text, then fastfetch, with less (or no picture) when the terminal is
    narrow so nothing wraps."""
    t = t if t is not None else _theme
    g = t.get("greeting") or {}
    narrow = t.get("narrow") or {"below": 90, "then": "hide"}
    text = g.get("text", "")
    text_w = max((len(ln) for ln in text.split("\n")), default=0) if text else 0
    out = ["# Written by Luminescent for the theme's start-up text (theme settings > Terminal). It takes the place",
           "# of your own fish_greeting while it is here; delete this file, or untick the start-up text in the",
           "# theme, to get yours back.",
           "function fish_greeting",
           "    set -l w $COLUMNS"]
    if text:
        out += [f"    if test $w -ge {text_w}",
                f"        printf '\\e[{SLOTS.get(g.get('colour', 'colour16'), '38;5;16')}m'",
                "        printf '%s\\n' " + " ".join(_fish_str(ln) for ln in text.split("\n")),
                "        set_color normal",
                "    end"]
    info = g.get("info", "own")
    if info != "none":
        cfg = f" -c {_fish_str(LUM_FASTFETCH)}" if info == "luminescent" else ""
        args = (" " + g["args"]) if g.get("args") else ""
        run = f"fastfetch{cfg}{args}"
        then = narrow.get("then", "hide")
        out += ["    command -q fastfetch; or return"]
        out += [f"    if test $w -lt {narrow.get('below', 90)}"]
        if then == "hide":
            out += [f"        {run} --logo-type none"]
        elif then == "top":
            lw = t.get("logo_width", 30)
            out += [f"        {run} --logo-position top --logo-width (math \"min($w - 4, {lw})\")"]
        else:
            out += ["        return"]
        out += ["    else", f"        {run}", "    end"]
    out += ["end", ""]
    return "\n".join(out)


def write_greeting(backup_dir: str) -> str:
    """Put the theme's start-up text in place, or take Luminescent's away when the theme has none. ''/error."""
    g = _theme.get("greeting")
    try:
        if not g:
            for p in (FISH_GREETING, LUM_FASTFETCH):
                if os.path.isfile(p):
                    with open(p, "r", encoding="utf-8") as f:
                        if "Luminescent" in f.readline() + f.readline():
                            os.remove(p)
            return ""
        if not os.path.isdir(FISH_DIR):
            return "fish's config folder was not found, so the start-up text was not set."
        if g.get("info") == "luminescent":
            os.makedirs(os.path.dirname(LUM_FASTFETCH), exist_ok=True)
            with open(LUM_FASTFETCH, "w", encoding="utf-8") as f:
                f.write(_fastfetch_config(g))
        os.makedirs(os.path.dirname(FISH_GREETING), exist_ok=True)
        if os.path.isfile(FISH_GREETING):
            with open(FISH_GREETING, "r", encoding="utf-8") as f:
                if "Luminescent" not in f.readline():
                    # somebody else's file by that name: keep a copy
                    os.makedirs(backup_dir, exist_ok=True)
                    shutil.copy2(FISH_GREETING, os.path.join(backup_dir, "luminescent-greeting.fish.yours"))
        with open(FISH_GREETING, "w", encoding="utf-8") as f:
            f.write(greeting_script())
    except OSError as e:
        return str(e)
    return ""
