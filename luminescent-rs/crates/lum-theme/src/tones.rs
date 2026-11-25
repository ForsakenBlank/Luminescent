//! Colour maths for the theme editor's quick tools, from `core/tones.py`: lighter and darker steps that look
//! evenly spaced, whole schemes made from a few main colours, and bulk changes to a whole set of colours.
//!
//! Colours are worked on in OKLCH (lightness, chroma, hue in degrees), which keeps a colour's hue when it is
//! made lighter or darker. A "tone" is the lightness people see, 0 (black) to 100 (white), the scale Material 3
//! (and so Caelestia) uses. The floating point steps follow the Python code one for one (`powf` where it has
//! `**`) so the results match to the last hex digit.

use std::collections::HashMap;

use indexmap::IndexMap;

use crate::colour::Rgb;

/// Colour roles or scheme names to `#RRGGBB` text, in the order they were made.
pub type Colours = IndexMap<String, String>;

fn lin(c: f64) -> f64 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn gam(c: f64) -> f64 {
    if c <= 0.003_130_8 { 12.92 * c } else { 1.055 * c.max(0.0).powf(1.0 / 2.4) - 0.055 }
}

fn lin_rgb(c: Rgb) -> [f64; 3] {
    let [r, g, b] = c.floats();
    [lin(r), lin(g), lin(b)]
}

fn rgb_of_lin(lin: [f64; 3]) -> Rgb {
    let [r, g, b] = lin.map(|c| gam(c.clamp(0.0, 1.0)));
    Rgb::from_floats(r, g, b)
}

fn cbrt_keeping_sign(x: f64) -> f64 {
    x.abs().powf(1.0 / 3.0).copysign(x)
}

fn oklab_of_lin([r, g, b]: [f64; 3]) -> [f64; 3] {
    let l = 0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b;
    let m = 0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b;
    let s = 0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b;
    let (l, m, s) = (cbrt_keeping_sign(l), cbrt_keeping_sign(m), cbrt_keeping_sign(s));
    [
        0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s,
    ]
}

