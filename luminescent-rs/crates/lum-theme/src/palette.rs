//! The palette a theme gives (`core/theme.py`): sixteen colour roles, the look (corner radius, font, glow,
//! picture) and the gradients, plus the looks single modules can have.

use serde_json::{Map, Value, json};

use crate::colour::Rgb;
use crate::gradient::{Gradient, clean_all, gradients_to_value};
use crate::tones::Colours;

pub const DANGER: Rgb = Rgb::from_u32(0xE8645E);
pub const MONO_FAMILY: &str = "Geist Mono";
pub const DEFAULT_FONT: &str = "Geist";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Primary,
    PrimaryHover,
    PrimaryMuted,
    PrimaryText,
    OnPrimary,
    BgBase,
    BgCard,
    BgCardHover,
    BgFrame,
    BgHeader,
    Border,
    TextPrimary,
    TextSecondary,
    TextMuted,
    FolderColor,
    FolderBorder,
}

impl Role {
    /// In the order of `COLOUR_ROLES`, which is the order the theme editor lists them in.
    pub const ALL: [Role; 16] = [
        Role::Primary,
        Role::PrimaryHover,
        Role::PrimaryMuted,
        Role::PrimaryText,
        Role::OnPrimary,
        Role::BgBase,
        Role::BgCard,
        Role::BgCardHover,
        Role::BgFrame,
        Role::BgHeader,
        Role::Border,
        Role::TextPrimary,
        Role::TextSecondary,
        Role::TextMuted,
        Role::FolderColor,
        Role::FolderBorder,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Role::Primary => "primary",
            Role::PrimaryHover => "primary_hover",
            Role::PrimaryMuted => "primary_muted",
            Role::PrimaryText => "primary_text",
            Role::OnPrimary => "on_primary",
            Role::BgBase => "bg_base",
            Role::BgCard => "bg_card",
            Role::BgCardHover => "bg_card_hover",
            Role::BgFrame => "bg_frame",
            Role::BgHeader => "bg_header",
            Role::Border => "border",
            Role::TextPrimary => "text_primary",
            Role::TextSecondary => "text_secondary",
            Role::TextMuted => "text_muted",
            Role::FolderColor => "folder_color",
            Role::FolderBorder => "folder_border",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Role::Primary => "Accent",
            Role::PrimaryHover => "Accent (hover)",
            Role::PrimaryMuted => "Accent (muted background)",
            Role::PrimaryText => "Accent text",
            Role::OnPrimary => "Text on accent",
            Role::BgBase => "Background",
            Role::BgCard => "Cards",
            Role::BgCardHover => "Cards (hover)",
            Role::BgFrame => "Frames / inputs",
            Role::BgHeader => "Headers",
            Role::Border => "Borders",
            Role::TextPrimary => "Text",
            Role::TextSecondary => "Text (secondary)",
            Role::TextMuted => "Text (muted)",
            Role::FolderColor => "Folder cards",
            Role::FolderBorder => "Folder card outline",
        }
    }

    pub fn from_key(key: &str) -> Option<Role> {
        Role::ALL.into_iter().find(|r| r.key() == key)
    }

    fn index(self) -> usize {
        Role::ALL.iter().position(|r| *r == self).unwrap_or(0)
    }
}

