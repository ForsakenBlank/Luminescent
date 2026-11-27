//! The colour picker's text formats, the arrow-key nudge, and `colours.json`: the last 10 colours picked
//! from the screen, which the tray, the palette and the colour dialog's recent row list.

use std::path::{Path, PathBuf};

use lum_core::pyjson::Style;
use lum_core::store;
use lum_theme::colour::Rgb;
use serde_json::{Map, Value};

use crate::{Point, py_round};

/// How many colours `colours.json` keeps.
pub const RECENT: usize = 10;

/// The "Copy as" setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    #[default]
    Hex,
    Rgb,
    Hsl,
}

impl Format {
    /// From the setting's value; anything unknown is hex, as in Python.
    pub fn from_setting(value: &str) -> Format {
        match value {
            "rgb" => Format::Rgb,
            "hsl" => Format::Hsl,
            _ => Format::Hex,
        }
    }

    /// The word on the picker's hint ("CLICK COPY HEX").
    pub fn name(self) -> &'static str {
        match self {
            Format::Hex => "hex",
            Format::Rgb => "rgb",
            Format::Hsl => "hsl",
        }
    }
}

/// `colorsys.rgb_to_hls` on 0..1 channels: (hue 0..1, lightness, saturation).
pub fn rgb_to_hls(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let (maxc, minc) = (r.max(g).max(b), r.min(g).min(b));
    let (sum, range) = (maxc + minc, maxc - minc);
    let l = sum / 2.0;
    if maxc == minc {
        return (0.0, l, 0.0);
    }
    let s = if l <= 0.5 { range / sum } else { range / (2.0 - maxc - minc) };
    let (rc, gc, bc) = ((maxc - r) / range, (maxc - g) / range, (maxc - b) / range);
    let h = if r == maxc {
        bc - gc
    } else if g == maxc {
        2.0 + rc - bc
    } else {
        4.0 + gc - rc
    };
    ((h / 6.0).rem_euclid(1.0), l, s)
}

/// The colour as the picker copies it: `#7B6CF6` (or lower case), `rgb(123, 108, 246)` or `hsl(246, 89%, 69%)`.
pub fn text(c: Rgb, format: Format, upper: bool) -> String {
    match format {
        Format::Rgb => format!("rgb({}, {}, {})", c.r, c.g, c.b),
        Format::Hsl => {
            let [r, g, b] = c.floats();
            let (h, l, s) = rgb_to_hls(r, g, b);
            format!("hsl({}, {}%, {}%)", py_round(h * 360.0), py_round(s * 100.0), py_round(l * 100.0))
        }
        Format::Hex if upper => c.hex(),
        Format::Hex => c.hex().to_lowercase(),
    }
}

/// Where an arrow key moves the pointer: 1 px, or 10 with Shift.
pub fn nudge(at: Point, dx: i32, dy: i32, shift: bool) -> Point {
    let step = if shift { 10 } else { 1 };
    Point::new(at.x + dx * step, at.y + dy * step)
}

/// The last colours picked from the screen, newest first, as `colours.json` keeps them.
#[derive(Debug, Clone, PartialEq)]
pub struct Recent {
    path: PathBuf,
    /// The file's other keys, kept as they were.
    raw: Map<String, Value>,
    pub colours: Vec<String>,
}

impl Recent {
    /// Reads the file; a missing or broken one is an empty list (a broken one is kept as `.bad`).
    pub fn load(path: &Path) -> Recent {
        let raw = match store::load_json(path) {
            Some(Value::Object(map)) => map,
            _ => Map::new(),
        };
        let colours = raw
            .get("recent")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(Value::as_str)
                    .filter(|c| Rgb::is_hex(c))
                    .take(RECENT)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        Recent { path: path.to_owned(), raw, colours }
    }

    /// Puts a picked colour at the front (`#RRGGBB`), once, and keeps ten.
    pub fn push(&mut self, c: Rgb) {
        let name = c.hex();
        let mut list = vec![name.clone()];
        list.extend(self.colours.iter().filter(|r| **r != name).take(RECENT - 1).cloned());
        self.colours = list;
    }

