//! QR codes for the QR Code tool: the text turned into dark and light modules, and the picture and SVG writers
//! that work from those. The Python app used `segno`; the `qrcode` crate makes the same kind of code (the
//! smallest version that holds the text, error correction M below 2 000 bytes and L above it), so a phone
//! reads either.

use qrcode::{Color, EcLevel, QrCode};

/// About what a QR code holds as bytes at the lowest error correction.
pub const MAX_BYTES: usize = 2900;

/// The quiet zone round the code, in modules.
pub const BORDER: usize = 4;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum QrError {
    #[error("Copy some text or a link, then press From clipboard (or type it below).")]
    Empty,
    #[error("Too long for one QR code ({bytes} bytes, {MAX_BYTES} at most).")]
    TooLong { bytes: usize },
    #[error("Could not make a QR code: {0}")]
    Failed(String),
}

/// A finished code: `width` x `width` modules, row by row, true for a dark one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Qr {
    width: usize,
    dark: Vec<bool>,
}

impl Qr {
    pub fn make(text: &str) -> Result<Qr, QrError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(QrError::Empty);
        }
        let bytes = text.len();
        if bytes > MAX_BYTES {
            return Err(QrError::TooLong { bytes });
        }
        let level = if bytes < 2000 { EcLevel::M } else { EcLevel::L };
        let code = QrCode::with_error_correction_level(text.as_bytes(), level)
            .map_err(|e| QrError::Failed(e.to_string()))?;
        let dark = code.to_colors().into_iter().map(|c| c == Color::Dark).collect();
        Ok(Qr { width: code.width(), dark })
    }

    /// Modules along one side, without the quiet zone.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Whether the module at (x, y) is dark; everything outside the code, the quiet zone, is light.
    pub fn is_dark(&self, x: isize, y: isize) -> bool {
        let inside = |v: isize| (0..self.width as isize).contains(&v);
        inside(x) && inside(y) && self.dark[y as usize * self.width + x as usize]
    }

    /// Black on white, `scale` pixels to a module, the quiet zone round it: width, height and RGBA bytes.
    pub fn rgba(&self, scale: usize) -> (usize, Vec<u8>) {
        let side = (self.width + 2 * BORDER) * scale;
        let mut pixels = vec![255u8; side * side * 4];
        for y in 0..self.width {
            for x in 0..self.width {
                if !self.dark[y * self.width + x] {
                    continue;
                }
                for py in (y + BORDER) * scale..(y + BORDER + 1) * scale {
                    for px in (x + BORDER) * scale..(x + BORDER + 1) * scale {
                        let at = (py * side + px) * 4;
                        pixels[at..at + 3].fill(0);
                    }
                }
            }
        }
        (side, pixels)
    }

    /// An SVG at `scale` pixels to a module: a white square with one black path for all the dark modules.
    pub fn svg(&self, scale: usize) -> String {
        let side = self.width + 2 * BORDER;
        let mut path = String::new();
        for y in 0..self.width {
            let mut x = 0;
            while x < self.width {
                if !self.dark[y * self.width + x] {
                    x += 1;
                    continue;
                }
                let start = x;
                while x < self.width && self.dark[y * self.width + x] {
                    x += 1;
                }
                path.push_str(&format!("M{},{}h{}v1h-{}z", start + BORDER, y + BORDER, x - start, x - start));
            }
        }
        let px = side * scale;
        format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
             <svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{px}\" height=\"{px}\" viewBox=\"0 0 {side} {side}\">\
             <rect width=\"{side}\" height=\"{side}\" fill=\"#fff\"/>\
             <path d=\"{path}\" fill=\"#000\"/></svg>\n"
        )
    }
}

