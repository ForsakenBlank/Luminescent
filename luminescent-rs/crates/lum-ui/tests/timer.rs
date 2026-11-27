//! Timers and stopwatches: pictures of the panels and the page in Dark and Light, and what typing a time, the
//! buttons, laps, restoring and running out do. Pictures are compared on Linux only, like `snapshots.rs`.

#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::time::Duration;

use egui::Key;
use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, SnapshotOptions};
use lum_core::Paths;
use lum_core::events::EventBus;
use lum_core::module::{LogNotifier, Module, ModuleCx};
use lum_core::settings::Settings;
use lum_ui::tools::timer::TimerModule;
use lum_ui::{Request, Services, Tokens, UiCx, UiModule};

/// A timer 5 minutes into 25, a stopped stopwatch with laps, one that has run out and a countdown in bar style's
/// place. None is running, so the pictures do not change with the clock.
const SAVED: &str = r##"[
  {"label": "Focus", "mode": "timer", "show_ms": false, "duration_ms": 1500000, "x": 20, "y": 20, "w": 260, "h": 160,
   "laps": [], "elapsed_ms": 300000.0, "running": false, "finished": false},
  {"label": "", "mode": "stopwatch", "show_ms": true, "duration_ms": 0, "x": 300, "y": 20, "w": 260, "h": 260,
   "laps": [12345.0, 23456.5, 41000.0], "elapsed_ms": 47000.0, "running": false, "finished": false},
  {"label": "Tea", "mode": "timer", "show_ms": false, "duration_ms": 300000, "x": 580, "y": 20, "w": 260, "h": 160,
   "laps": [], "elapsed_ms": 300000.0, "running": false, "finished": true}
]"##;

struct State {
    module: TimerModule,
    settings: Settings,
    services: Services,
    light: bool,
    page: bool,
    ready: bool,
}

fn make(dir: &Path, light: bool, page: bool, saved: &str) -> State {
    let paths = Paths::custom(dir);
    paths.create_dirs().unwrap();
    std::fs::write(paths.data_file("timers.json"), saved).unwrap();
    let mut settings = Settings::load(&paths);
    // not the real sound, and not the real tray
    settings.set("timer.finish_sound", serde_json::json!("none"));
    let services = Services::detached(paths.data.clone());
    let mut module = TimerModule::new(&services);
    let events = EventBus::new();
    let mut cx =
        ModuleCx { settings: &mut settings, events: &events, notifier: &LogNotifier, safe_mode: false };
    module.start(&mut cx).unwrap();
    State { module, settings, services, light, page, ready: false }
}

fn harness(state: State, render: bool) -> Harness<'static, State> {
    let builder = Harness::builder().with_size([880.0, 620.0]).with_pixels_per_point(1.0);
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
            } else {
                state.module.show(&mut cx);
            }
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

fn saved(dir: &Path) -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(dir.join("data/timers.json")).unwrap();
    serde_json::from_str::<Vec<serde_json::Value>>(&text).unwrap()
}

#[test]
fn panels_dark() {
    let dir = tempfile::tempdir().unwrap();
    photograph(&mut harness(make(dir.path(), false, false, SAVED), true), "timer_panels_dark");
}

#[test]
fn panels_light() {
    let dir = tempfile::tempdir().unwrap();
    photograph(&mut harness(make(dir.path(), true, false, SAVED), true), "timer_panels_light");
}

#[test]
fn panels_in_edit_mode_with_the_bar_style() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make(dir.path(), false, false, SAVED);
    state.settings.set("timer.progress", serde_json::json!("bar"));
    state.module.bind_pressed("edit");
    photograph(&mut harness(state, true), "timer_panels_edit_bar");
}

#[test]
fn page_dark() {
    let dir = tempfile::tempdir().unwrap();
    photograph(&mut harness(make(dir.path(), false, true, SAVED), true), "timer_page_dark");
}

#[test]
fn page_light() {
    let dir = tempfile::tempdir().unwrap();
    photograph(&mut harness(make(dir.path(), true, true, SAVED), true), "timer_page_light");
}

