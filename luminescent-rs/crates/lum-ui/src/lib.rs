//! The egui side of Luminescent: fonts, design tokens, the widget kit, floating panels and dialogs. Only this
//! crate and the binary know about egui.

pub mod alert;
pub mod anim;
pub mod cheatsheet;
pub mod dialog;
pub mod fonts;
pub mod gallery;
pub mod module;
pub mod overlay;
pub mod paint;
pub mod palette;
pub mod panel;
pub mod picker;
pub mod text;
pub mod theme;
pub mod themes;
pub mod toast;
pub mod widgets;
pub mod window;

pub use anim::Motion;
pub use module::{UiCx, UiModule};
pub use theme::{Palette, Tokens, Tone};

/// Gets a fresh context ready: the bundled fonts and the given look.
pub fn setup(ctx: &egui::Context, tokens: Tokens) {
    fonts::install(ctx);
    tokens.install(ctx);
}

/// One pass of `ui` on a context with no window behind it (tests, `--smoke`). Nothing is drawn, so the
/// texture changes the pass asks for (fonts, glow) are dropped on purpose.
pub fn headless_pass(
    ctx: &egui::Context,
    input: egui::RawInput,
    ui: impl FnMut(&mut egui::Ui),
) -> egui::FullOutput {
    let mut output = ctx.run_ui(input, ui);
    output.textures_delta.clear();
    output
}
