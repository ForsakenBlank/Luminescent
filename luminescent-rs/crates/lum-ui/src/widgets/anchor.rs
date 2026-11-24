//! The 9-way anchor bar from edit mode (`AnchorBar`) and where each anchor puts a box on a screen.

use egui::{Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

use crate::fonts::Weight;
use crate::paint;
use crate::text::Text;
use crate::theme::{Role, Tokens, Tone};

/// A snap position: top / middle / bottom by left / centre / right. The key is what settings store ("tl").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Anchor {
    TopLeft,
    TopCentre,
    TopRight,
    MiddleLeft,
    Centre,
    MiddleRight,
    BottomLeft,
    BottomCentre,
    BottomRight,
}

impl Anchor {
    pub const ALL: [Anchor; 9] = [
        Anchor::TopLeft,
        Anchor::TopCentre,
        Anchor::TopRight,
        Anchor::MiddleLeft,
        Anchor::Centre,
        Anchor::MiddleRight,
        Anchor::BottomLeft,
        Anchor::BottomCentre,
        Anchor::BottomRight,
    ];

    pub fn key(self) -> &'static str {
        ["tl", "tc", "tr", "ml", "mc", "mr", "bl", "bc", "br"][self as usize]
    }

    pub fn from_key(key: &str) -> Option<Anchor> {
        Anchor::ALL.into_iter().find(|a| a.key() == key)
    }

    /// The arrow on its button, as `ANCHOR_ICONS`.
    pub fn glyph(self) -> &'static str {
        ["↖", "↑", "↗", "←", "·", "→", "↙", "↓", "↘"][self as usize]
    }

    /// Where a `size` box goes on `screen` with `padding` from the edges it hugs (`anchor_position`).
    pub fn place(self, size: Vec2, padding: f32, screen: Rect) -> Pos2 {
        let key = self.key().as_bytes();
        let y = match key[0] {
            b't' => screen.min.y + padding,
            b'm' => screen.min.y + (screen.height() - size.y) / 2.0,
            _ => screen.max.y - size.y - padding,
        };
        let x = match key[1] {
            b'l' => screen.min.x + padding,
            b'c' => screen.min.x + (screen.width() - size.x) / 2.0,
            _ => screen.max.x - size.x - padding,
        };
        pos2(x.round(), y.round())
    }
}

/// The bar: an optional hint, then the nine buttons. Gives back the anchor that was clicked.
pub fn anchor_bar(ui: &mut Ui, hint: &str) -> Option<Anchor> {
    let tokens = Tokens::of(ui.ctx());
    let background = ui.painter().add(egui::Shape::Noop);
    let mut chosen = None;
    let inner = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.add_space(10.0);
            if !hint.is_empty() {
                ui.add(Text::new(hint).px(11.0).tone(Tone::Secondary));
                ui.add_space(6.0);
            }
            for anchor in Anchor::ALL {
                if anchor_button(ui, &tokens, anchor).clicked() {
                    chosen = Some(anchor);
                }
            }
            ui.add_space(10.0);
        })
        .response;
    let rect = inner.rect.expand2(vec2(0.0, 6.0));
    ui.painter().set(
        background,
        egui::Shape::Vec(vec![
            paint::fill_shape(ui.ctx(), rect, 8.0, &tokens, Role::BgBase, 1.0),
            egui::Shape::rect_stroke(rect, 8.0, Stroke::new(1.0, tokens.p.border), StrokeKind::Inside),
        ]),
    );
    chosen
}

fn anchor_button(ui: &mut Ui, tokens: &Tokens, anchor: Anchor) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let p = &tokens.p;
        let painter = ui.painter();
        let role = if response.hovered() { Role::BgCardHover } else { Role::BgCard };
        paint::fill(painter, rect, tokens.rs(), tokens, role, 1.0);
        painter.rect_stroke(rect, tokens.rs(), Stroke::new(1.0, p.border), StrokeKind::Inside);
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            anchor.glyph(),
            tokens.font(14.0, Weight::Regular, false),
            p.text_primary,
        );
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response.on_hover_text(format!("Snap to {}", anchor.key()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_like_anchor_position() {
        let screen = Rect::from_min_size(pos2(1920.0, 0.0), vec2(2560.0, 1440.0));
        let size = vec2(400.0, 300.0);
        assert_eq!(Anchor::TopLeft.place(size, 20.0, screen), pos2(1940.0, 20.0));
        assert_eq!(Anchor::Centre.place(size, 20.0, screen), pos2(1920.0 + 1080.0, 570.0));
        assert_eq!(
            Anchor::BottomRight.place(size, 20.0, screen),
            pos2(1920.0 + 2560.0 - 420.0, 1440.0 - 320.0)
        );
        assert_eq!(Anchor::from_key("bc"), Some(Anchor::BottomCentre));
        assert_eq!(Anchor::from_key("xx"), None);
    }
}
