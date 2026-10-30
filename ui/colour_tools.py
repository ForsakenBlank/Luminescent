"""
The theme editor's colour helpers: every Caelestia colour sorted by what it does on screen (with a plain name and
where it shows), a preview that lights up where a colour is used, colour rows with a favourite star, undo and
redo, a list of recent changes, and the quick tools that change a whole set of colours at once (main colours,
light/dark flip, adjust all, follow the accent, add variance, from a picture, harmonies, surprise me). Every quick
tool shows the colours before and after and is one step to undo.
"""
import copy
import json
import random

from PyQt6.QtWidgets import (
    QWidget, QFrame, QVBoxLayout, QHBoxLayout, QGridLayout, QCheckBox, QSlider, QPushButton, QLineEdit,
    QFileDialog, QButtonGroup, QSizePolicy, QScrollArea
)
from PyQt6.QtCore import Qt, QRectF, QPointF, pyqtSignal, QTimer
from PyQt6.QtGui import QPainter, QColor, QPen, QPainterPath, QPolygonF

from core import theme, widgets, colourpick, tones
from core.guard import alive

# ── what every Caelestia colour does ──────────────────────────────────────────
# (group id, title, what the group is, [(key, plain name, where it shows)])
CAEL_GROUPS = [
    ("accent", "Main accent", "The colour that stands out: what is on, picked or active.", [
        ("primary", "Accent", "The active workspace, sliders, switches that are on, the picked item in the "
                              "launcher, progress, links."),
        ("onPrimary", "Text on the accent", "Text and icons drawn on accent-coloured things (the picked launcher "
                                            "item, a switch's knob)."),
        ("primaryContainer", "Soft accent background", "Softer accent areas: the avatar circle, media and network "
                                                       "cards, Bluetooth and Wi-Fi rows."),
        ("onPrimaryContainer", "Text on the soft accent", "Text and icons on the soft accent background."),
        ("inversePrimary", "Accent on dark panels", "The audio visualiser on the desktop; Qt apps' highlights."),
        ("surfaceTint", "Accent tint", "A faint wash of the accent on raised surfaces."),
        ("primaryFixed", "Accent (fixed light)", "Pandora and a few spots that keep one shade in light and dark."),
        ("primaryFixedDim", "Accent (fixed dim)", "As above, a little darker."),
        ("onPrimaryFixed", "Text on fixed accent", "Text on the fixed accent."),
        ("onPrimaryFixedVariant", "Soft text on fixed accent", "Quieter text on the fixed accent."),
        ("primary_paletteKeyColor", "Accent seed", "The colour Caelestia built the accent shades from."),
    ]),
    ("accent2", "Second accent", "A calmer partner to the accent: buttons, the resource rings, toggles.", [
        ("secondary", "Second accent", "Icon buttons, resource rings (CPU, memory) and filled sliders on the "
                                       "dashboard, the terminal cursor."),
        ("onSecondary", "Text on second accent", "Text drawn on the second accent."),
        ("secondaryContainer", "Button background", "The background of most buttons, toggled icon buttons and the "
                                                    "slider tracks. One of the most used colours."),
        ("onSecondaryContainer", "Button text and icons", "Icons and text on those buttons."),
        ("secondaryFixed", "Second accent (fixed light)", "Pandora and Qt apps (light)."),
        ("secondaryFixedDim", "Second accent (fixed dim)", "Pandora."),
        ("onSecondaryFixed", "Text on fixed second accent", "Rarely used."),
        ("onSecondaryFixedVariant", "Soft text on fixed second accent", "Rarely used."),
        ("secondary_paletteKeyColor", "Second accent seed", "The colour the second accent was built from."),
    ]),
    ("accent3", "Third accent", "Little highlights: the clock, the user card, network speed.", [
        ("tertiary", "Third accent", "The bar's clock, the desktop clock, network and resource numbers."),
        ("onTertiary", "Text on third accent", "Text on the third accent."),
        ("tertiaryContainer", "Soft third accent", "The user card on the dashboard, menu highlights, media "
                                                   "shapes."),
        ("onTertiaryContainer", "Text on soft third accent", "Text on that soft background."),
        ("tertiaryFixed", "Third accent (fixed light)", "Rarely used."),
        ("tertiaryFixedDim", "Third accent (fixed dim)", "Rarely used."),
        ("onTertiaryFixed", "Text on fixed third accent", "Rarely used."),
        ("onTertiaryFixedVariant", "Soft text on fixed third accent", "Rarely used."),
        ("tertiary_paletteKeyColor", "Third accent seed", "The colour the third accent was built from."),
    ]),
    ("surfaces", "Backgrounds and surfaces", "From the bar's background up through cards, each step a little "
                                              "closer to the text colour.", [
        ("surface", "Bar and panel background", "The bar, the border around the screen and every drawer that "
                                                 "slides out of it."),
        ("background", "Background", "GTK apps' window background and Pandora."),
        ("surfaceContainerLowest", "Lowest layer", "The deepest wells; Qt apps' views."),
        ("surfaceContainerLow", "Low layer", "Menus and the sidebar."),
        ("surfaceContainer", "Card background", "Most cards and boxes on the dashboard, the launcher, notifications "
                                                "groups, switch tracks. The most used background."),
        ("surfaceContainerHigh", "Raised card", "Text fields, notifications, the lock screen's password box."),
        ("surfaceContainerHighest", "Highest layer", "Occupied workspaces in the bar, switch tracks when off, "
                                                     "spin boxes."),
        ("surfaceDim", "Dim surface", "GTK and Qt apps' dimmer areas."),
        ("surfaceBright", "Bright surface", "Qt apps (light)."),
        ("surfaceVariant", "Surface (variant)", "nvtop, Qt apps and Pandora."),
        ("inverseSurface", "Inverted surface", "Tooltips and snackbars drawn the other way round; filled sliders."),
        ("neutral_paletteKeyColor", "Grey seed", "The grey the surfaces were built from."),
        ("neutral_variant_paletteKeyColor", "Tinted grey seed", "The tinted grey outlines were built from."),
    ]),
    ("text", "Text and icons", "Readable on the backgrounds above.", [
        ("onSurface", "Text", "Most text: titles, names, the clock on the lock screen, the terminal's text."),
        ("onSurfaceVariant", "Quiet text and icons", "Labels, hints and most icons. The most used colour of all."),
        ("onBackground", "Text on the background", "GTK apps and Pandora."),
        ("inverseOnSurface", "Text on inverted surface", "Text on tooltips and snackbars."),
    ]),
    ("lines", "Outlines and shadows", "Borders, dividers and what is drawn behind panels.", [
        ("outline", "Outline", "Borders of fields and buttons, dividers, inactive switch knobs, icons in btop and "
                               "htop."),
        ("outlineVariant", "Soft outline", "Quieter dividers: under the dashboard's tabs, empty workspaces, the "
                                           "tray menu."),
        ("shadow", "Shadow", "The shadow under panels and the desktop clock."),
        ("scrim", "Dim behind pop-ups", "The darkening behind pop-ups and the session menu."),
    ]),
    ("status", "Good and bad", "Errors, warnings and success.", [
        ("error", "Error", "The power button, a low battery, wrong passwords, failed things."),
        ("onError", "Text on error", "Text on the error colour."),
        ("errorContainer", "Soft error background", "Error messages and toasts."),
        ("onErrorContainer", "Text on soft error", "Text in error messages."),
        ("success", "Success", "Done and connected toasts; Zed and Pandora."),
        ("onSuccess", "Text on success", "Text on the success colour."),
        ("successContainer", "Soft success background", "Success toasts."),
        ("onSuccessContainer", "Text on soft success", "Text in success toasts."),
    ]),
    ("terminal", "Terminal", "The 16 colours terminals get from Caelestia (each new shell loads them), and the "
                             "Warp terminal theme.", [
        ("term0", "Black", "Colour 0: dark backgrounds of prompts and tools."),
        ("term1", "Red", "Colour 1: errors, deleted lines."),
        ("term2", "Green", "Colour 2: success, added lines, many prompts."),
        ("term3", "Yellow", "Colour 3: warnings."),
        ("term4", "Blue", "Colour 4: folders, info."),
        ("term5", "Magenta", "Colour 5."),
        ("term6", "Cyan", "Colour 6."),
        ("term7", "White", "Colour 7: plain light text."),
        ("term8", "Bright black (grey)", "Colour 8: comments, hints, suggestions as you type."),
        ("term9", "Bright red", "Colour 9."),
        ("term10", "Bright green", "Colour 10."),
        ("term11", "Bright yellow", "Colour 11."),
        ("term12", "Bright blue", "Colour 12."),
        ("term13", "Bright magenta", "Colour 13."),
        ("term14", "Bright cyan", "Colour 14."),
        ("term15", "Bright white", "Colour 15."),
    ]),
    ("named", "Named colours for apps", "Not used by the shell itself: btop, cava, htop, nvtop, Zed, Pandora and "
                                        "Discord take these when Caelestia themes them.", [
        ("rosewater", "Rosewater", "btop and cava."),
        ("flamingo", "Flamingo", "Rarely used."),
        ("pink", "Pink", "Zed and Pandora."),
        ("mauve", "Mauve", "btop, cava, Zed, Pandora."),
        ("red", "Red", "btop, htop, nvtop (high use), Zed."),
        ("maroon", "Maroon", "btop, cava, htop, nvtop, Zed."),
        ("peach", "Peach", "btop, htop, nvtop, Zed."),
        ("yellow", "Yellow", "btop, htop, nvtop, Zed."),
        ("green", "Green", "btop, cava, htop, nvtop, Zed: graphs and meters."),
        ("teal", "Teal", "btop, cava, htop, nvtop, Zed."),
        ("sky", "Sky", "btop, cava, Zed."),
        ("sapphire", "Sapphire", "btop, cava, htop, nvtop, Zed."),
        ("blue", "Blue", "btop, cava, htop, nvtop, Zed."),
        ("lavender", "Lavender", "btop and cava."),
    ]),
    ("steps", "Text and surface steps for apps", "Catppuccin-style steps a few apps use: text at the top, crust at "
                                                 "the bottom.", [
        ("text", "Text", "Apps' main text."),
        ("subtext1", "Subtext 1", "Zed."),
        ("subtext0", "Subtext 0", "Zed and Pandora."),
        ("overlay2", "Overlay 2", "Rarely used."),
        ("overlay1", "Overlay 1", "Pandora."),
        ("overlay0", "Overlay 0", "Rarely used."),
        ("surface2", "Surface 2", "Rarely used."),
        ("surface1", "Surface 1", "Rarely used."),
        ("surface0", "Surface 0", "Rarely used."),
        ("base", "Base", "Rarely used."),
        ("mantle", "Mantle", "Rarely used."),
        ("crust", "Crust", "Rarely used."),
    ]),
    ("kde", "KDE and Qt apps", "Links and states in Qt and KDE apps (Dolphin, Kate...).", [
        ("klink", "Links", "Links in Qt apps."),
        ("klinkSelection", "Links (selected)", "Links in a selection."),
        ("kvisited", "Visited links", "Links you have opened."),
        ("kvisitedSelection", "Visited links (selected)", "Visited links in a selection."),
        ("knegative", "Bad", "Errors in Qt apps."),
        ("knegativeSelection", "Bad (selected)", "Errors in a selection."),
        ("kneutral", "Warning", "Warnings in Qt apps."),
        ("kneutralSelection", "Warning (selected)", "Warnings in a selection."),
        ("kpositive", "Good", "Success in Qt apps."),
        ("kpositiveSelection", "Good (selected)", "Success in a selection."),
    ]),
]
_INFO = {k: (gid, name, where) for gid, _t, _n, rows in CAEL_GROUPS for k, name, where in rows}
GROUP_TITLES = {gid: title for gid, title, _n, _r in CAEL_GROUPS}

