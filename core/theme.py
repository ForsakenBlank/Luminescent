"""
Palette + one global Qt stylesheet.

Modules use `theme.P.primary`, `theme.P.bg_card`, ... in their own f-string
styles, and everything standard (buttons, inputs, menus, scrollbars, dialogs)
is styled once here so each module does not have to repeat it.  Which palette
is active is decided by the theme profiles in core/themes.py.

Type: Geist for the UI and Geist Mono for hotkeys, counts and labels (both
bundled in assets/fonts, loaded by load_fonts()).  Labels get their colour from
a `tone` property (primary / secondary / muted / accent) so they restyle live.

Button variants:  btn.setProperty("variant", "primary" | "ghost" | "danger")
"""
import os
import re
import glob
from dataclasses import dataclass, fields

MONO_FAMILY = "Geist Mono"
MONO_CSS = '"Geist Mono", "Cascadia Mono", Consolas, monospace'
DANGER = "#E8645E"

# Built-in looks (the "Dark" and "Light" themes in core/themes.py).
PRESETS = {
    "dark": dict(  # near-black with a bluey-purple accent (the default)
        primary="#7B6CF6", primary_hover="#9184F8", primary_muted="#24213B", primary_text="#A99FFA",
        on_primary="#FFFFFF",
        bg_base="#131317", bg_card="#1B1B21", bg_card_hover="#23232A", bg_frame="#0E0E11", bg_header="#17171C",
        border="#2A2A30", text_primary="#ECECF1", text_secondary="#A3A3AF", text_muted="#6C6C78",
        folder_color="#1B1B21", folder_border="#A99FFA",
    ),
    "light": dict(  # white with a yellow accent
        primary="#F2B705", primary_hover="#DCA704", primary_muted="#FCF1CD", primary_text="#8A6A00",
        on_primary="#1A1600",
        bg_base="#FFFFFF", bg_card="#F6F6F3", bg_card_hover="#EEEEEA", bg_frame="#F1F1ED", bg_header="#FAFAF8",
        border="#E4E4E1", text_primary="#18180F", text_secondary="#4F4F46", text_muted="#86867C",
        folder_color="#F6F6F3", folder_border="#F2B705",
    ),
}

# The built-in colours before the 2026-10-25 redesign: untouched built-ins are moved to the new ones.
OLD_PRESETS = {
    "dark": dict(
        primary="#7B6CF6", primary_hover="#6656E8", primary_muted="#28234A", on_primary="#FFFFFF",
        bg_base="#121214", bg_card="#1C1C20", bg_card_hover="#25252B", bg_frame="#17171A", bg_header="#141417",
        border="#3A3466", text_primary="#ECEAF6", text_secondary="#9A97AE", text_muted="#55536A",
        folder_color="#221E3D", folder_border="#A99CFF",
    ),
    "light": dict(
        primary="#F2B705", primary_hover="#DDA600", primary_muted="#FFF3C4", on_primary="#1E1A0E",
        bg_base="#FFFFFF", bg_card="#F6F5F1", bg_card_hover="#EEEBE1", bg_frame="#FAF9F6", bg_header="#F3F1EA",
        border="#E3D29A", text_primary="#1F1D18", text_secondary="#5F5B50", text_muted="#A29D8E",
        folder_color="#FFF6D6", folder_border="#F2B705",
    ),
}

# Colour roles a theme can set, with the label shown in the theme editor.
COLOUR_ROLES = [
    ("primary", "Accent"), ("primary_hover", "Accent (hover)"), ("primary_muted", "Accent (muted background)"),
    ("primary_text", "Accent text"), ("on_primary", "Text on accent"), ("bg_base", "Background"),
    ("bg_card", "Cards"), ("bg_card_hover", "Cards (hover)"), ("bg_frame", "Frames / inputs"),
    ("bg_header", "Headers"), ("border", "Borders"), ("text_primary", "Text"),
    ("text_secondary", "Text (secondary)"), ("text_muted", "Text (muted)"), ("folder_color", "Folder cards"),
    ("folder_border", "Folder card outline"),
]


