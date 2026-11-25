//! Plain colours as `#RRGGBB` text and as three bytes, with the small bits of maths `core/theme.py` has.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb::new(0, 0, 0);
    pub const WHITE: Rgb = Rgb::new(255, 255, 255);

    pub const fn new(r: u8, g: u8, b: u8) -> Rgb {
        Rgb { r, g, b }
    }

    pub const fn from_u32(rgb: u32) -> Rgb {
        Rgb::new((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
    }

    /// What `tones._rgb` accepts: `#RGB`, `#RRGGBB`, or `#AARRGGBB` from Qt (the alpha is dropped). Longer text
    /// is read from its first six digits, as Python does, so `#abcdef0` is a colour.
    pub fn parse(text: &str) -> Option<Rgb> {
        let trimmed = text.trim().trim_start_matches('#');
        let digits: Vec<char> = match trimmed.chars().count() {
            3 => trimmed.chars().flat_map(|c| [c, c]).collect(),
            8 => trimmed.chars().skip(2).collect(),
            _ => trimmed.chars().collect(),
        };
        if digits.len() < 6 {
            return None;
        }
        let pair = |i: usize| -> Option<u8> {
            let hi = digits[i].to_digit(16)?;
            let lo = digits[i + 1].to_digit(16)?;
            Some((hi * 16 + lo) as u8)
        };
        Some(Rgb::new(pair(0)?, pair(2)?, pair(4)?))
    }

    /// Only `#RRGGBB`, the shape themes.json stores (`theme._hex_ok`).
    pub fn parse_strict(text: &str) -> Option<Rgb> {
        let digits = text.strip_prefix('#')?;
        if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        Rgb::parse(text)
    }

    /// `tones.is_hex`: text that starts with `#` and reads as a colour.
    pub fn is_hex(text: &str) -> bool {
        text.trim().starts_with('#') && Rgb::parse(text).is_some()
    }

    pub fn hex(self) -> String {
        self.to_string()
    }

    pub fn floats(self) -> [f64; 3] {
        [f64::from(self.r) / 255.0, f64::from(self.g) / 255.0, f64::from(self.b) / 255.0]
    }

    /// Back from 0..1 channels, rounding halves to the even number like Python's `round`.
    pub fn from_floats(r: f64, g: f64, b: f64) -> Rgb {
        let ch = |v: f64| (v * 255.0).round_ties_even().clamp(0.0, 255.0) as u8;
        Rgb::new(ch(r), ch(g), ch(b))
    }

    /// `theme.luminance`: 0 to 255, used to tell light backgrounds from dark ones.
    pub fn luminance(self) -> f64 {
        0.299 * f64::from(self.r) + 0.587 * f64::from(self.g) + 0.114 * f64::from(self.b)
    }

    /// `theme.mix`: `self` blended towards `other` by `t`, channel by channel.
    pub fn mix(self, other: Rgb, t: f64) -> Rgb {
        let ch = |x: u8, y: u8| {
            (f64::from(x) + (f64::from(y) - f64::from(x)) * t).round_ties_even().clamp(0.0, 255.0) as u8
        };
        Rgb::new(ch(self.r, other.r), ch(self.g, other.g), ch(self.b, other.b))
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

/// `theme.mix` on text. Unreadable colours give `a` back.
pub fn mix(a: &str, b: &str, t: f64) -> String {
    match (Rgb::parse(a), Rgb::parse(b)) {
        (Some(x), Some(y)) => x.mix(y, t).hex(),
        _ => a.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_shapes_python_reads() {
        assert_eq!(Rgb::parse("#abc"), Some(Rgb::new(0xAA, 0xBB, 0xCC)));
        assert_eq!(Rgb::parse("  #7b6cf6 "), Some(Rgb::new(0x7B, 0x6C, 0xF6)));
        assert_eq!(Rgb::parse("#FF7B6CF6"), Some(Rgb::new(0x7B, 0x6C, 0xF6)));
        assert_eq!(Rgb::parse("#abcdef0"), Some(Rgb::new(0xAB, 0xCD, 0xEF)));
        assert_eq!(Rgb::parse("#abcd"), None);
        assert_eq!(Rgb::parse("#GGGGGG"), None);
        assert!(!Rgb::is_hex("ABCDEF"));
        assert_eq!(Rgb::parse_strict("#abc"), None);
        assert_eq!(Rgb::parse_strict("#abcdef").map(Rgb::hex).as_deref(), Some("#ABCDEF"));
    }

    #[test]
    fn mixes_with_even_rounding() {
        assert_eq!(mix("#7B6CF6", "#FFFFFF", 0.35), "#A99FF9");
        assert_eq!(Rgb::BLACK.mix(Rgb::new(3, 3, 3), 0.5), Rgb::new(2, 2, 2));
    }
}