# Luminescent's own theme colours in groups (the quick tools tick these)
LUM_GROUPS = [
    ("accent", "Accent", ("primary", "primary_hover", "primary_muted", "primary_text", "on_primary")),
    ("backgrounds", "Backgrounds", ("bg_base", "bg_card", "bg_card_hover", "bg_frame", "bg_header", "border")),
    ("text", "Text", ("text_primary", "text_secondary", "text_muted")),
    ("folders", "Folders", ("folder_color", "folder_border")),
]


def _group_keys(gid: str) -> list[str]:
    return next(([k for k, _n, _w in rows] for g, _t, _n, rows in CAEL_GROUPS if g == gid), [])


def cael_info(key: str) -> tuple[str, str, str]:
    """(group id, plain name, where it shows) for a Caelestia colour; unknown ones go under Other."""
    if key in _INFO:
        return _INFO[key]
    from modules.caelestia import nice
    return "other", nice(key), "A colour this version of Caelestia has that Luminescent does not know yet."


def cael_groups(keys) -> list[tuple[str, str, str, list[str]]]:
    """[(group id, title, note, [keys present])] in display order, unknown keys last under Other."""
    keys = list(keys)
    out = []
    for gid, title, note, rows in CAEL_GROUPS:
        present = [k for k, _n, _w in rows if k in keys]
        if present:
            out.append((gid, title, note, present))
    rest = sorted(k for k in keys if k not in _INFO)
    if rest:
        out.append(("other", "Other", "Colours Luminescent has no description for yet.", rest))
    return out


def lum_name(role: str) -> str:
    return dict(theme.COLOUR_ROLES).get(role, role)


def _solid(v) -> str:
    """A plain colour for a value that may be a gradient."""
    if isinstance(v, dict):
        return theme.gradient_mid(v)
    return v if isinstance(v, str) and v else "#888888"


# ── the Caelestia preview ─────────────────────────────────────────────────────
PREVIEW_W = 380.0          # the preview is drawn at this width and scaled to fit


def _txt(qp, r, text, colour, px, weight=400, align=Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter,
         mono=False):
    qp.setPen(QColor(colour))
    qp.setFont(theme.qfont(px, weight, mono, scaled=False))
    qp.drawText(r, align, text)


class _Reg:
    """Collects (rect, keys) while painting, so a colour can be found on the picture and back."""

    def __init__(self):
        self.items: list[tuple[QRectF, tuple]] = []

    def add(self, r: QRectF, *keys):
        self.items.append((QRectF(r), keys))

    def at(self, p: QPointF) -> str | None:
        hits = [(r.width() * r.height(), keys) for r, keys in self.items if r.contains(p)]
        if not hits:
            return None
        return min(hits, key=lambda h: h[0])[1][0]

    def rects(self, key: str) -> list[QRectF]:
        return [r for r, keys in self.items if key in keys]