@dataclass
class Palette:
    primary: str = "#7B6CF6"
    primary_hover: str = "#9184F8"
    primary_muted: str = "#24213B"
    primary_text: str = "#A99FFA"       # accent-coloured text (links, PINNED, "+" glyphs)
    bg_base: str = "#131317"
    bg_card: str = "#1B1B21"
    bg_card_hover: str = "#23232A"
    bg_frame: str = "#0E0E11"
    bg_header: str = "#17171C"
    border: str = "#2A2A30"
    text_primary: str = "#ECECF1"
    text_secondary: str = "#A3A3AF"
    text_muted: str = "#6C6C78"
    folder_color: str = "#1B1B21"
    folder_border: str = "#A99FFA"
    on_primary: str = "#FFFFFF"         # text drawn on an accent-coloured background
    radius: int = 10
    font_family: str = "Geist"
    glow: bool = True                   # accent glow around the overlay (off in performance mode)
    background_image: str = ""          # drawn behind the overlay content
    image_opacity: float = 0.35

    @property
    def font(self) -> str:
        return f'"{self.font_family}"'


P = Palette()
_FIELDS = {f.name for f in fields(Palette)}

# Settings → Overlay & Interface → Text size. Every font Luminescent sets goes through fs() or qfont().
TEXT_SCALE = 1.0


def set_text_scale(scale) -> bool:
    """Returns True when it changed (the caller then restyles)."""
    global TEXT_SCALE
    try:
        scale = min(1.6, max(0.8, float(scale)))
    except (TypeError, ValueError):
        scale = 1.0
    changed = abs(scale - TEXT_SCALE) > 0.001
    TEXT_SCALE = scale
    return changed


def fs(px: float) -> int:
    """A font size in pixels at the chosen text size."""
    return max(1, round(px * TEXT_SCALE))


def _scale_css(css: str) -> str:
    return re.sub(r"font-size:\s*(\d+(?:\.\d+)?)px", lambda m: f"font-size: {fs(float(m.group(1)))}px", css)


def luminance(hex_colour: str) -> float:
    h = hex_colour.lstrip("#")
    r, g, b = int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16)
    return 0.299 * r + 0.587 * g + 0.114 * b


def mix(a: str, b: str, t: float) -> str:
    """`a` blended towards `b` by t (0..1), as #RRGGBB."""
    ha, hb = a.lstrip("#"), b.lstrip("#")
    out = []
    for i in (0, 2, 4):
        x, y = int(ha[i:i + 2], 16), int(hb[i:i + 2], 16)
        out.append(round(x + (y - x) * t))
    return "#" + "".join(f"{v:02X}" for v in out)


def derive_primary_text(colours: dict) -> str:
    """Accent text for themes made before the role existed: a lighter accent on dark, a darker one on light."""
    prim = colours.get("primary") or "#7B6CF6"
    bg = colours.get("bg_base") or "#131317"
    try:
        return mix(prim, "#000000", 0.45) if luminance(bg) > 140 else mix(prim, "#FFFFFF", 0.35)
    except ValueError:
        return prim


def use(values: dict) -> Palette:
    """Make `values` (a theme's colours + look) the active palette. Unknown keys are ignored."""
    global P
    base = {k: v for k, v in Palette().__dict__.items()}
    vals = dict(values or {})
    if not vals.get("primary_text"):
        vals["primary_text"] = derive_primary_text(vals)
    base.update({k: v for k, v in vals.items() if k in _FIELDS and v not in (None, "")})
    try:
        base["radius"] = int(base["radius"])
        base["image_opacity"] = float(base["image_opacity"])
    except (TypeError, ValueError):
        base["radius"], base["image_opacity"] = 10, 0.35
    P = Palette(**base)
    return P


def is_light() -> bool:
    """True when the background is light (picks the light tray icon etc.)."""
    return luminance(P.bg_base) > 140


# ── per-module accents ────────────────────────────────────────────────────────
# A theme can give a module its own accent ("modules": {"notes": "#3DBE96"}). Windows built inside
# `with module_colours("notes"):` bake that accent in and carry a stylesheet made from it.
_module_accents: dict[str, str] = {}
_overridden = False


def set_module_accents(accents: dict | None):
    _module_accents.clear()
    for k, v in (accents or {}).items():
        if isinstance(v, str) and len(v) == 7 and v.startswith("#"):
            _module_accents[k] = v


def accent_roles(accent: str, bg: str) -> dict:
    """The accent-derived colours for `accent` on background `bg`."""
    light = luminance(bg) > 140
    return {
        "primary": accent,
        "primary_hover": mix(accent, "#000000", 0.12) if light else mix(accent, "#FFFFFF", 0.14),
        "primary_muted": mix(accent, bg, 0.80 if light else 0.78),
        "primary_text": mix(accent, "#000000", 0.45) if light else mix(accent, "#FFFFFF", 0.35),
        "on_primary": "#1A1600" if luminance(accent) > 150 else "#FFFFFF",
    }


