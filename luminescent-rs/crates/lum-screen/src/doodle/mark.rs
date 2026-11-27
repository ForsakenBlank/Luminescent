//! The marks a drawing is made of, and how each is painted. Points are in the screen's layout pixels; a
//! canvas paints them at its own scale, so a drawing on a 2x monitor is as sharp as the monitor.

use lum_theme::colour::Rgb;
use tiny_skia::{
    BlendMode, Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform,
};

use crate::IRect;

/// A point in layout pixels.
pub type P = (f32, f32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeKind {
    Line,
    Rect,
    Ellipse,
    Arrow,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mark {
    /// A pen stroke, or the right button rubbing out (`erase`, three times as wide).
    Pen {
        points: Vec<P>,
        width: f32,
        colour: Rgb,
        erase: bool,
        smooth: bool,
    },
    /// The pixel brush: squares of `cell` pixels on a grid, MS Paint style.
    Pixels {
        points: Vec<P>,
        cell: u32,
        colour: Rgb,
    },
    /// The highlighter: see-through ink in one path, so it does not darken where it crosses itself.
    Highlight {
        points: Vec<P>,
        width: f32,
        colour: Rgb,
        opacity: f32,
        smooth: bool,
    },
    Shape {
        kind: ShapeKind,
        a: P,
        b: P,
        width: f32,
        colour: Rgb,
        erase: bool,
        fill: bool,
        smooth: bool,
    },
    /// Typed text: `x` is where it starts, `centre_y` the middle of its line.
    Text {
        x: f32,
        centre_y: f32,
        text: String,
        px: f32,
        colour: Rgb,
    },
    /// Clear wipes the screen (and can be undone like anything else).
    Clear,
}

/// How a pen paints: its width in layout pixels, its colour, whether it rubs out and whether its edges are
/// smoothed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ink {
    pub width: f32,
    pub colour: Rgb,
    pub erase: bool,
    pub smooth: bool,
}

/// The highlighter's width for a brush size.
pub fn highlight_width(size: u32) -> f32 {
    (size as f32 * 4.0).max(10.0)
}

/// The arrow head's length for a brush size.
pub fn head_length(size: f32) -> f32 {
    (size * 4.0).max(10.0)
}

/// The two short lines of an arrow head at `b`, coming from `a` (none when the arrow has no length).
pub fn arrow_head(a: P, b: P, head: f32) -> Vec<(P, P)> {
    if a == b {
        return Vec::new();
    }
    let angle = (b.1 - a.1).atan2(b.0 - a.0);
    let spread = 25f32.to_radians();
    [spread, -spread]
        .into_iter()
        .map(|d| (b, (b.0 - head * (angle + d).cos(), b.1 - head * (angle + d).sin())))
        .collect()
}

/// The grid cells a pixel-brush line from `a` to `b` covers (Bresenham), each as its top-left cell index.
pub fn pixel_cells(a: P, b: P, cell: u32) -> Vec<(i32, i32)> {
    let s = cell.max(1) as i32;
    let (mut x0, mut y0) = ((a.0 as i32).div_euclid(s), (a.1 as i32).div_euclid(s));
    let (x1, y1) = ((b.0 as i32).div_euclid(s), (b.1 as i32).div_euclid(s));
    let (dx, dy) = ((x1 - x0).abs(), (y1 - y0).abs());
    let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
    let mut err = dx - dy;
    let mut out = Vec::new();
    loop {
        out.push((x0, y0));
        if x0 == x1 && y0 == y1 {
            return out;
        }
        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x0 += sx;
        }
        if e2 < dx {
            err += dx;
            y0 += sy;
        }
    }
}

fn paint(colour: Rgb, alpha: f32, erase: bool, smooth: bool) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(
        colour.r,
        colour.g,
        colour.b,
        (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
    ));
    paint.anti_alias = smooth;
    if erase {
        paint.blend_mode = BlendMode::Clear;
    }
    paint
}

fn round_stroke(width: f32) -> Stroke {
    Stroke { width, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() }
}

fn polyline(points: &[P], scale: f32) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    let (first, rest) = points.split_first()?;
    pb.move_to(first.0 * scale, first.1 * scale);
    for p in rest {
        pb.line_to(p.0 * scale, p.1 * scale);
    }
    if rest.is_empty() {
        // one point: a tiny step, so the cap still shows
        pb.line_to(first.0 * scale + 0.1, first.1 * scale);
    }
    pb.finish()
}