fn lin_of_oklab([big_l, a, b]: [f64; 3]) -> [f64; 3] {
    let l = (big_l + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powf(3.0);
    let m = (big_l - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powf(3.0);
    let s = (big_l - 0.089_484_177_5 * a - 1.291_485_548_0 * b).powf(3.0);
    [
        4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s,
        -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s,
        -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701_0 * s,
    ]
}

/// (lightness 0..1, chroma 0..about 0.37, hue 0..360).
pub fn oklch(c: Rgb) -> (f64, f64, f64) {
    let [l, a, b] = oklab_of_lin(lin_rgb(c));
    (l, a.hypot(b), b.atan2(a).to_degrees().rem_euclid(360.0))
}

fn in_gamut(rgb: [f64; 3]) -> bool {
    rgb.iter().all(|c| (-0.0005..=1.0005).contains(c))
}

fn lin_at(l: f64, chroma: f64, hue: f64) -> [f64; 3] {
    let rad = hue.to_radians();
    lin_of_oklab([l, chroma * rad.cos(), chroma * rad.sin()])
}

/// The colour, with its chroma lowered until it fits on a screen (so the hue and lightness stay).
pub fn from_oklch(l: f64, c: f64, h: f64) -> Rgb {
    let l = l.clamp(0.0, 1.0);
    let c = c.max(0.0);
    let mut lin = lin_at(l, c, h);
    if !in_gamut(lin) {
        let (mut lo, mut hi) = (0.0, c);
        for _ in 0..16 {
            let mid = (lo + hi) / 2.0;
            if in_gamut(lin_at(l, mid, h)) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lin = lin_at(l, lo, h);
    }
    rgb_of_lin(lin)
}

/// CIE lightness L* (0..100) to relative luminance Y (0..1).
fn y_of_tone(t: f64) -> f64 {
    let t = t.clamp(0.0, 100.0);
    let f = (t + 16.0) / 116.0;
    let cubed = f.powf(3.0);
    if cubed > 216.0 / 24389.0 { cubed } else { (116.0 * f - 16.0) / (24389.0 / 27.0) }
}

/// How light a colour looks, 0 (black) to 100 (white).
pub fn tone(c: Rgb) -> f64 {
    let [r, g, b] = lin_rgb(c);
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    if y > 216.0 / 24389.0 { 116.0 * y.powf(1.0 / 3.0) - 16.0 } else { y * 24389.0 / 27.0 }
}

/// A colour of this hue and chroma (OKLCH) at tone `t`. Near black and white there is less room for colour,
/// so the chroma is eased off there instead of the hue drifting.
pub fn at_tone(hue: f64, chroma: f64, t: f64) -> Rgb {
    let t = t.clamp(0.0, 100.0);
    let room = if t > 0.0 && t < 100.0 { 1.0_f64.min(t / 18.0).min((100.0 - t) / 10.0) } else { 0.0 };
    let c = chroma * room.max(0.0);
    let mut l = y_of_tone(t).powf(1.0 / 3.0);
    let mut out = from_oklch(l, c, hue);
    // colourful colours are not exactly as light as greys: nudge
    for _ in 0..3 {
        let diff = t - tone(out);
        if diff.abs() < 0.4 {
            break;
        }
        l = (l + diff / 100.0 * 0.9).clamp(0.0, 1.0);
        out = from_oklch(l, c, hue);
    }
    out
}

/// The same colour (hue and colourfulness) made lighter or darker to tone `t`.
pub fn with_tone(c: Rgb, t: f64) -> Rgb {
    let (_, chroma, hue) = oklch(c);
    at_tone(hue, chroma, t)
}

pub fn shift_tone(c: Rgb, by: f64) -> Rgb {
    with_tone(c, tone(c) + by)
}

pub fn is_light(c: Rgb) -> bool {
    tone(c) > 60.0
}

fn relative_luminance(c: Rgb) -> f64 {
    let [r, g, b] = lin_rgb(c);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// WCAG contrast ratio, 1 to 21.
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Text for on top of `bg`: a very light or very dark shade of `hint` (or of `bg`), whichever reads better.
pub fn readable_on(bg: Rgb, hint: Option<Rgb>) -> Rgb {
    let (_, c, h) = oklch(hint.unwrap_or(bg));
    let light = at_tone(h, c.min(0.03), 98.0);
    let dark = at_tone(h, c.min(0.05), 12.0);
    if contrast(light, bg) >= contrast(dark, bg) { light } else { dark }
}

/// `a` blended towards `b` by `t`, in OKLab (no muddy middle).
pub fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let la = oklab_of_lin(lin_rgb(a));
    let lb = oklab_of_lin(lin_rgb(b));
    let m = [0, 1, 2].map(|i| la[i] + (lb[i] - la[i]) * t);
    rgb_of_lin(lin_of_oklab(m))
}

/// `b - a` the short way round, -180..180.
pub fn hue_diff(a: f64, b: f64) -> f64 {
    (b - a + 180.0).rem_euclid(360.0) - 180.0
}

pub fn toward_hue(h: f64, target: f64, amount: f64) -> f64 {
    (h + hue_diff(h, target) * amount).rem_euclid(360.0)
}

/// OKLCH hue ranges, so "make every green follow the accent" knows what a green is.
pub const FAMILIES: [(&str, &str, &[(f64, f64)]); 7] = [
    ("reds", "Reds and pinks", &[(345.0, 360.0), (0.0, 40.0)]),
    ("oranges", "Oranges and browns", &[(40.0, 75.0)]),
    ("yellows", "Yellows", &[(75.0, 115.0)]),
    ("greens", "Greens", &[(115.0, 175.0)]),
    ("cyans", "Teals and cyans", &[(175.0, 225.0)]),
    ("blues", "Blues", &[(225.0, 275.0)]),
    ("purples", "Purples and violets", &[(275.0, 345.0)]),
];

/// Below this chroma a colour counts as grey, whatever its hue.
pub const GREY_CHROMA: f64 = 0.025;

/// `"greys"` or the key of one of [`FAMILIES`].
pub fn family(c: Rgb) -> &'static str {
    let (_, chroma, hue) = oklch(c);
    if chroma < GREY_CHROMA {
        return "greys";
    }
    FAMILIES
        .iter()
        .find(|(_, _, spans)| spans.iter().any(|&(lo, hi)| lo <= hue && hue < hi))
        .map_or("reds", |f| f.0)
}

/// Every colour in one of `families` takes the hue of `target`, keeping how light and colourful it is.
/// `"all"` stands for every colourful colour.
pub fn follow_hue(colours: &Colours, families: &[&str], target: Rgb, keep_names: &[&str]) -> Colours {
    let (_, target_chroma, target_hue) = oklch(target);
    colours
        .iter()
        .map(|(key, value)| {
            let kept = keep_names.contains(&key.as_str());
            let out = match Rgb::parse(value).filter(|_| !kept && Rgb::is_hex(value)) {
                Some(c) => {
                    let fam = family(c);
                    if fam != "greys" && (families.contains(&"all") || families.contains(&fam)) {
                        let (l, chroma, _) = oklch(c);
                        let chroma = if target_chroma > GREY_CHROMA {
                            chroma.max(target_chroma.min(0.06))
                        } else {
                            chroma * 0.4
                        };
                        from_oklch(l, chroma, target_hue).hex()
                    } else {
                        value.clone()
                    }
                }
                None => value.clone(),
            };
            (key.clone(), out)
        })
        .collect()
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Adjust {
    /// Degrees.
    pub hue: f64,
    /// The rest are -100 to 100, 0 meaning unchanged.
    pub brightness: f64,
    pub contrast: f64,
    pub saturation: f64,
    pub warmth: f64,
    /// Where contrast spreads from; the middle of the set when `None`.
    pub pivot: Option<f64>,
}

/// Every colour at once. Contrast spreads the lightnesses away from (or towards) the middle of the set, so
/// backgrounds and text move apart together.
pub fn adjust(colours: &Colours, by: Adjust) -> Colours {
    let parsed: Vec<(&String, Rgb)> = colours
        .iter()
        .filter(|(_, v)| Rgb::is_hex(v))
        .filter_map(|(k, v)| Rgb::parse(v).map(|c| (k, c)))
        .collect();
    if parsed.is_empty() {
        return colours.clone();
    }
    let labs: Vec<(&String, (f64, f64, f64))> = parsed.iter().map(|&(k, c)| (k, oklch(c))).collect();
    let pivot = by.pivot.unwrap_or_else(|| {
        let mut lights: Vec<f64> = labs.iter().map(|(_, (l, _, _))| *l).collect();
        lights.sort_by(f64::total_cmp);
        (lights[0] + lights[lights.len() - 1]) / 2.0
    });
    let sat = 1.0 + by.saturation / 100.0;
    let con = 1.0 + by.contrast / 100.0 * if by.contrast > 0.0 { 0.8 } else { 1.0 };
    let mut out = colours.clone();
    for (key, (l, c, h)) in labs {
        let l = pivot + (l - pivot) * con + by.brightness / 100.0 * 0.35;
        let mut c = c * sat;
        let mut h = (h + by.hue).rem_euclid(360.0);
        if by.warmth != 0.0 {
            // warmth leans every colour towards orange (or blue when cooler) and gives greys a hint of it
            let rad = h.to_radians();
            let (mut a, mut b) = (c * rad.cos(), c * rad.sin());
            let w = by.warmth / 100.0;
            let target = if w > 0.0 { 55.0_f64 } else { 250.0_f64 };
            a += w.abs() * 0.035 * target.to_radians().cos();
            b += w.abs() * 0.035 * target.to_radians().sin();
            c = a.hypot(b);
            h = b.atan2(a).to_degrees().rem_euclid(360.0);
        }
        out.insert(key.clone(), from_oklch(l, c, h).hex());
    }
    out
}

/// Light becomes dark and dark becomes light, every hue kept: a dark scheme from a light one and back.
pub fn flip(colours: &Colours) -> Colours {
    colours
        .iter()
        .map(|(key, value)| {
            let out = match Rgb::parse(value).filter(|_| Rgb::is_hex(value)) {
                Some(c) => {
                    let (_, chroma, hue) = oklch(c);
                    let t = tone(c);
                    // accents keep a little of their brightness so they still stand out after the flip
                    let new = if chroma < 0.08 {
                        100.0 - t
                    } else {
                        (100.0 - t + if t < 50.0 { 8.0 } else { -8.0 }).clamp(30.0, 85.0)
                    };
                    at_tone(hue, chroma, new).hex()
                }
                None => value.clone(),
            };
            (key.clone(), out)
        })
        .collect()
}

/// Colours that came out the same (a whole scheme of `#FFFFFF`) a few tones apart, the first one kept.
pub fn spread(values: &[Rgb], light: bool, step: f64) -> Vec<Rgb> {
    let mut seen: HashMap<Rgb, u32> = HashMap::new();
    values
        .iter()
        .map(|&v| {
            let n = seen.entry(v).or_insert(0);
            let count = *n;
            *n += 1;
            if count == 0 { v } else { shift_tone(v, (if light { -step } else { step }) * f64::from(count)) }
        })
        .collect()
}

/// The main colours of a picture, the most eye-catching first. `pixels` are RGBA, already scaled down (the
/// Python app uses 72 by 72); see-through ones are skipped.
pub fn from_picture(pixels: &[[u8; 4]], count: usize) -> Vec<Rgb> {
    #[derive(Default)]
    struct Bucket {
        n: f64,
        l: f64,
        a: f64,
        b: f64,
        chroma: f64,
    }
    let mut buckets: HashMap<(bool, i64, i64), Bucket> = HashMap::new();
    for &[r, g, b, a] in pixels {
        if a < 128 {
            continue;
        }
        let (l, c, h) = oklch(Rgb::new(r, g, b));
        let key = if c < GREY_CHROMA {
            (false, 0, (l * 6.0).round_ties_even() as i64)
        } else {
            (true, (h / 24.0).floor() as i64, (l * 3.0).round_ties_even() as i64)
        };
        let bucket = buckets.entry(key).or_default();
        bucket.n += 1.0;
        bucket.l += l;
        let rad = h.to_radians();
        bucket.a += c * rad.cos();
        bucket.b += c * rad.sin();
        bucket.chroma += c;
    }
    let mut found: Vec<(f64, String, f64, f64)> = buckets
        .values()
        .map(|b| {
            let (l, a, bb) = (b.l / b.n, b.a / b.n, b.b / b.n);
            let (c, h) = (a.hypot(bb), bb.atan2(a).to_degrees().rem_euclid(360.0));
            let score = b.n
                * (0.25 + (b.chroma / b.n / 0.12).min(1.0) * 2.0)
                * (0.6 + 0.4 * (1.0 - (l - 0.65).abs()));
            (score, from_oklch(l, c, h).hex(), h, c)
        })
        .collect();
    found.sort_by(|x, y| y.0.total_cmp(&x.0).then_with(|| y.1.cmp(&x.1)));
    let mut out: Vec<(Rgb, f64, f64)> = Vec::new();
    for (_, hex, h, c) in found {
        let Some(colour) = Rgb::parse(&hex) else { continue };
        let apart = out.iter().all(|&(other, other_hue, other_chroma)| {
            hue_diff(h, other_hue).abs() > 18.0
                || (c - other_chroma).abs() > 0.08
                || (tone(colour) - tone(other)).abs() > 25.0
        });
        if apart {
            out.push((colour, h, c));
        }
        if out.len() >= count {
            break;
        }
    }
    out.into_iter().map(|(c, _, _)| c).collect()
}

/// The names of the main colours, with the labels the quick tools show.
pub const MAIN: [(&str, &str); 7] = [
    ("accent", "Accent"),
    ("accent2", "Second accent"),
    ("accent3", "Third accent"),
    ("background", "Background"),
    ("text", "Text"),
    ("success", "Good (success)"),
    ("error", "Bad (error)"),
];

/// A few main colours a whole scheme is made from. Empty text means "not set".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Main {
    pub accent: String,
    pub accent2: String,
    pub accent3: String,
    pub background: String,
    pub text: String,
    pub success: String,
    pub error: String,
}

impl Main {
    pub fn get(&self, key: &str) -> &str {
        match key {
            "accent" => &self.accent,
            "accent2" => &self.accent2,
            "accent3" => &self.accent3,
            "background" => &self.background,
            "text" => &self.text,
            "success" => &self.success,
            "error" => &self.error,
            _ => "",
        }
    }

    pub fn set(&mut self, key: &str, value: String) {
        match key {
            "accent" => self.accent = value,
            "accent2" => self.accent2 = value,
            "accent3" => self.accent3 = value,
            "background" => self.background = value,
            "text" => self.text = value,
            "success" => self.success = value,
            "error" => self.error = value,
            _ => {}
        }
    }

    fn colour(&self, key: &str) -> Option<Rgb> {
        Rgb::parse(self.get(key)).filter(|_| !self.get(key).is_empty())
    }
}

pub const HARMONIES: [(&str, &str, &str); 7] = [
    ("matching", "Matching", "Neighbours on the colour wheel: calm and close together."),
    ("opposite", "Opposites", "The colour across the wheel: lively, high contrast."),
    ("triad", "Three-way", "Three colours evenly around the wheel: bright and playful."),
    ("split", "Split", "The two colours either side of the opposite: bold but balanced."),
    ("mono", "One colour", "Only shades of the accent: tidy and quiet."),
    ("pastel", "Pastel", "Soft, light versions of matching colours."),
    ("neon", "Neon", "As colourful as a screen can show."),
];

/// Main colours for a harmony around `accent`.
pub fn harmony(accent: Rgb, kind: &str, light: bool, background: Option<Rgb>, text: Option<Rgb>) -> Main {
    let (_, chroma, h) = oklch(accent);
    let mut t = tone(accent);
    let mut c = chroma.max(0.06);
    let (o1, o2) = match kind {
        "opposite" => (180.0, 150.0),
        "triad" => (120.0, 240.0),
        "split" => (150.0, 210.0),
        "mono" => (0.0, 0.0),
        "pastel" => (40.0, -40.0),
        "neon" => (120.0, 200.0),
        _ => (35.0, -35.0),
    };
    let (mut c2, mut c3) = (c, c);
    if kind == "mono" {
        c2 = c * 0.45;
        c3 = c * 0.7;
    }
    if kind == "pastel" {
        c = c.min(0.09);
        c2 = 0.07;
        c3 = 0.07;
        t = if light { 70.0 } else { 80.0 };
    }
    if kind == "neon" {
        c = 0.32;
        c2 = 0.32;
        c3 = 0.32;
        t = if light { t.min(55.0) } else { t.max(70.0) };
    }
    let acc = at_tone(h, c, t);
    let second_tone = if kind == "mono" { t + if light { -8.0 } else { 8.0 } } else { t };
    let sec = at_tone((h + o1).rem_euclid(360.0), c2, second_tone);
    let ter = at_tone((h + o2).rem_euclid(360.0), c3, t);
    let bg = background.unwrap_or_else(|| {
        at_tone(h, if kind == "neon" { 0.03 } else { 0.02 }, if light { 97.0 } else { 8.0 })
    });
    Main {
        accent: acc.hex(),
        accent2: sec.hex(),
        accent3: ter.hex(),
        background: bg.hex(),
        text: text.unwrap_or_else(|| readable_on(bg, Some(acc))).hex(),
        ..Main::default()
    }
}

/// A small random number source for [`surprise`]; nothing here needs to be unpredictable.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03)
    }

    pub fn from_clock() -> Rng {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.as_nanos() as u64);
        Rng::new(nanos)
    }

    fn next_u64(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn uniform(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[(self.next_u64() % items.len() as u64) as usize]
    }
}

