//! Text drawn into pictures (the doodle's text tool, test pictures for OCR) with the app's own font, Geist
//! SemiBold, as Python's text tool used `qfont(px, 600)`.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};

static GEIST_SEMIBOLD: &[u8] = include_bytes!("../../../../assets/fonts/Geist-SemiBold.ttf");

pub fn semibold() -> FontRef<'static> {
    FontRef::try_from_slice(GEIST_SEMIBOLD).expect("the bundled font is a valid font file")
}

/// How wide `text` is at `px`, in pixels.
pub fn width(text: &str, px: f32) -> f32 {
    let font = semibold();
    let scaled = font.as_scaled(PxScale::from(px));
    let mut w = 0.0;
    let mut last = None;
    for c in text.chars() {
        let id = scaled.glyph_id(c);
        if let Some(prev) = last {
            w += scaled.kern(prev, id);
        }
        w += scaled.h_advance(id);
        last = Some(id);
    }
    w
}

/// The line's height at `px` (ascent to descent).
pub fn line_height(px: f32) -> f32 {
    let font = semibold();
    let scaled = font.as_scaled(PxScale::from(px));
    scaled.ascent() - scaled.descent()
}

/// How far the tallest letters reach above the baseline at `px`.
pub fn semibold_ascent(px: f32) -> f32 {
    semibold().as_scaled(PxScale::from(px)).ascent()
}

/// Calls `plot(x, y, coverage)` for every pixel of `text` drawn with its baseline's left end at
/// (`x`, `baseline`).
pub fn rasterise(text: &str, px: f32, x: f32, baseline: f32, mut plot: impl FnMut(i32, i32, f32)) {
    let font = semibold();
    let scale = PxScale::from(px);
    let scaled = font.as_scaled(scale);
    let mut pen = x;
    let mut last = None;
    for c in text.chars() {
        let id = scaled.glyph_id(c);
        if let Some(prev) = last {
            pen += scaled.kern(prev, id);
        }
        let glyph = id.with_scale_and_position(scale, ab_glyph::point(pen, baseline));
        pen += scaled.h_advance(id);
        last = Some(id);
        let Some(outline) = font.outline_glyph(glyph) else { continue };
        let bounds = outline.px_bounds();
        outline.draw(|gx, gy, coverage| {
            plot(bounds.min.x as i32 + gx as i32, bounds.min.y as i32 + gy as i32, coverage);
        });
    }
}