/// What the panel says under a code that was made.
pub fn describe(text: &str) -> String {
    let text = text.trim();
    let kind =
        if ["http://", "https://", "www."].iter().any(|p| text.starts_with(p)) { "Link" } else { "Text" };
    let n = text.chars().count();
    format!("{kind}, {n} character{}. Point a phone camera at it.", if n == 1 { "" } else { "s" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_text_makes_a_version_one_code() {
        let qr = Qr::make("hi").expect("a code");
        assert_eq!(qr.width(), 21, "version 1 is 21 modules wide");
    }

    #[test]
    fn a_longer_link_needs_a_bigger_code() {
        let qr = Qr::make("https://example.com/some/path?with=a&few=parameters").expect("a code");
        assert!(qr.width() > 21 && (qr.width() - 17) % 4 == 0, "{}", qr.width());
    }

    #[test]
    fn the_finder_squares_are_where_a_reader_looks() {
        let qr = Qr::make("hello").expect("a code");
        let n = qr.width() as isize;
        for (x0, y0) in [(0, 0), (n - 7, 0), (0, n - 7)] {
            assert!(qr.is_dark(x0, y0) && qr.is_dark(x0 + 6, y0 + 6), "corner of the finder at {x0},{y0}");
            assert!(
                !qr.is_dark(x0 + 1, y0 + 1) && qr.is_dark(x0 + 3, y0 + 3),
                "ring and centre at {x0},{y0}"
            );
        }
        assert!(!qr.is_dark(-1, 0) && !qr.is_dark(0, n), "the quiet zone is light");
    }

    fn decoded(qr: &Qr, scale: usize) -> Vec<String> {
        let (side, rgba) = qr.rgba(scale);
        let mut image =
            rqrr::PreparedImage::prepare_from_greyscale(side, side, |x, y| rgba[(y * side + x) * 4]);
        image.detect_grids().iter().filter_map(|g| g.decode().ok()).map(|(_, text)| text).collect()
    }

    #[test]
    fn a_real_reader_gets_the_text_back_from_the_picture() {
        for text in [
            "hi",
            "https://example.com/some/path?with=a&few=parameters",
            "Grüße, 世界! Mixed text, with numbers 0123456789 and symbols #@%",
            &"long text ".repeat(150),
        ] {
            let qr = Qr::make(text).expect("a code");
            assert_eq!(decoded(&qr, 8), vec![text.trim().to_owned()], "{text}");
        }
    }

    #[test]
    fn the_same_text_makes_the_same_code_and_other_text_another() {
        assert_eq!(Qr::make("abc"), Qr::make("  abc \n"));
        assert_ne!(Qr::make("abc"), Qr::make("abd"));
    }

    #[test]
    fn nothing_and_too_much_are_refused_with_the_python_wording() {
        assert_eq!(Qr::make("  \n").unwrap_err(), QrError::Empty);
        let long = "x".repeat(MAX_BYTES + 1);
        let err = Qr::make(&long).unwrap_err();
        assert_eq!(err.to_string(), "Too long for one QR code (2901 bytes, 2900 at most).");
        assert!(Qr::make(&"x".repeat(MAX_BYTES)).is_ok(), "2 900 bytes fit at the lowest correction");
    }

    #[test]
    fn the_picture_is_black_on_white_with_a_quiet_zone() {
        let qr = Qr::make("hi").expect("a code");
        let (side, rgba) = qr.rgba(3);
        assert_eq!(side, (21 + 8) * 3);
        assert_eq!(rgba.len(), side * side * 4);
        assert_eq!(&rgba[0..4], &[255, 255, 255, 255], "the corner is quiet zone");
        let first_dark = ((BORDER * 3) * side + BORDER * 3) * 4;
        assert_eq!(&rgba[first_dark..first_dark + 4], &[0, 0, 0, 255], "the finder square starts dark");
        let dark = rgba.chunks(4).filter(|p| p[0] == 0).count();
        assert_eq!(dark, qr.dark.iter().filter(|d| **d).count() * 9);
    }

    #[test]
    fn the_svg_draws_the_same_modules() {
        let qr = Qr::make("hi").expect("a code");
        let svg = qr.svg(10);
        assert!(svg.contains("width=\"290\" height=\"290\" viewBox=\"0 0 29 29\""), "{svg}");
        let squares: usize = svg
            .split('M')
            .skip(1)
            .map(|run| {
                run.split('h')
                    .nth(1)
                    .and_then(|w| w.split('v').next())
                    .and_then(|w| w.parse::<usize>().ok())
                    .unwrap_or(0)
            })
            .sum();
        assert_eq!(squares, qr.dark.iter().filter(|d| **d).count());
    }

    #[test]
    fn the_line_under_the_code_tells_a_link_from_text() {
        assert_eq!(describe("https://example.com"), "Link, 19 characters. Point a phone camera at it.");
        assert_eq!(describe("www.example.com"), "Link, 15 characters. Point a phone camera at it.");
        assert_eq!(describe("x"), "Text, 1 character. Point a phone camera at it.");
        assert_eq!(describe(" naïve "), "Text, 5 characters. Point a phone camera at it.");
    }
}