def paint_caelestia(qp: QPainter, cols: dict, solid: float = 1.0, reg: _Reg | None = None,
                    highlight: str | None = None) -> float:
    """A rough Caelestia desktop at PREVIEW_W wide: the bar, a dashboard drawer, the launcher, notifications,
    a terminal and the colours other apps take. Returns the height used."""
    reg = reg if reg is not None else _Reg()
    w = PREVIEW_W

    def c(name, panel=False):
        col = QColor(_solid(cols.get(name, "#888888")))
        if panel:
            col.setAlphaF(max(0.05, min(1.0, solid)))
        return col

    def box(r, key, rad, panel=False, pen=None):
        qp.setPen(QPen(c(pen), 1) if pen else Qt.PenStyle.NoPen)
        qp.setBrush(c(key, panel))
        qp.drawRoundedRect(r, rad, rad)
        reg.add(r, key, *((pen,) if pen else ()))

    def text(r, s, key, px, weight=400, align=Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter):
        _txt(qp, r, s, c(key).name(), px, weight, align)
        reg.add(r, key)

    qp.save()
    qp.setRenderHint(QPainter.RenderHint.Antialiasing)
    if solid < 0.999:
        # a striped "wallpaper" behind the bar and the drawer, so see-through panels show it
        back = QRectF(0, 0, w, 262)
        clip = QPainterPath()
        clip.addRoundedRect(back, 14, 14)
        qp.save()
        qp.setClipPath(clip)
        qp.fillRect(back, c("tertiaryContainer"))
        qp.setPen(Qt.PenStyle.NoPen)
        qp.setBrush(c("primaryContainer"))
        for i in range(-300, int(w) + 300, 34):
            qp.drawPolygon(QPolygonF([QPointF(i, 0), QPointF(i + 14, 0), QPointF(i + 14 - 262, 262),
                                      QPointF(i - 262, 262)]))
        qp.restore()
    # the bar, down the left side, and the border around the screen
    bar = QRectF(0, 0, 36, 262)
    box(bar, "surface", 12, panel=True)
    text(QRectF(0, 6, 36, 20), "◆", "primary", 11, 700, Qt.AlignmentFlag.AlignCenter)
    for i in range(5):
        r = QRectF(10, 34 + i * 20, 16, 16 if i != 1 else 30)
        if i > 1:
            r.translate(0, 14)
        if i == 1:
            box(r, "primary", 8)
            text(r, "2", "onPrimary", 8, 700, Qt.AlignmentFlag.AlignCenter)
        elif i in (0, 3):
            box(r, "surfaceContainerHighest", 8)
            text(r, str(i + 1), "onSurfaceVariant", 8, 600, Qt.AlignmentFlag.AlignCenter)
        else:
            text(r, "·", "outlineVariant", 12, 700, Qt.AlignmentFlag.AlignCenter)
    text(QRectF(0, 150, 36, 30), "12\n45", "tertiary", 9.5, 700, Qt.AlignmentFlag.AlignCenter)
    for i, g in enumerate(("◉", "♪")):
        text(QRectF(0, 186 + i * 18, 36, 16), g, "onSurfaceVariant", 10, 400, Qt.AlignmentFlag.AlignCenter)
    text(QRectF(0, 236, 36, 18), "⏻", "error", 12, 700, Qt.AlignmentFlag.AlignCenter)
    # the dashboard drawer
    dash = QRectF(46, 0, w - 46, 262)
    box(dash, "surface", 16, panel=True)
    x0 = dash.left() + 12
    dw = dash.width() - 24
    for i, t in enumerate(("Dashboard", "Media", "Stats")):
        r = QRectF(x0 + i * dw / 3, 6, dw / 3, 22)
        text(r, t, "primary" if i == 0 else "onSurfaceVariant", 10.5, 600 if i == 0 else 400,
             Qt.AlignmentFlag.AlignCenter)
    under = QRectF(x0, 30, dw, 1)
    qp.fillRect(under, c("outlineVariant"))
    reg.add(under.adjusted(0, -2, 0, 2), "outlineVariant")
    ind = QRectF(x0 + dw / 6 - 18, 28, 36, 3)
    box(ind, "primary", 1.5)
    # the user card
    card = QRectF(x0, 40, dw * 0.58, 64)
    box(card, "tertiaryContainer", 12)
    av = QRectF(card.left() + 10, card.top() + 12, 40, 40)
    qp.setPen(Qt.PenStyle.NoPen)
    qp.setBrush(c("primaryContainer"))
    qp.drawEllipse(av)
    reg.add(av, "primaryContainer")
    text(av, "B", "onPrimaryContainer", 15, 700, Qt.AlignmentFlag.AlignCenter)
    text(QRectF(av.right() + 10, card.top() + 12, card.width() - 64, 20), "blank", "onTertiaryContainer", 12, 600)
    text(QRectF(av.right() + 10, card.top() + 32, card.width() - 64, 18), "up 3 hours", "onTertiaryContainer",
         10)
    # the clock card
    ck = QRectF(card.right() + 8, 40, dash.right() - 12 - card.right() - 8, 64)
    box(ck, "surfaceContainer", 12)
    text(ck.adjusted(0, 4, 0, -24), "12:45", "tertiary", 17, 700, Qt.AlignmentFlag.AlignCenter)
    text(ck.adjusted(0, 38, 0, -6), "Friday", "onSurfaceVariant", 9.5, 400, Qt.AlignmentFlag.AlignCenter)
    # resource rings
    rr = QRectF(x0, 112, dw, 64)
    box(rr, "surfaceContainer", 12)
    for i, (lbl, frac) in enumerate((("CPU", 0.62), ("MEM", 0.41), ("GPU", 0.25))):
        cx = rr.left() + 30 + i * (dw - 60) / 2
        ring = QRectF(cx - 18, rr.top() + 9, 36, 36)
        qp.setBrush(Qt.BrushStyle.NoBrush)
        qp.setPen(QPen(c("secondaryContainer"), 5))
        qp.drawEllipse(ring)
        qp.setPen(QPen(c("secondary"), 5, cap=Qt.PenCapStyle.RoundCap))
        qp.drawArc(ring, 90 * 16, int(-frac * 360 * 16))
        reg.add(ring, "secondary", "secondaryContainer")
        text(QRectF(cx - 24, rr.top() + 47, 48, 14), lbl, "onSurfaceVariant", 8.5, 600, Qt.AlignmentFlag.AlignCenter)
    # a slider, a switch on and off, icon buttons
    y = 186
    track = QRectF(x0, y + 6, dw * 0.55, 8)
    box(track, "secondaryContainer", 4)
    fillr = QRectF(x0, y + 6, dw * 0.33, 8)
    box(fillr, "primary", 4)
    knob = QRectF(fillr.right() - 3, y + 1, 6, 18)
    box(knob, "primary", 3)
    sw_on = QRectF(track.right() + 14, y + 1, 34, 18)
    box(sw_on, "primary", 9)
    k = QRectF(sw_on.right() - 16, y + 3, 14, 14)
    qp.setBrush(c("onPrimary"))
    qp.drawEllipse(k)
    reg.add(k, "onPrimary")
    sw_off = QRectF(sw_on.right() + 8, y + 1, 34, 18)
    box(sw_off, "surfaceContainerHighest", 9, pen="outline")
    k = QRectF(sw_off.left() + 4, y + 5, 10, 10)
    qp.setPen(Qt.PenStyle.NoPen)
    qp.setBrush(c("outline"))
    qp.drawEllipse(k)
    y = 216
    for i, (bg, fg, g) in enumerate((("secondaryContainer", "onSecondaryContainer", "☾"),
                                     ("secondaryContainer", "onSecondaryContainer", "✈"),
                                     ("tertiaryContainer", "onTertiaryContainer", "♫"),
                                     ("primaryContainer", "onPrimaryContainer", "⚙"))):
        b = QRectF(x0 + i * 42, y, 34, 34)
        box(b, bg, 12)
        text(b, g, fg, 13, 400, Qt.AlignmentFlag.AlignCenter)
    tf = QRectF(x0 + 4 * 42 + 4, y + 2, dw - 4 * 42 - 4, 30)
    box(tf, "surfaceContainerHigh", 15, pen="outline")
    text(tf.adjusted(12, 0, 0, 0), "Search…", "onSurfaceVariant", 10.5)

    # the launcher
    y = 274.0
    lau = QRectF(0, y, w, 128)
    box(lau, "surfaceContainer", 16, panel=True)
    fld = QRectF(12, y + 10, w - 24, 28)
    box(fld, "surfaceContainerHigh", 14)
    text(fld.adjusted(14, 0, 0, 0), "⌕  Search apps…", "onSurfaceVariant", 11)
    for i, name in enumerate(("Files", "Browser", "Terminal")):
        r = QRectF(12, y + 44 + i * 26, w - 24, 24)
        if i == 0:
            box(r, "primary", 10)
        text(r.adjusted(12, 0, 0, 0), name, "onPrimary" if i == 0 else "onSurface", 11, 600 if i == 0 else 400)
        if i:
            text(r.adjusted(0, 0, -12, 0), "app", "outline", 9.5, 400,
                 Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)

    # notifications: a normal one, an error and a success toast
    y += 140
    nt = QRectF(0, y, w, 50)
    box(nt, "surfaceContainerHigh", 14, pen="outlineVariant")
    text(QRectF(12, y + 6, w - 24, 18), "Screenshot saved", "onSurface", 11.5, 600)
    text(QRectF(12, y + 25, w - 24, 18), "Click to open it in the image viewer", "onSurfaceVariant", 10)
    y += 58
    half = (w - 8) / 2
    er = QRectF(0, y, half, 34)
    box(er, "errorContainer", 12)
    text(er.adjusted(12, 0, 0, 0), "✕  Wrong password", "onErrorContainer", 10.5, 600)
    ok = QRectF(half + 8, y, half, 34)
    box(ok, "successContainer", 12)
    text(ok.adjusted(12, 0, 0, 0), "✓  Connected", "onSuccessContainer", 10.5, 600)
    y += 44

    # a terminal
    term = QRectF(0, y, w, 96)
    box(term, "surface", 12, pen="outlineVariant")
    text(QRectF(12, y + 8, 14, 16), "❯", "term2", 11, 700)
    text(QRectF(28, y + 8, w - 40, 16), "ls ~/Pictures", "onSurface", 11, 400)
    cur = QRectF(124, y + 9, 7, 14)
    qp.fillRect(cur, c("secondary"))
    reg.add(cur, "secondary")
    bw = (w - 24) / 8
    for row in range(2):
        for i in range(8):
            key = f"term{i + row * 8}"
            r = QRectF(12 + i * bw, y + 32 + row * 22, bw - 3, 18)
            box(r, key, 4)
    text(QRectF(12, y + 76, w - 24, 16), "0  1  2  3  4  5  6  7   (top: normal, bottom: bright)", "outline", 8.5)
    y += 106

    # what other apps take
    for title, keys in (("APPS (btop, cava, Zed…)", _group_keys("named")), ("STEPS", _group_keys("steps")),
                        ("KDE / QT", _group_keys("kde")[::2])):
        keys = [k for k in keys if k in cols]
        if not keys:
            continue
        _txt(qp, QRectF(0, y, w, 14), title, theme.P.text_muted, 8.5, 600, mono=True)
        y += 16
        cw = w / len(keys)
        for i, k in enumerate(keys):
            r = QRectF(i * cw, y, cw - 2, 16)
            box(r, k, 3)
        y += 24

    # where the highlighted colour is used
    if highlight:
        found = reg.rects(highlight)
        hc = QColor(theme.P.primary)
        for r in found:
            qp.setBrush(Qt.BrushStyle.NoBrush)
            qp.setPen(QPen(QColor(0, 0, 0, 170), 4))
            qp.drawRoundedRect(r.adjusted(-3, -3, 3, 3), 6, 6)
            qp.setPen(QPen(hc, 2, Qt.PenStyle.SolidLine))
            qp.drawRoundedRect(r.adjusted(-3, -3, 3, 3), 6, 6)
        name = cael_info(highlight)[1]
        tag = name if found else f"{name}: not in this preview"
        qp.setFont(theme.qfont(10, 600, scaled=False))
        tw = qp.fontMetrics().horizontalAdvance(tag) + 18
        if found:
            top = min(found, key=lambda r: r.top())
            tr = QRectF(max(0, min(w - tw, top.left())), max(0, top.top() - 24), tw, 20)
        else:
            tr = QRectF(w - tw, 0, tw, 20)
        qp.setPen(Qt.PenStyle.NoPen)
        qp.setBrush(hc)
        qp.drawRoundedRect(tr, 10, 10)
        _txt(qp, tr, tag, theme.P.on_primary, 10, 600, Qt.AlignmentFlag.AlignCenter)
    qp.restore()
    return y


