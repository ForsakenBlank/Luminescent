//! The overlay's backdrop: every screen dimmed with the theme's scrim, the dot grid and the halo at the top.
//! Phase 3 shows it empty (the Hello tray milestone); phase 5 puts the switcher and the pages on it.
//!
//! One window per screen. The first screen's window (the focused monitor on Hyprland, the primary one on
//! Windows) takes the keyboard so Esc reaches it; the others never do. The backdrops are not pinned, so other
//! programs' windows can come up above them (the Python app's `lum_no_pin`).

use crate::Tokens;
use crate::paint;
use crate::widgets::dim;
use crate::window::{self, Screen};
use egui::{Context, Key, ViewportBuilder, ViewportId};
use lum_platform::desktop::{self, APP_ID, marker};

/// The overlay's windows are titled per monitor, so on Hyprland a cover rule opens each full size on its own
/// screen.
pub const COVER_PREFIX: &str = "Luminescent overlay";

/// What happened on the overlay this frame.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct OverlayResponse {
    /// Esc, or a click on the empty backdrop.
    pub close: bool,
}

#[derive(Debug)]
pub struct Overlay {
    /// How dark the backdrop is, 0 to 1 (Settings → Overlay & Interface).
    pub dim: f32,
    /// Set when it opens, so the first frame hands it the keyboard.
    just_opened: bool,
}

impl Default for Overlay {
    fn default() -> Overlay {
        Overlay { dim: 0.64, just_opened: false }
    }
}

fn title(screen: &Screen, focused: bool) -> String {
    let mark = if focused {
        marker::NO_PIN.to_string()
    } else {
        format!("{}{}", marker::NO_PIN, marker::NO_ACTIVATE)
    };
    format!("{}{mark}", desktop::cover_title(COVER_PREFIX, &screen.name))
}

impl Overlay {
    /// Call when it opens (a hotkey, the tray, `luminescent ctl overlay`).
    pub fn opened(&mut self) {
        self.just_opened = true;
    }

    /// Draws the backdrop on every one of `screens` for this frame; call it every frame it stays open.
    pub fn show(&mut self, ctx: &Context, screens: &[Screen]) -> OverlayResponse {
        let mut response = OverlayResponse::default();
        if ctx.embed_viewports() {
            // no separate windows (tests, --smoke): one area over this window
            let rect = ctx.content_rect();
            let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("lum-overlay"));
            backdrop(&ctx.layer_painter(layer), rect, &Tokens::of(ctx), self.dim);
            response.close = ctx.input(|i| i.key_pressed(Key::Escape));
            return response;
        }
        let focus_now = std::mem::take(&mut self.just_opened);
        for (i, screen) in screens.iter().enumerate() {
            let focused = i == 0;
            let title = title(screen, focused);
            let builder = ViewportBuilder::default()
                .with_title(title.clone())
                .with_app_id(APP_ID)
                .with_decorations(false)
                .with_transparent(true)
                .with_always_on_top()
                .with_taskbar(false)
                .with_resizable(false)
                .with_active(focused)
                .with_position(screen.rect.min)
                .with_inner_size(screen.rect.size());
            let id = ViewportId::from_hash_of(("lum-overlay", &screen.name));
            let dim = self.dim;
            ctx.show_viewport_immediate(id, builder, |ui, _| {
                let rect = ui.max_rect();
                backdrop(ui.painter(), rect, &Tokens::of(ui.ctx()), dim);
                let (escape, clicked) = ui.input(|i| (i.key_pressed(Key::Escape), i.pointer.any_click()));
                response.close |= escape || clicked;
            });
            window::shown(ctx, &title, None, focused);
            if focused && focus_now {
                window::placer(ctx).focus(&title);
            }
        }
        response
    }
}

fn backdrop(painter: &egui::Painter, rect: egui::Rect, tokens: &Tokens, dim: f32) {
    dim::scrim(painter, rect, tokens, dim.clamp(0.02, 1.0));
    paint::dot_grid(painter, rect, tokens);
    paint::top_halo(painter, rect, tokens);
}