/// The built-in colours, as the `PRESETS` of `core/theme.py` (in `COLOUR_ROLES` order).
const DARK: [u32; 16] = [
    0x7B6CF6, 0x9184F8, 0x24213B, 0xA99FFA, 0xFFFFFF, 0x131317, 0x1B1B21, 0x23232A, 0x0E0E11, 0x17171C,
    0x2A2A30, 0xECECF1, 0xA3A3AF, 0x6C6C78, 0x1B1B21, 0xA99FFA,
];
const LIGHT: [u32; 16] = [
    0xF2B705, 0xDCA704, 0xFCF1CD, 0x8A6A00, 0x1A1600, 0xFFFFFF, 0xF6F6F3, 0xEEEEEA, 0xF1F1ED, 0xFAFAF8,
    0xE4E4E1, 0x18180F, 0x4F4F46, 0x86867C, 0xF6F6F3, 0xF2B705,
];
/// The built-in colours before the 2026-10-25 redesign: untouched built-ins are moved to the new ones. Only the
/// roles that existed then (no `primary_text`).
const OLD_DARK: [(Role, u32); 15] = [
    (Role::Primary, 0x7B6CF6),
    (Role::PrimaryHover, 0x6656E8),
    (Role::PrimaryMuted, 0x28234A),
    (Role::OnPrimary, 0xFFFFFF),
    (Role::BgBase, 0x121214),
    (Role::BgCard, 0x1C1C20),
    (Role::BgCardHover, 0x25252B),
    (Role::BgFrame, 0x17171A),
    (Role::BgHeader, 0x141417),
    (Role::Border, 0x3A3466),
    (Role::TextPrimary, 0xECEAF6),
    (Role::TextSecondary, 0x9A97AE),
    (Role::TextMuted, 0x55536A),
    (Role::FolderColor, 0x221E3D),
    (Role::FolderBorder, 0xA99CFF),
];
const OLD_LIGHT: [(Role, u32); 15] = [
    (Role::Primary, 0xF2B705),
    (Role::PrimaryHover, 0xDDA600),
    (Role::PrimaryMuted, 0xFFF3C4),
    (Role::OnPrimary, 0x1E1A0E),
    (Role::BgBase, 0xFFFFFF),
    (Role::BgCard, 0xF6F5F1),
    (Role::BgCardHover, 0xEEEBE1),
    (Role::BgFrame, 0xFAF9F6),
    (Role::BgHeader, 0xF3F1EA),
    (Role::Border, 0xE3D29A),
    (Role::TextPrimary, 0x1F1D18),
    (Role::TextSecondary, 0x5F5B50),
    (Role::TextMuted, 0xA29D8E),
    (Role::FolderColor, 0xFFF6D6),
    (Role::FolderBorder, 0xF2B705),
];

/// The colours of the built-in `dark` or `light` theme as theme-file text.
pub fn preset_colours(light: bool) -> Colours {
    let table = if light { &LIGHT } else { &DARK };
    Role::ALL.iter().zip(table).map(|(r, c)| (r.key().to_owned(), Rgb::from_u32(*c).hex())).collect()
}

/// The pre-redesign colours of a built-in theme (`"dark"` or `"light"`), if it has some.
pub fn old_preset(id: &str) -> Option<Vec<(Role, Rgb)>> {
    let table = match id {
        "dark" => &OLD_DARK,
        "light" => &OLD_LIGHT,
        _ => return None,
    };
    Some(table.iter().map(|&(r, c)| (r, Rgb::from_u32(c))).collect())
}

#[derive(Debug, Clone, PartialEq)]
pub struct Palette {
    colours: [Rgb; 16],
    /// Corner radius of panels and big cards.
    pub radius: i64,
    pub font_family: String,
    /// The accent glow around the overlay (off in performance mode).
    pub glow: bool,
    /// Drawn behind the overlay content.
    pub background_image: String,
    pub image_opacity: f64,
    /// Roles that are painted as gradients; the plain colour of such a role is the gradient's middle.
    pub gradients: Vec<(Role, Gradient)>,
}

impl Palette {
    pub fn dark() -> Palette {
        Palette::preset(false)
    }

    pub fn light() -> Palette {
        Palette::preset(true)
    }

    fn preset(light: bool) -> Palette {
        let table = if light { &LIGHT } else { &DARK };
        Palette {
            colours: table.map(Rgb::from_u32),
            radius: 10,
            font_family: DEFAULT_FONT.to_owned(),
            glow: true,
            background_image: String::new(),
            image_opacity: 0.35,
            gradients: Vec::new(),
        }
    }

    pub fn get(&self, role: Role) -> Rgb {
        self.colours[role.index()]
    }

    pub fn set(&mut self, role: Role, colour: Rgb) {
        self.colours[role.index()] = colour;
    }

    pub fn gradient(&self, role: Role) -> Option<&Gradient> {
        self.gradients.iter().find(|(r, _)| *r == role).map(|(_, g)| g)
    }

    /// The background is light (picks the light tray icon, the pale scrim and so on).
    pub fn is_light(&self) -> bool {
        self.get(Role::BgBase).luminance() > 140.0
    }

