//! The colour picker's logic without the drawing, from `core/colourpick.py`: reading colours people type,
//! HSV, contrast, the gradient ideas and the recent picks in `picker.json`.

use std::path::Path;

use lum_core::pyjson::Style;
use lum_core::store::{load_json, save_json};
use serde_json::{Value, json};

use crate::colour::Rgb;
use crate::gradient::{Gradient, GradientKind};
use crate::tones::Colours;

pub const BASICS: [u32; 11] = [
    0xFFFFFF, 0x9CA3AF, 0x111111, 0xEF4444, 0xF97316, 0xFACC15, 0x22C55E, 0x14B8A6, 0x3B82F6, 0x8B5CF6,
    0xEC4899,
];

/// The gradients the picker offers as presets.
pub fn presets() -> Vec<(&'static str, Gradient)> {
    let stops = |list: &[(f64, u32)]| -> Vec<(f64, Rgb)> {
        list.iter().map(|&(p, c)| (p, Rgb::from_u32(c))).collect()
    };
    let make = |kind, angle, list: &[(f64, u32)]| Gradient::new(kind, angle, stops(list));
    [
        ("Sunset", make(GradientKind::Linear, 135.0, &[(0.0, 0xFF7E5F), (1.0, 0xFEB47B)])),
        ("Ocean", make(GradientKind::Linear, 135.0, &[(0.0, 0x2E3192), (1.0, 0x1BFFFF)])),
        ("Aurora", make(GradientKind::Linear, 120.0, &[(0.0, 0x00C9A7), (0.5, 0x845EC2), (1.0, 0xFF6F91)])),
        ("Ember", make(GradientKind::Radial, 0.0, &[(0.0, 0xF9D423), (1.0, 0xE65C00)])),
        ("Night", make(GradientKind::Linear, 90.0, &[(0.0, 0x232526), (1.0, 0x0F0F12)])),
        (
            "Prism",
            make(
                GradientKind::Sweep,
                0.0,
                &[(0.0, 0xFF0080), (0.33, 0x7928CA), (0.66, 0x2AF598), (1.0, 0xFF0080)],
            ),
        ),
    ]
    .into_iter()
    .filter_map(|(name, g)| g.map(|g| (name, g)))
    .collect()
}

/// `#abc`, `abcdef`, `rgb(1, 2, 3)`, `hsl(...)` or a colour name as `#RRGGBB` (`None` when it is not a colour).
pub fn parse(text: &str) -> Option<Rgb> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    let low: String = t.to_lowercase().replace(' ', "");
    if let Some(inner) = low.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        let mut parts = inner.split(',').take(3).map(|x| x.parse::<f64>().ok().map(|f| f.trunc()));
        let (r, g, b) = (parts.next()??, parts.next()??, parts.next()??);
        let ch = |v: f64| v.clamp(0.0, 255.0) as u8;
        return Some(Rgb::new(ch(r), ch(g), ch(b)));
    }
    if let Some(inner) = low.strip_prefix("hsl(").and_then(|s| s.strip_suffix(')')) {
        let mut parts = inner.split(',').take(3);
        let h: f64 = parts.next()?.parse().ok()?;
        let s: f64 = parts.next()?.trim_end_matches('%').parse().ok()?;
        let l: f64 = parts.next()?.trim_end_matches('%').parse().ok()?;
        let (r, g, b) = crate::palette::rgb_of_hls(
            (h / 360.0).rem_euclid(1.0),
            (l / 100.0).clamp(0.0, 1.0),
            (s / 100.0).clamp(0.0, 1.0),
        );
        return Some(Rgb::from_floats(r, g, b));
    }
    if !t.starts_with('#') && matches!(t.len(), 3 | 6) && t.chars().all(|c| c.is_ascii_hexdigit()) {
        return qt_colour(&format!("#{t}"));
    }
    qt_colour(t)
}