class module_colours:
    """Context manager: the palette with this module's accent (if the theme sets one) while building its windows."""

    def __init__(self, key: str):
        self.key = key
        self._old = None

    def __enter__(self):
        global P, _overridden
        accent = _module_accents.get(self.key)
        if accent:
            self._old = (P, _overridden)
            P = Palette(**{**P.__dict__, **accent_roles(accent, P.bg_base)})
            _overridden = True
        return self

    def __exit__(self, *exc):
        global P, _overridden
        if self._old is not None:
            P, _overridden = self._old
        return False


def overridden() -> bool:
    """True while a module_colours() block is active (FloatingPanel then carries its own stylesheet)."""
    return _overridden


# ── generating a theme ────────────────────────────────────────────────────────
def _hsl(h: float, s: float, l: float) -> str:
    import colorsys
    r, g, b = colorsys.hls_to_rgb(h % 1.0, max(0.0, min(1.0, l)), max(0.0, min(1.0, s)))
    return "#" + "".join(f"{round(v * 255):02X}" for v in (r, g, b))


def generate(accent: str, light: bool) -> dict:
    """A full set of theme colours that go with one accent colour, on a dark or light background
    tinted very slightly towards the accent's hue."""
    import colorsys
    h = accent.lstrip("#")
    r, g, b = int(h[0:2], 16) / 255, int(h[2:4], 16) / 255, int(h[4:6], 16) / 255
    hue, _l, sat = colorsys.rgb_to_hls(r, g, b)
    tint = min(0.22, sat * 0.18)
    if light:
        c = {"bg_base": _hsl(hue, tint, 0.995), "bg_card": _hsl(hue, tint, 0.965), "bg_card_hover": _hsl(hue, tint, 0.935),
             "bg_frame": _hsl(hue, tint, 0.95), "bg_header": _hsl(hue, tint, 0.98), "border": _hsl(hue, tint, 0.89),
             "text_primary": _hsl(hue, tint, 0.08), "text_secondary": _hsl(hue, tint * 0.8, 0.30),
             "text_muted": _hsl(hue, tint * 0.6, 0.52)}
    else:
        c = {"bg_base": _hsl(hue, tint, 0.08), "bg_card": _hsl(hue, tint, 0.115), "bg_card_hover": _hsl(hue, tint, 0.145),
             "bg_frame": _hsl(hue, tint, 0.06), "bg_header": _hsl(hue, tint, 0.10), "border": _hsl(hue, tint, 0.18),
             "text_primary": _hsl(hue, tint * 0.6, 0.93), "text_secondary": _hsl(hue, tint * 0.6, 0.67),
             "text_muted": _hsl(hue, tint * 0.5, 0.45)}
    c.update(accent_roles(accent.upper(), c["bg_base"]))
    c["folder_color"] = c["bg_card"]
    c["folder_border"] = c["primary_text"]
    return c


def rgba(hex_colour: str, alpha: float) -> str:
    h = hex_colour.lstrip("#")
    r, g, b = int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16)
    return f"rgba({r},{g},{b},{int(max(0.0, min(1.0, alpha)) * 255)})"


def scrim_rgb() -> tuple[int, int, int]:
    """The tint of the dimmed screen: near-black on dark themes, a pale paper colour on light ones."""
    return (236, 236, 230) if is_light() else (6, 6, 10)


def glow_alpha() -> float:
    return 0.32 if is_light() else 0.26


# ── fonts ─────────────────────────────────────────────────────────────────────
def load_fonts():
    """Register the bundled Geist / Geist Mono files with Qt (once, at start-up)."""
    from PyQt6.QtGui import QFontDatabase
    from core import APP_DIR
    for path in sorted(glob.glob(os.path.join(APP_DIR, "assets", "fonts", "*.ttf"))):
        QFontDatabase.addApplicationFont(path)


def qfont(px: float, weight: int = 400, mono: bool = False, spacing: float = 0.0, family: str = "",
          scaled: bool = True):
    """A QFont in pixels. `spacing` is letter spacing in em (0.14 = the design's .14em). `scaled` follows the
    Text size setting (off for text drawn on a zoomable canvas, which has its own zoom)."""
    from PyQt6.QtGui import QFont
    f = QFont(family or (MONO_FAMILY if mono else P.font_family))
    if scaled:
        px *= TEXT_SCALE
    f.setPixelSize(max(1, round(px)))
    f.setWeight(QFont.Weight(weight))
    if spacing:
        f.setLetterSpacing(QFont.SpacingType.AbsoluteSpacing, px * spacing)
    if mono:
        f.setStyleHint(QFont.StyleHint.Monospace)
    return f


