"""
The Caelestia shell in the theme's colours.

Caelestia keeps its colour scheme in ~/.local/state/caelestia/scheme.json and watches that file, so the
bar, launcher, dashboard and the rest change colour the moment it is rewritten:

    {"name": "catppuccin", "flavour": "mocha", "mode": "dark", "variant": "tonalspot",
     "colours": {"primary": "cba6f7", "secondary": "...", "surface": "...", ..., "term0": "...", ...}}

Colours have no "#", keys missing from it keep the shell's defaults, and only mode == "light" makes it
light. write_scheme() fills in a full Material palette from a Luminescent theme: the shell's primary and
secondary colours (the theme's accent and a matching second colour unless the theme picks its own), its
surfaces from the theme's backgrounds and its text from the theme's text. name, flavour and variant are
kept, so Caelestia's own `caelestia scheme set ...` keeps working and puts its colours back.

The first time, Caelestia's own scheme.json is copied to data/caelestia-backups/scheme-before-luminescent.json,
which restore() puts back.

apply_to_apps() hands the same colours to the Caelestia CLI's own theming (GTK, terminals, Hyprland borders,
Discord, btop... whatever ~/.config/caelestia/cli.json allows), by running the CLI's Python.

Shell look (shell.json → appearance): transparency, rounding, spacing, padding, font size and animation
speed, written by set_appearance() with a backup first.
"""
import os
import json
import shutil
import logging
import subprocess
import threading
import colorsys
from datetime import datetime

from core import DATA_DIR, IS_WIN, theme, save_json

log = logging.getLogger("caelestia")

CONFIG_DIR = os.path.join(os.environ.get("XDG_CONFIG_HOME") or os.path.expanduser("~/.config"), "caelestia")
STATE_DIR = os.path.join(os.environ.get("XDG_STATE_HOME") or os.path.expanduser("~/.local/state"), "caelestia")
SCHEME = os.path.join(STATE_DIR, "scheme.json")
SHELL_JSON = os.path.join(CONFIG_DIR, "shell.json")
BACKUPS = os.path.join(DATA_DIR, "caelestia-backups")
ORIGINAL = os.path.join(BACKUPS, "scheme-before-luminescent.json")

# shell.json → appearance: (settings key, path in shell.json, kind)
APPEARANCE = [
    ("shell_transparency", ("transparency", "enabled"), "bool"),
    ("shell_transparency_base", ("transparency", "base"), "float"),
    ("shell_transparency_layers", ("transparency", "layers"), "float"),
    ("shell_rounding", ("rounding", "scale"), "float"),
    ("shell_spacing", ("spacing", "scale"), "float"),
    ("shell_padding", ("padding", "scale"), "float"),
    ("shell_font_scale", ("font", "scale"), "float"),
    ("shell_anim_scale", ("anim", "durations", "scale"), "float"),
]

_last_written: dict = {}


def installed() -> bool:
    """Caelestia is (or was) set up here: its config or state folder, or its command."""
    return not IS_WIN and (os.path.isdir(STATE_DIR) or os.path.isdir(CONFIG_DIR) or bool(shutil.which("caelestia")))


# ── colours ───────────────────────────────────────────────────────────────────
def _hsl(c: str) -> tuple[float, float, float]:
    h = c.lstrip("#")
    r, g, b = (int(h[i:i + 2], 16) / 255 for i in (0, 2, 4))
    return colorsys.rgb_to_hls(r, g, b)


def _from_hsl(h: float, l: float, s: float) -> str:
    r, g, b = colorsys.hls_to_rgb(h % 1.0, max(0.0, min(1.0, l)), max(0.0, min(1.0, s)))
    return "#" + "".join(f"{round(v * 255):02X}" for v in (r, g, b))


def auto_secondary(primary: str) -> str:
    """A second colour that goes with `primary`: the hue a little further round, a touch softer."""
    try:
        h, l, s = _hsl(primary)
    except (ValueError, IndexError):
        return primary
    return _from_hsl(h + 0.09, l, s * 0.85)


def auto_tertiary(primary: str) -> str:
    try:
        h, l, s = _hsl(primary)
    except (ValueError, IndexError):
        return primary
    return _from_hsl(h - 0.12, l, s * 0.8)


def shell_colours(t: dict) -> tuple[str, str]:
    """The shell's (primary, secondary) for theme `t`: its own picks, else the accent and a matching colour."""
    c = t.get("colours") or {}
    own = t.get("caelestia") or {}
    primary = own.get("primary") or c.get("primary") or "#7B6CF6"
    secondary = own.get("secondary") or auto_secondary(primary)
    return primary.upper(), secondary.upper()


def _on(c: str) -> str:
    return "#1A1A1A" if theme.luminance(c) > 150 else "#FFFFFF"


