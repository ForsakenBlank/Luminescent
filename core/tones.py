"""
Colour maths for the theme editor's quick tools: lighter and darker steps that look evenly spaced, whole schemes
made from a few main colours, and bulk changes to a whole set of colours at once.

Colours are worked on in OKLCH (lightness, chroma = how colourful, hue in degrees), which keeps a colour's hue
when it is made lighter or darker. A "tone" is the lightness people see, 0 (black) to 100 (white), the same
scale Material 3 (and so Caelestia) uses for its tonal palettes: tone 98 is a near-white surface, tone 90 a
light container, tone 10 near-black text. So a white theme still gets cards, borders and text a few tones
apart instead of everything being #FFFFFF.

Everything here takes and gives "#RRGGBB" strings and needs nothing but Python (pictures need Qt).
"""
import math
import random

# ── conversions ───────────────────────────────────────────────────────────────


def _rgb(hex_colour: str) -> tuple[float, float, float]:
    h = str(hex_colour).strip().lstrip("#")
    if len(h) == 3:
        h = "".join(c * 2 for c in h)
    if len(h) == 8:                     # #AARRGGBB from Qt: the alpha is not kept
        h = h[2:]
    return int(h[0:2], 16) / 255, int(h[2:4], 16) / 255, int(h[4:6], 16) / 255


def _hex(r: float, g: float, b: float) -> str:
    return "#" + "".join(f"{max(0, min(255, round(v * 255))):02X}" for v in (r, g, b))


def is_hex(v) -> bool:
    try:
        _rgb(v)
        return isinstance(v, str) and v.strip().startswith("#")
    except (ValueError, IndexError):
        return False


def _lin(c: float) -> float:
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def _gam(c: float) -> float:
    return 12.92 * c if c <= 0.0031308 else 1.055 * max(0.0, c) ** (1 / 2.4) - 0.055


def _oklab_of_lin(r: float, g: float, b: float) -> tuple[float, float, float]:
    l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b
    m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b
    s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b
    l, m, s = (math.copysign(abs(x) ** (1 / 3), x) for x in (l, m, s))
    return (0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
            1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
            0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s)


def _lin_of_oklab(L: float, a: float, b: float) -> tuple[float, float, float]:
    l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3
    m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3
    s = (L - 0.0894841775 * a - 1.2914855480 * b) ** 3
    return (4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
            -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
            -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s)


def oklch(hex_colour: str) -> tuple[float, float, float]:
    """(lightness 0..1, chroma 0..~0.37, hue 0..360)."""
    L, a, b = _oklab_of_lin(*(_lin(c) for c in _rgb(hex_colour)))
    return L, math.hypot(a, b), math.degrees(math.atan2(b, a)) % 360


def _in_gamut(rgb) -> bool:
    return all(-0.0005 <= c <= 1.0005 for c in rgb)


def from_oklch(L: float, C: float, h: float) -> str:
    """The colour, with its chroma lowered until it fits on a screen (so the hue and lightness stay)."""
    L = max(0.0, min(1.0, L))
    C = max(0.0, C)
    rad = math.radians(h)
    lin = _lin_of_oklab(L, C * math.cos(rad), C * math.sin(rad))
    if not _in_gamut(lin):
        lo, hi = 0.0, C
        for _ in range(16):
            mid = (lo + hi) / 2
            if _in_gamut(_lin_of_oklab(L, mid * math.cos(rad), mid * math.sin(rad))):
                lo = mid
            else:
                hi = mid
        lin = _lin_of_oklab(L, lo * math.cos(rad), lo * math.sin(rad))
    return _hex(*(_gam(max(0.0, min(1.0, c))) for c in lin))


# ── tones ─────────────────────────────────────────────────────────────────────
def _y_of_tone(t: float) -> float:
    """CIE lightness L* (0..100) to relative luminance Y (0..1)."""
    t = max(0.0, min(100.0, t))
    f = (t + 16) / 116
    return f ** 3 if f ** 3 > 216 / 24389 else (116 * f - 16) / (24389 / 27)


