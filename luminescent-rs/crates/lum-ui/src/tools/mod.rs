//! The everyday tools: each one a module with floating panels, an overlay page where there is one, and a box for
//! the dashboard. The logic they share with the command line and the tests lives in `lum-calc` and `lum-core`.

pub mod calculator;
pub mod clip;
pub mod draw;
pub mod edit;
pub mod notes;
pub mod overview;
pub mod qr;
pub mod reminders;
pub mod snippets;
pub mod timer;

use egui::{Context, Id};

use crate::dialog::{AskDialog, DialogWindow};

/// A dialog in a window of its own over the screen with the mouse, wherever the tool's panel is.
pub fn dialog_window(dialog: AskDialog) -> DialogWindow {
    let screens = crate::window::screens();
    let screen = crate::window::cursor_screen(&screens).or((!screens.is_empty()).then_some(0));
    DialogWindow::new(dialog, screen.map(|i| screens[i].clone()))
}

pub fn dialog_id(what: &str) -> Id {
    Id::unique(("lum-tool-dialog", what))
}

/// Wakes the UI for a repaint at `ms` from now (a debounced save, a fade).
pub fn repaint_in(ctx: &Context, ms: u64) {
    ctx.request_repaint_after(std::time::Duration::from_millis(ms));
}
