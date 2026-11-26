//! The Themes window on a store of its own, for the snapshot tests and `--smoke`: the same code the app runs,
//! restyled the way the app restyles when a theme is applied.

use std::path::Path;

use egui::{Rect, Ui, Vec2, pos2};
use lum_core::paths::Paths;
use lum_core::settings::Settings;
use lum_theme::{Gradient, GradientKind, Rgb, ThemeEvent, ThemeStore};
use serde_json::Map;

use crate::anim::Motion;
use crate::picker::Picked;
use crate::theme::Tokens;
use crate::themes::editor::ThemeEditor;
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
    edit_next: bool,
    /// How many times the tokens were installed.
    pub restyles: u32,
}

impl Demo {
    pub fn new(dir: &Path, look: Look) -> Demo {
        let paths = Paths::at(dir);
        if let Err(e) = paths.create_dirs() {
            tracing::warn!("could not make {}: {e}", dir.display());
        }
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
        let mut demo = Demo { ui, store, settings, fonts: false, edit_next: false, restyles: 0 };
        demo.ui.open();
        demo
    }

    /// Opens the theme editor on the active theme.
    pub fn edit_active(&mut self) {
        self.edit_next = true;
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
        if std::mem::take(&mut self.edit_next) {
            let id = self.store.active_id().to_owned();
            self.ui.edit(&ctx, &self.store, &id);
        }
        let profile_name = |id: &str| id.to_owned();
        let key_owner = |_: &str| None;
        let env = Env {
            profile_name: &profile_name,
            profiles: vec![("work".to_owned(), "Work".to_owned())],
            key_owner: &key_owner,
            toast_style: "glow".to_owned(),
            hyprland: true,
            caelestia: false,
        };
        drop(self.ui.show(&ctx, &mut self.store, &env));
    }
}

/// Builds the window in all three looks with the page, every page of the editor, every quick tool and the
/// colour dialog in gradient mode open: every part's code runs once. `--smoke` calls it with a folder to keep
/// its files in; gives back what went wrong.
pub fn build_headless(dir: &Path) -> Result<(), String> {
    let picture = dir.join("smoke-picture.png");
    let mut pixels = image::RgbaImage::new(16, 16);
    for (x, y, pixel) in pixels.enumerate_pixels_mut() {
        *pixel = image::Rgba([(x * 16) as u8, (y * 16) as u8, 160, 255]);
    }
    pixels.save(&picture).map_err(|e| format!("could not write a test picture: {e}"))?;
    for (name, look) in [("dark", Look::Dark), ("light", Look::Light), ("gradient", Look::Gradient)] {
        let ctx = egui::Context::default();
        let mut demo = Demo::new(&dir.join(name), look);
        let mut frame = 0.0;
        let mut run = |demo: &mut Demo, what: &str| -> Result<(), String> {
            frame += 1.0;
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), Vec2::new(1360.0, 900.0))),
                time: Some(frame),
                ..Default::default()
            };
            let output = crate::headless_pass(&ctx, input, |ui| demo.ui(ui));
            // a window's first pass only measures it, so nothing is drawn yet
            match ctx.tessellate(output.shapes, output.pixels_per_point).len() {
                0 if frame > 1.0 => Err(format!("the Themes window drew nothing ({what}, {name} theme)")),
                _ => Ok(()),
            }
        };
        run(&mut demo, "the page")?;
        demo.edit_active();
        run(&mut demo, "the editor")?;
        for key in ThemeEditor::page_keys() {
            if let Some(editor) = demo.ui.editor() {
                editor.show_page(&key);
            }
            run(&mut demo, &format!("the {key} page"))?;
        }
        if let Some(editor) = demo.ui.editor() {
            editor.show_page("colours");
        }
        for index in 0..crate::themes::editor::tools::TOOLS.len() {
            if let Some(editor) = demo.ui.editor() {
                editor.show_tool(index, Some(picture.clone()));
            }
            run(&mut demo, &format!("quick tool {index}"))?;
            run(&mut demo, &format!("quick tool {index} with its picture"))?;
        }
        let near = Rect::from_min_size(pos2(0.0, 0.0), Vec2::new(1360.0, 900.0));
        let stops = vec![(0.0, Rgb::from_u32(0xF0_6A_B8)), (1.0, Rgb::from_u32(0x3D_BE_96))];
        let gradient = Gradient::new(GradientKind::Linear, 135.0, stops).ok_or("a gradient of two stops")?;
        demo.ui.picker().open("smoke", "Colour", Picked::Gradient(gradient), true, near);
        run(&mut demo, "the colour dialog")?;
    }
    Ok(())
}
