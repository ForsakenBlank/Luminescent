//! The screen ruler's measuring: two points in layout pixels, the size between them in real pixels.

use crate::{IRect, Point};

/// The hint across the top of the screen.
pub const HINT: &str =
    "DRAG TO MEASURE  ·  SHIFT LEVEL  ·  CLICK COPY W×H  ·  CTRL+CLICK COPY DISTANCE  ·  ESC";

/// What a drag measured, in real pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measure {
    pub w: i64,
    pub h: i64,
    pub distance: f64,
}

impl Measure {
    /// The line on the tag next to the pointer.
    pub fn label(&self) -> String {
        format!("{} × {} px   ·   {:.1} px", self.w, self.h, self.distance)
    }

    /// Nothing was measured yet (a click without a drag).
    pub fn is_nothing(&self) -> bool {
        self.w == 0 && self.h == 0
    }

    /// (what goes on the clipboard, what the notification says): `WxH` or the distance.
    pub fn copy_text(&self, distance: bool) -> (String, String) {
        if distance {
            let d = format!("{:.0}", self.distance);
            (d.clone(), format!("{d} px copied"))
        } else {
            (format!("{}x{}", self.w, self.h), format!("{} × {} px copied", self.w, self.h))
        }
    }
}

/// With Shift the end point keeps the line level or upright, whichever it is closer to.
pub fn locked(a: Point, b: Point, shift: bool) -> Point {
    if !shift {
        return b;
    }
    if (b.x - a.x).abs() >= (b.y - a.y).abs() { Point::new(b.x, a.y) } else { Point::new(a.x, b.y) }
}

/// Width, height and straight distance in real pixels (`ratio` real pixels to a layout pixel).
pub fn measure(a: Point, b: Point, ratio: f64) -> Measure {
    let dx = f64::from((b.x - a.x).abs()) * ratio;
    let dy = f64::from((b.y - a.y).abs()) * ratio;
    Measure { w: crate::py_round(dx), h: crate::py_round(dy), distance: dx.hypot(dy) }
}

/// Where the tag with the measurement goes for an end point `b`: below and to the right, moved to the other
/// side of the point when it would leave the screen.
pub fn tag_rect(b: Point, tag_w: i32, tag_h: i32, screen_w: i32, screen_h: i32) -> IRect {
    let mut tag = IRect::new(b.x + 14, b.y + 14, tag_w, tag_h);
    if tag.right() > screen_w {
        tag.x = b.x - 14 - tag_w;
    }
    if tag.bottom() > screen_h {
        tag.y = b.y - 14 - tag_h;
    }
    tag
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_keeps_the_line_level_or_upright() {
        let a = Point::new(10, 10);
        assert_eq!(locked(a, Point::new(50, 14), true), Point::new(50, 10));
        assert_eq!(locked(a, Point::new(12, 60), true), Point::new(10, 60));
        assert_eq!(locked(a, Point::new(12, 60), false), Point::new(12, 60));
    }

    #[test]
    fn sizes_are_in_real_pixels() {
        let m = measure(Point::new(0, 0), Point::new(30, 40), 1.0);
        assert_eq!((m.w, m.h), (30, 40));
        assert!((m.distance - 50.0).abs() < 1e-9);
        assert_eq!(m.label(), "30 × 40 px   ·   50.0 px");
        // on a 1.5x monitor 100 layout pixels are 150 real ones
        let m = measure(Point::new(100, 0), Point::new(0, 0), 1.5);
        assert_eq!((m.w, m.h), (150, 0));
        assert_eq!(m.copy_text(false), ("150x0".to_owned(), "150 × 0 px copied".to_owned()));
        assert_eq!(m.copy_text(true), ("150".to_owned(), "150 px copied".to_owned()));
        assert!(measure(Point::new(5, 5), Point::new(5, 5), 2.0).is_nothing());
    }

    #[test]
    fn the_tag_stays_on_screen() {
        assert_eq!(tag_rect(Point::new(100, 100), 200, 30, 1920, 1080), IRect::new(114, 114, 200, 30));
        assert_eq!(tag_rect(Point::new(1900, 1070), 200, 30, 1920, 1080), IRect::new(1686, 1026, 200, 30));
    }
}
