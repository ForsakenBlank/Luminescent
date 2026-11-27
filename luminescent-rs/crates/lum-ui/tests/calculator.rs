//! The Calculator: pictures of the panel and the page in Dark and Light, and what typing, the keypad,
//! Ctrl+Enter and the palette do to the history. Pictures are compared on Linux only, like `snapshots.rs`.

#![allow(clippy::unwrap_used)]

use std::path::Path;

use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, SnapshotOptions};
use lum_core::Paths;
use lum_core::events::EventBus;
use lum_core::module::{LogNotifier, Module, ModuleCx};
use lum_core::settings::Settings;
use lum_ui::tools::calculator::CalculatorModule;
use lum_ui::{Request, Services, Tokens, UiCx, UiModule};

const SAVED: &str = r##"{
 "history": [
  {"expr": "950*12", "result": "11,400", "value": 11400, "t": "2026-11-22 18:02"},
  {"expr": "5 km to mi", "result": "3.1068559612 mi", "value": 3.1068559611866697, "t": "2026-11-23 09:15"},
  {"expr": "today + 45 days", "result": "Thu 7 Jan 2027", "value": null, "t": "2026-11-23 09:16"}
 ],
 "pins": [
  {"name": "Rent", "var": "rent", "expr": "950*12", "value": 11400, "result": "11,400", "folder": "Money", "colour": "#30A46C"},
  {"name": "Bonus", "var": "bonus", "expr": "7.5", "value": 7.5, "result": "7.5"}
 ],
 "panel": {"visible": true, "x": 40, "y": 30, "w": 296, "h": 520, "keypad": false, "base": "dec"}
}"##;

struct State {
    module: CalculatorModule,
    settings: Settings,
    services: Services,
    light: bool,
    ready: bool,
    page: bool,
}

fn make(dir: &Path, light: bool, page: bool, saved: &str) -> State {
    let paths = Paths::custom(dir);
    paths.create_dirs().unwrap();
    // the page and the panel each have an input: pictures of the page show the page alone
    let saved =
        if page { saved.replace("\"visible\": true", "\"visible\": false") } else { saved.to_owned() };
    std::fs::write(paths.data_file("calculator.json"), saved).unwrap();
    // no network in tests: a file this fresh means no fetch is made
    std::fs::write(
        paths.data_file("currency.json"),
        r#"{"date": "2026-11-23", "base": "EUR", "rates": {"USD": 1.08}}"#,
    )
    .unwrap();
    let mut settings = Settings::load(&paths);
    settings.set("calculator.currency", serde_json::json!(false));
    let services = Services::detached(paths.data.clone());
    let mut module = CalculatorModule::new(&services);
    let events = EventBus::new();
    let mut cx =
        ModuleCx { settings: &mut settings, events: &events, notifier: &LogNotifier, safe_mode: false };
    module.start(&mut cx).unwrap();
    State { module, settings, services, light, ready: false, page }
}

fn harness(state: State, render: bool) -> Harness<'static, State> {
    let builder = Harness::builder().with_size([760.0, 640.0]).with_pixels_per_point(1.0);
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
            if state.page {
                let rect = ui.max_rect();
                state.module.page(ui, rect, &mut cx);
            }
            state.module.show(&mut cx);
        },
        state,
    )
}

fn photograph(harness: &mut Harness<'_, State>, name: &str) {
    harness.run();
    if cfg!(target_os = "linux") {
        let options = SnapshotOptions::new().threshold(1.5).max_failed_pixels(300);
        harness.snapshot_options(name, &options);
    }
}

#[test]
fn panel_dark() {
    let dir = tempfile::tempdir().unwrap();
    photograph(&mut harness(make(dir.path(), false, false, SAVED), true), "calculator_panel_dark");
}

#[test]
fn panel_light() {
    let dir = tempfile::tempdir().unwrap();
    photograph(&mut harness(make(dir.path(), true, false, SAVED), true), "calculator_panel_light");
}