    /// `theme.palette_from`: a palette from a theme's colours and look, with the gradients' middles as the plain
    /// colours of their roles. Unknown keys are ignored.
    pub fn from_values(values: &Map<String, Value>) -> Palette {
        let mut p = Palette::dark();
        let text =
            |key: &str| values.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
        let mut colours: Vec<(Role, Rgb)> =
            Role::ALL.iter().filter_map(|&r| text(r.key()).and_then(Rgb::parse).map(|c| (r, c))).collect();
        if text("primary_text").is_none() {
            let primary = text("primary").and_then(Rgb::parse).unwrap_or(Rgb::from_u32(0x7B6CF6));
            let bg = text("bg_base").and_then(Rgb::parse).unwrap_or(Rgb::from_u32(0x131317));
            colours.push((Role::PrimaryText, derive_primary_text(primary, bg)));
        }
        for (role, colour) in colours {
            p.set(role, colour);
        }
        if let Some(f) = text("font_family") {
            p.font_family = f.to_owned();
        }
        if let Some(g) = values.get("glow").filter(|v| !v.is_null()) {
            p.glow = truthy(g);
        }
        if let Some(path) = text("background_image") {
            p.background_image = path.to_owned();
        }
        for (key, gradient) in clean_all(values.get("gradients")) {
            if let Some(role) = Role::from_key(&key) {
                p.set(role, gradient.mid());
                p.gradients.push((role, gradient));
            }
        }
        let radius = values.get("radius").filter(|v| !v.is_null() && v.as_str() != Some(""));
        let opacity = values.get("image_opacity").filter(|v| !v.is_null() && v.as_str() != Some(""));
        let parsed_radius = radius.map_or(Some(10), as_int);
        let parsed_opacity = opacity.map_or(Some(0.35), as_float);
        match (parsed_radius, parsed_opacity) {
            (Some(r), Some(o)) => {
                p.radius = r;
                p.image_opacity = o;
            }
            _ => {
                p.radius = 10;
                p.image_opacity = 0.35;
            }
        }
        p
    }

    /// `base` with one module's own colours, gradients and corner radius on top (`theme.section_palette`).
    pub fn with_section(&self, section: &Section) -> Palette {
        let mut p = self.clone();
        if let Some(accent) = section.accent {
            for (role, colour) in
                accent_roles(accent, section.colour(Role::BgBase).unwrap_or(self.get(Role::BgBase)))
            {
                p.set(role, colour);
                p.gradients.retain(|(r, _)| *r != role);
            }
        }
        for &(role, colour) in &section.colours {
            p.set(role, colour);
            // a plain colour of its own replaces the theme's gradient
            p.gradients.retain(|(r, _)| *r != role);
        }
        for (role, gradient) in &section.gradients {
            p.gradients.retain(|(r, _)| r != role);
            p.gradients.push((*role, gradient.clone()));
            if section.colour(*role).is_none() {
                p.set(*role, gradient.mid());
            }
        }
        if let Some(r) = section.radius {
            p.radius = r;
        }
        p
    }
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Null => false,
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

fn as_int(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f.trunc() as i64)),
        Value::String(s) => s.trim().parse().ok(),
        Value::Bool(b) => Some(i64::from(*b)),
        _ => None,
    }
}

fn as_float(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        _ => None,
    }
}

/// Accent text for themes made before the role existed: a lighter accent on dark, a darker one on light.
pub fn derive_primary_text(primary: Rgb, bg: Rgb) -> Rgb {
    if bg.luminance() > 140.0 { primary.mix(Rgb::BLACK, 0.45) } else { primary.mix(Rgb::WHITE, 0.35) }
}

/// The accent-derived colours for `accent` on background `bg`.
pub fn accent_roles(accent: Rgb, bg: Rgb) -> [(Role, Rgb); 5] {
    let light = bg.luminance() > 140.0;
    [
        (Role::Primary, accent),
        (Role::PrimaryHover, if light { accent.mix(Rgb::BLACK, 0.12) } else { accent.mix(Rgb::WHITE, 0.14) }),
        (Role::PrimaryMuted, accent.mix(bg, if light { 0.80 } else { 0.78 })),
        (Role::PrimaryText, if light { accent.mix(Rgb::BLACK, 0.45) } else { accent.mix(Rgb::WHITE, 0.35) }),
        (Role::OnPrimary, if accent.luminance() > 150.0 { Rgb::from_u32(0x1A1600) } else { Rgb::WHITE }),
    ]
}