/// A random but tasteful set of main colours.
pub fn surprise(light: bool, rng: &mut Rng) -> Main {
    let h = rng.uniform(0.0, 360.0);
    let t = if light { rng.uniform(38.0, 55.0) } else { rng.uniform(68.0, 84.0) };
    let kind = rng.pick(&HARMONIES).0;
    let acc = at_tone(h, rng.uniform(0.09, 0.2), t);
    let bg_chroma = rng.pick(&[0.0, 0.012, 0.02, 0.035]);
    let bg_turn = rng.pick(&[0.0, 0.0, 20.0, -20.0, 180.0]);
    let bg_tone = if light { rng.uniform(94.0, 99.0) } else { rng.uniform(5.0, 13.0) };
    let bg = at_tone((h + bg_turn).rem_euclid(360.0), bg_chroma, bg_tone);
    harmony(acc, kind, light, Some(bg), None)
}

pub fn main_from_picture(pixels: &[[u8; 4]], light: bool) -> Option<Main> {
    let cols = from_picture(pixels, 8);
    if cols.is_empty() {
        return None;
    }
    let mut colourful = cols.clone();
    colourful.sort_by(|a, b| oklch(*b).1.total_cmp(&oklch(*a).1));
    let accent = colourful[0];
    let (_, accent_chroma, accent_hue) = oklch(accent);
    let others: Vec<Rgb> =
        colourful[1..].iter().copied().filter(|&c| hue_diff(oklch(c).2, accent_hue).abs() > 25.0).collect();
    let second = others
        .first()
        .copied()
        .unwrap_or_else(|| at_tone((accent_hue + 30.0).rem_euclid(360.0), accent_chroma * 0.6, tone(accent)));
    let third = others
        .get(1)
        .copied()
        .unwrap_or_else(|| at_tone((accent_hue - 40.0).rem_euclid(360.0), accent_chroma * 0.8, tone(accent)));
    let darkest = cols.iter().copied().min_by(|a, b| tone(*a).total_cmp(&tone(*b)))?;
    let lightest = cols.iter().copied().max_by(|a, b| tone(*a).total_cmp(&tone(*b)))?;
    let (_, bc, bh) = oklch(if light { lightest } else { darkest });
    let bg = at_tone(bh, bc.min(0.03), if light { 97.0 } else { 8.0 });
    Some(Main {
        accent: fit_accent(accent, light).hex(),
        accent2: fit_accent(second, light).hex(),
        accent3: fit_accent(third, light).hex(),
        background: bg.hex(),
        text: readable_on(bg, Some(accent)).hex(),
        ..Main::default()
    })
}

