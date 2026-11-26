//! What every screen of the overlay shows behind the pages: the dim, the theme's background picture, the dot
//! grid and, on the screen with the switcher, the halo hanging from the top edge.
//!
//! In the Python app this was painted once into a full-screen pixmap per screen. Here each layer is a texture
//! made once (the dot tile, the halo, the glow, the picture decoded on its own thread) and a frame draws a
//! handful of quads, which costs the same as one blit without a full-screen texture per screen in memory.

use egui::{Color32, Painter, Pos2, Rect, Stroke, StrokeKind, pos2, vec2};

use crate::paint;
use crate::theme::{Role, Tokens};
use crate::themes::pictures::Pictures;
use crate::widgets::dim;

/// Big enough for a 4K screen; anything larger is shrunk on its decoding thread.
const LARGEST_PICTURE: u32 = 3840;

#[derive(Debug)]
pub struct Backdrop {
    pictures: Pictures,
}

impl Default for Backdrop {
    fn default() -> Backdrop {
        Backdrop { pictures: Pictures::with_largest(LARGEST_PICTURE) }
    }
}

impl Backdrop {
    /// Paints one screen's backdrop over `rect`. `halo`: this is the screen with the switcher.
    pub fn paint(&mut self, painter: &Painter, rect: Rect, tokens: &Tokens, dim_amount: f32, halo: bool) {
        dim::scrim(painter, rect, tokens, dim_amount.clamp(0.02, 1.0));
        if !tokens.performance {
            self.picture(painter, rect, tokens);
        }
        paint::dot_grid(painter, rect, tokens);
        if halo {
            paint::top_halo(painter, rect, tokens);
        }
    }

    /// Starts decoding the theme's picture ahead of the first open (prewarm), so it is there on the first frame.
    pub fn prepare(&mut self, ctx: &egui::Context, tokens: &Tokens) {
        if !tokens.performance {
            let _ = self.pictures.get(ctx, &tokens.p.background_image);
        }
    }

    /// The picture scaled to cover the whole screen and centred, like `KeepAspectRatioByExpanding`.
    fn picture(&mut self, painter: &Painter, rect: Rect, tokens: &Tokens) {
        let path = &tokens.p.background_image;
        let Some(texture) = self.pictures.get(painter.ctx(), path) else { return };
        let size = texture.size_vec2();
        if size.x <= 0.0 || size.y <= 0.0 {
            return;
        }
        let scale = (rect.width() / size.x).max(rect.height() / size.y);
        let shown = size * scale;
        let at = rect.center() - shown / 2.0;
        let opacity = tokens.p.image_opacity.clamp(0.0, 1.0);
        painter.image(
            texture.id(),
            Rect::from_min_size(at, shown),
            Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
            Color32::from_white_alpha((opacity * 255.0).round() as u8),
        );
    }
}

/// The panel the pages other than the Dashboard sit in: its glow and ring on the backdrop, the background
/// (a little see-through when the theme has a picture) and a 1 px border.
pub fn panel(painter: &Painter, rect: Rect, tokens: &Tokens) {
    paint::panel_glow(painter, rect, tokens);
    let see_through = if tokens.p.background_image.is_empty() || tokens.performance { 1.0 } else { 0.9 };
    paint::fill(painter, rect, tokens.r(), tokens, Role::BgBase, see_through);
    painter.rect_stroke(rect, tokens.r(), Stroke::new(1.0, tokens.p.border), StrokeKind::Inside);
}

/// Where the panel goes on a screen of `size` whose bars take `top` and `bottom`: at most 1180 x 780, centred,
/// below the switcher.
pub fn panel_rect(size: egui::Vec2, top: f32, bottom: f32) -> Rect {
    const PANEL_TOP: f32 = 96.0;
    let free = size.y - top - bottom;
    let w = 1180.0_f32.min((size.x * 0.84).floor());
    let h = 780.0_f32.min(free - PANEL_TOP - 56.0).max(200.0);
    let x = ((size.x - w) / 2.0).floor();
    let y = top + PANEL_TOP.max(((free - h) / 2.0).floor() + 20.0);
    Rect::from_min_size(pos2(x, y), vec2(w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_panel_sits_below_the_switcher_and_keeps_its_size_limits() {
        let r = panel_rect(vec2(2560.0, 1440.0), 0.0, 0.0);
        assert_eq!(r.size(), vec2(1180.0, 780.0));
        assert_eq!(r.min.x, 690.0);
        assert_eq!(r.min.y, 350.0);
        let small = panel_rect(vec2(1280.0, 720.0), 40.0, 0.0);
        assert_eq!(small.width(), 1075.0);
        assert_eq!(small.min.y, 40.0 + 96.0);
        assert!(small.max.y <= 720.0 - 56.0 + 0.5);
    }
}