/// A full set of theme colours that go with one accent, on a dark or light background tinted very slightly
/// towards the accent's hue (`theme.generate`).
pub fn generate(accent: Rgb, light: bool) -> Colours {
    let [r, g, b] = accent.floats();
    let (hue, _lightness, sat) = hls_of(r, g, b);
    let tint = 0.22_f64.min(sat * 0.18);
    let hsl = |s: f64, l: f64| hex_hsl(hue, s, l);
    let mut c = Colours::new();
    let mut put = |key: &str, v: String| {
        c.insert(key.to_owned(), v);
    };
    if light {
        put("bg_base", hsl(tint, 0.995));
        put("bg_card", hsl(tint, 0.965));
        put("bg_card_hover", hsl(tint, 0.935));
        put("bg_frame", hsl(tint, 0.95));
        put("bg_header", hsl(tint, 0.98));
        put("border", hsl(tint, 0.89));
        put("text_primary", hsl(tint, 0.08));
        put("text_secondary", hsl(tint * 0.8, 0.30));
        put("text_muted", hsl(tint * 0.6, 0.52));
    } else {
        put("bg_base", hsl(tint, 0.08));
        put("bg_card", hsl(tint, 0.115));
        put("bg_card_hover", hsl(tint, 0.145));
        put("bg_frame", hsl(tint, 0.06));
        put("bg_header", hsl(tint, 0.10));
        put("border", hsl(tint, 0.18));
        put("text_primary", hsl(tint * 0.6, 0.93));
        put("text_secondary", hsl(tint * 0.6, 0.67));
        put("text_muted", hsl(tint * 0.5, 0.45));
    }
    let bg = Rgb::parse(&c["bg_base"]).unwrap_or(Rgb::BLACK);
    for (role, colour) in accent_roles(accent, bg) {
        c.insert(role.key().to_owned(), colour.hex());
    }
    let card = c["bg_card"].clone();
    let primary_text = c["primary_text"].clone();
    c.insert("folder_color".into(), card);
    c.insert("folder_border".into(), primary_text);
    c
}

/// Python's `colorsys.rgb_to_hls`: (hue 0..1, lightness, saturation).
fn hls_of(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let maxc = r.max(g).max(b);
    let minc = r.min(g).min(b);
    let sumc = maxc + minc;
    let rangec = maxc - minc;
    let l = sumc / 2.0;
    if minc == maxc {
        return (0.0, l, 0.0);
    }
    let s = if l <= 0.5 { rangec / sumc } else { rangec / (2.0 - maxc - minc) };
    let rc = (maxc - r) / rangec;
    let gc = (maxc - g) / rangec;
    let bc = (maxc - b) / rangec;
    let h = if r == maxc {
        bc - gc
    } else if g == maxc {
        2.0 + rc - bc
    } else {
        4.0 + gc - rc
    };
    ((h / 6.0).rem_euclid(1.0), l, s)
}

/// Python's `colorsys.hls_to_rgb`.
pub(crate) fn rgb_of_hls(h: f64, l: f64, s: f64) -> (f64, f64, f64) {
    if s == 0.0 {
        return (l, l, l);
    }
    let m2 = if l <= 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let m1 = 2.0 * l - m2;
    let v = |hue: f64| {
        let hue = hue.rem_euclid(1.0);
        if hue < 1.0 / 6.0 {
            m1 + (m2 - m1) * hue * 6.0
        } else if hue < 0.5 {
            m2
        } else if hue < 2.0 / 3.0 {
            m1 + (m2 - m1) * (2.0 / 3.0 - hue) * 6.0
        } else {
            m1
        }
    };
    (v(h + 1.0 / 3.0), v(h), v(h - 1.0 / 3.0))
}

fn hex_hsl(h: f64, s: f64, l: f64) -> String {
    let (r, g, b) = rgb_of_hls(h.rem_euclid(1.0), l.clamp(0.0, 1.0), s.clamp(0.0, 1.0));
    Rgb::from_floats(r, g, b).hex()
}