/// A dot for a click without a move (a line between equal points paints nothing).
fn dot(pixmap: &mut Pixmap, at: P, ink: Ink, scale: f32) {
    let r = (ink.width / 2.0).max(1.0) * scale;
    if let Some(circle) = PathBuilder::from_circle(at.0 * scale, at.1 * scale, r) {
        let paint = paint(ink.colour, 1.0, ink.erase, ink.smooth);
        pixmap.fill_path(&circle, &paint, FillRule::Winding, Transform::identity(), None);
    }
}

/// One step of a pen stroke, painted as the mouse moves.
pub fn pen_segment(pixmap: &mut Pixmap, a: P, b: P, ink: Ink, scale: f32) {
    if a == b {
        dot(pixmap, a, ink, scale);
        return;
    }
    if let Some(path) = polyline(&[a, b], scale) {
        let paint = paint(ink.colour, 1.0, ink.erase, ink.smooth);
        pixmap.stroke_path(&path, &paint, &round_stroke(ink.width * scale), Transform::identity(), None);
    }
}

/// One step of the pixel brush.
pub fn pixel_segment(pixmap: &mut Pixmap, a: P, b: P, cell: u32, colour: Rgb, scale: f32) {
    let s = cell.max(1) as f32;
    let paint = paint(colour, 1.0, false, false);
    for (cx, cy) in pixel_cells(a, b, cell) {
        let (x, y) = (cx as f32 * s * scale, cy as f32 * s * scale);
        if let Some(r) =
            Rect::from_xywh(x.round(), y.round(), (s * scale).round().max(1.0), (s * scale).round().max(1.0))
        {
            pixmap.fill_rect(r, &paint, Transform::identity(), None);
        }
    }
}

impl Mark {
    /// Paints the whole mark.
    pub fn draw(&self, pixmap: &mut Pixmap, scale: f32) {
        match self {
            Mark::Pen { points, width, colour, erase, smooth } => {
                if points.len() == 1 {
                    let ink = Ink { width: *width, colour: *colour, erase: *erase, smooth: *smooth };
                    dot(pixmap, points[0], ink, scale);
                } else if let Some(path) = polyline(points, scale) {
                    let paint = paint(*colour, 1.0, *erase, *smooth);
                    pixmap.stroke_path(
                        &path,
                        &paint,
                        &round_stroke(width * scale),
                        Transform::identity(),
                        None,
                    );
                }
            }
            Mark::Pixels { points, cell, colour } => {
                let mut last = None;
                for p in points {
                    pixel_segment(pixmap, last.unwrap_or(*p), *p, *cell, *colour, scale);
                    last = Some(*p);
                }
            }
            Mark::Highlight { points, width, colour, opacity, smooth } => {
                if let Some(path) = polyline(points, scale) {
                    let stroke = Stroke {
                        width: width * scale,
                        line_cap: LineCap::Square,
                        line_join: LineJoin::Round,
                        ..Stroke::default()
                    };
                    let paint = paint(*colour, opacity.clamp(0.05, 1.0), false, *smooth);
                    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
                }
            }
            Mark::Shape { kind, a, b, width, colour, erase, fill, smooth } => {
                let ink = Ink { width: *width, colour: *colour, erase: *erase, smooth: *smooth };
                draw_shape(pixmap, *kind, (*a, *b), ink, *fill, scale);
            }
            Mark::Text { x, centre_y, text, px, colour } => {
                draw_text(pixmap, *x, *centre_y, text, *px, *colour, scale)
            }
            Mark::Clear => pixmap.fill(Color::TRANSPARENT),
        }
    }

    /// The part of the canvas the mark can touch, in the canvas's own pixels (`None`: all of it).
    pub fn bounds(&self, scale: f32) -> Option<IRect> {
        let (points, pad): (Vec<P>, f32) = match self {
            Mark::Pen { points, width, .. } => (points.clone(), *width),
            Mark::Pixels { points, cell, .. } => (points.clone(), *cell as f32 * 2.0),
            Mark::Highlight { points, width, .. } => (points.clone(), *width),
            Mark::Shape { kind, a, b, width, .. } => {
                let head = if *kind == ShapeKind::Arrow { head_length(*width) } else { 0.0 };
                (vec![*a, *b], width + head)
            }
            Mark::Text { x, centre_y, text, px, .. } => {
                let w = crate::font::width(text, *px);
                (vec![(*x, centre_y - px), (x + w, centre_y + px)], 4.0)
            }
            Mark::Clear => return None,
        };
        Some(points_bounds(&points, pad, scale))
    }
}