# ── the logo ──────────────────────────────────────────────────────────────────
def sparkle_path(x: float, y: float, size: float):
    """The Luminescent mark: a four-pointed sparkle with curved sides, in a size×size box at (x, y)."""
    from PyQt6.QtGui import QPainterPath
    s = size / 100.0

    def pt(a, b):
        return x + a * s, y + b * s

    path = QPainterPath()
    path.moveTo(*pt(50, 0))
    path.cubicTo(*pt(54, 34), *pt(66, 46), *pt(100, 50))
    path.cubicTo(*pt(66, 54), *pt(54, 66), *pt(50, 100))
    path.cubicTo(*pt(46, 66), *pt(34, 54), *pt(0, 50))
    path.cubicTo(*pt(34, 46), *pt(46, 34), *pt(50, 0))
    path.closeSubpath()
    return path


def sparkle_pixmap(size: int, colour: str = "", pad: float = 0.0):
    """The mark as a transparent pixmap (pad = empty margin as a fraction of size)."""
    from PyQt6.QtGui import QPixmap, QPainter, QColor
    from PyQt6.QtCore import Qt
    px = QPixmap(size, size)
    px.fill(QColor(0, 0, 0, 0))
    p = QPainter(px)
    p.setRenderHint(QPainter.RenderHint.Antialiasing)
    p.setPen(Qt.PenStyle.NoPen)
    p.setBrush(QColor(colour or P.primary))
    m = size * pad
    p.drawPath(sparkle_path(m, m, size - 2 * m))
    p.end()
    return px


# ── generated stylesheet images ───────────────────────────────────────────────
def _ui_png(name: str, colour: str, w: int, h: int, draw) -> str:
    from core import DATA_DIR
    from PyQt6.QtGui import QImage, QPainter
    folder = os.path.join(DATA_DIR, "ui")
    path = os.path.join(folder, f"{name}-{colour.lstrip('#').lower()}.png")
    if not os.path.isfile(path):
        os.makedirs(folder, exist_ok=True)
        img = QImage(w, h, QImage.Format.Format_ARGB32_Premultiplied)
        img.fill(0)
        p = QPainter(img)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        draw(p)
        p.end()
        img.save(path, "PNG")
    return path.replace("\\", "/")


def _arrow(direction: str, colour: str) -> str:
    """A small triangle PNG for spin boxes and combo boxes (Qt has no CSS border-triangle trick)."""
    from PyQt6.QtGui import QColor, QPolygonF
    from PyQt6.QtCore import QPointF, Qt

    def draw(p):
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor(colour))
        pts = [QPointF(1, 1), QPointF(19, 1), QPointF(10, 11)] if direction == "down" else \
            [QPointF(1, 11), QPointF(19, 11), QPointF(10, 1)]
        p.drawPolygon(QPolygonF(pts))
    return _ui_png(f"arrow-{direction}", colour, 20, 12, draw)


def _check(colour: str) -> str:
    """The ✓ inside a ticked check box."""
    from PyQt6.QtGui import QColor, QPen, QPainterPath
    from PyQt6.QtCore import Qt

    def draw(p):
        pen = QPen(QColor(colour), 3.2)
        pen.setCapStyle(Qt.PenCapStyle.RoundCap)
        pen.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
        p.setPen(pen)
        path = QPainterPath()
        path.moveTo(9, 17)
        path.lineTo(14, 22)
        path.lineTo(23, 11)
        p.drawPath(path)
    return _ui_png("check", colour, 32, 32, draw)


