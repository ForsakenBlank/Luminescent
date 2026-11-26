//! Pictures of the Themes window in Dark, Light and a custom gradient theme, and checks of how a switch of theme
//! reaches the screen. Pictures are compared on Linux only, like `snapshots.rs`.

#![allow(clippy::unwrap_used)]

use egui_kittest::{Harness, SnapshotOptions};
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