def paint_luminescent(qp: QPainter, w: float, roles: dict, reg: _Reg | None = None,
                      highlight: str | None = None) -> float:
    """Luminescent's overlay and a panel in these theme colours, w wide. Returns the height used."""
    from ui.themes_page import paint_overlay, paint_panel
    reg = reg if reg is not None else _Reg()
    vals = {k: _solid(v) for k, v in roles.items()}
    p = theme.palette_from(vals)
    h = min(w * 0.6, 230)
    paint_overlay(qp, QRectF(0, 0, w, h), p, h / 112.0, reg)
    paint_panel(qp, QRectF(0, h + 14, w, 250), p, "A PANEL", reg)
    y = h + 14 + 250
    if highlight:
        found = reg.rects(highlight)
        hc = QColor(theme.P.primary)
        for r in found:
            qp.setBrush(Qt.BrushStyle.NoBrush)
            qp.setPen(QPen(QColor(0, 0, 0, 170), 4))
            qp.drawRoundedRect(r.adjusted(-2, -2, 2, 2), 5, 5)
            qp.setPen(QPen(hc, 2))
            qp.drawRoundedRect(r.adjusted(-2, -2, 2, 2), 5, 5)
    return y


# ── undo and redo ─────────────────────────────────────────────────────────────
class History:
    """Snapshots of the theme being edited. A burst of small edits (dragging in the colour picker) becomes one
    step; quick tools are always one step."""

    def __init__(self, first: dict, limit: int = 80):
        self.undo_stack: list[tuple[str, str]] = []     # (label, json of the theme before)
        self.redo_stack: list[tuple[str, str]] = []
        self.now = json.dumps(first, sort_keys=True)
        self.limit = limit

    def record(self, t: dict, label: str = "Change") -> bool:
        cur = json.dumps(t, sort_keys=True)
        if cur == self.now:
            return False
        self.undo_stack.append((label, self.now))
        del self.undo_stack[:-self.limit]
        self.redo_stack.clear()
        self.now = cur
        return True

    def undo(self) -> tuple[str, dict] | None:
        if not self.undo_stack:
            return None
        label, before = self.undo_stack.pop()
        self.redo_stack.append((label, self.now))
        self.now = before
        return label, json.loads(before)

    def redo(self) -> tuple[str, dict] | None:
        if not self.redo_stack:
            return None
        label, after = self.redo_stack.pop()
        self.undo_stack.append((label, self.now))
        self.now = after
        return label, json.loads(after)


# ── a colour row ──────────────────────────────────────────────────────────────
class Star(QPushButton):
    """☆ / ★: a favourite pinned at the top."""

    def __init__(self, on: bool, tip: str = "Favourite: pin it at the top"):
        super().__init__()
        self.setCheckable(True)
        self.setChecked(on)
        self.setFixedSize(24, 24)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.setToolTip(tip)
        self.setObjectName("starBtn")
        self._paint()
        self.toggled.connect(lambda _on: self._paint())

    def _paint(self):
        on = self.isChecked()
        self.setText("★" if on else "☆")
        self.setStyleSheet(f"#starBtn {{ background: transparent; border: none; font-size: {theme.fs(14)}px; "
                           f"color: {theme.P.primary if on else theme.P.text_muted}; padding: 0; }}"
                           f"#starBtn:hover {{ color: {theme.P.primary}; }}")


class ColourRow(QFrame):
    """One colour: star, swatch, its plain name and where it shows, a dot when the theme sets it and a reset."""
    hovered = pyqtSignal(object)             # the key, or None when the mouse leaves
    changed = pyqtSignal(str, object)
    reset = pyqtSignal(str)
    starred = pyqtSignal(str, bool)

    def __init__(self, key: str, name: str, where: str, value, is_set: bool, fav: bool, gradient: bool = False,
                 reset_tip: str = "Back to Caelestia's own colour", swatch_w: int = 132):
        super().__init__()
        self.key, self.name, self.where = key, name, where
        self.setObjectName("colourRow")
        self.setMouseTracking(True)
        self.setStyleSheet(f"#colourRow {{ border-radius: 8px; background: transparent; }}"
                           f"#colourRow[hl=\"true\"] {{ background: {theme.rgba(theme.P.primary, 0.12)}; }}")
        lay = QHBoxLayout(self)
        lay.setContentsMargins(4, 3, 6, 3)
        lay.setSpacing(8)
        self.star = Star(fav)
        self.star.toggled.connect(lambda on: self.starred.emit(self.key, on))
        lay.addWidget(self.star)
        self.swatch = colourpick.ColourSwatch(value, name, gradient=gradient, width=swatch_w, live=True)
        self.swatch.changed.connect(lambda v: self.changed.emit(self.key, v))
        lay.addWidget(self.swatch)
        col = QVBoxLayout()
        col.setSpacing(0)
        self.title = widgets.label(name, 12)
        col.addWidget(self.title)
        self.note = widgets.label(where, 10.5, tone="muted")
        self.note.setWordWrap(True)
        col.addWidget(self.note)
        lay.addLayout(col, 1)
        self.reset_btn = widgets.icon_button("↺", reset_tip, 24, 12, ghost=True)
        self.reset_btn.clicked.connect(lambda: self.reset.emit(self.key))
        lay.addWidget(self.reset_btn)
        self.set_state(value, is_set)

    def set_state(self, value, is_set: bool):
        if self.swatch.value() != value:
            self.swatch.set_value(value)
        self.title.setText(f"{self.name}  ●" if is_set else self.name)
        self.title.setToolTip("Set for this theme" if is_set else "")
        self.reset_btn.setVisible(is_set)

    def set_fav(self, on: bool):
        self.star.blockSignals(True)
        self.star.setChecked(on)
        self.star.blockSignals(False)
        self.star._paint()

    def set_highlight(self, on: bool):
        self.setProperty("hl", on)
        self.style().unpolish(self)
        self.style().polish(self)

    def enterEvent(self, e):
        self.hovered.emit(self.key)
        super().enterEvent(e)

    def leaveEvent(self, e):
        self.hovered.emit(None)
        super().leaveEvent(e)