def _roles(name: str, c: str, bg: str, text: str, light: bool) -> dict:
    """primary / onPrimary / primaryContainer ... for one key colour."""
    cap = name[0].upper() + name[1:]
    return {
        name: c, f"on{cap}": _on(c),
        f"{name}Container": theme.mix(c, bg, 0.72 if light else 0.68),
        f"on{cap}Container": theme.mix(c, text, 0.55) if light else theme.mix(c, "#FFFFFF", 0.55),
        f"{name}Fixed": theme.mix(c, "#FFFFFF", 0.7), f"{name}FixedDim": theme.mix(c, "#FFFFFF", 0.45),
        f"on{cap}Fixed": theme.mix(c, "#000000", 0.8), f"on{cap}FixedVariant": theme.mix(c, "#000000", 0.55),
        f"{name}_paletteKeyColor": c,
    }


def palette(t: dict) -> tuple[str, dict]:
    """("dark" or "light", {Caelestia colour name: "RRGGBB"}) for theme `t`."""
    c = {**theme.PRESETS["dark"], **(t.get("colours") or {})}
    bg, card, hover = c["bg_base"], c["bg_card"], c["bg_card_hover"]
    text = c["text_primary"]
    light = theme.luminance(bg) > 140
    primary, secondary = shell_colours(t)
    tertiary = (t.get("caelestia") or {}).get("tertiary") or auto_tertiary(primary)
    out = {
        "background": bg, "onBackground": text, "surface": bg,
        "surfaceDim": theme.mix(bg, "#000000", 0.06 if light else 0.25),
        "surfaceBright": theme.mix(bg, "#FFFFFF", 0.5 if light else 0.08),
        "surfaceContainerLowest": c["bg_frame"], "surfaceContainerLow": theme.mix(bg, card, 0.5),
        "surfaceContainer": card, "surfaceContainerHigh": hover,
        "surfaceContainerHighest": theme.mix(hover, text, 0.06),
        "onSurface": text, "surfaceVariant": hover, "onSurfaceVariant": c["text_secondary"],
        "inverseSurface": text, "inverseOnSurface": bg,
        "outline": c["text_muted"], "outlineVariant": c["border"],
        "shadow": "#000000", "scrim": "#000000", "surfaceTint": primary,
        "inversePrimary": theme.mix(primary, "#000000" if not light else "#FFFFFF", 0.4),
        "neutral_paletteKeyColor": card, "neutral_variant_paletteKeyColor": c["border"],
        "error": "#B3261E" if light else "#F2B8B5", "onError": "#FFFFFF" if light else "#601410",
        "errorContainer": "#F9DEDC" if light else "#8C1D18", "onErrorContainer": "#410E0B" if light else "#F9DEDC",
        "success": "#2E7D32" if light else "#A6E3A1", "onSuccess": "#FFFFFF" if light else "#0B3B12",
        "successContainer": "#C8E6C9" if light else "#1E4D25", "onSuccessContainer": "#0B3B12" if light else "#C8E6C9",
    }
    for name, col in (("primary", primary), ("secondary", secondary), ("tertiary", tertiary)):
        out.update(_roles(name, col, bg, text, light))
    try:
        from core import terminal
        old = theme.P
        theme.use({**c, **{k: t.get(k) for k in ("radius",) if k in t}})
        try:
            ansi = terminal._ansi(terminal.scheme())
        finally:
            theme.P = old
        out.update({f"term{i}": col for i, col in enumerate(ansi)})
    except Exception:
        log.exception("terminal colours for Caelestia")
    return ("light" if light else "dark"), {k: v.lstrip("#").lower() for k, v in out.items() if isinstance(v, str)}


def read_scheme() -> dict:
    try:
        with open(SCHEME, "r", encoding="utf-8") as f:
            d = json.load(f)
        return d if isinstance(d, dict) else {}
    except (OSError, ValueError):
        return {}


def write_scheme(t: dict) -> str:
    """Put theme `t`'s colours into Caelestia's scheme.json. '' or what went wrong."""
    if IS_WIN:
        return "Caelestia is a Linux shell."
    mode, cols = palette(t)
    old = read_scheme()
    try:
        os.makedirs(STATE_DIR, exist_ok=True)
        if not os.path.exists(ORIGINAL) and os.path.exists(SCHEME):
            os.makedirs(BACKUPS, exist_ok=True)
            shutil.copy2(SCHEME, ORIGINAL)
        # a scheme the CLI knows, so `caelestia scheme set -m light` and the like still work afterwards
        d = {"name": old.get("name") or "caelestia", "flavour": old.get("flavour") or "default",
             "mode": mode, "variant": old.get("variant") or "tonalspot", "colours": cols}
        save_json(SCHEME, d)
    except OSError as e:
        return str(e)
    _last_written.clear()
    _last_written.update(cols)
    log.info("Caelestia colours from the theme %s (%s)", t.get("name"), mode)
    return ""


