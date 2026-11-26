//! The Themes window on a store of its own, for the snapshot tests and `--smoke`: the same code the app runs,
//! restyled the way the app restyles when a theme is applied.

use std::path::Path;

use egui::{Rect, Ui, Vec2, pos2};
use lum_core::paths::Paths;
use lum_core::settings::Settings;
use lum_theme::{Gradient, GradientKind, Rgb, ThemeEvent, ThemeStore};
use serde_json::Map;

use crate::anim::Motion;
use crate::theme::Tokens;
use crate::themes::{Env, ThemesUi};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Dark,
    Light,
    /// A custom theme whose accent and background are gradients.
    Gradient,
}

pub struct Demo {
    pub ui: ThemesUi,
    pub store: ThemeStore,
    settings: Settings,
    fonts: bool,
    /// How many times the tokens were installed.
    pub restyles: u32,
}

impl Demo {
    pub fn new(dir: &Path, look: Look) -> Demo {
        let paths = Paths::at(dir);
        let mut counter = 0;
        let ids = Box::new(move || {
            counter += 1;
            format!("demo{counter:03}")
        });
        let mut store = ThemeStore::open(
            paths.data_file("themes.json"),
            paths.data_file("theme-images"),
            &Map::new(),
            ids,
        );
        match look {
            Look::Dark => {
                store.apply("dark");
            }
            Look::Light => {
                store.apply("light");
            }
            Look::Gradient => {
                let mut t = store.new_theme("Dusk", None, "");
                let stops = |a: u32, b: u32| {
                    Gradient::new(
                        GradientKind::Linear,
                        135.0,
                        vec![(0.0, Rgb::from_u32(a)), (1.0, Rgb::from_u32(b))],
                    )
                };
                let gradients: Vec<(String, Gradient)> =
                    [("primary", 0xF0_6A_B8, 0x7B_6C_F6), ("bg_base", 0x1B_12_2E, 0x0D_1B_2A)]
                        .into_iter()
                        .filter_map(|(role, a, b)| Some((role.to_owned(), stops(a, b)?)))
                        .collect();
                for (role, gradient) in &gradients {
                    t.colours_mut().insert(role.clone(), serde_json::json!(gradient.mid().hex()));
                }
                t.set_gradients(&gradients);
                let id = t.id().to_owned();
                store.update_theme(t);
                store.apply(&id);
            }
        }
        let settings = Settings::load(&paths);
        let ui = ThemesUi::new(paths.data_file("picker.json"), paths.data_file("colours.json"));
        let mut demo = Demo { ui, store, settings, fonts: false, restyles: 0 };
        demo.ui.open();
        demo
    }

    fn restyle(&mut self, ctx: &egui::Context) {
        if !self.fonts {
            crate::fonts::install(ctx);
            Motion::off().install(ctx);
            self.fonts = true;
        }
        let (palette, sections) = self.store.active_look();
        Tokens::of_theme(&palette, &sections, &self.settings).install(ctx);
        self.restyles += 1;
    }

    /// One frame of the window. A theme that was applied since the last frame restyles first, so the pass
    /// already draws with it.
    pub fn ui(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let events = self.store.take_events();
        if self.restyles == 0 || events.iter().any(|e| matches!(e, ThemeEvent::Applied(_))) {
            self.restyle(&ctx);
        }
        let profile_name = |id: &str| id.to_owned();
        let out = self.ui.show(&ctx, &mut self.store, &Env { profile_name: &profile_name });
        drop(out);
    }
}

/// Builds the window in all three looks with the page, the colour dialog and the editor open: every part's
/// code runs once. `--smoke` calls it with a folder to keep its files in; gives back what went wrong.
pub fn build_headless(dir: &Path) -> Result<(), String> {
    for (name, look) in [("dark", Look::Dark), ("light", Look::Light), ("gradient", Look::Gradient)] {
        let ctx = egui::Context::default();
        let mut demo = Demo::new(&dir.join(name), look);
        let mut frame = 0.0;
        let mut run = |demo: &mut Demo, ctx: &egui::Context| {
            frame += 1.0;
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), Vec2::new(1280.0, 900.0))),
                time: Some(frame),
                ..Default::default()
            };
            let output = crate::headless_pass(ctx, input, |ui| demo.ui(ui));
            ctx.tessellate(output.shapes, output.pixels_per_point).len()
        };
        if run(&mut demo, &ctx) == 0 {
            return Err(format!("the Themes page drew nothing ({name} theme)"));
        }
    }
    Ok(())
}