class Collapsible(QFrame):
    """A card whose header folds it."""
    toggled = pyqtSignal(str, bool)

    def __init__(self, gid: str, title: str, note: str, collapsed: bool):
        super().__init__()
        self.gid = gid
        self.setObjectName("lumCard")
        v = QVBoxLayout(self)
        v.setContentsMargins(14, 10, 14, 10)
        v.setSpacing(4)
        self.head = QPushButton()
        self.head.setCursor(Qt.CursorShape.PointingHandCursor)
        self.head.setFlat(True)
        self.head.setObjectName("foldHead")
        self.head.setStyleSheet(f"#foldHead {{ text-align: left; border: none; background: transparent; "
                                f"padding: 2px 0; color: {theme.P.primary_text}; font-family: {theme.MONO_CSS}; "
                                f"font-size: {theme.fs(10.5)}px; font-weight: 600; letter-spacing: 1.4px; }}")
        self.head.clicked.connect(self.flip)
        v.addWidget(self.head)
        self.note = widgets.label(note, 11, tone="muted")
        self.note.setWordWrap(True)
        v.addWidget(self.note)
        self.body = QWidget()
        self.rows = QVBoxLayout(self.body)
        self.rows.setContentsMargins(0, 4, 0, 0)
        self.rows.setSpacing(1)
        v.addWidget(self.body)
        self.title = title
        self.count = ""
        self.collapsed = collapsed
        self._show()

    def set_count(self, text: str):
        self.count = text
        self._show()

    def _show(self):
        self.head.setText(("▸  " if self.collapsed else "▾  ") + self.title.upper() +
                          (f"   ·   {self.count}" if self.count else ""))
        self.body.setVisible(not self.collapsed)
        self.note.setVisible(not self.collapsed)

    def flip(self):
        self.collapsed = not self.collapsed
        self._show()
        self.toggled.emit(self.gid, self.collapsed)

    def open(self):
        if self.collapsed:
            self.flip()


# ── recent changes ────────────────────────────────────────────────────────────
class RecentCard(QFrame):
    """The last changes, old → new, each with a button to put the old colours back."""

    def __init__(self, dlg, scope: str):
        super().__init__()
        self.dlg, self.scope = dlg, scope
        self.setObjectName("lumCard")
        v = QVBoxLayout(self)
        v.setContentsMargins(14, 10, 14, 10)
        v.setSpacing(4)
        head = QHBoxLayout()
        head.addWidget(widgets.label("RECENTLY CHANGED", 10.5, 600, mono=True, spacing=0.14, tone="accent"))
        head.addStretch()
        self.undo_btn = widgets.icon_button("↶", "Undo (Ctrl+Z)", 26, 13, ghost=True)
        self.undo_btn.clicked.connect(dlg.undo)
        self.redo_btn = widgets.icon_button("↷", "Redo (Ctrl+Shift+Z)", 26, 13, ghost=True)
        self.redo_btn.clicked.connect(dlg.redo)
        head.addWidget(self.undo_btn)
        head.addWidget(self.redo_btn)
        v.addLayout(head)
        self.list = QVBoxLayout()
        self.list.setSpacing(2)
        v.addLayout(self.list)
        self.fill()

    def fill(self):
        widgets.clear_layout(self.list)
        items = [(i, r) for i, r in enumerate(self.dlg.recents) if r["scope"] == self.scope][:8]
        self.undo_btn.setEnabled(bool(self.dlg.history.undo_stack))
        self.redo_btn.setEnabled(bool(self.dlg.history.redo_stack))
        if not items:
            self.list.addWidget(widgets.label("Nothing yet. Every change shows up here with a button to undo just "
                                              "that one.", 11, tone="muted"))
            return
        for i, r in items:
            row = QHBoxLayout()
            row.setSpacing(6)
            keys = list(r["new"])
            if len(keys) == 1:
                k = keys[0]
                row.addWidget(_chip(r["old"].get(k)))
                row.addWidget(widgets.label("→", 11, tone="muted"))
                row.addWidget(_chip(r["new"].get(k)))
            else:
                strip = _Strip([r["old"].get(k) for k in keys[:12]], [r["new"].get(k) for k in keys[:12]])
                row.addWidget(strip)
            lbl = widgets.elided(r["label"], 11.5)
            lbl.setToolTip(r["label"])
            row.addWidget(lbl, 1)
            back = widgets.icon_button("↺", "Put the old colour back" if len(keys) == 1 else
                                       f"Put these {len(keys)} colours back", 24, 12, ghost=True)
            back.clicked.connect(lambda _c=False, n=i: self.dlg.reset_recent(n))
            row.addWidget(back)
            self.list.addLayout(row)


def _chip(v) -> QWidget:
    w = colourpick.Chip(v if v else "#00000000", (v or "not set").upper() if isinstance(v, str) or v is None else
                        "gradient", 18)
    w.setEnabled(False)
    return w


class _Strip(QWidget):
    """Old colours on top, new below: what a quick tool changed."""

    def __init__(self, old: list, new: list):
        super().__init__()
        self.old, self.new = old, new
        self.setFixedSize(min(12, len(old)) * 9 + 2, 20)
        self.setToolTip("Top: before · bottom: after")

    def paintEvent(self, e):
        qp = QPainter(self)
        for row, vals in enumerate((self.old, self.new)):
            for i, v in enumerate(vals):
                qp.fillRect(QRectF(1 + i * 9, 1 + row * 9, 8, 8), QColor(_solid(v) if v else "#00000000"))
        qp.end()


# ── quick tools ───────────────────────────────────────────────────────────────
TOOLS = [
    ("main", "✦  Main colours", "Pick a few colours (accent, background, text…) and every other colour is made "
                                "from them, with proper lighter and darker steps."),
    ("harmony", "◑  Harmony", "Colours that go together, around your accent."),
    ("picture", "▣  From a picture", "Take the colours from your wallpaper or any picture."),
    ("surprise", "⚄  Surprise me", "A random scheme that still works. Press Again until you like one."),
    ("adjust", "≋  Adjust all", "Sliders for every colour at once: hue, brightness, contrast, colourfulness, "
                                "warmth."),
    ("follow", "◎  Follow the accent", "Make every green (or any hue you choose) take your accent's hue instead."),
    ("variance", "▤  Add variance", "Colours that came out the same (everything #FFFFFF) get a few steps between "
                                    "them."),
    ("flip", "◐  Light ⇄ dark", "Turn a dark scheme into a light one or the other way round."),
]