#[test]
fn panel_with_the_keypad() {
    let dir = tempfile::tempdir().unwrap();
    let saved = SAVED.replace("\"keypad\": false", "\"keypad\": true").replace("\"h\": 520", "\"h\": 600");
    photograph(&mut harness(make(dir.path(), false, false, &saved), true), "calculator_panel_keypad");
}

#[test]
fn page_dark() {
    let dir = tempfile::tempdir().unwrap();
    photograph(&mut harness(make(dir.path(), false, true, SAVED), true), "calculator_page_dark");
}

#[test]
fn page_light() {
    let dir = tempfile::tempdir().unwrap();
    photograph(&mut harness(make(dir.path(), true, true, SAVED), true), "calculator_page_light");
}

fn input<'a>(harness: &'a Harness<'_, State>) -> egui_kittest::Node<'a> {
    harness.get_by_role_and_label(egui::Role::TextInput, "Calculator input")
}

fn history(dir: &Path) -> Vec<(String, String)> {
    let text = std::fs::read_to_string(dir.join("data/calculator.json")).unwrap();
    let saved: serde_json::Value = serde_json::from_str(&text).unwrap();
    saved["history"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| (h["expr"].as_str().unwrap().to_owned(), h["result"].as_str().unwrap().to_owned()))
        .collect()
}

fn finish(mut harness: Harness<'static, State>) {
    let state = harness.state_mut();
    let mut cx = ModuleCx {
        settings: &mut state.settings,
        events: &EventBus::new(),
        notifier: &LogNotifier,
        safe_mode: false,
    };
    state.module.stop(&mut cx);
}

#[test]
fn a_typed_sum_goes_into_the_history_and_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), false, true, SAVED), false);
    h.run();
    input(&h).focus();
    h.run();
    input(&h).type_text("12*4.5 + 10%");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    finish(h);
    let list = history(dir.path());
    assert_eq!(list.last().unwrap(), &("12*4.5 + 10%".to_owned(), "59.4".to_owned()));
    assert_eq!(list.len(), 4);
}

#[test]
fn the_keypad_types_and_equals_calculates() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), false, true, SAVED), false);
    h.run();
    for key in ["7", "×", "6", "="] {
        h.get_by_role_and_label(egui::Role::Button, key).click();
        h.run();
    }
    finish(h);
    assert_eq!(history(dir.path()).last().unwrap(), &("7×6".to_owned(), "42".to_owned()));
}

#[test]
fn ctrl_enter_calculates_copies_and_asks_for_the_overlay_to_close() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), false, true, SAVED), false);
    h.run();
    input(&h).focus();
    h.run();
    input(&h).type_text("2+2");
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::Enter);
    h.run();
    std::thread::sleep(std::time::Duration::from_millis(300));
    h.step();
    h.step();
    let requests = h.state().services.take();
    assert!(requests.contains(&Request::HideOverlay), "{requests:?}");
    finish(h);
    assert_eq!(history(dir.path()).last().unwrap().1, "4");
}

#[test]
fn a_sum_with_a_mistake_says_so_and_adds_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), false, true, SAVED), false);
    h.run();
    input(&h).focus();
    h.run();
    input(&h).type_text("1/0");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    h.get_by_label("can't divide by 0");
    finish(h);
    assert_eq!(history(dir.path()).len(), 3);
}

#[test]
fn the_palette_shows_pins_and_a_typed_sum() {
    let dir = tempfile::tempdir().unwrap();
    let state = make(dir.path(), false, false, SAVED);
    let pins = state.module.palette_entries();
    assert_eq!(pins.len(), 2);
    assert_eq!(pins[0].label, "📌 rent = 11,400");
    let live = state.module.palette_live("rent/4 + 1");
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].label, "= 2,851");
    assert!(state.module.palette_live("hello").is_empty());
    assert!(state.module.palette_live("7").is_empty());
}
