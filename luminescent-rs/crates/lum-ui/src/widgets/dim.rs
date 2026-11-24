//! Darkening the screens (`DimOverlay`): behind edit mode, the snapper and dialogs that want the whole
//! attention. One frameless, see-through window per screen; click-through unless the caller wants the click.

use egui::{Color32, Context, Rect, ViewportBuilder, ViewportId};
use lum_platform::desktop::{APP_ID, marker};

/// The dim over a set of screens.
#[derive(Debug, Clone, Copy)]
pub struct DimOverlay {
    /// 0 to 1, how dark.
    pub opacity: f32,
    /// Clicks go through to the windows below.
    pub click_through: bool,
}

impl Default for DimOverlay {
    fn default() -> DimOverlay {
        DimOverlay { opacity: 0.35, click_through: true }
    }
}

impl DimOverlay {
    fn colour(&self) -> Color32 {
        Color32::from_black_alpha((self.opacity.clamp(0.0, 1.0) * 255.0) as u8)
    }

    /// Shows the dim on each of `screens` (desktop coordinates) for this frame; call it every frame it should
    /// stay. Gives back true when a dim that is not click-through was clicked.
    pub fn show(&self, ctx: &Context, screens: &[Rect]) -> bool {
        let mut clicked = false;
        if ctx.embed_viewports() {
            // no separate windows (tests, the gallery): darken the given areas of this window instead
            let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("lum-dim"));
            let painter = ctx.layer_painter(layer);
            for screen in screens {
                painter.rect_filled(*screen, 0.0, self.colour());
            }
            return false;
        }
        for (i, screen) in screens.iter().enumerate() {
            let title = format!("Luminescent dim {}{}", i + 1, marker::NO_ACTIVATE);
            let builder = ViewportBuilder::default()
                .with_title(title)
                .with_app_id(APP_ID)
                .with_decorations(false)
                .with_transparent(true)
                .with_always_on_top()
                .with_taskbar(false)
                .with_resizable(false)
                .with_active(false)
                .with_mouse_passthrough(self.click_through)
                .with_position(screen.min)
                .with_inner_size(screen.size());
            let colour = self.colour();
            let click_through = self.click_through;
            ctx.show_viewport_immediate(ViewportId::from_hash_of(("lum-dim", i)), builder, |ui, _| {
                ui.painter().rect_filled(ui.max_rect(), 0.0, colour);
                if !click_through && ui.input(|i| i.pointer.any_click()) {
                    clicked = true;
                }
            });
        }
        clicked
    }
}

/// Darkens part of the window being drawn, with the theme's scrim (near-black on dark themes, a pale paper
/// colour on light ones). The overlay's backdrop uses it.
pub fn scrim(painter: &egui::Painter, rect: Rect, tokens: &crate::Tokens, opacity: f32) {
    painter.rect_filled(rect, 0.0, crate::theme::alpha(tokens.scrim(), opacity.max(0.02)));
}