/// What `QColor(text)` accepts that matters here: `#RGB`, `#RRGGBB`, `#AARRGGBB` and the SVG colour names.
fn qt_colour(text: &str) -> Option<Rgb> {
    if let Some(digits) = text.strip_prefix('#') {
        if !digits.is_ascii() || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        return match digits.len() {
            3 | 6 => Rgb::parse(text),
            8 => Rgb::parse(text),
            _ => None,
        };
    }
    let key = text.to_lowercase();
    NAMES.binary_search_by(|(n, _)| (*n).cmp(key.as_str())).ok().map(|i| Rgb::from_u32(NAMES[i].1))
}

/// (hue 0..360, saturation 0..1, value 0..1), like `hsv_of`. Greys have hue 0.
pub fn hsv_of(c: Rgb) -> (f64, f64, f64) {
    let [r, g, b] = c.floats();
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let v = max;
    let s = if max == 0.0 { 0.0 } else { delta / max };
    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        (g - b) / delta
    } else if max == g {
        2.0 + (b - r) / delta
    } else {
        4.0 + (r - g) / delta
    };
    ((h * 60.0).rem_euclid(360.0), s, v)
}

pub fn rgb_of_hsv(h: f64, s: f64, v: f64) -> Rgb {
    let (s, v) = (s.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
    let h6 = (h.rem_euclid(360.0) / 60.0).min(5.999_999);
    let sector = h6.floor();
    let f = h6 - sector;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match sector as u8 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    Rgb::from_floats(r, g, b)
}

/// WCAG contrast ratio of two colours (1 to 21), with the picker's own 0.03928 threshold.
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let lum = |c: Rgb| {
        let [r, g, bl] =
            c.floats().map(|v| if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) });
        0.2126 * r + 0.7152 * g + 0.0722 * bl
    };
    let (la, lb) = (lum(a), lum(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// A second colour that goes well with this one in a gradient: its hue turned a little, a touch lighter (or
/// darker when it is already light).
pub fn partner(c: Rgb) -> Rgb {
    let (h, s, v) = hsv_of(c);
    let hue = h / 360.0;
    if s < 0.08 {
        // a grey: a lighter or darker grey
        return rgb_of_hsv(h, s, if v > 0.5 { (v - 0.35).max(0.0) } else { (v + 0.35).min(1.0) });
    }
    let v2 = if v < 0.75 { (v * 1.15).min(1.0) } else { v * 0.8 };
    rgb_of_hsv(((hue + 0.09) % 1.0) * 360.0, (s * 1.05).min(1.0), v2)
}

/// Gradients worked out from one colour (the "Ideas" row).
pub fn ideas_for(c: Rgb) -> Vec<(&'static str, Gradient)> {
    let (h, s, v) = hsv_of(c);
    let hue = h / 360.0;
    let sat = s.max(0.45);
    let lin = |name: &'static str, cols: &[Rgb], angle: f64| {
        let n = (cols.len() - 1) as f64;
        let stops = cols.iter().enumerate().map(|(i, &x)| (i as f64 / n, x)).collect();
        Gradient::new(GradientKind::Linear, angle, stops).map(|g| (name, g))
    };
    let turned = |by: f64| rgb_of_hsv(((hue + by) % 1.0) * 360.0, sat, v.max(0.6));
    let glow = Gradient::new(GradientKind::Radial, 0.0, vec![(0.0, c.mix(Rgb::WHITE, 0.35)), (1.0, c)]);
    [
        lin("Next to it", &[c, partner(c)], 135.0),
        lin("Into the dark", &[c, c.mix(Rgb::BLACK, 0.7)], 180.0),
        lin("Into the light", &[c.mix(Rgb::WHITE, 0.55), c], 180.0),
        lin("Opposite", &[c, turned(0.5)], 135.0),
        lin("Three of a kind", &[c, turned(0.33), turned(0.66)], 90.0),
        lin("There and back", &[c, partner(c), c], 90.0),
        glow.map(|g| ("Glow", g)),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// The recent picks: the picker's own list, then the screen colour picker's, without repeats, at most eleven.
pub fn recent(picker_file: &Path, colours_file: &Path) -> Vec<Rgb> {
    let list = |path: &Path| -> Vec<String> {
        load_json(path)
            .and_then(|d| d.get("recent").and_then(Value::as_array).cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect()
    };
    let mut out: Vec<Rgb> = Vec::new();
    for text in list(picker_file).into_iter().chain(list(colours_file)) {
        if let Some(c) = parse(&text)
            && !out.contains(&c)
        {
            out.push(c);
        }
    }
    out.truncate(11);
    out
}

/// Puts a pick at the front of `picker.json`, keeping the other keys and the last fifteen picks.
pub fn remember(picker_file: &Path, colour: Rgb) {
    let mut data = load_json(picker_file).filter(Value::is_object).unwrap_or_else(|| json!({}));
    let hex = colour.hex();
    let mut recent = vec![json!(hex)];
    let old = data.get("recent").and_then(Value::as_array).cloned().unwrap_or_default();
    recent.extend(old.into_iter().filter(|c| c.as_str() != Some(hex.as_str())).take(15));
    data["recent"] = Value::Array(recent);
    if let Err(e) = save_json(picker_file, &data, Style::COMPACT) {
        tracing::warn!("could not save the recent colours: {e}");
    }
}

/// The colours of a palette the picker offers: accent, accent text, muted accent, the backgrounds, the borders
/// and the text colours.
pub fn theme_colours(palette: &crate::Palette) -> Vec<Rgb> {
    use crate::Role::*;
    let mut out: Vec<Rgb> = Vec::new();
    for role in [
        Primary,
        PrimaryText,
        PrimaryMuted,
        BgBase,
        BgCard,
        BgHeader,
        Border,
        TextPrimary,
        TextSecondary,
        TextMuted,
        FolderBorder,
    ] {
        let c = palette.get(role);
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out.truncate(11);
    out
}

/// Plain `#RRGGBB` text for a map of colours, keeping what is not a colour as it is. Handy for the editors.
pub fn normalise(colours: &Colours) -> Colours {
    colours
        .iter()
        .map(|(k, v)| (k.clone(), Rgb::parse_strict(v).map_or_else(|| v.clone(), Rgb::hex)))
        .collect()
}

/// The SVG colour names `QColor(text)` understands, sorted for a binary search.
const NAMES: [(&str, u32); 148] = [
    ("aliceblue", 0xF0F8FF),
    ("antiquewhite", 0xFAEBD7),
    ("aqua", 0x00FFFF),
    ("aquamarine", 0x7FFFD4),
    ("azure", 0xF0FFFF),
    ("beige", 0xF5F5DC),
    ("bisque", 0xFFE4C4),
    ("black", 0x000000),
    ("blanchedalmond", 0xFFEBCD),
    ("blue", 0x0000FF),
    ("blueviolet", 0x8A2BE2),
    ("brown", 0xA52A2A),
    ("burlywood", 0xDEB887),
    ("cadetblue", 0x5F9EA0),
    ("chartreuse", 0x7FFF00),
    ("chocolate", 0xD2691E),
    ("coral", 0xFF7F50),
    ("cornflowerblue", 0x6495ED),
    ("cornsilk", 0xFFF8DC),
    ("crimson", 0xDC143C),
    ("cyan", 0x00FFFF),
    ("darkblue", 0x00008B),
    ("darkcyan", 0x008B8B),
    ("darkgoldenrod", 0xB8860B),
    ("darkgray", 0xA9A9A9),
    ("darkgreen", 0x006400),
    ("darkgrey", 0xA9A9A9),
    ("darkkhaki", 0xBDB76B),
    ("darkmagenta", 0x8B008B),
    ("darkolivegreen", 0x556B2F),
    ("darkorange", 0xFF8C00),
    ("darkorchid", 0x9932CC),
    ("darkred", 0x8B0000),
    ("darksalmon", 0xE9967A),
    ("darkseagreen", 0x8FBC8F),
    ("darkslateblue", 0x483D8B),
    ("darkslategray", 0x2F4F4F),
    ("darkslategrey", 0x2F4F4F),
    ("darkturquoise", 0x00CED1),
    ("darkviolet", 0x9400D3),
    ("deeppink", 0xFF1493),
    ("deepskyblue", 0x00BFFF),
    ("dimgray", 0x696969),
    ("dimgrey", 0x696969),
    ("dodgerblue", 0x1E90FF),
    ("firebrick", 0xB22222),
    ("floralwhite", 0xFFFAF0),
    ("forestgreen", 0x228B22),
    ("fuchsia", 0xFF00FF),
    ("gainsboro", 0xDCDCDC),
    ("ghostwhite", 0xF8F8FF),
    ("gold", 0xFFD700),
    ("goldenrod", 0xDAA520),
    ("gray", 0x808080),
    ("green", 0x008000),
    ("greenyellow", 0xADFF2F),
    ("grey", 0x808080),
    ("honeydew", 0xF0FFF0),
    ("hotpink", 0xFF69B4),
    ("indianred", 0xCD5C5C),
    ("indigo", 0x4B0082),
    ("ivory", 0xFFFFF0),
    ("khaki", 0xF0E68C),
    ("lavender", 0xE6E6FA),
    ("lavenderblush", 0xFFF0F5),
    ("lawngreen", 0x7CFC00),
    ("lemonchiffon", 0xFFFACD),
    ("lightblue", 0xADD8E6),
    ("lightcoral", 0xF08080),
    ("lightcyan", 0xE0FFFF),
    ("lightgoldenrodyellow", 0xFAFAD2),
    ("lightgray", 0xD3D3D3),
    ("lightgreen", 0x90EE90),
    ("lightgrey", 0xD3D3D3),
    ("lightpink", 0xFFB6C1),
    ("lightsalmon", 0xFFA07A),
    ("lightseagreen", 0x20B2AA),
    ("lightskyblue", 0x87CEFA),
    ("lightslategray", 0x778899),
    ("lightslategrey", 0x778899),
    ("lightsteelblue", 0xB0C4DE),
    ("lightyellow", 0xFFFFE0),
    ("lime", 0x00FF00),
    ("limegreen", 0x32CD32),
    ("linen", 0xFAF0E6),
    ("magenta", 0xFF00FF),
    ("maroon", 0x800000),
    ("mediumaquamarine", 0x66CDAA),
    ("mediumblue", 0x0000CD),
    ("mediumorchid", 0xBA55D3),
    ("mediumpurple", 0x9370DB),
    ("mediumseagreen", 0x3CB371),
    ("mediumslateblue", 0x7B68EE),
    ("mediumspringgreen", 0x00FA9A),
    ("mediumturquoise", 0x48D1CC),
    ("mediumvioletred", 0xC71585),
    ("midnightblue", 0x191970),
    ("mintcream", 0xF5FFFA),
    ("mistyrose", 0xFFE4E1),
    ("moccasin", 0xFFE4B5),
    ("navajowhite", 0xFFDEAD),
    ("navy", 0x000080),
    ("oldlace", 0xFDF5E6),
    ("olive", 0x808000),
    ("olivedrab", 0x6B8E23),
    ("orange", 0xFFA500),
    ("orangered", 0xFF4500),
    ("orchid", 0xDA70D6),
    ("palegoldenrod", 0xEEE8AA),
    ("palegreen", 0x98FB98),
    ("paleturquoise", 0xAFEEEE),
    ("palevioletred", 0xDB7093),
    ("papayawhip", 0xFFEFD5),
    ("peachpuff", 0xFFDAB9),
    ("peru", 0xCD853F),
    ("pink", 0xFFC0CB),
    ("plum", 0xDDA0DD),
    ("powderblue", 0xB0E0E6),
    ("purple", 0x800080),
    ("rebeccapurple", 0x663399),
    ("red", 0xFF0000),
    ("rosybrown", 0xBC8F8F),
    ("royalblue", 0x4169E1),
    ("saddlebrown", 0x8B4513),
    ("salmon", 0xFA8072),
    ("sandybrown", 0xF4A460),
    ("seagreen", 0x2E8B57),
    ("seashell", 0xFFF5EE),
    ("sienna", 0xA0522D),
    ("silver", 0xC0C0C0),
    ("skyblue", 0x87CEEB),
    ("slateblue", 0x6A5ACD),
    ("slategray", 0x708090),
    ("slategrey", 0x708090),
    ("snow", 0xFFFAFA),
    ("springgreen", 0x00FF7F),
    ("steelblue", 0x4682B4),
    ("tan", 0xD2B48C),
    ("teal", 0x008080),
    ("thistle", 0xD8BFD8),
    ("tomato", 0xFF6347),
    ("turquoise", 0x40E0D0),
    ("violet", 0xEE82EE),
    ("wheat", 0xF5DEB3),
    ("white", 0xFFFFFF),
    ("whitesmoke", 0xF5F5F5),
    ("yellow", 0xFFFF00),
    ("yellowgreen", 0x9ACD32),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_people_type() {
        assert_eq!(parse("abc"), Some(Rgb::new(0xAA, 0xBB, 0xCC)));
        assert_eq!(parse("7b6cf6"), Some(Rgb::new(0x7B, 0x6C, 0xF6)));
        assert_eq!(parse("rgb(10, 20.9, 300)"), Some(Rgb::new(10, 20, 255)));
        assert_eq!(parse("hsl(0, 100%, 50%)"), Some(Rgb::new(255, 0, 0)));
        assert_eq!(parse("Rebeccapurple"), Some(Rgb::new(0x66, 0x33, 0x99)));
        assert_eq!(parse("rgb(1, 2)"), None);
        assert_eq!(parse("not a colour"), None);
        assert_eq!(parse(""), None);
    }

    #[test]
    fn names_are_sorted() {
        assert!(NAMES.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn hsv_goes_both_ways() {
        for c in
            [Rgb::new(0x7B, 0x6C, 0xF6), Rgb::new(0, 0, 0), Rgb::new(255, 255, 255), Rgb::new(10, 200, 30)]
        {
            let (h, s, v) = hsv_of(c);
            assert_eq!(rgb_of_hsv(h, s, v), c);
        }
    }

    #[test]
    fn contrast_is_wcag() {
        assert!((contrast(Rgb::WHITE, Rgb::BLACK) - 21.0).abs() < 1e-9);
    }

    #[test]
    fn ideas_and_presets_are_real_gradients() {
        assert_eq!(presets().len(), 6);
        assert_eq!(ideas_for(Rgb::from_u32(0x7B6CF6)).len(), 7);
        let grey = partner(Rgb::new(0x20, 0x20, 0x20));
        assert!(grey.r > 0x20);
    }

    #[test]
    fn recent_picks_are_remembered_once() {
        let dir = tempfile::tempdir().expect("temp dir");
        let (picker, eye) = (dir.path().join("picker.json"), dir.path().join("colours.json"));
        remember(&picker, Rgb::from_u32(0xFF0000));
        remember(&picker, Rgb::from_u32(0x00FF00));
        remember(&picker, Rgb::from_u32(0xFF0000));
        std::fs::write(&eye, r##"{"recent": ["#00ff00", "#0000ff"]}"##).expect("write");
        let got: Vec<String> = recent(&picker, &eye).iter().map(|c| c.hex()).collect();
        assert_eq!(got, ["#FF0000", "#00FF00", "#0000FF"]);
    }
}