/// The box around `points`, grown by `pad`, in canvas pixels.
pub fn points_bounds(points: &[P], pad: f32, scale: f32) -> IRect {
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in points {
        (x0, y0, x1, y1) = (x0.min(p.0), y0.min(p.1), x1.max(p.0), y1.max(p.1));
    }
    if points.is_empty() {
        return IRect::default();
    }
    let pad = pad + 2.0;
    let (x, y) = (((x0 - pad) * scale).floor() as i32, ((y0 - pad) * scale).floor() as i32);
    let (r, b) = (((x1 + pad) * scale).ceil() as i32, ((y1 + pad) * scale).ceil() as i32);
    IRect::new(x, y, r - x, b - y)
}

fn draw_shape(pixmap: &mut Pixmap, kind: ShapeKind, (a, b): (P, P), ink: Ink, fill: bool, scale: f32) {
    let Ink { width, colour, erase, smooth } = ink;
    let pen = paint(colour, 1.0, erase, smooth);
    let stroke = Stroke {
        width: width * scale,
        line_cap: LineCap::Square,
        line_join: LineJoin::Miter,
        ..Stroke::default()
    };
    let mut pb = PathBuilder::new();
    let (ax, ay, bx, by) = (a.0 * scale, a.1 * scale, b.0 * scale, b.1 * scale);
    let area = Rect::from_ltrb(
        ax.min(bx),
        ay.min(by),
        ax.max(bx).max(ax.min(bx) + 1.0),
        ay.max(by).max(ay.min(by) + 1.0),
    );
    match kind {
        ShapeKind::Line | ShapeKind::Arrow => {
            pb.move_to(ax, ay);
            pb.line_to(bx, by);
            if kind == ShapeKind::Arrow {
                for (from, tip) in arrow_head(a, b, head_length(width)) {
                    pb.move_to(from.0 * scale, from.1 * scale);
                    pb.line_to(tip.0 * scale, tip.1 * scale);
                }
            }
        }
        ShapeKind::Rect => {
            if let Some(r) = area {
                pb.push_rect(r);
            }
        }
        ShapeKind::Ellipse => {
            if let Some(r) = area {
                pb.push_oval(r);
            }
        }
    }
    let Some(path) = pb.finish() else { return };
    if fill && matches!(kind, ShapeKind::Rect | ShapeKind::Ellipse) {
        pixmap.fill_path(&path, &pen, FillRule::Winding, Transform::identity(), None);
    }
    // lines and arrows have round ends like Qt's pen on them
    let stroke =
        if matches!(kind, ShapeKind::Line | ShapeKind::Arrow) { round_stroke(width * scale) } else { stroke };
    pixmap.stroke_path(&path, &pen, &stroke, Transform::identity(), None);
}