def tool_bar(dlg, scope: str) -> QWidget:
    """The quick tool buttons for Caelestia's ("cael") or the theme's own ("lum") colours."""
    box = QWidget()
    g = QGridLayout(box)
    g.setContentsMargins(0, 0, 0, 0)
    g.setHorizontalSpacing(6)
    g.setVerticalSpacing(6)
    for i, (key, text, tip) in enumerate(TOOLS):
        b = widgets.button(text, "primary" if key == "main" else "ghost", tip)
        b.setMinimumHeight(30)
        b.clicked.connect(lambda _c=False, k=key: open_tool(dlg, scope, k))
        g.addWidget(b, i // 4, i % 4)
    return box


def open_tool(dlg, scope: str, key: str):
    if scope == "cael" and not dlg.cael_values():
        widgets.message(dlg, "Quick tools", "Caelestia's colour scheme was not found.")
        return
    if key == "picture":
        path = _ask_picture(dlg)
        if not path:
            return
        BulkDialog(dlg, scope, key, picture=path).exec()
        return
    BulkDialog(dlg, scope, key).exec()


def _ask_picture(dlg) -> str:
    from modules import caelestia as cael
    wall = cael.wallpaper() if dlg.launcher is not None else ""
    if wall:
        pick = widgets.choose(dlg, "Colours from a picture", "Take the colours from your wallpaper, or choose "
                                                             "another picture?", ["My wallpaper", "Choose…"])
        if pick is None:
            return ""
        if pick == "My wallpaper":
            return wall
    p, _ = QFileDialog.getOpenFileName(dlg, "Choose a picture", wall or "",
                                       "Pictures (*.png *.jpg *.jpeg *.webp *.bmp *.gif);;All files (*)")
    return p


class _Before(QWidget):
    """Before and after, side by side, drawn live."""
    picked = pyqtSignal(str)

    def __init__(self, bd: "BulkDialog"):
        super().__init__()
        self.bd = bd
        self.setMinimumSize(520, 360)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

    def paintEvent(self, e):
        qp = QPainter(self)
        qp.setRenderHint(QPainter.RenderHint.Antialiasing)
        half = (self.width() - 18) / 2
        for i, (title, cols) in enumerate((("BEFORE", self.bd.before), ("AFTER", self.bd.after))):
            x = i * (half + 18)
            _txt(qp, QRectF(x, 0, half, 16), title, theme.P.text_muted if i == 0 else theme.P.primary_text,
                 9.5, 600, mono=True)
            qp.save()
            qp.translate(x, 22)
            if self.bd.scope == "cael":
                s = min(half / PREVIEW_W, (self.height() - 24) / 770)
                qp.scale(s, s)
                paint_caelestia(qp, {**self.bd.base, **cols}, self.bd.solid)
            else:
                s = min(1.0, (self.height() - 24) / 520)
                qp.scale(s, s)
                paint_luminescent(qp, half / s, {**self.bd.base, **cols})
            qp.restore()
        qp.end()


class BulkDialog(widgets.RoundedDialog):
    """One quick tool: its controls, which colours it may change, and the colours before and after."""

    def __init__(self, dlg, scope: str, tool: str, picture: str = ""):
        title = dict((k, t.split("  ", 1)[1]) for k, t, _d in TOOLS)[tool]
        super().__init__(dlg, title, resizable=True)
        self.dlg, self.scope, self.tool, self.picture = dlg, scope, tool, picture
        self.rng = random.Random()
        self.seed = self.rng.random()
        if scope == "cael":
            self.base = dict(dlg.cael_values())          # every colour as it is now
            self.groups = [(gid, title_, keys) for gid, title_, _n, keys in cael_groups(self.base)]
            self.solid = dlg.cael_solid()
        else:
            self.base = {k: _solid(v) for k, v in dlg.lum_values().items()}
            self.groups = [(gid, t, [r for r in roles if r in self.base]) for gid, t, roles in LUM_GROUPS]
            self.solid = 1.0
        self.light = tones.tone(self.base.get("surface" if scope == "cael" else "bg_base", "#131317")) > 55
        self.before = dict(self.base)
        self.after = dict(self.base)
        self.main = (tones.main_from_scheme(self.base) if scope == "cael" else tones.main_from_roles(self.base))
        self.vals = {"hue": 0, "brightness": 0, "contrast": 0, "saturation": 0, "warmth": 0, "step": 4}
        self.families = {"greens"}
        self.target = self.main.get("accent")
        self.harmony_kind = "matching"
        self.picks: list[str] = []

        top = QHBoxLayout()
        top.setSpacing(16)
        left = QVBoxLayout()
        left.setSpacing(8)
        desc = widgets.label(dict((k, d) for k, _t, d in TOOLS)[tool], 11.5, tone="muted")
        desc.setWordWrap(True)
        left.addWidget(desc)
        self.controls = QVBoxLayout()
        self.controls.setSpacing(6)
        left.addLayout(self.controls)
        left.addSpacing(6)
        left.addWidget(widgets.label("COLOURS IT MAY CHANGE", 9.5, 600, mono=True, spacing=0.14, tone="muted"))
        self.ticks: dict[str, QCheckBox] = {}
        grid = QGridLayout()
        grid.setHorizontalSpacing(10)
        grid.setVerticalSpacing(2)
        default_off = {"named", "steps", "kde"} if tool in ("adjust", "flip") else set()
        for i, (gid, gtitle, keys) in enumerate(self.groups):
            cb = QCheckBox(gtitle)
            cb.setChecked(gid not in default_off)
            cb.setToolTip(f"{len(keys)} colours")
            cb.toggled.connect(lambda _on: self.update_after())
            self.ticks[gid] = cb
            grid.addWidget(cb, i, 0)
        left.addLayout(grid)
        left.addStretch()
        lw = QWidget()
        lw.setLayout(left)
        lw.setFixedWidth(340)
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.Shape.NoFrame)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        scroll.setWidget(lw)
        scroll.setFixedWidth(346)
        top.addWidget(scroll)
        self.view = _Before(self)
        top.addWidget(self.view, 1)
        self.body.addLayout(top, 1)

        foot = QHBoxLayout()
        self.summary = widgets.label("", 11, tone="muted")
        foot.addWidget(self.summary, 1)
        if tool in ("surprise", "harmony", "picture"):
            again = widgets.button("⚄  Again", "ghost", "Another one")
            again.clicked.connect(self._again)
            foot.addWidget(again)
        cancel = widgets.button("Cancel", "ghost")
        cancel.clicked.connect(self.reject)
        ok = widgets.button("Use these colours", "primary")
        ok.setDefault(True)
        ok.clicked.connect(self._apply)
        foot.addWidget(cancel)
        foot.addWidget(ok)
        self.body.addLayout(foot)
        self._build_controls()
        self.fit_to_screen(1080, 720, dlg)
        self.update_after()

    # ── controls per tool ──────────────────────────────────────────────────
    def _mode_row(self):
        row = QHBoxLayout()
        row.addWidget(widgets.label("Make it", 12))
        grp = QButtonGroup(self)
        for text, light in (("Dark", False), ("Light", True)):
            b = QPushButton(text)
            b.setCheckable(True)
            b.setChecked(self.light == light)
            b.setCursor(Qt.CursorShape.PointingHandCursor)
            b.clicked.connect(lambda _c=False, lt=light: self._set_light(lt))
            grp.addButton(b)
            row.addWidget(b)
        row.addStretch()
        self.controls.addLayout(row)

    def _set_light(self, light: bool):
        if light == self.light:
            return
        self.light = light
        if self.tool == "main":
            # flip the background and text to match, keep the accents
            for k in ("background", "text"):
                if self.main.get(k):
                    self.main[k] = tones.flip({"x": self.main[k]})["x"]
            for k, sw in self._main_swatches.items():
                sw.set_value(self.main.get(k) or None)
        self.update_after()

    def _slider(self, key: str, label: str, lo: int, hi: int, unit: str = ""):
        row = QHBoxLayout()
        lbl = widgets.label(label, 12)
        lbl.setFixedWidth(104)
        row.addWidget(lbl)
        s = QSlider(Qt.Orientation.Horizontal)
        s.setRange(lo, hi)
        s.setValue(self.vals[key])
        val = widgets.label(f"{self.vals[key]}{unit}", 11, mono=True, tone="secondary")
        val.setFixedWidth(44)
        val.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)

        def moved(v):
            self.vals[key] = v
            val.setText(f"{v}{unit}")
            self.update_after()
        s.valueChanged.connect(moved)
        s.setToolTip("Double-click the number to put it back")
        val.mouseDoubleClickEvent = lambda _e: s.setValue(0 if lo < 0 else lo)
        row.addWidget(s, 1)
        row.addWidget(val)
        self.controls.addLayout(row)
        return s

    def _build_controls(self):
        t = self.tool
        if t == "main":
            self._mode_row()
            self._main_swatches = {}
            for key, label in tones.MAIN:
                row = QHBoxLayout()
                lbl = widgets.label(label, 12)
                lbl.setFixedWidth(110)
                row.addWidget(lbl)
                optional = key not in ("accent", "background", "text")
                sw = colourpick.ColourSwatch(self.main.get(key) or None, label, unset_text="Worked out" if optional
                                             else "", width=140, live=True)
                sw.changed.connect(lambda v, k=key: (self.main.__setitem__(k, v if isinstance(v, str) else ""),
                                                     self.update_after()))
                row.addWidget(sw)
                if optional:
                    clr = widgets.icon_button("✕", "Let it be worked out from the others", 24, 11, ghost=True)
                    clr.clicked.connect(lambda _c=False, k=key, s=sw: (s.set_value(None), self.main.__setitem__(k, ""),
                                                                       self.update_after()))
                    row.addWidget(clr)
                row.addStretch()
                self._main_swatches[key] = sw
                if self.scope == "lum" and key not in ("accent", "background", "text"):
                    continue
                self.controls.addLayout(row)
            hint = widgets.label("Only the accent, background and text are needed; the rest are worked out when "
                                 "left empty.", 10.5, tone="muted")
            hint.setWordWrap(True)
            self.controls.addWidget(hint)
        elif t == "adjust":
            for key, label, lo, hi, unit in (("hue", "Hue", -180, 180, "°"), ("brightness", "Brightness", -100, 100, ""),
                                             ("contrast", "Contrast", -100, 100, ""),
                                             ("saturation", "Colourfulness", -100, 100, ""),
                                             ("warmth", "Warmth", -100, 100, "")):
                self._slider(key, label, lo, hi, unit)
            reset = widgets.button("Put the sliders back", "ghost")
            reset.clicked.connect(self._reset_sliders)
            self.controls.addWidget(reset, 0, Qt.AlignmentFlag.AlignLeft)
        elif t == "follow":
            row = QHBoxLayout()
            row.addWidget(widgets.label("Take the hue of", 12))
            sw = colourpick.ColourSwatch(self.target, "The colour to follow", width=140, live=True)
            sw.changed.connect(lambda v: (setattr(self, "target", v if isinstance(v, str) else self.target),
                                          self.update_after()))
            row.addWidget(sw)
            row.addStretch()
            self.controls.addLayout(row)
            self.controls.addWidget(widgets.label("Which colours follow it", 12))
            grid = QGridLayout()
            grid.setVerticalSpacing(2)
            fams = list(tones.FAMILIES.items()) + [("all", ("Every colourful one", []))]
            counts = {}
            for v in self.base.values():
                if tones.is_hex(v):
                    f = tones.family(v)
                    counts[f] = counts.get(f, 0) + 1
            for i, (key, (name, _spans)) in enumerate(fams):
                n = counts.get(key, 0) if key != "all" else sum(v for k, v in counts.items() if k != "greys")
                cb = QCheckBox(f"{name} ({n})")
                cb.setChecked(key in self.families)
                cb.toggled.connect(lambda on, k=key: (self.families.add(k) if on else self.families.discard(k),
                                                      self.update_after()))
                grid.addWidget(cb, i, 0)
            self.controls.addLayout(grid)
        elif t == "variance":
            self._mode_row()
            self.vals["step"] = 4
            self._slider("step", "Step size", 2, 10)
        elif t == "harmony":
            self._mode_row()
            row = QHBoxLayout()
            row.addWidget(widgets.label("Around", 12))
            sw = colourpick.ColourSwatch(self.main.get("accent"), "The accent", width=140, live=True)
            sw.changed.connect(lambda v: (self.main.__setitem__("accent", v if isinstance(v, str) else
                                                                self.main["accent"]), self.update_after()))
            row.addWidget(sw)
            row.addStretch()
            self.controls.addLayout(row)
            grp = QButtonGroup(self)
            for key, name, desc in tones.HARMONIES:
                b = QPushButton(name)
                b.setCheckable(True)
                b.setToolTip(desc)
                b.setChecked(key == self.harmony_kind)
                b.setCursor(Qt.CursorShape.PointingHandCursor)
                b.clicked.connect(lambda _c=False, k=key: (setattr(self, "harmony_kind", k), self.update_after()))
                grp.addButton(b)
                self.controls.addWidget(b)
        elif t == "picture":
            self._mode_row()
            try:
                self.picks = tones.from_picture(self.picture, 8)
            except Exception:
                self.picks = []
            self.controls.addWidget(widgets.label("Colours in the picture (click one to make it the accent)", 12))
            row = QHBoxLayout()
            row.setSpacing(4)
            for c in self.picks:
                chip = colourpick.Chip(c, c, 26)
                chip.clicked.connect(lambda _c=False, col=c: self._pick_accent(col))
                row.addWidget(chip)
            row.addStretch()
            self.controls.addLayout(row)
            self._pic_main = tones.main_from_picture(self.picture, self.light) if self.picks else {}
            if not self.picks:
                self.controls.addWidget(widgets.label("Could not read that picture.", 11.5, tone="secondary"))
        elif t == "surprise":
            self._mode_row()
        elif t == "flip":
            note = widgets.label("Every colour keeps its hue; light ones become dark and dark ones light.", 11.5,
                                 tone="muted")
            note.setWordWrap(True)
            self.controls.addWidget(note)

    def _reset_sliders(self):
        for k in ("hue", "brightness", "contrast", "saturation", "warmth"):
            self.vals[k] = 0
        widgets.clear_layout(self.controls)
        self._build_controls()
        self.update_after()

    def _pick_accent(self, col: str):
        self._pic_main = dict(self._pic_main or {})
        self._pic_main["accent"] = tones._fit_accent(col, self.light)
        self.update_after()

    def _again(self):
        self.seed = self.rng.random()
        if self.tool == "harmony":
            kinds = [k for k, _n, _d in tones.HARMONIES]
            self.harmony_kind = kinds[(kinds.index(self.harmony_kind) + 1) % len(kinds)]
            for b in self.findChildren(QPushButton):
                if b.isCheckable() and b.text() == dict((k, n) for k, n, _d in tones.HARMONIES)[self.harmony_kind]:
                    b.setChecked(True)
        if self.tool == "picture" and self.picks:
            self._pick_accent(self.picks[int(self.seed * len(self.picks))])
            return
        self.update_after()

    # ── the result ─────────────────────────────────────────────────────────
    def allowed(self) -> set[str]:
        return {k for gid, _t, keys in self.groups if self.ticks[gid].isChecked() for k in keys}

    def _scheme(self, main: dict) -> dict:
        main = {k: v for k, v in main.items() if v}
        if self.scope == "cael":
            return tones.caelestia_scheme(main, keys=self.base.keys())
        return tones.lum_roles(main)

    def compute(self) -> dict:
        t, base = self.tool, self.before
        if t == "main":
            return self._scheme(self.main)
        if t == "harmony":
            return self._scheme(tones.harmony(self.main.get("accent") or "#7B6CF6", self.harmony_kind, self.light))
        if t == "surprise":
            return self._scheme(tones.surprise(self.light, random.Random(self.seed)))
        if t == "picture":
            return self._scheme(self._pic_main) if getattr(self, "_pic_main", None) else dict(base)
        if t == "adjust":
            v = self.vals
            return tones.adjust(base, v["hue"], v["brightness"], v["contrast"], v["saturation"], v["warmth"])
        if t == "follow":
            return tones.follow_hue(base, set(self.families), self.target or "#7B6CF6")
        if t == "flip":
            return tones.flip(base)
        if t == "variance":
            return self._variance()
        return dict(base)

    def _variance(self) -> dict:
        """Colours in the same group that came out the same get steps between them, using the steps a scheme
        made from the current main colours would have."""
        base = self.before
        fresh = self._scheme(self.main)
        out = dict(base)
        step = self.vals.get("step", 4)
        for _gid, _t, keys in self.groups:
            seen: dict[str, list[str]] = {}
            for k in keys:
                if tones.is_hex(base.get(k)):
                    seen.setdefault(base[k].upper(), []).append(k)
            for _v, same in seen.items():
                if len(same) < 2:
                    continue
                for n, k in enumerate(same):
                    if k in fresh and fresh[k].upper() != base[k].upper():
                        out[k] = fresh[k]
                    elif n:
                        out[k] = tones.shift_tone(base[k], (-step if self.light else step) * n)
        # whatever still sits on the same colour as something it should stand out from moves a step
        if self.scope == "cael":
            ladder = ["surface", "surfaceContainerLow", "surfaceContainer", "surfaceContainerHigh",
                      "surfaceContainerHighest"]
            prev = None
            for k in ladder:
                if k in out and prev and tones.is_hex(out[k]) and abs(tones.tone(out[k]) - tones.tone(prev)) < 1.5:
                    out[k] = tones.shift_tone(prev, -step * 0.7 if self.light else step * 0.7)
                prev = out.get(k, prev)
        return out

    def update_after(self):
        new = self.compute()
        ok = self.allowed()
        self.after = {k: (new[k] if k in ok and k in new else self.before.get(k)) for k in self.before}
        changed = [k for k in self.before if tones.is_hex(self.after.get(k)) and tones.is_hex(self.before.get(k))
                   and self.after[k].upper() != self.before[k].upper()]
        self.changes = {k: self.after[k].upper() for k in changed}
        n = len(changed)
        self.summary.setText(f"{n} colour{'s' if n != 1 else ''} will change. It is one step to undo." if n else
                             "Nothing changes yet.")
        self.view.update()

    def _apply(self):
        if self.changes:
            name = dict((k, t.split("  ", 1)[1]) for k, t, _d in TOOLS)[self.tool]
            self.dlg.apply_bulk(self.scope, self.changes, f"{name} ({len(self.changes)} colours)")
        self.accept()