    /// The file's text: Python's `json.dump` without indentation.
    pub fn to_json(&self) -> String {
        let mut raw = self.raw.clone();
        raw.insert("recent".into(), Value::Array(self.colours.iter().cloned().map(Value::String).collect()));
        lum_core::pyjson::to_string(&Value::Object(raw), Style::COMPACT)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn save(&self) -> Result<(), store::StoreError> {
        store::save_text(&self.path, &self.to_json())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_match_the_python_picker() {
        let c = Rgb::new(0x7B, 0x6C, 0xF6);
        assert_eq!(text(c, Format::Hex, true), "#7B6CF6");
        assert_eq!(text(c, Format::Hex, false), "#7b6cf6");
        assert_eq!(text(c, Format::Rgb, true), "rgb(123, 108, 246)");
        // what Python's colorsys gives (the setting's label rounds its example differently)
        assert_eq!(text(c, Format::Hsl, true), "hsl(247, 88%, 69%)");
        assert_eq!(text(Rgb::new(128, 128, 128), Format::Hsl, true), "hsl(0, 0%, 50%)");
        assert_eq!(text(Rgb::new(255, 0, 0), Format::Hsl, true), "hsl(0, 100%, 50%)");
        assert_eq!(text(Rgb::new(0, 255, 0), Format::Hsl, true), "hsl(120, 100%, 50%)");
        assert_eq!(Format::from_setting("nonsense"), Format::Hex);
    }

    #[test]
    fn hls_rounds_halves_to_even_like_python() {
        // 0.5 * 100 = 50 exactly; 0.125 lightness * 100 = 12.5 rounds down to 12
        let (_, l, _) = rgb_to_hls(0.125, 0.125, 0.125);
        assert_eq!(py_round(l * 100.0), 12);
        let (h, _, _) = rgb_to_hls(0.0, 0.0, 1.0);
        assert_eq!(py_round(h * 360.0), 240);
        let (h, _, _) = rgb_to_hls(1.0, 0.0, 0.5);
        assert!((h - 0.9166).abs() < 1e-3, "{h}");
    }

    #[test]
    fn arrows_nudge_one_or_ten_pixels() {
        assert_eq!(nudge(Point::new(10, 10), -1, 0, false), Point::new(9, 10));
        assert_eq!(nudge(Point::new(10, 10), 0, 1, true), Point::new(10, 20));
    }

    #[test]
    fn the_sample_file_is_read_and_saved_back_unchanged() {
        let sample = lum_fixtures::path("sample_home/data/colours.json");
        let text = std::fs::read_to_string(&sample).expect("the sample colours.json");
        let recent = Recent::load(&sample);
        assert_eq!(recent.colours, ["#7B6CF6", "#FF5A8A", "#30A46C"]);
        assert_eq!(recent.to_json(), text.trim_end());
    }

    #[test]
    fn a_pick_goes_first_once_and_ten_are_kept() {
        let dir = tempfile::tempdir().expect("a temp folder");
        let path = dir.path().join("colours.json");
        std::fs::write(&path, r##"{"recent": ["#111111", "nope", "#222222"], "kept": 1}"##).expect("written");
        let mut recent = Recent::load(&path);
        assert_eq!(recent.colours, ["#111111", "#222222"]);
        recent.push(Rgb::new(0x22, 0x22, 0x22));
        assert_eq!(recent.colours, ["#222222", "#111111"]);
        for i in 0..20u8 {
            recent.push(Rgb::new(i, i, i));
        }
        assert_eq!(recent.colours.len(), RECENT);
        assert_eq!(recent.colours[0], "#131313");
        recent.save().expect("saved");
        let back = std::fs::read_to_string(&path).expect("read");
        assert!(back.starts_with(r##"{"recent": ["#131313", "##), "{back}");
        assert!(back.ends_with(r#""kept": 1}"#), "{back}");
    }
}