/// Every colour role of a theme from a handful (`primary`, `bg`, `card`, `text`, `muted`, `border`), as
/// `themes.colours_from_roles`. Empty without an accent.
pub fn colours_from_roles(roles: &Map<String, Value>) -> Colours {
    let get = |key: &str| roles.get(key).and_then(Value::as_str).unwrap_or("");
    let Some(accent) = get("primary").strip_prefix('#').and_then(|_| Rgb::parse(get("primary"))) else {
        return Colours::new();
    };
    let bg = get("bg");
    let bg_rgb = Rgb::parse(bg).filter(|_| !bg.is_empty());
    let light = bg_rgb.is_some_and(|c| c.luminance() > 140.0);
    let mut cols = preset_colours(light);
    for (k, v) in generate(accent, light) {
        cols.insert(k, v);
    }
    for (role, keys) in [
        ("bg", &["bg_base"][..]),
        ("card", &["bg_card", "folder_color"]),
        ("text", &["text_primary"]),
        ("muted", &["text_secondary"]),
        ("border", &["border"]),
    ] {
        if get(role).starts_with('#') {
            for k in keys {
                cols.insert((*k).to_owned(), get(role).to_owned());
            }
        }
    }
    if bg.starts_with('#') {
        let bg_rgb = Rgb::parse(bg).unwrap_or(Rgb::BLACK);
        let card = cols.get("bg_card").and_then(|c| Rgb::parse(c)).unwrap_or(bg_rgb);
        let text = cols.get("text_primary").and_then(|c| Rgb::parse(c)).unwrap_or(Rgb::WHITE);
        let secondary = cols.get("text_secondary").and_then(|c| Rgb::parse(c)).unwrap_or(text);
        cols.insert("bg_frame".into(), bg_rgb.mix(if light { Rgb::WHITE } else { Rgb::BLACK }, 0.25).hex());
        cols.insert("bg_header".into(), bg_rgb.mix(card, 0.5).hex());
        cols.insert("bg_card_hover".into(), card.mix(text, 0.06).hex());
        cols.insert("text_muted".into(), secondary.mix(bg_rgb, 0.4).hex());
    }
    cols
}

/// One `"modules"` entry (also `"toasts"` and `"alerts"`): an accent, any roles, gradients and a corner radius
/// for that part of the app only.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Section {
    pub accent: Option<Rgb>,
    pub colours: Vec<(Role, Rgb)>,
    pub gradients: Vec<(Role, Gradient)>,
    pub radius: Option<i64>,
}

impl Section {
    /// `theme.clean_section`: `None` when the entry changes nothing. A bare colour (older themes) is an accent.
    pub fn clean(value: &Value) -> Option<Section> {
        let mut out = Section::default();
        match value {
            Value::String(s) => out.accent = Some(Rgb::parse_strict(s)?),
            Value::Object(map) => {
                if let Some(cols) = map.get("colours").and_then(Value::as_object) {
                    out.colours = cols
                        .iter()
                        .filter_map(|(k, v)| Some((Role::from_key(k)?, Rgb::parse_strict(v.as_str()?)?)))
                        .collect();
                }
                for (key, gradient) in clean_all(map.get("gradients")) {
                    if let Some(role) = Role::from_key(&key) {
                        out.gradients.push((role, gradient));
                    }
                }
                out.accent = map.get("accent").and_then(Value::as_str).and_then(Rgb::parse_strict);
                out.radius = map
                    .get("radius")
                    .filter(|v| !v.is_null() && v.as_str() != Some(""))
                    .and_then(as_int)
                    .map(|r| r.clamp(0, 30));
            }
            _ => return None,
        }
        if out.accent.is_none() && out.colours.is_empty() && out.gradients.is_empty() && out.radius.is_none()
        {
            return None;
        }
        Some(out)
    }

    pub fn colour(&self, role: Role) -> Option<Rgb> {
        self.colours.iter().find(|(r, _)| *r == role).map(|(_, c)| *c)
    }

    /// The entry as the theme editor writes it.
    pub fn to_value(&self) -> Value {
        let colours: Map<String, Value> =
            self.colours.iter().map(|(r, c)| (r.key().to_owned(), json!(c.hex()))).collect();
        let gradients: Vec<(String, Gradient)> =
            self.gradients.iter().map(|(r, g)| (r.key().to_owned(), g.clone())).collect();
        let mut out = Map::new();
        if let Some(a) = self.accent {
            out.insert("accent".into(), json!(a.hex()));
        }
        out.insert("colours".into(), Value::Object(colours));
        out.insert("gradients".into(), gradients_to_value(&gradients));
        if let Some(r) = self.radius {
            out.insert("radius".into(), json!(r));
        }
        Value::Object(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_have_every_role() {
        assert_eq!(preset_colours(false)["primary"], "#7B6CF6");
        assert_eq!(preset_colours(true)["folder_border"], "#F2B705");
        assert_eq!(Palette::light().get(Role::OnPrimary), Rgb::from_u32(0x1A1600));
    }

    #[test]
    fn hls_matches_colorsys() {
        let (h, l, s) = hls_of(0.5, 0.25, 0.75);
        let (r, g, b) = rgb_of_hls(h, l, s);
        assert!((r - 0.5).abs() < 1e-12 && (g - 0.25).abs() < 1e-12 && (b - 0.75).abs() < 1e-12);
    }
}