def tone(hex_colour: str) -> float:
    """How light a colour looks, 0 (black) to 100 (white)."""
    r, g, b = (_lin(c) for c in _rgb(hex_colour))
    y = 0.2126 * r + 0.7152 * g + 0.0722 * b
    return 116 * y ** (1 / 3) - 16 if y > 216 / 24389 else y * 24389 / 27


def at_tone(hue: float, chroma: float, t: float) -> str:
    """A colour of this hue and chroma (OKLCH) at tone t. Near black and white there is less room for colour, so
    the chroma is eased off there instead of the hue drifting."""
    t = max(0.0, min(100.0, t))
    room = min(1.0, t / 18, (100 - t) / 10) if 0 < t < 100 else 0.0
    C = chroma * max(0.0, room)
    L = _y_of_tone(t) ** (1 / 3)
    out = from_oklch(L, C, hue)
    for _ in range(3):                       # colourful colours are not exactly as light as greys: nudge
        diff = t - tone(out)
        if abs(diff) < 0.4:
            break
        L = max(0.0, min(1.0, L + diff / 100 * 0.9))
        out = from_oklch(L, C, hue)
    return out


def with_tone(hex_colour: str, t: float) -> str:
    """The same colour (hue and colourfulness) made lighter or darker to tone t."""
    _L, C, h = oklch(hex_colour)
    return at_tone(h, C, t)


def shift_tone(hex_colour: str, by: float) -> str:
    return with_tone(hex_colour, tone(hex_colour) + by)


def is_light(hex_colour: str) -> bool:
    return tone(hex_colour) > 60


def contrast(a: str, b: str) -> float:
    def lum(c):
        r, g, bb = (_lin(x) for x in _rgb(c))
        return 0.2126 * r + 0.7152 * g + 0.0722 * bb
    la, lb = lum(a), lum(b)
    return (max(la, lb) + 0.05) / (min(la, lb) + 0.05)


def readable_on(bg: str, hint: str = "") -> str:
    """Text for on top of `bg`: a very light or very dark shade of `hint` (or of bg), whichever reads better."""
    src = hint or bg
    _L, C, h = oklch(src)
    light, dark = at_tone(h, min(C, 0.03), 98), at_tone(h, min(C, 0.05), 12)
    return light if contrast(light, bg) >= contrast(dark, bg) else dark


def mix(a: str, b: str, t: float) -> str:
    """a blended towards b by t, in OKLab (no muddy middle)."""
    la = _oklab_of_lin(*(_lin(c) for c in _rgb(a)))
    lb = _oklab_of_lin(*(_lin(c) for c in _rgb(b)))
    m = [x + (y - x) * t for x, y in zip(la, lb)]
    return _hex(*(_gam(max(0.0, min(1.0, c))) for c in _lin_of_oklab(*m)))


def hue_diff(a: float, b: float) -> float:
    """b - a the short way round, -180..180."""
    return (b - a + 180) % 360 - 180


def toward_hue(h: float, target: float, amount: float) -> float:
    return (h + hue_diff(h, target) * amount) % 360


# ── hue families ──────────────────────────────────────────────────────────────
# OKLCH hue ranges, so "make every green follow the accent" knows what a green is
FAMILIES = {
    "reds": ("Reds and pinks", [(345, 360), (0, 40)]),
    "oranges": ("Oranges and browns", [(40, 75)]),
    "yellows": ("Yellows", [(75, 115)]),
    "greens": ("Greens", [(115, 175)]),
    "cyans": ("Teals and cyans", [(175, 225)]),
    "blues": ("Blues", [(225, 275)]),
    "purples": ("Purples and violets", [(275, 345)]),
}
GREY_CHROMA = 0.025              # below this a colour counts as grey, whatever its hue


def family(hex_colour: str) -> str:
    """'greys' or one of FAMILIES."""
    _L, C, h = oklch(hex_colour)
    if C < GREY_CHROMA:
        return "greys"
    for key, (_name, spans) in FAMILIES.items():
        if any(lo <= h < hi for lo, hi in spans):
            return key
    return "reds"