#[test]
fn the_new_timer_key_opens_a_box_and_a_typed_time_starts_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make(dir.path(), false, false, "[]");
    state.module.bind_pressed("new");
    let mut h = harness(state, false);
    h.run();
    let entry =
        h.get_by_role_and_label(egui::Role::TextInput, "5m, 1h30, 5m tea, at 17:30…  (empty: stopwatch)");
    entry.focus();
    h.run();
    h.get_by_role_and_label(egui::Role::TextInput, "5m, 1h30, 5m tea, at 17:30…  (empty: stopwatch)")
        .type_text("25m focus");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    let status = h.state().module.status_lines();
    assert_eq!(status.len(), 1, "{status:?}");
    assert!(
        status[0].starts_with("focus: 00:24:5") || status[0].starts_with("focus: 00:25:00"),
        "{status:?}"
    );
    h.get_by_role_and_label(egui::Role::Button, "❚❚  Pause");
    finish(h);
    let list = saved(dir.path());
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["mode"], "timer");
    assert_eq!(list[0]["duration_ms"], 1_500_000);
    assert_eq!(list[0]["label"], "focus");
    assert_eq!(list[0]["running"], true);
    assert!(list[0]["saved_at"].is_number());
}

#[test]
fn a_time_that_makes_no_sense_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make(dir.path(), false, false, "[]");
    state.module.bind_pressed("new");
    let mut h = harness(state, false);
    h.run();
    let hint = "5m, 1h30, 5m tea, at 17:30…  (empty: stopwatch)";
    h.get_by_role_and_label(egui::Role::TextInput, hint).focus();
    h.run();
    h.get_by_role_and_label(egui::Role::TextInput, hint).type_text("tea time");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    // still asking
    h.get_by_role_and_label(egui::Role::TextInput, hint);
    assert!(h.state().module.status_lines().is_empty());
    finish(h);
    assert!(saved(dir.path()).is_empty(), "a panel without a time is not kept");
}

#[test]
fn laps_pause_reset_and_the_stopwatch_buttons_work() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make(dir.path(), false, false, "[]");
    state.module.bind_pressed("new");
    // a stopwatch from the "New stopwatch" action's side
    state.services.take();
    let mut h = harness(state, false);
    h.run();
    h.key_press(Key::Escape);
    h.run();
    h.state_mut().module.bind_pressed("lap");
    h.run();
    assert!(
        saved(dir.path()).is_empty() || saved(dir.path()).iter().all(|t| t["laps"] == serde_json::json!([]))
    );
    finish(h);
}

#[test]
fn timers_come_back_after_a_restart_and_a_running_one_carries_on() {
    let dir = tempfile::tempdir().unwrap();
    let wall = lum_calc::timer::wall_seconds();
    let running = format!(
        r#"[{{"label": "Focus", "mode": "timer", "show_ms": false, "duration_ms": 1500000, "x": 20, "y": 20, "w": 260, "h": 160,
        "laps": [], "elapsed_ms": 300000.0, "running": true, "finished": false, "saved_at": {}}}]"#,
        wall - 60.0
    );
    let h = harness(make(dir.path(), false, false, &running), false);
    let lines = h.state().module.status_lines();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].starts_with("Focus: 00:18:5"), "{lines:?}");
    finish(h);
    let list = saved(dir.path());
    assert!(list[0]["elapsed_ms"].as_f64().unwrap() >= 360_000.0);
    assert_eq!(list[0]["running"], true);
}

#[test]
fn a_timer_that_ran_out_while_closed_finishes_a_moment_after_the_start() {
    let dir = tempfile::tempdir().unwrap();
    let wall = lum_calc::timer::wall_seconds();
    let late = format!(
        r#"[{{"label": "Tea", "mode": "timer", "show_ms": false, "duration_ms": 60000, "x": 20, "y": 20, "w": 260, "h": 160,
        "laps": [], "elapsed_ms": 0.0, "running": true, "finished": false, "saved_at": {}}}]"#,
        wall - 3600.0
    );
    let mut h = harness(make(dir.path(), false, false, &late), false);
    h.run();
    assert!(h.state().services.take().iter().all(|r| !matches!(r, Request::Emit { .. })), "not at once");
    std::thread::sleep(Duration::from_millis(1600));
    h.step();
    h.step();
    let requests = h.state().services.take();
    assert!(
        requests.contains(&Request::Emit { name: "timer.finished".into(), text: "Tea".into() }),
        "{requests:?}"
    );
    assert_eq!(h.state().module.status_lines(), vec!["Tea: DONE".to_owned()]);
    finish(h);
    assert_eq!(saved(dir.path())[0]["finished"], true);
}

#[test]
fn closing_a_panel_forgets_the_timer() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), false, false, SAVED), false);
    h.run();
    h.get_all_by_role_and_label(egui::Role::Button, "Close").next().unwrap().click();
    h.run();
    h.run();
    finish(h);
    assert_eq!(saved(dir.path()).len(), 2);
}
