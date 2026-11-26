//! Pictures of the Themes window in Dark, Light and a custom gradient theme, and checks of how a switch of theme
//! reaches the screen. Pictures are compared on Linux only, like `snapshots.rs`.

#![allow(clippy::unwrap_used)]

use egui_kittest::{Harness, SnapshotOptions, SnapshotResults};
use lum_ui::themes::demo::{Demo, Look};

fn harness(look: Look, dir: &tempfile::TempDir) -> Harness<'static, Demo> {
    let builder = Harness::builder().with_size([1280.0, 860.0]).with_pixels_per_point(1.0);
    let builder = if cfg!(target_os = "linux") { builder.wgpu() } else { builder };
    builder.build_ui_state(|ui, demo: &mut Demo| demo.ui(ui), Demo::new(dir.path(), look))
}

fn photograph(harness: &mut Harness<'_, Demo>, name: &str) {
    harness.run();
    if cfg!(target_os = "linux") {
        let options = SnapshotOptions::new().threshold(1.5).max_failed_pixels(300);
        harness.snapshot_options(name, &options);
    }
}

#[test]
fn page_dark() {
    let dir = tempfile::tempdir().unwrap();
    photograph(&mut harness(Look::Dark, &dir), "themes_page_dark");
}

#[test]
fn page_light() {
    let dir = tempfile::tempdir().unwrap();
    photograph(&mut harness(Look::Light, &dir), "themes_page_light");
}

#[test]
fn page_gradient() {
    let dir = tempfile::tempdir().unwrap();
    photograph(&mut harness(Look::Gradient, &dir), "themes_page_gradient");
}

mod picker {
    use egui::{Rect, pos2, vec2};
    use egui_kittest::{Harness, SnapshotOptions};
    use lum_theme::{Gradient, GradientKind, Rgb};
    use lum_ui::Tokens;
    use lum_ui::picker::{ColourDialog, Picked};

    struct State {
        dialog: ColourDialog,
        ready: bool,
        light: bool,
    }

    fn harness(dir: &tempfile::TempDir, light: bool, start: Picked) -> Harness<'static, State> {
        let mut dialog = ColourDialog::new(dir.path().join("picker.json"), dir.path().join("colours.json"));
        dialog.open(
            "test",
            "Accent colour",
            start,
            true,
            Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 1100.0)),
        );
        let builder = Harness::builder().with_size([1000.0, 1100.0]).with_pixels_per_point(1.0);
        let builder = if cfg!(target_os = "linux") { builder.wgpu() } else { builder };
        builder.build_ui_state(
            |ui, state: &mut State| {
                if !state.ready {
                    lum_ui::fonts::install(ui.ctx());
                    lum_ui::Motion::off().install(ui.ctx());
                    let tokens = if state.light { Tokens::light() } else { Tokens::dark() };
                    tokens.install(ui.ctx());
                    state.ready = true;
                }
                let _ = state.dialog.show(ui.ctx());
            },
            State { dialog, ready: false, light },
        )
    }

    fn dusk() -> Gradient {
        let stops = vec![
            (0.0, Rgb::from_u32(0xF06AB8)),
            (0.5, Rgb::from_u32(0x7B6CF6)),
            (1.0, Rgb::from_u32(0x3DBE96)),
        ];
        Gradient::new(GradientKind::Linear, 135.0, stops).unwrap()
    }

    #[test]
    fn gradient_mode_opens_on_a_gradient() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = harness(&dir, false, Picked::Gradient(dusk()));
        harness.run();
        assert_eq!(harness.state().dialog.value(), Some(Picked::Gradient(dusk())));
        if cfg!(target_os = "linux") {
            let options = SnapshotOptions::new().threshold(1.5).max_failed_pixels(300);
            harness.snapshot_options("picker_gradient_dark", &options);
        }
    }

    #[test]
    fn solid_mode_opens_on_a_colour() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = harness(&dir, true, Picked::Colour(Rgb::from_u32(0x7B6CF6)));
        harness.run();
        assert_eq!(harness.state().dialog.value(), Some(Picked::Colour(Rgb::from_u32(0x7B6CF6))));
        if cfg!(target_os = "linux") {
            let options = SnapshotOptions::new().threshold(1.5).max_failed_pixels(300);
            harness.snapshot_options("picker_solid_light", &options);
        }
    }
}

mod editor {
    use egui_kittest::kittest::Queryable;

    use super::*;

