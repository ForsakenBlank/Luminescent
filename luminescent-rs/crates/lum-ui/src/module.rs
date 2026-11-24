//! The drawing side of a module (D12): lum-core's [`Module`] knows nothing about windows, so a module that
//! shows something implements [`UiModule`] as well, and the app's host holds `dyn UiModule`s.

use std::sync::Arc;

use lum_core::module::Module;
use lum_core::settings::Settings;

use crate::anim::Motion;
use crate::theme::Tokens;

/// What a module gets while it draws.
pub struct UiCx<'a> {
    pub ctx: &'a egui::Context,
    pub settings: &'a Settings,
    pub tokens: Arc<Tokens>,
    pub motion: Arc<Motion>,
}

impl<'a> UiCx<'a> {
    pub fn new(ctx: &'a egui::Context, settings: &'a Settings) -> UiCx<'a> {
        UiCx { ctx, settings, tokens: Tokens::of(ctx), motion: Motion::of(ctx) }
    }
}

pub trait UiModule: Module {
    /// Draws the module's windows for this frame, each one it has open (a panel, a pop-up). Runs whenever
    /// the app repaints, so it must not do anything slow; a module with nothing open does nothing.
    fn show(&mut self, _cx: &mut UiCx<'_>) {}
}
