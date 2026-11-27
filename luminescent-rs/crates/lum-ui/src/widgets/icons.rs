//! Small icons drawn with the painter, for the glyphs no bundled font has (`⧉ ⌗ ⏲`) and the ones that are
//! better as shapes (the eye of a preview). They take the colour of the text around them.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, pos2, vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconShape {
    /// Two sheets, one over the other (copy).
    Copy,
    /// Three by three dots (the keypad).
    Keypad,
    /// A clock face (type a new time).
    Clock,
    /// An eye (preview).
    Eye,
}

impl IconShape {
    /// Draws the icon inside `rect`, about 60 % of its size.
    pub fn paint(self, painter: &Painter, rect: Rect, colour: Color32) {
        let unit = rect.width().min(rect.height()) * 0.6;
        let c = rect.center();
        let stroke = Stroke::new((unit / 11.0).max(1.2), colour);
        let at = |x: f32, y: f32| pos2(c.x + x * unit, c.y + y * unit);
        match self {
            IconShape::Copy => {
                let side = unit * 0.62;
                let back = Rect::from_min_size(at(-0.5, -0.5), vec2(side, side));
                let front = Rect::from_min_size(at(-0.12, -0.12), vec2(side, side));
                // only the part of the back sheet that shows: its top and left edges
                let corner = [back.left_bottom() - vec2(0.0, side * 0.45), back.left_top(), back.right_top()];
                painter.add(Shape::line(
                    vec![corner[0], corner[1], corner[2] + vec2(-side * 0.45, 0.0)],
                    stroke,
                ));
                painter.rect_stroke(front, 2.0, stroke, StrokeKind::Inside);
            }
            IconShape::Keypad => {
                for row in [-0.32, 0.0, 0.32] {
                    for col in [-0.32, 0.0, 0.32] {
                        painter.circle_filled(at(col, row), unit * 0.07, colour);
                    }
                }
            }
            IconShape::Clock => {
                painter.circle_stroke(c, unit * 0.46, stroke);
                painter.line_segment([c, at(0.0, -0.3)], stroke);
                painter.line_segment([c, at(0.2, 0.08)], stroke);
            }
            IconShape::Eye => {
                let outline: Vec<Pos2> = (0..=16)
                    .map(|i| {
                        let t = i as f32 / 16.0;
                        let x = -0.5 + t;
                        at(x, -(0.32 * (1.0 - (2.0 * x).powi(2))))
                    })
                    .chain((0..=16).rev().map(|i| {
                        let t = i as f32 / 16.0;
                        let x = -0.5 + t;
                        at(x, 0.32 * (1.0 - (2.0 * x).powi(2)))
                    }))
                    .collect();
                painter.add(Shape::closed_line(outline, stroke));
                painter.circle_filled(c, unit * 0.13, colour);
            }
        }
    }
}