def changed_by_someone_else() -> bool:
    """True when scheme.json no longer holds the colours write_scheme() put there."""
    return bool(_last_written) and (read_scheme().get("colours") or {}) != _last_written


def restore() -> str:
    """Put Caelestia's own scheme back (the copy made before Luminescent first changed it)."""
    if not os.path.exists(ORIGINAL):
        return "There is no copy of Caelestia's own colours (Luminescent never changed them)."
    try:
        shutil.copy2(ORIGINAL, SCHEME)
    except OSError as e:
        return str(e)
    _last_written.clear()
    return ""


def has_original() -> bool:
    return os.path.exists(ORIGINAL)


def _cli_python() -> str:
    """The Python the caelestia command runs with ('' when it is not a Python script)."""
    exe = shutil.which("caelestia")
    if not exe:
        return ""
    try:
        with open(exe, "rb") as f:
            first = f.readline(300).decode("utf-8", "replace").strip()
    except OSError:
        return ""
    if not first.startswith("#!"):
        return ""
    parts = first[2:].split()
    if not parts:
        return ""
    if os.path.basename(parts[0]) == "env" and len(parts) > 1:
        return shutil.which(parts[1]) or ""
    return parts[0] if "python" in os.path.basename(parts[0]) else ""


def apply_to_apps(t: dict, done=None):
    """Let the Caelestia CLI colour the other apps (GTK, terminals, Hyprland, Discord...) in the background.
    done(error or '') is called from the worker thread."""
    py = _cli_python()
    if not py:
        if done:
            done("The caelestia command was not found (or is not the Python one).")
        return
    mode, cols = palette(t)
    code = ("import json, sys\n"
            "from caelestia.utils.theme import apply_colours\n"
            "d = json.load(sys.stdin)\n"
            "apply_colours(d['colours'], d['mode'])\n")

    def work():
        try:
            r = subprocess.run([py, "-c", code], input=json.dumps({"colours": cols, "mode": mode}), text=True,
                               capture_output=True, timeout=60)
            err = (r.stderr.strip().splitlines() or [""])[-1] if r.returncode else ""
        except (OSError, subprocess.SubprocessError) as e:
            err = str(e)
        if err:
            log.warning("caelestia apply_colours: %s", err)
        if done:
            done(err)
    threading.Thread(target=work, name="caelestia-apps", daemon=True).start()


# ── shell.json appearance ─────────────────────────────────────────────────────
def read_shell() -> dict:
    try:
        with open(SHELL_JSON, "r", encoding="utf-8") as f:
            d = json.load(f)
        return d if isinstance(d, dict) else {}
    except (OSError, ValueError):
        return {}


def read_appearance() -> dict:
    """{settings key: value} for the appearance values shell.json sets now."""
    app = read_shell().get("appearance") or {}
    out = {}
    for key, path, _kind in APPEARANCE:
        node = app
        for p in path:
            node = node.get(p) if isinstance(node, dict) else None
        if node is not None:
            out[key] = node
    return out


def set_appearance(values: dict) -> str:
    """Write {settings key: value} into shell.json → appearance (a copy of the old file goes to the backups).
    Only changes the file when a value differs. '' or what went wrong."""
    if IS_WIN:
        return ""
    d = read_shell()
    app = d.setdefault("appearance", {})
    if not isinstance(app, dict):
        return "shell.json's appearance is not an object; edit it in Caelestia & Glass → Shell files."
    changed = False
    for key, path, kind in APPEARANCE:
        if key not in values:
            continue
        v = bool(values[key]) if kind == "bool" else round(float(values[key]), 3)
        node = app
        for p in path[:-1]:
            if not isinstance(node.get(p), dict):
                node[p] = {}
            node = node[p]
        if node.get(path[-1]) != v:
            node[path[-1]] = v
            changed = True
    if not changed:
        return ""
    try:
        os.makedirs(CONFIG_DIR, exist_ok=True)
        if os.path.exists(SHELL_JSON):
            os.makedirs(BACKUPS, exist_ok=True)
            shutil.copy2(SHELL_JSON, os.path.join(BACKUPS, f"{datetime.now():%Y%m%d-%H%M%S}-shell.json"))
        with open(SHELL_JSON + ".tmp", "w", encoding="utf-8") as f:
            f.write(json.dumps(d, indent=4) + "\n")
        os.replace(SHELL_JSON + ".tmp", SHELL_JSON)
    except OSError as e:
        return str(e)
    return ""
