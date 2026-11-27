//! Text grab's bits that do not need Tesseract or Windows: which languages to ask Tesseract for, how a
//! selection is prepared for it, and how its answer becomes the copied text.

use crate::Frame;

/// Tesseract's `-l`: every installed language but its `osd` and `snum` helpers, English first when it is there
/// (`eng+deu+...`). `listing` is what `tesseract --list-langs` prints (a heading line, then one per line).
pub fn languages(listing: &str) -> Result<String, String> {
    let langs: Vec<&str> = listing
        .lines()
        .skip(1)
        .map(str::trim)
        .filter(|l| !l.is_empty() && *l != "osd" && *l != "snum")
        .collect();
    if langs.is_empty() {
        return Err("Tesseract has no language data. Run:\nsudo pacman -S tesseract-data-eng".to_owned());
    }
    if langs.contains(&"eng") {
        let mut out = vec!["eng"];
        out.extend(langs.iter().filter(|l| **l != "eng"));
        return Ok(out.join("+"));
    }
    Ok(langs.join("+"))
}

/// The recognised lines, without blank ones, one per line or joined with spaces.
pub fn join_lines<'a>(lines: impl IntoIterator<Item = &'a str>, keep_lines: bool) -> String {
    let lines: Vec<&str> = lines.into_iter().map(str::trim).filter(|l| !l.is_empty()).collect();
    lines.join(if keep_lines { "\n" } else { " " })
}

/// Small selections are read better when they are made bigger first: 3 times under 60 pixels high, twice
/// under 200, and nothing ends up wider or taller than 9 000 pixels.
pub fn prepare(frame: &Frame, upscale: bool) -> Frame {
    let mut image = frame.to_image();
    let (w, h) = image.dimensions();
    if upscale && h < 200 && h > 0 {
        let factor = if h < 60 { 3 } else { 2 };
        image =
            image::imageops::resize(&image, w * factor, h * factor, image::imageops::FilterType::Triangle);
    }
    let (w, h) = image.dimensions();
    if w.max(h) > 9000 {
        let k = 9000.0 / f64::from(w.max(h));
        let (nw, nh) = (((f64::from(w) * k) as u32).max(1), ((f64::from(h) * k) as u32).max(1));
        image = image::imageops::resize(&image, nw, nh, image::imageops::FilterType::Triangle);
    }
    Frame::from_image(image)
}

/// A box smaller than this on either side is a slip of the mouse, not a selection.
pub const MIN_SIDE: i32 = 6;

/// The notification after copying: the text on one line, cut to 60 characters.
pub fn copied_message(text: &str) -> String {
    let flat = text.replace('\n', " ");
    let cut: String = flat.chars().take(60).collect();
    let more = if flat.chars().count() > 60 { "…" } else { "" };
    format!("Copied: {cut}{more}")
}

/// How big the review panel opens: wide enough for the longest line, tall enough for the lines, within limits.
pub fn review_size(text: &str) -> (f32, f32) {
    let lines = text.matches('\n').count() + 1;
    let longest = text.lines().map(|l| l.chars().count()).max().unwrap_or(20);
    let w = (longest * 8 + 60).clamp(380, 720);
    let h = (130 + lines * 22).min(560);
    (w as f32, h as f32)
}

/// Black text on white, the way it looks on screen: for checking an OCR engine end to end.
pub fn sample_text_picture(text: &str) -> Frame {
    let px = 20.0;
    let w = (crate::font::width(text, px) + 24.0) as u32;
    let h = (crate::font::line_height(px) + 16.0) as u32;
    let mut grey = vec![255u8; w as usize * h as usize];
    let baseline = 8.0 + crate::font::semibold_ascent(px);
    crate::font::rasterise(text, px, 12.0, baseline, |x, y, c| {
        if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
            let i = y as usize * w as usize + x as usize;
            grey[i] = grey[i].min((255.0 * (1.0 - c)) as u8);
        }
    });
    let rgba = grey.iter().flat_map(|&g| [g, g, g, 255]).collect();
    Frame { width: w, height: h, rgba }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lum_theme::colour::Rgb;

    #[test]
    fn english_goes_first_and_helpers_are_left_out() {
        let listing = "List of available languages in \"/usr/share/tessdata/\" (4):\ndeu\neng\nosd\nsnum\n";
        assert_eq!(languages(listing), Ok("eng+deu".to_owned()));
        assert_eq!(languages("List (1):\nfra\n"), Ok("fra".to_owned()));
        assert!(languages("List (1):\nosd\n").is_err());
    }

    #[test]
    fn lines_are_kept_or_joined() {
        let raw = ["  Hello ", "", "world", "  "];
        assert_eq!(join_lines(raw, true), "Hello\nworld");
        assert_eq!(join_lines(raw, false), "Hello world");
    }

    #[test]
    fn small_boxes_are_enlarged_and_huge_ones_shrunk() {
        let small = Frame::filled(100, 40, Rgb::WHITE);
        let up = prepare(&small, true);
        assert_eq!((up.width, up.height), (300, 120));
        assert_eq!(prepare(&Frame::filled(100, 150, Rgb::WHITE), true).height, 300);
        assert_eq!(prepare(&small, false).height, 40);
        let wide = prepare(&Frame::filled(9500, 300, Rgb::WHITE), true);
        assert_eq!(wide.width, 9000);
    }

    #[test]
    fn the_message_is_one_cut_line() {
        assert_eq!(copied_message("a\nb"), "Copied: a b");
        let long = "x".repeat(70);
        assert_eq!(copied_message(&long), format!("Copied: {}…", "x".repeat(60)));
        assert_eq!(review_size("short"), (380.0, 152.0));
        assert_eq!(review_size(&"y".repeat(200)), (720.0, 152.0));
    }
}