/// An accent bright enough to read on a dark background (dark enough on a light one).
fn fit_accent(c: Rgb, light: bool) -> Rgb {
    let t = tone(c);
    if light { with_tone(c, t.min(52.0)) } else { with_tone(c, t.max(62.0)) }
}

/// The ANSI terminal hues (OKLCH) before they are leaned a little towards the accent.
const ANSI_HUES: [(&str, f64); 6] =
    [("red", 22.0), ("green", 145.0), ("yellow", 95.0), ("blue", 255.0), ("magenta", 330.0), ("cyan", 200.0)];

/// The named colours some apps take from Caelestia's scheme (btop, cava, htop, Zed...).
const NAMED_HUES: [(&str, f64, f64); 14] = [
    ("rosewater", 40.0, 0.04),
    ("flamingo", 20.0, 0.06),
    ("pink", 350.0, 0.12),
    ("mauve", 305.0, 0.13),
    ("red", 15.0, 0.15),
    ("maroon", 5.0, 0.11),
    ("peach", 55.0, 0.13),
    ("yellow", 95.0, 0.12),
    ("green", 145.0, 0.13),
    ("teal", 185.0, 0.1),
    ("sky", 220.0, 0.1),
    ("sapphire", 235.0, 0.11),
    ("blue", 260.0, 0.13),
    ("lavender", 280.0, 0.1),
];