def stylesheet() -> str:
    p = P
    r = p.radius
    rs = max(0, r - 4)          # small controls: buttons, inputs
    rb = max(0, r - 2)          # boxes inside a panel
    mono = MONO_CSS
    return _scale_css(f"""
    QWidget {{
        color: {p.text_primary};
    }}
    *[mono="true"] {{ font-family: {mono}; }}
    QLabel {{ background: transparent; }}
    QLabel[tone="primary"] {{ color: {p.text_primary}; }}
    QLabel[tone="secondary"] {{ color: {p.text_secondary}; }}
    QLabel[tone="muted"] {{ color: {p.text_muted}; }}
    QLabel[tone="accent"] {{ color: {p.primary_text}; }}
    QLabel[tone="accent2"] {{ color: {p.primary}; }}
    QLabel[tone="danger"] {{ color: {DANGER}; }}
    QToolTip {{
        background: {p.bg_base}; color: {p.text_primary};
        border: 1px solid {p.border}; border-radius: 4px; padding: 5px 8px;
    }}
    QDialog, QMessageBox, QInputDialog, QColorDialog, QFileDialog {{
        background: {p.bg_base};
    }}

    QMenu {{
        background-color: {p.bg_base}; color: {p.text_primary};
        border: 1px solid {p.border}; border-radius: {rb}px; padding: 5px; font-size: 13px;
    }}
    QMenu::item {{ padding: 7px 24px 7px 12px; border-radius: {rs}px; }}
    QMenu::item:selected {{ background-color: {p.bg_card_hover}; color: {p.text_primary}; }}
    QMenu::item:disabled {{ color: {p.text_muted}; }}
    QMenu::separator {{ height: 1px; background: {p.border}; margin: 4px 8px; }}
    QMenu::indicator {{ width: 14px; height: 14px; margin-left: 4px; }}
    QMenu::indicator:checked {{ background: {p.primary}; border-radius: 4px; image: url({_check(p.on_primary)}); }}
    QMenu::indicator:unchecked {{ background: transparent; border: 1px solid {p.text_muted}; border-radius: 4px; }}

    QPushButton {{
        background: {p.bg_card}; color: {p.text_primary};
        border: 1px solid {p.border}; border-radius: {rs}px;
        padding: 6px 12px; font-size: 13px; font-weight: 500;
    }}
    QPushButton:hover {{ background: {p.bg_card_hover}; }}
    QPushButton:pressed {{ background: {p.bg_card_hover}; border-color: {p.text_muted}; }}
    QPushButton:checked {{ background: {p.primary_muted}; color: {p.text_primary}; border-color: {p.primary}; }}
    QPushButton:disabled {{ color: {p.text_muted}; background: {p.bg_frame}; }}
    QPushButton[variant="primary"] {{
        background: {p.primary}; color: {p.on_primary}; border: 1px solid {p.primary}; padding: 6px 14px;
    }}
    QPushButton[variant="primary"]:hover {{ background: {p.primary_hover}; border-color: {p.primary_hover}; }}
    QPushButton[variant="ghost"] {{ background: transparent; border: 1px solid transparent; color: {p.text_secondary}; }}
    QPushButton[variant="ghost"]:hover {{ background: {p.bg_card_hover}; color: {p.text_primary}; }}
    QPushButton[variant="ghost"]:checked {{ background: {p.primary_muted}; color: {p.primary_text}; }}
    QPushButton[variant="outline"] {{ background: transparent; color: {p.text_primary}; }}
    QPushButton[variant="outline"]:hover {{ background: {p.bg_card_hover}; }}
    QPushButton[variant="danger"] {{ background: transparent; border: 1px solid transparent; color: {DANGER}; }}
    QPushButton[variant="danger"]:hover {{ background: {rgba(DANGER, 0.12)}; }}
    QPushButton[variant="pill"] {{ border-radius: 16px; padding: 7px 18px; font-size: 13px; }}
    QPushButton[variant="pill"][accent="true"] {{ background: {p.primary}; color: {p.on_primary}; border-color: {p.primary}; }}
    QPushButton[variant="pill"][accent="true"]:hover {{ background: {p.primary_hover}; }}

    QLineEdit, QTextEdit, QPlainTextEdit, QSpinBox, QDoubleSpinBox, QComboBox, QDateEdit, QTimeEdit, QDateTimeEdit {{
        background: {p.bg_frame}; color: {p.text_primary};
        border: 1px solid {p.border}; border-radius: {rs}px; padding: 5px 10px; font-size: 13px;
        selection-background-color: {p.primary_muted}; selection-color: {p.text_primary};
    }}
    QLineEdit:focus, QTextEdit:focus, QPlainTextEdit:focus, QSpinBox:focus, QDoubleSpinBox:focus,
    QComboBox:focus, QDateEdit:focus, QTimeEdit:focus, QDateTimeEdit:focus {{ border-color: {p.primary}; }}
    QLineEdit[invalid="true"] {{ border-color: {DANGER}; }}
    QLineEdit:disabled, QSpinBox:disabled, QComboBox:disabled {{ color: {p.text_muted}; }}
    QSpinBox, QDoubleSpinBox, QTimeEdit, QDateEdit, QDateTimeEdit {{ font-family: {mono}; font-size: 12px; padding-right: 26px; }}
    QSpinBox::up-button, QDoubleSpinBox::up-button, QTimeEdit::up-button, QDateEdit::up-button, QDateTimeEdit::up-button {{
        subcontrol-origin: border; subcontrol-position: top right; width: 22px;
        border-left: 1px solid {p.border}; border-top-right-radius: {rs}px; background: transparent;
    }}
    QSpinBox::down-button, QDoubleSpinBox::down-button, QTimeEdit::down-button, QDateEdit::down-button, QDateTimeEdit::down-button {{
        subcontrol-origin: border; subcontrol-position: bottom right; width: 22px;
        border-left: 1px solid {p.border}; border-top: 1px solid {p.border};
        border-bottom-right-radius: {rs}px; background: transparent;
    }}
    QSpinBox::up-button:hover, QDoubleSpinBox::up-button:hover, QSpinBox::down-button:hover,
    QDoubleSpinBox::down-button:hover {{ background: {p.bg_card_hover}; }}
    QSpinBox::up-arrow, QDoubleSpinBox::up-arrow, QTimeEdit::up-arrow, QDateEdit::up-arrow, QDateTimeEdit::up-arrow {{
        image: url({_arrow("up", p.text_muted)}); width: 7px; height: 4px;
    }}
    QSpinBox::down-arrow, QDoubleSpinBox::down-arrow, QTimeEdit::down-arrow, QDateEdit::down-arrow, QDateTimeEdit::down-arrow {{
        image: url({_arrow("down", p.text_muted)}); width: 7px; height: 4px;
    }}
    QComboBox {{ font-weight: 500; }}
    QComboBox::drop-down {{ border: none; width: 26px; }}
    QComboBox::down-arrow {{ image: url({_arrow("down", p.text_muted)}); width: 8px; height: 5px; margin-right: 8px; }}
    QComboBox QAbstractItemView {{
        background: {p.bg_base}; color: {p.text_primary}; border: 1px solid {p.border}; padding: 4px;
        selection-background-color: {p.bg_card_hover}; selection-color: {p.text_primary}; outline: none;
    }}

    QCheckBox {{ spacing: 9px; background: transparent; font-size: 13px; }}
    QCheckBox::indicator {{
        width: 12px; height: 12px; border: 2px solid {p.text_muted}; border-radius: 4px; background: transparent;
    }}
    QCheckBox::indicator:hover {{ border-color: {p.primary}; }}
    QCheckBox::indicator:checked {{
        background: {p.primary}; border-color: {p.primary}; image: url({_check(p.on_primary)});
    }}
    QRadioButton {{ spacing: 9px; background: transparent; }}
    QRadioButton::indicator {{
        width: 12px; height: 12px; border: 2px solid {p.text_muted}; border-radius: 8px; background: transparent;
    }}
    QRadioButton::indicator:checked {{ background: {p.primary}; border-color: {p.primary}; }}

    QScrollBar:vertical {{ background: transparent; width: 8px; margin: 2px; }}
    QScrollBar::handle:vertical {{ background: {p.border}; border-radius: 3px; min-height: 24px; }}
    QScrollBar::handle:vertical:hover {{ background: {p.text_muted}; }}
    QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical {{ height: 0; }}
    QScrollBar::add-page:vertical, QScrollBar::sub-page:vertical {{ background: transparent; }}
    QScrollBar:horizontal {{ background: transparent; height: 8px; margin: 2px; }}
    QScrollBar::handle:horizontal {{ background: {p.border}; border-radius: 3px; min-width: 24px; }}
    QScrollBar::handle:horizontal:hover {{ background: {p.text_muted}; }}
    QScrollBar::add-line:horizontal, QScrollBar::sub-line:horizontal {{ width: 0; }}
    QScrollBar::add-page:horizontal, QScrollBar::sub-page:horizontal {{ background: transparent; }}
    QScrollArea {{ background: transparent; border: none; }}
    QAbstractScrollArea {{ background: transparent; }}
    QScrollArea > #qt_scrollarea_viewport, QScrollArea > #qt_scrollarea_viewport > QWidget {{ background: transparent; }}

    QListWidget, QTreeWidget, QTableWidget {{
        background: {p.bg_card}; border: 1px solid {p.border}; border-radius: {rb}px; outline: none;
    }}
    QListWidget::item {{ padding: 6px 8px; border-radius: {rs}px; }}
    QListWidget::item:selected, QTableWidget::item:selected {{ background: {p.primary_muted}; color: {p.text_primary}; }}
    QListWidget::item:hover {{ background: {p.bg_card_hover}; }}
    QHeaderView::section {{
        background: {p.bg_header}; color: {p.text_muted}; border: none; font-family: {mono}; font-size: 10px;
        border-bottom: 1px solid {p.border}; padding: 6px 8px;
    }}
    QTableWidget {{ gridline-color: {p.border}; }}
    QTableWidget::item {{ padding: 4px; }}

    QSlider::groove:horizontal {{
        height: 4px; background: {p.bg_frame}; border: 1px solid {p.border}; border-radius: 3px;
    }}
    QSlider::sub-page:horizontal {{ background: {p.primary}; border: 1px solid {p.primary}; border-radius: 3px; }}
    QSlider::handle:horizontal {{
        width: 10px; height: 10px; margin: -6px 0; background: {p.text_primary};
        border: 2px solid {p.primary}; border-radius: 7px;
    }}
    QProgressBar {{ background: {p.bg_frame}; border: none; border-radius: 2px; text-align: center; color: transparent; }}
    QProgressBar::chunk {{ background: {p.primary}; border-radius: 2px; }}
    QGroupBox {{
        border: 1px solid {p.border}; border-radius: {r}px; margin-top: 10px; padding: 10px 6px 6px 6px;
    }}
    QGroupBox::title {{ subcontrol-origin: margin; left: 10px; padding: 0 4px; color: {p.text_secondary}; }}
    QSplitter::handle {{ background: {p.border}; }}
    QStatusBar {{ background: {p.bg_header}; color: {p.text_secondary}; }}
    QCalendarWidget QWidget {{ background: {p.bg_card}; }}
    QCalendarWidget QAbstractItemView {{ selection-background-color: {p.primary_muted}; selection-color: {p.text_primary}; }}

    /* ── Luminescent overlay (ui/) ── */
    #lumPanel {{
        background: {rgba(p.bg_base, 0.9 if p.background_image else 1.0)};
        border: 1px solid {p.border}; border-radius: {r}px;
    }}
    #lumNav {{
        background: {p.bg_base}; border: 1px solid {p.border}; border-top: none;
        border-bottom-left-radius: 16px; border-bottom-right-radius: 16px;
    }}
    #lumNavLine {{ background: {p.primary}; }}
    QPushButton[nav="true"] {{
        background: transparent; border: none; border-radius: 16px; padding: 9px 20px;
        color: {p.text_secondary}; font-size: 14px; font-weight: 500;
    }}
    QPushButton[nav="true"]:hover {{ background: {p.bg_card_hover}; color: {p.text_primary}; }}
    QPushButton[nav="true"]:checked {{ background: {p.primary}; color: {p.on_primary}; }}
    #lumClose {{
        background: {p.bg_base}; border: 1px solid {p.border}; border-radius: 18px;
        color: {p.text_secondary}; font-size: 13px; padding: 0;
    }}
    #lumClose:hover {{ color: {p.text_primary}; background: {p.bg_card_hover}; }}
    #lumDock {{ background: {p.bg_base}; border: 1px solid {p.border}; border-radius: 22px; }}
    #lumDock QPushButton {{ border-radius: 16px; padding: 7px 14px; font-size: 13px; }}
    #lumDock QPushButton[variant="ghost"] {{ color: {p.text_secondary}; }}
    #lumDockSep {{ background: {p.border}; }}
    #lumChip {{ background: {rgba(p.bg_base, 0.88)}; border: 1px solid {p.border}; border-radius: 13px; }}
    #lumSearch {{ background: {p.bg_frame}; border-radius: 16px; padding: 4px 12px; }}
    #tabRow[match="true"] {{ background: {p.primary_muted}; }}
    #tabRow[dim="true"] #tabRowText {{ color: {p.text_muted}; }}
    #tabHeading {{ color: {p.text_muted}; font-family: {mono}; }}
    #boardTab {{ background: transparent; border: none; border-radius: 14px; padding: 5px 12px; color: {p.text_secondary}; }}
    #boardTab:checked {{ background: {p.primary_muted}; color: {p.primary_text}; }}
    #lumHint {{ color: {p.text_muted}; font-size: 12px; }}
    #lumH1 {{ font-size: 20px; font-weight: 600; }}
    #lumPageTitle {{ font-size: 15px; font-weight: 600; }}
    #lumRule {{ background: {p.border}; }}

    #tabBox {{ background: {p.bg_card}; border: 1px solid {p.border}; border-radius: {rb}px; }}
    #tabBox[active="true"] {{ border-color: {p.primary}; }}
    #tabBox[selected="true"] {{ border: 1px solid {p.primary}; background: {mix(p.bg_card, p.primary, 0.07)}; }}
    #pageBody {{ background: {p.bg_base}; border: none; border-bottom-left-radius: {rb}px;
                 border-bottom-right-radius: {rb}px; }}
    #lumBand {{ background: {rgba(p.primary, 0.12)}; border: 1px solid {rgba(p.primary, 0.75)}; border-radius: 3px; }}
    #tabHeader {{
        background: {p.bg_header}; border: none; border-bottom: 1px solid {p.border};
        border-top-left-radius: {rb}px; border-top-right-radius: {rb}px;
    }}
    #tabHeader[collapsed="true"] {{ border-bottom: none; border-radius: {rb}px; }}
    #tabAccent {{ background: {p.primary}; border-radius: 1px; }}
    #tabTitle {{ color: {p.text_primary}; font-family: {mono}; }}
    #tabRow {{ background: transparent; border-radius: 0; }}
    #tabRow:hover {{ background: {p.bg_card_hover}; }}
    #tabRowText {{ color: {p.text_primary}; }}
    #typeBadge {{
        color: {p.text_muted}; border: 1px solid {p.border}; border-radius: 4px; font-family: {mono};
    }}

    #themeCard {{ background: {p.bg_card}; border: 1px solid {p.border}; border-radius: {r}px; }}
    #themeCard:hover {{ background: {p.bg_card_hover}; }}
    #themeCard[active="true"] {{ border: 2px solid {p.primary}; }}
    #addCard {{ background: transparent; border: 1px dashed {p.border}; border-radius: {r}px; }}
    #addCard:hover {{ border-color: {p.text_muted}; }}
    #folderHeader {{ background: transparent; border: none; border-bottom: 1px solid {p.border}; }}

    #clipCard {{ background: {p.bg_card}; border: 1px solid {p.border}; border-radius: {r}px; }}
    #clipCard:hover {{ background: {p.bg_card_hover}; }}
    #clipPreview {{ background: transparent; border: none; border-bottom: 1px solid {p.border};
                    border-top-left-radius: {r}px; border-top-right-radius: {r}px; }}

    #setProfileBar, #setFooter {{ background: {p.bg_header}; border: none; }}
    #setProfileBar {{ border-bottom: 1px solid {p.border}; }}
    #setFooter {{ border-top: 1px solid {p.border}; }}
    #setSidebar {{ background: {p.bg_frame}; border: none; border-right: 1px solid {p.border}; }}
    QPushButton[side="true"] {{
        background: transparent; border: none; border-radius: {rs}px; padding: 0 12px; text-align: left;
        color: {p.text_secondary}; font-size: 13px; font-weight: 500;
    }}
    QPushButton[side="true"]:hover {{ background: {p.bg_card_hover}; }}
    QPushButton[side="true"]:checked {{ background: {p.primary_muted}; color: {p.text_primary}; }}
    #setRow {{ border: none; border-bottom: 1px solid {p.border}; }}
    #keyCap {{
        background: {p.bg_card}; border: 1px solid {p.border}; border-radius: 4px; color: {p.text_primary};
        font-family: {mono}; padding: 2px 6px;
    }}
    #keyBox {{ background: {p.bg_frame}; border: 1px solid {p.border}; border-radius: {rs}px; }}
    #keyBox:hover {{ border-color: {p.text_muted}; }}
    #navRow {{ background: {p.bg_card}; border: 1px solid {p.border}; border-radius: {rs}px; }}

    #paletteBox {{ background: {p.bg_base}; border: 1px solid {p.border}; border-radius: {r}px; }}
    #paletteHead {{ border: none; border-bottom: 1px solid {p.border}; }}
    #paletteFoot {{ background: {p.bg_header}; border: none; border-top: 1px solid {p.border};
                    border-bottom-left-radius: {r}px; border-bottom-right-radius: {r}px; }}
    #paletteInput {{ font-size: 16px; padding: 0; border: none; border-radius: 0; background: transparent; }}
    #paletteList {{ background: transparent; border: none; }}
    #paletteList::item {{ padding: 0; border-radius: {rs}px; }}
    #paletteList::item:selected {{ background: transparent; }}

    #cheatCard {{ background: {p.bg_card}; border: 1px solid {p.border}; border-radius: {r}px; }}
    """)