# ── the Caelestia colours page ────────────────────────────────────────────────
# words you can search for to find every colour that looks like it, whatever its name
SEARCH_FAMILIES = {"green": "greens", "blue": "blues", "red": "reds", "pink": "reds", "orange": "oranges",
                   "brown": "oranges", "yellow": "yellows", "teal": "cyans", "cyan": "cyans", "purple": "purples",
                   "violet": "purples", "magenta": "purples", "grey": "greys", "gray": "greys"}


class CaelestiaPage:
    """Every Caelestia colour in groups, with search, favourites, recent changes and the quick tools."""

    def __init__(self, dlg):
        self.dlg = dlg
        self.rows: dict[str, ColourRow] = {}
        self.cards: dict[str, Collapsible] = {}
        self.fav_rows: dict[str, ColourRow] = {}

    def build(self, v: QVBoxLayout):
        dlg = self.dlg
        v.addWidget(widgets.label("QUICK TOOLS", 9.5, 600, mono=True, spacing=0.14, tone="muted"))
        v.addWidget(tool_bar(dlg, "cael"))
        bar = QHBoxLayout()
        bar.setSpacing(6)
        self.search = QLineEdit()
        self.search.setPlaceholderText("Find a colour: name, where it shows or #hex…")
        self.search.setClearButtonEnabled(True)
        self.search.textChanged.connect(lambda _t: self.filter())
        bar.addWidget(self.search, 1)
        self.only_set = QCheckBox("Only ones set")
        self.only_set.toggled.connect(lambda _on: self.filter())
        bar.addWidget(self.only_set)
        more = widgets.button("More ▾", "ghost")
        more.clicked.connect(lambda: self._more(more))
        bar.addWidget(more)
        v.addLayout(bar)
        self.status = widgets.label("", 11, tone="muted")
        v.addWidget(self.status)
        self.recent = RecentCard(dlg, "cael")
        v.addWidget(self.recent)
        # favourites, pinned at the top
        self.fav_card = Collapsible("favourites", "★ Favourites", "Star any colour below to pin it here.",
                                    False)
        v.addWidget(self.fav_card)
        closed = set(dlg.store_data().get("cael_collapsed") or [])
        vals = dlg.cael_values()
        own = dlg._cael_colours()
        favs = dlg.favourites()
        for gid, title, note, keys in cael_groups(vals):
            card = Collapsible(gid, title, note, gid in closed)
            card.toggled.connect(self._folded)
            for k in keys:
                _g, name, where = cael_info(k)
                row = self._row(k, name, where, vals[k], k in own, f"cael:{k}" in favs)
                card.rows.addWidget(row)
                self.rows[k] = row
            self.cards[gid] = card
            v.addWidget(card)
        self.fill_favourites()
        self.refresh()

    def _row(self, k, name, where, value, is_set, fav) -> ColourRow:
        row = ColourRow(k, name, where, value, is_set, fav)
        row.changed.connect(lambda key, val: self.dlg.set_cael(key, val))
        row.reset.connect(lambda key: self.dlg.set_cael(key, None))
        row.starred.connect(lambda key, on: self.dlg.toggle_favourite(f"cael:{key}", on))
        row.hovered.connect(self.dlg.set_highlight)
        return row

    def _folded(self, gid: str, closed: bool):
        data = self.dlg.store_data()
        s = set(data.get("cael_collapsed") or [])
        s = (s | {gid}) if closed else (s - {gid})
        data["cael_collapsed"] = sorted(s)
        self.dlg.store_save()

    def _more(self, anchor):
        m = widgets.menu(self.dlg)
        m.addAction("Match Caelestia to this theme's colours", self.dlg._match_caelestia)
        m.addAction("Fold every group", lambda: self._fold_all(True))
        m.addAction("Open every group", lambda: self._fold_all(False))
        m.addSeparator()
        m.addAction("Use Caelestia's own colours (forget the theme's)", self.dlg._forget_caelestia)
        m.exec(anchor.mapToGlobal(anchor.rect().bottomLeft()))

    def _fold_all(self, closed: bool):
        for card in self.cards.values():
            if card.collapsed != closed:
                card.flip()

    def fill_favourites(self):
        widgets.clear_layout(self.fav_card.rows)
        self.fav_rows = {}
        vals = self.dlg.cael_values()
        own = self.dlg._cael_colours()
        favs = [f.split(":", 1)[1] for f in self.dlg.favourite_list() if f.startswith("cael:")]
        favs = [k for k in favs if k in vals]
        self.fav_card.setVisible(bool(favs))
        for k in favs:
            _g, name, where = cael_info(k)
            row = self._row(k, name, where, vals[k], k in own, True)
            self.fav_card.rows.addWidget(row)
            self.fav_rows[k] = row
        self.fav_card.set_count(f"{len(favs)}")

    def refresh(self):
        """Values, set dots and favourites after a change (undo, a quick tool...)."""
        vals = self.dlg.cael_values()
        own = self.dlg._cael_colours()
        favs = self.dlg.favourites()
        for k, row in list(self.rows.items()) + list(self.fav_rows.items()):
            row.set_state(vals.get(k), k in own)
            row.set_fav(f"cael:{k}" in favs)
        for gid, card in self.cards.items():
            keys = [k for k in self.rows if self.rows[k].parent() is card.body]
            n = sum(1 for k in keys if k in own)
            card.set_count(f"{len(keys)} colours" + (f" · {n} set" if n else ""))
        n = len(own)
        self.status.setText(f"{n} colour{'s' if n != 1 else ''} set for this theme. Hover a colour to see where it "
                            "is in the preview, or click the preview to find a colour." if n else
                            "None set yet: Caelestia keeps its own colours with this theme. Hover a colour to see "
                            "where it shows, or click the preview to find one.")
        self.recent.fill()
        self.filter()

    def filter(self):
        q = self.search.text().strip().lower()
        fam = SEARCH_FAMILIES.get(q.rstrip("s")) if q else None
        only = self.only_set.isChecked()
        own = self.dlg._cael_colours()
        vals = self.dlg.cael_values()
        for gid, card in self.cards.items():
            shown = 0
            for k, row in self.rows.items():
                if row.parent() is not card.body:
                    continue
                looks = fam is not None and tones.is_hex(vals.get(k)) and tones.family(vals[k]) == fam
                hit = (not q or looks or q in k.lower() or q in row.name.lower() or q in row.where.lower()
                       or q.lstrip("#") in str(vals.get(k, "")).lower().lstrip("#")) and (not only or k in own)
                row.setVisible(hit)
                shown += hit
            card.setVisible(shown > 0)
            if (q or only) and shown and card.collapsed:
                card.body.setVisible(True)
            elif not q and not only:
                card.body.setVisible(not card.collapsed)

    def jump_to(self, key: str):
        row = self.rows.get(key)
        if row is None:
            return
        gid = cael_info(key)[0]
        card = self.cards.get(gid)
        if card is not None:
            card.open()
        self.search.clear()
        QTimer.singleShot(0, lambda: self._scroll_to(row))

    def _scroll_to(self, row: QWidget):
        p = row.parentWidget()
        while p is not None and not isinstance(p, QScrollArea):
            p = p.parentWidget()
        if isinstance(p, QScrollArea):
            p.ensureWidgetVisible(row, 0, 120)
        row.set_highlight(True)
        QTimer.singleShot(1400, lambda: row.set_highlight(False) if alive(row) else None)