def follow_hue(colours: dict, families: set[str], target: str, keep_names=()) -> dict:
    """Every colour in one of `families` takes the hue of `target` (keeping how light and colourful it is).
    "all" stands for every colourful colour."""
    _tl, tc, th = oklch(target)
    out = {}
    for k, v in colours.items():
        if k in keep_names or not is_hex(v):
            out[k] = v
            continue
        fam = family(v)
        if fam != "greys" and ("all" in families or fam in families):
            L, C, _h = oklch(v)
            out[k] = from_oklch(L, max(C, min(tc, 0.06)) if tc > GREY_CHROMA else C * 0.4, th)
        else:
            out[k] = v
    return out


# ── whole-set changes ─────────────────────────────────────────────────────────
def adjust(colours: dict, hue: float = 0.0, brightness: float = 0.0, contrast_: float = 0.0,
           saturation: float = 0.0, warmth: float = 0.0, pivot: float | None = None) -> dict:
    """Every colour at once. hue in degrees; the rest -100..100 (0 = unchanged). Contrast spreads the
    lightnesses away from (or towards) the middle of the set, so backgrounds and text move apart together."""
    vals = {k: v for k, v in colours.items() if is_hex(v)}
    if not vals:
        return dict(colours)
    labs = {k: oklch(v) for k, v in vals.items()}
    if pivot is None:
        ls = sorted(L for L, _c, _h in labs.values())
        pivot = (ls[0] + ls[-1]) / 2
    sat = 1 + saturation / 100 * (1.0 if saturation > 0 else 1.0)
    con = 1 + contrast_ / 100 * (0.8 if contrast_ > 0 else 1.0)
    out = dict(colours)
    for k, (L, C, h) in labs.items():
        L = pivot + (L - pivot) * con + brightness / 100 * 0.35
        C = C * sat
        h = (h + hue) % 360
        if warmth:
            # warmth leans every colour towards orange (or blue when cooler) and gives greys a hint of it
            rad = math.radians(h)
            a, b = C * math.cos(rad), C * math.sin(rad)
            w = warmth / 100
            ta = 55 if w > 0 else 250
            a += abs(w) * 0.035 * math.cos(math.radians(ta))
            b += abs(w) * 0.035 * math.sin(math.radians(ta))
            C, h = math.hypot(a, b), math.degrees(math.atan2(b, a)) % 360
        out[k] = from_oklch(L, C, h)
    return out


def flip(colours: dict) -> dict:
    """Light becomes dark and dark becomes light, every hue kept: a dark scheme from a light one and back."""
    out = dict(colours)
    for k, v in colours.items():
        if is_hex(v):
            _L, C, h = oklch(v)
            t = tone(v)
            # accents keep a little of their brightness so they still stand out after the flip
            new = 100 - t if C < 0.08 else max(30.0, min(85.0, 100 - t + (8 if t < 50 else -8)))
            out[k] = at_tone(h, C, new)
    return out


def spread(values: list[str], light: bool, step: float = 4.0) -> list[str]:
    """Colours that came out the same (a whole scheme of #FFFFFF) a few tones apart, the first one kept."""
    out, seen = [], {}
    for v in values:
        key = v.upper()
        n = seen.get(key, 0)
        seen[key] = n + 1
        out.append(v if n == 0 else shift_tone(v, (-step if light else step) * n))
    return out


