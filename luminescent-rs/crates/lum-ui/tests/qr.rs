//! QR Code: pictures of the panel in Dark and Light, the line under the code, and a real reader scanning what
//! the panel draws on screen. Pictures are compared on Linux only.

#![allow(clippy::unwrap_used)]

use std::time::Duration;

use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, SnapshotOptions};
use lum_core::Paths;
use lum_core::events::EventBus;
use lum_core::module::{LogNotifier, Module, ModuleCx};
use lum_core::settings::Settings;
use lum_ui::tools::qr::QrModule;
use lum_ui::{Services, Tokens, UiCx, UiModule};

struct State {
    module: QrModule,
    settings: Settings,
    light: bool,
    ready: bool,
}

fn make(dir: &std::path::Path, light: bool) -> State {
    let paths = Paths::custom(dir);
    paths.create_dirs().unwrap();
    let mut settings = Settings::load(&paths);
    let services = Services::detached(paths.data.clone());
    let mut module = QrModule::new(&services);
    let events = EventBus::new();
    let mut cx =
        ModuleCx { settings: &mut settings, events: &events, notifier: &LogNotifier, safe_mode: false };
    module.start(&mut cx).unwrap();
    State { module, settings, light, ready: false }
}

fn harness(state: State, render: bool) -> Harness<'static, State> {
    let builder = Harness::builder().with_size([460.0, 620.0]).with_pixels_per_point(1.0);
    let builder = if render && cfg!(target_os = "linux") { builder.wgpu() } else { builder };
    builder.build_ui_state(
        |ui, state: &mut State| {
            if !state.ready {
                lum_ui::fonts::install(ui.ctx());
                lum_ui::Motion::off().install(ui.ctx());
                if state.light { Tokens::light() } else { Tokens::dark() }.install(ui.ctx());
                state.ready = true;
            }
            let ctx = ui.ctx().clone();
            let mut cx = UiCx::new(&ctx, &state.settings);
            state.module.show(&mut cx);
        },
        state,
    )
}

/// Frames until the clipboard has been read on its thread and the pause after typing is over.
fn settle(h: &mut Harness<'_, State>) {
    for _ in 0..30 {
        h.step();
        std::thread::sleep(Duration::from_millis(15));
    }
    h.run();
}

fn opened(state: State, render: bool) -> Harness<'static, State> {
    let mut h = harness(state, render);
    h.run();
    h.state_mut().module.bind_pressed("clipboard");
    settle(&mut h);
    h
}

fn type_into_the_box(h: &mut Harness<'_, State>, text: &str) {
    h.get_by_role(egui::Role::MultilineTextInput).focus();
    h.run();
    h.get_by_role(egui::Role::MultilineTextInput).type_text(text);
    settle(h);
}

fn photograph(h: &mut Harness<'_, State>, name: &str) {
    h.run();
    if cfg!(target_os = "linux") {
        let options = SnapshotOptions::new().threshold(1.5).max_failed_pixels(600);
        h.snapshot_options(name, &options);
    }
}

const LINK: &str = "https://example.com/luminescent";

#[test]
fn an_empty_panel_asks_for_text() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = opened(make(dir.path(), false), true);
    assert!(
        h.query_by_label("Copy some text or a link, then press From clipboard (or type it below).").is_some()
    );
    photograph(&mut h, "qr_empty_dark");
}

#[test]
fn a_link_makes_a_code_dark() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = opened(make(dir.path(), false), true);
    type_into_the_box(&mut h, LINK);
    assert!(h.query_by_label("Link, 31 characters. Point a phone camera at it.").is_some());
    photograph(&mut h, "qr_link_dark");
}

#[test]
fn a_text_makes_a_code_light() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = opened(make(dir.path(), true), true);
    type_into_the_box(&mut h, "Meet at the station at six");
    assert!(h.query_by_label("Text, 26 characters. Point a phone camera at it.").is_some());
    photograph(&mut h, "qr_text_light");
}

#[test]
fn too_much_text_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = opened(make(dir.path(), false), false);
    type_into_the_box(&mut h, &"x".repeat(3000));
    assert!(h.query_by_label("Too long for one QR code (3000 bytes, 2900 at most).").is_some());
}

#[test]
fn the_code_does_not_follow_until_typing_pauses() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = opened(make(dir.path(), false), false);
    h.get_by_role(egui::Role::MultilineTextInput).focus();
    h.run();
    h.get_by_role(egui::Role::MultilineTextInput).type_text("abc");
    h.step();
    assert!(h.query_by_label("Text, 3 characters. Point a phone camera at it.").is_none(), "not yet");
    settle(&mut h);
    assert!(h.query_by_label("Text, 3 characters. Point a phone camera at it.").is_some());
}

#[test]
fn a_phone_could_scan_what_the_panel_draws() {
    if !cfg!(target_os = "linux") {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut h = opened(make(dir.path(), false), true);
    type_into_the_box(&mut h, LINK);
    h.run();
    let picture = h.render().unwrap();
    let (w, hgt) = (picture.width() as usize, picture.height() as usize);
    let mut prepared = rqrr::PreparedImage::prepare_from_greyscale(w, hgt, |x, y| {
        let p = picture.get_pixel(x as u32, y as u32);
        ((p[0] as u32 + p[1] as u32 + p[2] as u32) / 3) as u8
    });
    let grids = prepared.detect_grids();
    assert_eq!(grids.len(), 1, "one code on the screen");
    let (_, text) = grids[0].decode().unwrap();
    assert_eq!(text, LINK);
}

#[test]
fn the_hotkey_hides_the_panel_again() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = opened(make(dir.path(), false), false);
    assert!(h.query_by_label("From clipboard").is_some());
    h.state_mut().module.bind_pressed("clipboard");
    settle(&mut h);
    assert!(h.query_by_label("From clipboard").is_none());
}