const DEFAULT_ACCENT: Rgb = Rgb::from_u32(0x7B6CF6);
const DEFAULT_BACKGROUND: Rgb = Rgb::from_u32(0x131317);

/// The main colours a Caelestia scheme already has (to start the Main colours tool from).
pub fn main_from_scheme(cols: &Colours) -> Main {
    let pick = |names: &[&str], default: String| -> String {
        names
            .iter()
            .filter_map(|n| cols.get(*n))
            .find(|v| Rgb::is_hex(v))
            .and_then(|v| Rgb::parse(v))
            .map_or(default, Rgb::hex)
    };
    let bg = pick(&["surface", "background", "base"], DEFAULT_BACKGROUND.hex());
    let bg_rgb = Rgb::parse(&bg).unwrap_or(DEFAULT_BACKGROUND);
    Main {
        accent: pick(&["primary"], DEFAULT_ACCENT.hex()),
        accent2: pick(&["secondary"], String::new()),
        accent3: pick(&["tertiary"], String::new()),
        background: bg,
        text: pick(&["onSurface", "onBackground", "text"], readable_on(bg_rgb, None).hex()),
        success: pick(&["success"], String::new()),
        error: pick(&["error"], String::new()),
    }
}

/// Every Caelestia colour (Material 3 roles, terminal, named and KDE colours) from the main colours: the
/// accent (needed), second and third accent, background, text, success and error (each optional). Light or
/// dark comes from the background. Surfaces, outlines and text get proper steps a few tones apart.
pub fn caelestia_scheme(main: &Main, keys: Option<&[&str]>) -> Colours {
    let acc = main.colour("accent").unwrap_or(DEFAULT_ACCENT);
    let bg = main.colour("background").unwrap_or(DEFAULT_BACKGROUND);
    let light = tone(bg) > 55.0;
    let (_, ac, ah) = oklch(acc);
    let (a2, a3) = if ac < GREY_CHROMA {
        // a grey accent: the other two are greys a few steps away, so they still tell apart
        let at = tone(acc);
        (
            main.colour("accent2").unwrap_or_else(|| shift_tone(acc, if at < 50.0 { 14.0 } else { -14.0 })),
            main.colour("accent3").unwrap_or_else(|| shift_tone(acc, if at > 20.0 { -10.0 } else { 22.0 })),
        )
    } else {
        (
            main.colour("accent2")
                .unwrap_or_else(|| at_tone((ah + 25.0).rem_euclid(360.0), ac * 0.45, tone(acc))),
            main.colour("accent3")
                .unwrap_or_else(|| at_tone((ah + 60.0).rem_euclid(360.0), ac * 0.8, tone(acc))),
        )
    };
    let text = main.colour("text").unwrap_or_else(|| readable_on(bg, Some(acc)));
    let err = main
        .colour("error")
        .unwrap_or_else(|| at_tone(toward_hue(25.0, ah, 0.08), 0.16, if light { 45.0 } else { 75.0 }));
    let ok = main
        .colour("success")
        .unwrap_or_else(|| at_tone(toward_hue(150.0, ah, 0.12), 0.11, if light { 42.0 } else { 78.0 }));
    let bt = tone(bg);
    let (_, bc, bh) = oklch(bg);
    let nc = bc.min(0.04);
    let (_, tc, th) = oklch(text);
    let nh = if bc >= 0.006 {
        bh
    } else if tc >= 0.006 {
        th
    } else {
        ah
    };
    // "neutral variant": greys with a breath of the accent
    let nvc = nc.max((ac * 0.12).min(0.018));

    let n = |t: f64| at_tone(nh, nc, t).hex();
    let nv = |t: f64| at_tone(toward_hue(nh, ah, 0.5), nvc, t).hex();
    let pal = |colour: Rgb, t: f64| {
        let (_, c, h) = oklch(colour);
        at_tone(h, c, t).hex()
    };

    // towards the text: up the tones on dark, down on light
    let d = if light { -1.0 } else { 1.0 };
    let mut s = Colours::new();
    macro_rules! put {
        ($name:expr, $value:expr $(,)?) => {
            s.insert(String::from($name), $value)
        };
    }
    put!("surface", bg.hex());
    put!("background", bg.hex());
    put!("surfaceDim", n(if light { bt - 13.0 } else { (bt - 2.0).max(0.0) }));
    put!("surfaceBright", n(if light { (bt + 1.0).min(100.0) } else { bt + 18.0 }));
    put!("surfaceContainerLowest", n(if light { 100.0 } else { (bt - 3.0).max(0.0) }));
    put!("surfaceContainerLow", n(bt + d * 2.5));
    put!("surfaceContainer", n(bt + d * 5.0));
    put!("surfaceContainerHigh", n(bt + d * 8.0));
    put!("surfaceContainerHighest", n(bt + d * 11.0));
    put!("surfaceVariant", nv(bt + d * 12.0));
    put!("inverseSurface", n(if light { 20.0 } else { 90.0 }));
    put!("inverseOnSurface", n(if light { 95.0 } else { 20.0 }));
    let tt = tone(text);
    put!("onSurface", text.hex());
    put!("onBackground", text.hex());
    put!(
        "onSurfaceVariant",
        at_tone(toward_hue(th, ah, 0.3), nvc.max(tc.min(0.03)), tt + if light { 25.0 } else { -22.0 }).hex(),
    );
    put!("outline", nv(if light { 50.0 } else { 60.0 }));
    put!("outlineVariant", nv(bt + d * 20.0));
    put!("shadow", "#000000".into());
    put!("scrim", "#000000".into());
    for (name, cap, col) in
        [("primary", "Primary", acc), ("secondary", "Secondary", a2), ("tertiary", "Tertiary", a3)]
    {
        let ct = tone(col);
        put!(name, col.hex());
        let on = if (ct - 60.0).abs() > 8.0 {
            pal(col, if ct < 60.0 { 100.0 } else { 12.0 })
        } else {
            readable_on(col, None).hex()
        };
        put!(&format!("on{cap}"), on);
        put!(&format!("{name}Container"), pal(col, if light { 88.0 } else { 30.0 }));
        put!(&format!("on{cap}Container"), pal(col, if light { 15.0 } else { 92.0 }));
        put!(&format!("{name}Fixed"), pal(col, 90.0));
        put!(&format!("{name}FixedDim"), pal(col, 80.0));
        put!(&format!("on{cap}Fixed"), pal(col, 10.0));
        put!(&format!("on{cap}FixedVariant"), pal(col, 30.0));
        put!(&format!("{name}_paletteKeyColor"), with_tone(col, 50.0).hex());
    }
    put!("inversePrimary", pal(acc, if light { 80.0 } else { 40.0 }));
    put!("surfaceTint", acc.hex());
    put!("neutral_paletteKeyColor", n(50.0));
    put!("neutral_variant_paletteKeyColor", nv(50.0));
    for (name, cap, col) in [("error", "Error", err), ("success", "Success", ok)] {
        put!(name, col.hex());
        put!(&format!("on{cap}"), readable_on(col, None).hex());
        put!(&format!("{name}Container"), pal(col, if light { 88.0 } else { 30.0 }));
        put!(&format!("on{cap}Container"), pal(col, if light { 15.0 } else { 90.0 }));
    }
    // terminal colours: readable hues leaned a little towards the accent
    let (normal, bright) = if light { (42.0, 32.0) } else { (72.0, 82.0) };
    let mut ansi: HashMap<String, String> = HashMap::new();
    for (name, hue) in ANSI_HUES {
        let h2 = toward_hue(hue, ah, 0.12);
        ansi.insert(name.to_owned(), at_tone(h2, 0.13, normal).hex());
        ansi.insert(format!("b{name}"), at_tone(h2, 0.15, bright).hex());
    }
    put!("term0", n(if light { 25.0 } else { bt + d * 9.0 }));
    put!("term7", n(if light { 40.0 } else { 80.0 }));
    put!("term8", n(50.0));
    put!("term15", n(if light { 15.0 } else { 96.0 }));
    for (i, (name, _)) in ANSI_HUES.iter().enumerate() {
        put!(&format!("term{}", i + 1), ansi[*name].clone());
        put!(&format!("term{}", i + 9), ansi[&format!("b{name}")].clone());
    }
    // named colours for other apps
    let named_tone = if light { 45.0 } else { 78.0 };
    for (name, hue, chroma) in NAMED_HUES {
        put!(name, at_tone(toward_hue(hue, ah, 0.1), chroma, named_tone).hex());
    }
    put!("green", at_tone(oklch(ok).2, 0.13, named_tone).hex());
    put!("red", at_tone(oklch(err).2, 0.15, named_tone).hex());
    // the catppuccin-style text and surface steps
    for (name, f) in [
        ("text", 0.0),
        ("subtext1", 0.12),
        ("subtext0", 0.24),
        ("overlay2", 0.36),
        ("overlay1", 0.46),
        ("overlay0", 0.56),
    ] {
        put!(name, at_tone(nh, nc, tt + (bt - tt) * f).hex());
    }
    put!("surface2", n(bt + d * 16.0));
    put!("surface1", n(bt + d * 11.0));
    put!("surface0", n(bt + d * 6.0));
    put!("base", bg.hex());
    // mantle and crust are always a little darker than the base, light or dark
    put!("mantle", n(if bt >= 3.0 { bt - 3.0 } else { bt + 2.0 }));
    put!("crust", n(if bt >= 6.0 { bt - 6.0 } else { bt + 4.0 }));
    // KDE and Qt apps
    let yellow = s["yellow"].clone();
    for name in ["klink", "klinkSelection"] {
        put!(name, acc.hex());
    }
    for name in ["kvisited", "kvisitedSelection"] {
        put!(name, a3.hex());
    }
    for name in ["knegative", "knegativeSelection"] {
        put!(name, err.hex());
    }
    for name in ["kneutral", "kneutralSelection"] {
        put!(name, yellow.clone());
    }
    for name in ["kpositive", "kpositiveSelection"] {
        put!(name, ok.hex());
    }
    match keys {
        Some(keys) => s.into_iter().filter(|(k, _)| keys.contains(&k.as_str())).collect(),
        None => s,
    }
}