    fn editing(look: Look, dir: &tempfile::TempDir) -> Harness<'static, Demo> {
        let mut demo = Demo::new(dir.path(), look);
        demo.edit_active();
        let builder = Harness::builder().with_size([1360.0, 900.0]).with_pixels_per_point(1.0);
        let builder = if cfg!(target_os = "linux") { builder.wgpu() } else { builder };
        builder.build_ui_state(|ui, demo: &mut Demo| demo.ui(ui), demo)
    }

    #[test]
    fn general() {
        let mut results = SnapshotResults::new();
        for (look, name) in [(Look::Dark, "dark"), (Look::Light, "light"), (Look::Gradient, "gradient")] {
            let dir = tempfile::tempdir().unwrap();
            let mut harness = editing(look, &dir);
            photograph(&mut harness, &format!("editor_general_{name}"));
            results.extend_harness(&mut harness);
        }
    }

    #[test]
    fn colours() {
        let mut results = SnapshotResults::new();
        for (look, name) in [(Look::Dark, "dark"), (Look::Light, "light"), (Look::Gradient, "gradient")] {
            let dir = tempfile::tempdir().unwrap();
            let mut harness = editing(look, &dir);
            harness.run();
            harness.get_by_label("Colours").click();
            photograph(&mut harness, &format!("editor_colours_{name}"));
            results.extend_harness(&mut harness);
        }
    }

    #[test]
    fn a_module() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = editing(Look::Dark, &dir);
        harness.run();
        harness.get_by_label_contains("Notes").click();
        photograph(&mut harness, "editor_module_dark");
    }

    #[test]
    fn a_quick_tool() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = editing(Look::Dark, &dir);
        harness.run();
        harness.get_by_label("Colours").click();
        harness.run();
        harness.get_by_label_contains("Harmony").click();
        photograph(&mut harness, "editor_tool_dark");
    }
}

mod behaviour {
    use egui::{Key, Modifiers};
    use egui_kittest::kittest::Queryable;
    use lum_theme::Role;
    use lum_ui::Tokens;

    use super::*;

    fn editing(dir: &tempfile::TempDir) -> Harness<'static, Demo> {
        let mut demo = Demo::new(dir.path(), Look::Dark);
        demo.edit_active();
        Harness::builder()
            .with_size([1360.0, 900.0])
            .with_pixels_per_point(1.0)
            .build_ui_state(|ui, demo: &mut Demo| demo.ui(ui), demo)
    }

    fn background(harness: &Harness<'_, Demo>) -> lum_theme::Rgb {
        harness.state().store.get("dark").unwrap().role_value(Role::BgBase).solid()
    }

    fn flip_the_colours(harness: &mut Harness<'_, Demo>) {
        harness.run();
        harness.get_by_label("Colours").click();
        harness.run();
        harness.get_by_label_contains("Light ⇄ dark").click();
        harness.run();
        harness.get_by_label("Use these colours").click();
        harness.run();
    }

    #[test]
    fn a_new_theme_is_drawn_by_the_very_next_pass() {
        let dir = tempfile::tempdir().unwrap();
        let mut demo = Demo::new(dir.path(), Look::Dark);
        let mut harness = Harness::builder().with_size([1280.0, 860.0]).build_ui_state(
            |ui, demo: &mut Demo| demo.ui(ui),
            {
                demo.ui.open();
                demo
            },
        );
        harness.run();
        assert!(!Tokens::of(&harness.ctx).p.is_light());
        harness.state_mut().store.apply("light");
        harness.step();
        assert!(
            Tokens::of(&harness.ctx).p.is_light(),
            "one pass after applying, the tokens are the new theme's"
        );
    }

    #[test]
    fn saving_keeps_a_quick_tool_and_undo_takes_it_back() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = editing(&dir);
        let before = background(&harness);
        flip_the_colours(&mut harness);
        harness.get_by_label("Save").click();
        harness.run();
        let flipped = background(&harness);
        assert_ne!(before, flipped, "Save puts the flipped colours into the theme");

        let dir = tempfile::tempdir().unwrap();
        let mut harness = editing(&dir);
        flip_the_colours(&mut harness);
        harness.key_combination_modifiers(Modifiers::COMMAND, &[Key::Z]);
        harness.run();
        harness.get_by_label("Save").click();
        harness.run();
        assert_eq!(before, background(&harness), "Ctrl+Z put the colours back before saving");
    }

    #[test]
    fn cancel_asks_before_dropping_changes() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = editing(&dir);
        let before = background(&harness);
        flip_the_colours(&mut harness);
        harness.get_by_label("Cancel").click();
        harness.run();
        assert!(harness.query_by_label("Discard").is_some(), "there are changes, so it asks");
        harness.get_by_label("Discard").click();
        harness.run();
        assert_eq!(before, background(&harness), "nothing reached the theme");
        assert!(harness.query_by_label("Save").is_none(), "the editor closed");
    }

    #[test]
    fn cancel_without_changes_just_closes() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = editing(&dir);
        harness.run();
        harness.get_by_label("Cancel").click();
        harness.run();
        assert!(harness.query_by_label("Discard").is_none());
        assert!(harness.query_by_label("Save").is_none());
    }
}
