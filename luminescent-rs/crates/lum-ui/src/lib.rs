//! The egui side of Luminescent: fonts, design tokens, the widget kit, floating panels and dialogs. Only this
//! crate and the binary know about egui.

pub mod fonts;
pub mod theme;

pub use theme::{Palette, Tokens, Tone};

/// Gets a fresh context ready: the bundled fonts and the given look.
pub fn setup(ctx: &egui::Context, tokens: Tokens) {
    fonts::install(ctx);
    tokens.install(ctx);
}
