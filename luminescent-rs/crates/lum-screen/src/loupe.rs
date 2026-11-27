//! The colour picker's loupe: 11 x 11 real pixels at 13 times their size on a card that follows the mouse,
//! the hex and RGB of the centre pixel under it, and the hint at the top of the screen.

use lum_theme::colour::Rgb;

use crate::colour::Format;
use crate::{Frame, IRect, Point};

/// How many times bigger a pixel is drawn.
pub const ZOOM: i32 = 13;
/// Pixels across (and down) the loupe; the middle one is the colour you get.
pub const CELLS: i32 = 11;
pub const CARD_W: i32 = 156;
pub const PAD: i32 = 6;
/// The part under the pixels with the swatch, the hex and the RGB.
pub const FOOT_H: i32 = 41;
const OFFSET: i32 = 30;

pub fn card_height() -> i32 {
    PAD * 2 + CELLS * ZOOM + FOOT_H
}

/// Where the card goes for the mouse at `p`: below and to the right, flipped to the other side near an edge.
pub fn card_rect(p: Point, screen_w: i32, screen_h: i32) -> IRect {
    let h = card_height();
    let mut x = p.x + OFFSET;
    let mut y = p.y + OFFSET;
    if x + CARD_W + 4 > screen_w {
        x = p.x - OFFSET - CARD_W;
    }
    if y + h + 4 > screen_h {
        y = p.y - OFFSET - h;
    }
    IRect::new(x, y, CARD_W, h)
}

/// The zoomed pixels inside the card.
pub fn zoom_rect(card: IRect) -> IRect {
    IRect::new(card.x + PAD, card.y + PAD, CELLS * ZOOM, CELLS * ZOOM)
}

/// The middle cell, the one that is ringed.
pub fn centre_cell(zoom: IRect) -> IRect {
    let half = CELLS / 2;
    IRect::new(zoom.x + half * ZOOM, zoom.y + half * ZOOM, ZOOM, ZOOM)
}

/// The real pixels around the one under `p` (layout pixels), row by row; `None` past the frame's edge.
pub fn cells(frame: &Frame, p: Point, ratio: f64) -> Vec<Option<Rgb>> {
    let cx = (f64::from(p.x) * ratio) as i64;
    let cy = (f64::from(p.y) * ratio) as i64;
    let half = i64::from(CELLS / 2);
    let mut out = Vec::with_capacity((CELLS * CELLS) as usize);
    for dy in -half..=half {
        for dx in -half..=half {
            out.push(frame.get(cx + dx, cy + dy));
        }
    }
    out
}

/// The hint at the top of the screen.
pub fn hint_rect(screen_w: i32) -> IRect {
    IRect::new((screen_w - 460) / 2, 22, 460, 32)
}

/// The hint's parts, drawn with dots between them.
pub fn hint_parts(format: Format) -> [String; 4] {
    [
        format!("CLICK COPY {}", format.name().to_uppercase()),
        "ARROWS NUDGE".to_owned(),
        "ENTER PICK".to_owned(),
        "ESC".to_owned(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_flips_away_from_the_edges() {
        assert_eq!(card_rect(Point::new(100, 100), 1920, 1080), IRect::new(130, 130, 156, 196));
        let near = card_rect(Point::new(1900, 1000), 1920, 1080);
        assert_eq!((near.x, near.y), (1900 - 30 - 156, 1000 - 30 - 196));
        assert_eq!(zoom_rect(near).w, 143);
        assert_eq!(centre_cell(IRect::new(0, 0, 143, 143)), IRect::new(65, 65, 13, 13));
    }

    #[test]
    fn cells_are_read_around_the_real_pixel() {
        let mut f = Frame::filled(40, 40, Rgb::BLACK);
        let i = (20 * 40 + 20) * 4;
        f.rgba[i..i + 3].copy_from_slice(&[255, 0, 0]);
        // a 2x screen: layout (10, 10) is real (20, 20)
        let c = cells(&f, Point::new(10, 10), 2.0);
        assert_eq!(c.len(), 121);
        assert_eq!(c[60], Some(Rgb::new(255, 0, 0)));
        assert_eq!(cells(&f, Point::new(0, 0), 1.0)[0], None);
        assert_eq!(hint_parts(Format::Rgb)[0], "CLICK COPY RGB");
    }
}
