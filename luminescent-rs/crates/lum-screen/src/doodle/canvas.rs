//! One screen's drawing: the picture shown, the marks that can still be undone, and what changed since the
//! picture was last handed to the screen (only that part is uploaded again, so a long stroke over 500 others
//! costs the same as the first one).

use lum_theme::colour::Rgb;
use tiny_skia::{Color, Pixmap};

use super::mark::{self, Mark, P};
use crate::IRect;

pub struct Canvas {
    scale: f32,
    /// Marks older than the undo history, painted in for good.
    base: Pixmap,
    pixmap: Pixmap,
    marks: Vec<Mark>,
    /// The pen stroke being drawn (already on the picture).
    live: Option<Mark>,
    dirty: Option<IRect>,
    /// Anything drawn (after an undo this means "maybe").
    has_content: bool,
}

impl std::fmt::Debug for Canvas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Canvas({}x{} at {}, {} marks)",
            self.pixmap.width(),
            self.pixmap.height(),
            self.scale,
            self.marks.len()
        )
    }
}

impl Canvas {
    /// A clear canvas for a screen `width` x `height` layout pixels big at `scale`.
    pub fn new(width: u32, height: u32, scale: f32) -> Canvas {
        let scale = if scale > 0.0 { scale } else { 1.0 };
        let (w, h) = (
            ((width as f32) * scale).round().max(1.0) as u32,
            ((height as f32) * scale).round().max(1.0) as u32,
        );
        let blank = Pixmap::new(w, h).unwrap_or_else(|| Pixmap::new(1, 1).expect("a 1 x 1 pixmap"));
        Canvas {
            scale,
            base: blank.clone(),
            pixmap: blank,
            marks: Vec::new(),
            live: None,
            dirty: None,
            has_content: false,
        }
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }

    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }

    pub fn has_content(&self) -> bool {
        self.has_content
    }

    pub fn marks(&self) -> &[Mark] {
        &self.marks
    }

    /// The picture, premultiplied RGBA.
    pub fn pixmap(&self) -> &Pixmap {
        &self.pixmap
    }

    fn whole(&self) -> IRect {
        IRect::new(0, 0, self.pixmap.width() as i32, self.pixmap.height() as i32)
    }

    fn touch(&mut self, area: Option<IRect>) {
        let area = area.unwrap_or_else(|| self.whole()).clipped(&self.whole());
        if area.is_empty() {
            return;
        }
        self.dirty = Some(self.dirty.map_or(area, |d| d.union(&area)));
    }

    /// The part that changed since the last call, in canvas pixels.
    pub fn take_dirty(&mut self) -> Option<IRect> {
        self.dirty.take()
    }

    /// Starts a pen stroke (or the pixel brush, with `cell`) at `at`: a dot straight away.
    pub fn begin_pen(
        &mut self,
        at: P,
        width: f32,
        colour: Rgb,
        erase: bool,
        smooth: bool,
        cell: Option<u32>,
    ) {
        self.end();
        self.has_content = true;
        let live = match cell {
            Some(cell) if !erase => Mark::Pixels { points: vec![at], cell, colour },
            _ => Mark::Pen { points: vec![at], width, colour, erase, smooth },
        };
        self.paint_step(&live, at, at);
        self.live = Some(live);
    }

    /// The mouse moved on with the button down: the next piece of the stroke.
    pub fn extend(&mut self, to: P) {
        let Some(mut live) = self.live.take() else { return };
        let from = match &mut live {
            Mark::Pen { points, .. } | Mark::Pixels { points, .. } => {
                let from = *points.last().unwrap_or(&to);
                points.push(to);
                from
            }
            _ => to,
        };
        self.paint_step(&live, from, to);
        self.live = Some(live);
    }

    fn paint_step(&mut self, live: &Mark, from: P, to: P) {
        let scale = self.scale;
        let pad = match live {
            Mark::Pen { width, colour, erase, smooth, .. } => {
                let ink = mark::Ink { width: *width, colour: *colour, erase: *erase, smooth: *smooth };
                mark::pen_segment(&mut self.pixmap, from, to, ink, scale);
                *width
            }
            Mark::Pixels { cell, colour, .. } => {
                mark::pixel_segment(&mut self.pixmap, from, to, *cell, *colour, scale);
                *cell as f32 * 2.0
            }
            _ => 0.0,
        };
        self.touch(Some(mark::points_bounds(&[from, to], pad, scale)));
    }

    /// The button came up: the stroke becomes a mark that can be undone.
    pub fn end(&mut self) {
        if let Some(live) = self.live.take() {
            self.marks.push(live);
        }
    }

    /// Paints a finished mark (a shape, the highlighter, text, Clear).
    pub fn add(&mut self, mark: Mark) {
        self.end();
        mark.draw(&mut self.pixmap, self.scale);
        self.touch(mark.bounds(self.scale));
        self.has_content = !matches!(mark, Mark::Clear);
        self.marks.push(mark);
    }

    /// Takes the last mark off and paints the rest again.
    pub fn undo(&mut self) -> bool {
        self.end();
        if self.marks.pop().is_none() {
            return false;
        }
        self.repaint();
        self.has_content = true;
        true
    }

    /// The oldest mark can no longer be undone: it goes into the base picture for good.
    pub fn bake_oldest(&mut self) {
        if self.marks.is_empty() {
            return;
        }
        let oldest = self.marks.remove(0);
        oldest.draw(&mut self.base, self.scale);
    }

    /// Paints everything again from the base (after an undo).
    pub fn repaint(&mut self) {
        self.pixmap = self.base.clone();
        for m in &self.marks {
            m.draw(&mut self.pixmap, self.scale);
        }
        self.touch(None);
    }

    /// Everything gone for good, history included (panic, the module stopping).
    pub fn wipe(&mut self) {
        self.live = None;
        self.marks.clear();
        self.base.fill(Color::TRANSPARENT);
        self.pixmap.fill(Color::TRANSPARENT);
        self.has_content = false;
        self.touch(None);
    }

    /// The premultiplied RGBA bytes of `area` (canvas pixels), row by row, for a partial texture upload.
    pub fn region(&self, area: IRect) -> Vec<u8> {
        let area = area.clipped(&self.whole());
        let w = self.pixmap.width() as usize;
        let data = self.pixmap.data();
        let mut out = Vec::with_capacity(area.w.max(0) as usize * area.h.max(0) as usize * 4);
        for y in area.y..area.bottom() {
            let start = (y as usize * w + area.x as usize) * 4;
            out.extend_from_slice(&data[start..start + area.w as usize * 4]);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Rgb = Rgb::new(255, 0, 0);

    fn alpha(c: &Canvas, x: u32, y: u32) -> u8 {
        c.pixmap().pixel(x, y).map_or(0, |p| p.alpha())
    }

    #[test]
    fn a_stroke_paints_as_it_goes_and_reports_only_what_changed() {
        let mut c = Canvas::new(200, 100, 1.0);
        c.begin_pen((10.0, 10.0), 4.0, RED, false, true, None);
        let first = c.take_dirty().expect("the dot");
        assert!(first.w < 20 && first.h < 20, "{first:?}");
        c.extend((60.0, 10.0));
        let step = c.take_dirty().expect("the step");
        assert!(step.contains(crate::Point::new(35, 10)) && step.w < 70, "{step:?}");
        assert_eq!(alpha(&c, 35, 10), 255);
        c.end();
        assert_eq!(c.marks().len(), 1);
        assert!(c.take_dirty().is_none());
    }

    #[test]
    fn undo_takes_the_last_mark_off_and_baked_marks_stay() {
        let mut c = Canvas::new(100, 100, 1.0);
        c.begin_pen((10.0, 10.0), 4.0, RED, false, true, None);
        c.end();
        c.add(Mark::Pen {
            points: vec![(50.0, 50.0), (60.0, 50.0)],
            width: 4.0,
            colour: RED,
            erase: false,
            smooth: true,
        });
        assert!(c.undo());
        assert_eq!(alpha(&c, 55, 50), 0);
        assert_eq!(alpha(&c, 10, 10), 255);
        c.bake_oldest();
        assert!(c.marks().is_empty());
        c.repaint();
        assert_eq!(alpha(&c, 10, 10), 255, "a baked mark is still there");
        assert!(!c.undo());
    }

    #[test]
    fn clear_is_a_mark_that_can_be_undone() {
        let mut c = Canvas::new(100, 100, 1.0);
        c.add(Mark::Pen { points: vec![(50.0, 50.0)], width: 8.0, colour: RED, erase: false, smooth: true });
        c.add(Mark::Clear);
        assert!(!c.has_content());
        assert_eq!(alpha(&c, 50, 50), 0);
        c.undo();
        assert!(c.has_content());
        assert_eq!(alpha(&c, 50, 50), 255);
    }

    #[test]
    fn the_rubber_and_the_pixel_brush() {
        let mut c = Canvas::new(100, 100, 2.0);
        assert_eq!((c.width(), c.height()), (200, 200));
        c.begin_pen((0.0, 0.0), 1.0, RED, false, false, Some(4));
        c.extend((20.0, 0.0));
        c.end();
        // 4 layout pixels a cell at 2x: real pixels 0..8 of each cell are inked
        assert_eq!(alpha(&c, 39, 7), 255);
        assert_eq!(alpha(&c, 39, 9), 0);
        c.begin_pen((10.0, 2.0), 4.0 * 3.0, RED, true, true, Some(4));
        c.end();
        assert_eq!(alpha(&c, 20, 4), 0);
        let region = c.region(IRect::new(0, 0, 4, 2));
        assert_eq!(region.len(), 4 * 2 * 4);
        c.wipe();
        assert!(c.marks().is_empty() && !c.has_content());
    }
}
