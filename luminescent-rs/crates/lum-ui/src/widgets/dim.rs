//! Darkening the screens (`DimOverlay`): behind edit mode, the snapper and dialogs that want the whole
//! attention. One frameless, see-through window per screen; click-through unless the caller wants the click.

use crate::window::{self, Screen};
use egui::{Color32, Context, Rect, ViewportBuilder, ViewportId};
use lum_platform::desktop::{self, APP_ID, marker};

/// Dims are titled per monitor, so on Hyprland a cover rule opens each one full size on its own screen.
pub const COVER_PREFIX: &str = "Luminescent dim";

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
    pub fn show(&self, ctx: &Context, screens: &[Screen]) -> bool {
        let mut clicked = false;
        if ctx.embed_viewports() {
            // no separate windows (tests, the gallery): darken the given areas of this window instead
            let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("lum-dim"));
            let painter = ctx.layer_painter(layer);
            for screen in screens {
                painter.rect_filled(screen.rect, 0.0, self.colour());
            }
            return false;
        }
        // Wayland applies an input region on the window's next commit, and winit does not commit for us: draw
        // the dims once more after click-through changes, so the change takes effect now (D26)
        let id = egui::Id::unique("lum-dim-click-through");
        let before = ctx.data_mut(|d| {
            let before = d.get_temp::<bool>(id);
            d.insert_temp(id, self.click_through);
            before
        });
        if before.is_some_and(|b| b != self.click_through) {
            ctx.request_repaint();
        }
        for screen in screens {
            let title =
                format!("{}{}", desktop::cover_title(COVER_PREFIX, &screen.name), marker::NO_ACTIVATE);
            let builder = ViewportBuilder::default()
                .with_title(title.clone())
                .with_app_id(APP_ID)
                .with_decorations(false)
                .with_transparent(true)
                .with_always_on_top()
                .with_taskbar(false)
                .with_resizable(false)
                .with_active(false)
                .with_mouse_passthrough(self.click_through)
                .with_position(screen.rect.min)
                .with_inner_size(screen.rect.size());
            let colour = self.colour();
            let click_through = self.click_through;
            ctx.show_viewport_immediate(
                ViewportId::from_hash_of(("lum-dim", &screen.name)),
                builder,
                |ui, _| {
                    ui.painter().rect_filled(ui.max_rect(), 0.0, colour);
                    if !click_through && ui.input(|i| i.pointer.any_click()) {
                        clicked = true;
                    }
                },
            );
            window::shown(ctx, &title, None, false);
        }
        clicked
    }
}

/// Darkens part of the window being drawn, with the theme's scrim (near-black on dark themes, a pale paper
/// colour on light ones). The overlay's backdrop uses it.
pub fn scrim(painter: &egui::Painter, rect: Rect, tokens: &crate::Tokens, opacity: f32) {
    painter.rect_filled(rect, 0.0, crate::theme::alpha(tokens.scrim(), opacity.max(0.02)));
}