fn draw_text(pixmap: &mut Pixmap, x: f32, centre_y: f32, text: &str, px: f32, colour: Rgb, scale: f32) {
    let size = px * scale;
    let top = centre_y * scale - crate::font::line_height(size) / 2.0;
    let baseline = top + crate::font::semibold_ascent(size);
    let (w, h) = (pixmap.width() as i32, pixmap.height() as i32);
    let pixels = pixmap.pixels_mut();
    crate::font::rasterise(text, size, x * scale, baseline, |px, py, coverage| {
        if px < 0 || py < 0 || px >= w || py >= h || coverage <= 0.0 {
            return;
        }
        let i = (py * w + px) as usize;
        let a = coverage.min(1.0);
        let dst = pixels[i];
        let mix = |s: u8, d: u8| (f32::from(s) * a + f32::from(d) * (1.0 - a)).round() as u8;
        let out_a = mix(255, dst.alpha());
        let (r, g, b) = (mix(colour.r, dst.red()), mix(colour.g, dst.green()), mix(colour.b, dst.blue()));
        // premultiplied: no channel may be above the alpha
        if let Some(c) =
            tiny_skia::PremultipliedColorU8::from_rgba(r.min(out_a), g.min(out_a), b.min(out_a), out_a)
        {
            pixels[i] = c;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrow_heads_point_back_along_the_line() {
        let head = arrow_head((0.0, 0.0), (100.0, 0.0), 10.0);
        assert_eq!(head.len(), 2);
        let (_, tip) = head[0];
        assert!((tip.0 - (100.0 - 10.0 * 25f32.to_radians().cos())).abs() < 1e-3);
        assert!(arrow_head((5.0, 5.0), (5.0, 5.0), 10.0).is_empty());
        assert_eq!(head_length(1.0), 10.0);
        assert_eq!(highlight_width(4), 16.0);
    }

    #[test]
    fn the_pixel_brush_walks_the_grid() {
        assert_eq!(pixel_cells((0.0, 0.0), (12.0, 4.0), 4), [(0, 0), (1, 0), (2, 1), (3, 1)]);
        assert_eq!(pixel_cells((5.0, 5.0), (5.0, 5.0), 4), [(1, 1)]);
    }

    #[test]
    fn marks_paint_and_the_rubber_clears() {
        let mut pm = Pixmap::new(100, 100).expect("a pixmap");
        let red = Rgb::new(255, 0, 0);
        Mark::Pen {
            points: vec![(10.0, 50.0), (90.0, 50.0)],
            width: 6.0,
            colour: red,
            erase: false,
            smooth: true,
        }
        .draw(&mut pm, 1.0);
        assert_eq!(pm.pixel(50, 50).map(|c| (c.red(), c.alpha())), Some((255, 255)));
        Mark::Pen { points: vec![(50.0, 50.0)], width: 12.0, colour: red, erase: true, smooth: true }
            .draw(&mut pm, 1.0);
        assert_eq!(pm.pixel(50, 50).map(|c| c.alpha()), Some(0));
        Mark::Shape {
            kind: ShapeKind::Rect,
            a: (10.0, 10.0),
            b: (30.0, 30.0),
            width: 2.0,
            colour: red,
            erase: false,
            fill: true,
            smooth: true,
        }
        .draw(&mut pm, 1.0);
        assert_eq!(pm.pixel(20, 20).map(|c| c.alpha()), Some(255));
        Mark::Clear.draw(&mut pm, 1.0);
        assert!(pm.pixels().iter().all(|p| p.alpha() == 0));
    }

    #[test]
    fn highlighter_ink_does_not_darken_where_it_crosses_itself() {
        let mut pm = Pixmap::new(100, 100).expect("a pixmap");
        let points = vec![(10.0, 50.0), (90.0, 50.0), (50.0, 10.0), (50.0, 90.0)];
        Mark::Highlight { points, width: 16.0, colour: Rgb::new(255, 255, 0), opacity: 0.35, smooth: true }
            .draw(&mut pm, 1.0);
        let once = pm.pixel(20, 50).map(|c| c.alpha());
        let crossing = pm.pixel(50, 50).map(|c| c.alpha());
        assert_eq!(once, Some(89));
        assert_eq!(crossing, once);
    }

    #[test]
    fn a_2x_canvas_paints_at_twice_the_pixels() {
        let mut pm = Pixmap::new(200, 200).expect("a pixmap");
        Mark::Pen {
            points: vec![(10.0, 10.0), (90.0, 10.0)],
            width: 4.0,
            colour: Rgb::WHITE,
            erase: false,
            smooth: false,
        }
        .draw(&mut pm, 2.0);
        assert_eq!(pm.pixel(100, 20).map(|c| c.alpha()), Some(255));
        assert_eq!(pm.pixel(100, 30).map(|c| c.alpha()), Some(0));
        let b = Mark::Pen {
            points: vec![(10.0, 10.0)],
            width: 4.0,
            colour: Rgb::WHITE,
            erase: false,
            smooth: false,
        }
        .bounds(2.0)
        .expect("bounds");
        assert!(b.contains(crate::Point::new(20, 20)));
    }

    #[test]
    fn text_is_painted_in_its_colour() {
        let mut pm = Pixmap::new(300, 60).expect("a pixmap");
        Mark::Text { x: 5.0, centre_y: 30.0, text: "Hello".into(), px: 28.0, colour: Rgb::new(0, 0, 255) }
            .draw(&mut pm, 1.0);
        let inked: Vec<_> = pm.pixels().iter().filter(|p| p.alpha() == 255).collect();
        assert!(inked.len() > 100, "{}", inked.len());
        assert!(inked.iter().all(|p| p.blue() == 255 && p.red() == 0));
    }
}