/// Luminescent's own theme colours from the same main colours, with real steps between the backgrounds.
pub fn lum_roles(main: &Main) -> Colours {
    let acc = main.colour("accent").unwrap_or(DEFAULT_ACCENT);
    let bg = main.colour("background").unwrap_or(DEFAULT_BACKGROUND);
    let light = tone(bg) > 55.0;
    let bt = tone(bg);
    let (_, bc, bh) = oklch(bg);
    let nc = bc.min(0.04);
    let text = main.colour("text").unwrap_or_else(|| readable_on(bg, Some(acc)));
    let (_, tc, th) = oklch(text);
    let (_, ac, ah) = oklch(acc);
    let nh = if bc >= 0.006 {
        bh
    } else if tc >= 0.006 {
        th
    } else {
        ah
    };
    let d = if light { -1.0 } else { 1.0 };
    let n = |t: f64| at_tone(nh, nc, t);
    let tt = tone(text);
    let at = tone(acc);
    let mut c = Colours::new();
    macro_rules! put {
        ($name:expr, $value:expr $(,)?) => {
            c.insert(String::from($name), Rgb::hex($value))
        };
    }
    put!("bg_base", bg);
    put!("bg_card", n(bt + d * 3.5));
    put!("bg_card_hover", n(bt + d * 7.0));
    let frame = if light {
        bt - 4.5
    } else if bt >= 3.0 {
        bt - 2.5
    } else {
        bt + 1.5
    };
    put!("bg_frame", n(frame));
    put!("bg_header", n(bt + d * 1.8));
    put!("border", n(bt + d * 12.0));
    put!("text_primary", text);
    put!("text_secondary", at_tone(nh, nc.max(tc.min(0.03)), tt + (bt - tt) * 0.32));
    put!("text_muted", at_tone(nh, nc, tt + (bt - tt) * 0.55));
    put!("primary", acc);
    put!("primary_hover", at_tone(ah, ac, at + if light { -6.0 } else { 7.0 }));
    put!("primary_muted", at_tone(ah, ac.min(0.08), bt + d * 9.0));
    let primary_text = at_tone(ah, ac, if light { 38.0 } else { 78.0 });
    put!("primary_text", primary_text);
    put!("on_primary", readable_on(acc, Some(acc)));
    let card = c["bg_card"].clone();
    c.insert("folder_color".into(), card);
    c.insert("folder_border".into(), primary_text.hex());
    c
}

/// The main colours of a Luminescent theme's colours.
pub fn main_from_roles(roles: &Colours) -> Main {
    let get = |key: &str, default: Rgb| {
        roles.get(key).filter(|v| Rgb::is_hex(v)).and_then(|v| Rgb::parse(v)).unwrap_or(default)
    };
    let bg = get("bg_base", DEFAULT_BACKGROUND);
    Main {
        accent: get("primary", DEFAULT_ACCENT).hex(),
        background: bg.hex(),
        text: get("text_primary", readable_on(bg, None)).hex(),
        ..Main::default()
    }
}
