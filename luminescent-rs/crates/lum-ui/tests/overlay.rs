//! Pictures of the overlay (the backdrop, the switcher, the panel with the Themes page in it, a page that is not
//! built yet) and checks of how it opens, switches and closes. Pictures are compared on Linux only, like
//! `snapshots.rs`.

#![allow(clippy::unwrap_used)]

use egui::{Key, Modifiers, Rect, pos2, vec2};
use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, SnapshotOptions};
use lum_ui::overlay::{Config, MainOverlay, Out, PageFrame, PageSlot, pages};
use lum_ui::themes::demo::{Demo, Look};
use lum_ui::window::Screen;

const SIZE: [f32; 2] = [1600.0, 1000.0];

struct State {
    overlay: MainOverlay,
    config: Config,
    demo: Demo,
    out: Vec<Out>,
}

fn harness(dir: &tempfile::TempDir, look: Look, page: &str) -> Harness<'static, State> {
    let mut overlay = MainOverlay::default();
    let screen = Screen::new("screen", Rect::from_min_size(pos2(0.0, 0.0), vec2(SIZE[0], SIZE[1])));
    overlay.open(page, vec![screen], 0);
    let state =
        State { overlay, config: Config::default(), demo: Demo::new(dir.path(), look), out: Vec::new() };
    let builder = Harness::builder().with_size(SIZE).with_pixels_per_point(1.0);
    let builder = if cfg!(target_os = "linux") { builder.wgpu() } else { builder };
    builder.build_ui_state(
        |ui, state: &mut State| {
            let ctx = ui.ctx().clone();
            state.demo.ready(&ctx);
            let State { overlay, config, demo, out } = state;
            let mut page = |ui: &mut egui::Ui, slot: &PageSlot<'_>| {
                if slot.key == "themes" {
                    demo.page(ui, slot.on_desktop);
                } else if slot.extra_screen.is_none() {
                    pages::placeholder(ui, slot.key);
                }
                PageFrame::default()
            };
            out.push(overlay.show(&ctx, config, &mut page));
        },
        state,
    )
}

fn click_at(harness: &mut Harness<'_, State>, at: egui::Pos2) {
    harness.hover_at(at);
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
}

fn photograph(harness: &mut Harness<'_, State>, name: &str) {
    harness.run();
    if cfg!(target_os = "linux") {
        let options = SnapshotOptions::new().threshold(1.5).max_failed_pixels(300);
        harness.snapshot_options(name, &options);
    }
}

#[test]
fn themes_page_dark() {
    let dir = tempfile::tempdir().unwrap();
    photograph(&mut harness(&dir, Look::Dark, "themes"), "overlay_themes_dark");
}

#[test]
fn themes_page_light() {
    let dir = tempfile::tempdir().unwrap();
    photograph(&mut harness(&dir, Look::Light, "themes"), "overlay_themes_light");
}

#[test]
fn themes_page_gradient() {
    let dir = tempfile::tempdir().unwrap();
    photograph(&mut harness(&dir, Look::Gradient, "themes"), "overlay_themes_gradient");
}

#[test]
fn a_page_that_is_not_built_yet() {
    let dir = tempfile::tempdir().unwrap();
    photograph(&mut harness(&dir, Look::Dark, "settings"), "overlay_settings_placeholder");
}

#[test]
fn it_is_quiet_once_it_is_up() {
    // `run` fails when something asks for a frame every frame
    let dir = tempfile::tempdir().unwrap();
    let mut harness = harness(&dir, Look::Dark, "dashboard");
    harness.run();
    assert!(harness.state().out.iter().any(|o| o.first_frame));
}

#[test]
fn ctrl_and_a_number_switch_pages_and_esc_closes() {
    let dir = tempfile::tempdir().unwrap();
    let mut harness = harness(&dir, Look::Dark, "dashboard");
    harness.run();
    harness.key_press_modifiers(Modifiers::CTRL, Key::Num3);
    harness.run();
    assert_eq!(harness.state().overlay.current(), "themes");
    assert!(harness.state().out.iter().any(|o| o.page_shown.as_deref() == Some("themes")));
    harness.key_press(Key::Escape);
    harness.run();
    assert!(!harness.state().overlay.is_visible());
    assert!(harness.state().out.iter().any(|o| o.closed));
}

#[test]
fn clicking_a_page_in_the_switcher_shows_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut harness = harness(&dir, Look::Dark, "dashboard");
    harness.run();
    harness.get_by_label("Clipboard").click();
    harness.run();
    assert_eq!(harness.state().overlay.current(), "clipboard");
}

#[test]
fn typing_on_the_themes_page_asks_for_the_palette() {
    let dir = tempfile::tempdir().unwrap();
    let mut harness = harness(&dir, Look::Dark, "themes");
    harness.run();
    harness.state_mut().out.clear();
    harness.event(egui::Event::Text("ti".into()));
    harness.run();
    assert!(harness.state().out.iter().any(|o| o.palette.as_deref() == Some("ti")));
}

#[test]
fn a_click_on_the_dim_closes_it_around_a_panel_page_only() {
    let dir = tempfile::tempdir().unwrap();
    let mut harness = harness(&dir, Look::Dark, "dashboard");
    harness.run();
    click_at(&mut harness, pos2(40.0, 900.0));
    harness.run();
    assert!(harness.state().overlay.is_open(), "the Dashboard works like a desktop");
    harness.state_mut().overlay.show_page("settings");
    harness.run();
    click_at(&mut harness, pos2(40.0, 900.0));
    harness.run();
    assert!(!harness.state().overlay.is_visible());
}