# ── pictures ──────────────────────────────────────────────────────────────────
def from_picture(path: str, count: int = 6) -> list[str]:
    """The main colours of a picture, the most eye-catching first (needs Qt)."""
    from PyQt6.QtGui import QImage
    from PyQt6.QtCore import Qt
    img = QImage(path)
    if img.isNull():
        return []
    img = img.scaled(72, 72, Qt.AspectRatioMode.KeepAspectRatio, Qt.TransformationMode.SmoothTransformation)
    buckets: dict[tuple, list] = {}
    for y in range(img.height()):
        for x in range(img.width()):
            c = img.pixelColor(x, y)
            if c.alpha() < 128:
                continue
            hx = _hex(c.redF(), c.greenF(), c.blueF())
            L, C, h = oklch(hx)
            key = ("g", round(L * 6)) if C < GREY_CHROMA else ("c", int(h // 24), round(L * 3))
            b = buckets.setdefault(key, [0, 0.0, 0.0, 0.0, 0.0])
            b[0] += 1
            b[1] += L
            rad = math.radians(h)
            b[2] += C * math.cos(rad)
            b[3] += C * math.sin(rad)
            b[4] += C
    found = []
    for key, (n, sl, sa, sb, sc) in buckets.items():
        L, a, b = sl / n, sa / n, sb / n
        C, h = math.hypot(a, b), math.degrees(math.atan2(b, a)) % 360
        score = n * (0.25 + min(1.0, sc / n / 0.12) * 2.0) * (0.6 + 0.4 * (1 - abs(L - 0.65)))
        found.append((score, from_oklch(L, C, h), h, C))
    found.sort(reverse=True)
    out: list[tuple[str, float, float]] = []
    for _s, hx, h, C in found:
        if all(abs(hue_diff(h, oh)) > 18 or abs(C - oc) > 0.08 or abs(tone(hx) - tone(ox)) > 25
               for ox, oh, oc in out):
            out.append((hx, h, C))
        if len(out) >= count:
            break
    return [x for x, _h, _c in out]


def main_from_picture(path: str, light: bool) -> dict:
    """Main colours (see MAIN) taken from a picture."""
    cols = from_picture(path, 8)
    if not cols:
        return {}
    colourful = sorted(cols, key=lambda c: -oklch(c)[1])
    accent = colourful[0]
    others = [c for c in colourful[1:] if abs(hue_diff(oklch(c)[2], oklch(accent)[2])) > 25]
    second = others[0] if others else at_tone((oklch(accent)[2] + 30) % 360, oklch(accent)[1] * 0.6, tone(accent))
    third = others[1] if len(others) > 1 else at_tone((oklch(accent)[2] - 40) % 360, oklch(accent)[1] * 0.8,
                                                       tone(accent))
    darkest = min(cols, key=tone)
    lightest = max(cols, key=tone)
    bg_src = lightest if light else darkest
    _l, bc, bh = oklch(bg_src)
    bg = at_tone(bh, min(bc, 0.03), 97 if light else 8)
    return {"accent": _fit_accent(accent, light), "accent2": _fit_accent(second, light),
            "accent3": _fit_accent(third, light), "background": bg, "text": readable_on(bg, accent)}


def _fit_accent(c: str, light: bool) -> str:
    """An accent bright enough to read on a dark background (dark enough on a light one)."""
    t = tone(c)
    return with_tone(c, max(t, 62)) if not light else with_tone(c, min(t, 52))


# ── harmonies ─────────────────────────────────────────────────────────────────
HARMONIES = [
    ("matching", "Matching", "Neighbours on the colour wheel: calm and close together."),
    ("opposite", "Opposites", "The colour across the wheel: lively, high contrast."),
    ("triad", "Three-way", "Three colours evenly around the wheel: bright and playful."),
    ("split", "Split", "The two colours either side of the opposite: bold but balanced."),
    ("mono", "One colour", "Only shades of the accent: tidy and quiet."),
    ("pastel", "Pastel", "Soft, light versions of matching colours."),
    ("neon", "Neon", "As colourful as a screen can show."),
]


def harmony(accent: str, kind: str, light: bool, background: str = "", text: str = "") -> dict:
    """Main colours (see MAIN) for a harmony around `accent`."""
    _L, C, h = oklch(accent)
    t = tone(accent)
    C = max(C, 0.06)
    offs = {"matching": (35, -35), "opposite": (180, 150), "triad": (120, 240), "split": (150, 210),
            "mono": (0, 0), "pastel": (40, -40), "neon": (120, 200)}.get(kind, (35, -35))
    c2, c3 = C, C
    if kind == "mono":
        c2, c3 = C * 0.45, C * 0.7
    if kind == "pastel":
        C, c2, c3 = min(C, 0.09), 0.07, 0.07
        t = 80 if not light else 70
    if kind == "neon":
        C = c2 = c3 = 0.32
        t = max(t, 70) if not light else min(t, 55)
    acc = at_tone(h, C, t)
    sec = at_tone((h + offs[0]) % 360, c2, t if kind != "mono" else t + (8 if not light else -8))
    ter = at_tone((h + offs[1]) % 360, c3, t)
    bg = background or at_tone(h, 0.02 if kind != "neon" else 0.03, 97 if light else 8)
    return {"accent": acc, "accent2": sec, "accent3": ter, "background": bg,
            "text": text or readable_on(bg, acc)}


def surprise(light: bool, rng: random.Random | None = None) -> dict:
    rng = rng or random.Random()
    h = rng.uniform(0, 360)
    t = rng.uniform(68, 84) if not light else rng.uniform(38, 55)
    kind = rng.choice([k for k, _n, _d in HARMONIES])
    acc = at_tone(h, rng.uniform(0.09, 0.2), t)
    bgc = rng.choice([0.0, 0.012, 0.02, 0.035])
    bg = at_tone((h + rng.choice([0, 0, 20, -20, 180])) % 360, bgc,
                 rng.uniform(5, 13) if not light else rng.uniform(94, 99))
    return harmony(acc, kind, light, background=bg)


# ── a whole Caelestia scheme from a few main colours ──────────────────────────
MAIN = [("accent", "Accent"), ("accent2", "Second accent"), ("accent3", "Third accent"),
        ("background", "Background"), ("text", "Text"), ("success", "Good (success)"), ("error", "Bad (error)")]

# ANSI terminal hues (OKLCH) before they are leaned a little towards the accent
_ANSI_HUES = {"red": 22, "green": 145, "yellow": 95, "blue": 255, "magenta": 330, "cyan": 200}
# the named colours some apps take from Caelestia's scheme (btop, cava, htop, Zed...)
_NAMED_HUES = {"rosewater": (40, 0.04), "flamingo": (20, 0.06), "pink": (350, 0.12), "mauve": (305, 0.13),
               "red": (15, 0.15), "maroon": (5, 0.11), "peach": (55, 0.13), "yellow": (95, 0.12),
               "green": (145, 0.13), "teal": (185, 0.1), "sky": (220, 0.1), "sapphire": (235, 0.11),
               "blue": (260, 0.13), "lavender": (280, 0.1)}


def main_from_scheme(cols: dict, light: bool | None = None) -> dict:
    """The main colours a Caelestia scheme already has (to start the Main colours tool from)."""
    def g(*names, default=""):
        for n in names:
            v = cols.get(n)
            if is_hex(v):
                return v.upper()
        return default
    bg = g("surface", "background", "base", default="#131317")
    return {"accent": g("primary", default="#7B6CF6"), "accent2": g("secondary", default=""),
            "accent3": g("tertiary", default=""), "background": bg,
            "text": g("onSurface", "onBackground", "text", default=readable_on(bg)),
            "success": g("success", default=""), "error": g("error", default="")}


def caelestia_scheme(main: dict, keys=None) -> dict:
    """Every Caelestia colour (Material 3 roles, terminal, named and KDE colours) from the main colours:
    accent (needed), second and third accent, background, text, success and error (each optional). Light or
    dark comes from the background. Surfaces, outlines and text get proper steps a few tones apart."""
    acc = main.get("accent") or "#7B6CF6"
    bg = main.get("background") or ("#131317")
    light = tone(bg) > 55
    _al, ac, ah = oklch(acc)
    if ac < GREY_CHROMA:
        # a grey accent: the other two are greys a few steps away, so they still tell apart
        a2 = main.get("accent2") or shift_tone(acc, 14 if tone(acc) < 50 else -14)
        a3 = main.get("accent3") or shift_tone(acc, -10 if tone(acc) > 20 else 22)
    else:
        a2 = main.get("accent2") or at_tone((ah + 25) % 360, ac * 0.45, tone(acc))
        a3 = main.get("accent3") or at_tone((ah + 60) % 360, ac * 0.8, tone(acc))
    text = main.get("text") or readable_on(bg, acc)
    err = main.get("error") or at_tone(toward_hue(25, ah, 0.08), 0.16, 45 if light else 75)
    ok = main.get("success") or at_tone(toward_hue(150, ah, 0.12), 0.11, 42 if light else 78)
    bt = tone(bg)
    _bl, bc, bh = oklch(bg)
    nc = min(bc, 0.04)
    _tl, tc, th = oklch(text)
    nh = bh if bc >= 0.006 else (th if tc >= 0.006 else ah)
    nvc = max(nc, min(ac * 0.12, 0.018))         # "neutral variant": greys with a breath of the accent

    def N(t, c=None):
        return at_tone(nh, nc if c is None else c, t)

    def NV(t):
        return at_tone(toward_hue(nh, ah, 0.5), nvc, t)

    def pal(colour, t):
        _l, c, h = oklch(colour)
        return at_tone(h, c, t)

    d = -1 if light else 1          # towards the text: up the tones on dark, down on light
    s = {}
    # surfaces: the background is "surface", the containers step towards the text
    s["surface"] = bg.upper()
    s["background"] = bg.upper()
    s["surfaceDim"] = N(bt - 13 if light else max(0, bt - 2))
    s["surfaceBright"] = N(min(100, bt + 1) if light else bt + 18)
    s["surfaceContainerLowest"] = N(100 if light else max(0, bt - 3))
    s["surfaceContainerLow"] = N(bt + d * 2.5)
    s["surfaceContainer"] = N(bt + d * 5)
    s["surfaceContainerHigh"] = N(bt + d * 8)
    s["surfaceContainerHighest"] = N(bt + d * 11)
    s["surfaceVariant"] = NV(bt + d * 12)
    s["inverseSurface"] = N(20 if light else 90)
    s["inverseOnSurface"] = N(95 if light else 20)
    # text and lines
    tt = tone(text)
    s["onSurface"] = text.upper()
    s["onBackground"] = text.upper()
    s["onSurfaceVariant"] = at_tone(toward_hue(th, ah, 0.3), max(nvc, min(tc, 0.03)), tt + (25 if light else -22))
    s["outline"] = NV(50 if light else 60)
    s["outlineVariant"] = NV(bt + d * 20)
    s["shadow"] = "#000000"
    s["scrim"] = "#000000"
    # the three accents, their containers and the "fixed" versions
    for name, col in (("primary", acc), ("secondary", a2), ("tertiary", a3)):
        cap = name[0].upper() + name[1:]
        ct = tone(col)
        s[name] = col.upper()
        s["on" + cap] = pal(col, 100 if ct < 60 else 12) if abs(ct - 60) > 8 else readable_on(col)
        s[name + "Container"] = pal(col, 88 if light else 30)
        s["on" + cap + "Container"] = pal(col, 15 if light else 92)
        s[name + "Fixed"] = pal(col, 90)
        s[name + "FixedDim"] = pal(col, 80)
        s["on" + cap + "Fixed"] = pal(col, 10)
        s["on" + cap + "FixedVariant"] = pal(col, 30)
        s[name + "_paletteKeyColor"] = with_tone(col, 50)
    s["inversePrimary"] = pal(acc, 80 if light else 40)
    s["surfaceTint"] = acc.upper()
    s["neutral_paletteKeyColor"] = N(50)
    s["neutral_variant_paletteKeyColor"] = NV(50)
    # good and bad
    for name, col in (("error", err), ("success", ok)):
        cap = name[0].upper() + name[1:]
        s[name] = col.upper()
        s["on" + cap] = readable_on(col)
        s[name + "Container"] = pal(col, 88 if light else 30)
        s["on" + cap + "Container"] = pal(col, 15 if light else 90)
    # terminal colours: readable hues leaned a little towards the accent
    tn, tb = (42, 32) if light else (72, 82)
    ansi = {}
    for nm, hh in _ANSI_HUES.items():
        h2 = toward_hue(hh, ah, 0.12)
        ansi[nm] = at_tone(h2, 0.13, tn)
        ansi["b" + nm] = at_tone(h2, 0.15, tb)
    order = ["red", "green", "yellow", "blue", "magenta", "cyan"]
    s["term0"] = N(bt + d * 9 if not light else 25)
    s["term7"] = N(80 if not light else 40)
    s["term8"] = N(50)
    s["term15"] = N(96 if not light else 15)
    for i, nm in enumerate(order, 1):
        s[f"term{i}"] = ansi[nm]
        s[f"term{i + 8}"] = ansi["b" + nm]
    # named colours for other apps
    nt = 45 if light else 78
    for nm, (hh, cc) in _NAMED_HUES.items():
        s[nm] = at_tone(toward_hue(hh, ah, 0.1), cc, nt)
    s["green"] = at_tone(oklch(ok)[2], 0.13, nt)
    s["red"] = at_tone(oklch(err)[2], 0.15, nt)
    # the catppuccin-style text and surface steps
    steps = {"text": 0, "subtext1": 0.12, "subtext0": 0.24, "overlay2": 0.36, "overlay1": 0.46, "overlay0": 0.56}
    for nm, f in steps.items():
        s[nm] = at_tone(nh, nc, tt + (bt - tt) * f)
    s["surface2"] = N(bt + d * 16)
    s["surface1"] = N(bt + d * 11)
    s["surface0"] = N(bt + d * 6)
    s["base"] = bg.upper()
    # mantle and crust are always a little darker than the base, light or dark
    s["mantle"] = N(bt - 3 if bt >= 3 else bt + 2)
    s["crust"] = N(bt - 6 if bt >= 6 else bt + 4)
    # KDE and Qt apps
    s["klink"] = s["klinkSelection"] = acc.upper()
    s["kvisited"] = s["kvisitedSelection"] = a3.upper()
    s["knegative"] = s["knegativeSelection"] = err.upper()
    s["kneutral"] = s["kneutralSelection"] = s["yellow"]
    s["kpositive"] = s["kpositiveSelection"] = ok.upper()
    if keys is not None:
        keys = set(keys)
        s = {k: v for k, v in s.items() if k in keys}
    return s


def lum_roles(main: dict) -> dict:
    """Luminescent's own theme colours from the same main colours, with real steps between the backgrounds."""
    acc = main.get("accent") or "#7B6CF6"
    bg = main.get("background") or "#131317"
    light = tone(bg) > 55
    bt = tone(bg)
    _bl, bc, bh = oklch(bg)
    nc = min(bc, 0.04)
    text = main.get("text") or readable_on(bg, acc)
    _tl, tc, th = oklch(text)
    nh = bh if bc >= 0.006 else (th if tc >= 0.006 else oklch(acc)[2])
    d = -1 if light else 1

    def N(t):
        return at_tone(nh, nc, t)
    tt = tone(text)
    _al, ac, ah = oklch(acc)
    at = tone(acc)
    c = {"bg_base": bg.upper(), "bg_card": N(bt + d * 3.5), "bg_card_hover": N(bt + d * 7),
         "bg_frame": N(bt - 4.5 if light else (bt - 2.5 if bt >= 3 else bt + 1.5)), "bg_header": N(bt + d * 1.8), "border": N(bt + d * 12),
         "text_primary": text.upper(), "text_secondary": at_tone(nh, max(nc, min(tc, 0.03)), tt + (bt - tt) * 0.32),
         "text_muted": at_tone(nh, nc, tt + (bt - tt) * 0.55),
         "primary": acc.upper(), "primary_hover": at_tone(ah, ac, at + (-6 if light else 7)),
         "primary_muted": at_tone(ah, min(ac, 0.08), bt + d * 9), "primary_text": at_tone(ah, ac, 38 if light else 78),
         "on_primary": readable_on(acc, acc)}
    c["folder_color"] = c["bg_card"]
    c["folder_border"] = c["primary_text"]
    return c


def main_from_roles(roles: dict) -> dict:
    """The main colours of a Luminescent theme's colours."""
    def g(k, default):
        v = roles.get(k)
        return v.upper() if is_hex(v) else default
    bg = g("bg_base", "#131317")
    return {"accent": g("primary", "#7B6CF6"), "background": bg, "text": g("text_primary", readable_on(bg))}
