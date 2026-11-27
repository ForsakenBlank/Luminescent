//! The parts of the screen tools that draw nothing, so they can be tested without a desktop: the colour
//! picker's formats and its list of recent colours, the loupe's geometry, the ruler's measuring, the
//! snapper's spots, OCR text and the doodle's strokes, undo history and canvas.

pub mod colour;
pub mod doodle;
pub mod font;
pub mod frame;
pub mod loupe;
pub mod ocr;
pub mod ruler;
pub mod snap;

pub use frame::Frame;

/// A point in a screen's own layout coordinates (logical pixels, the top-left corner of the screen is 0, 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const fn new(x: i32, y: i32) -> Point {
        Point { x, y }
    }
}

/// A rectangle in whole pixels, as Qt's `QRect` would hold it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl IRect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> IRect {
        IRect { x, y, w, h }
    }

    /// The box with `a` and `b` as opposite corners, both inside it (Qt's `QRect(a, b).normalized()`).
    pub fn spanning(a: Point, b: Point) -> IRect {
        let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
        let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
        IRect::new(x0, y0, x1 - x0 + 1, y1 - y0 + 1)
    }

    pub fn right(&self) -> i32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }

    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x < self.right() && p.y >= self.y && p.y < self.bottom()
    }

    /// Grown by `by` on every side.
    pub fn grown(&self, by: i32) -> IRect {
        IRect::new(self.x - by, self.y - by, self.w + 2 * by, self.h + 2 * by)
    }

    pub fn union(&self, other: &IRect) -> IRect {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }
        let (x0, y0) = (self.x.min(other.x), self.y.min(other.y));
        let (x1, y1) = (self.right().max(other.right()), self.bottom().max(other.bottom()));
        IRect::new(x0, y0, x1 - x0, y1 - y0)
    }

    /// The part inside `other` (empty when they do not overlap).
    pub fn clipped(&self, other: &IRect) -> IRect {
        let (x0, y0) = (self.x.max(other.x), self.y.max(other.y));
        let (x1, y1) = (self.right().min(other.right()), self.bottom().min(other.bottom()));
        if x1 <= x0 || y1 <= y0 {
            return IRect::default();
        }
        IRect::new(x0, y0, x1 - x0, y1 - y0)
    }
}

/// Python's `round`: halves go to the even number (`round(2.5) == 2`).
pub fn py_round(v: f64) -> i64 {
    v.round_ties_even() as i64
}
