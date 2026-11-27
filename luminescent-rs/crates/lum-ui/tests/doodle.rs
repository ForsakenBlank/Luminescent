//! Screen Doodle on two screens (the second at 2x), drawn side by side in one test window: holding the draw
//! key, strokes, shapes, the highlighter, text, the shared undo, the wheel, the blackboard, Esc and panic, and
//! pictures of the drawing and the tool strip in Dark and Light. Pictures are compared on Linux only.

#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use egui::{Pos2, Rect, pos2, vec2};
use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, SnapshotOptions};
use lum_core::Paths;
use lum_core::events::EventBus;
use lum_core::module::{LogNotifier, ModuleCx};
use lum_core::settings::Settings;
use lum_platform::capture::Fake;
use lum_screen::doodle::{Mark, ShapeKind, Tool};
use lum_ui::tools::doodle::DoodleModule;
use lum_ui::window::{self, Screen};
use lum_ui::{HoldEvent, Request, Services, Tokens, UiCx, UiModule};

const W: f32 = 640.0;
const H: f32 = 400.0;

struct State {
    module: DoodleModule,
    settings: Settings,
    services: Services,
    light: bool,
    ready: bool,
}

fn make(dir: &std::path::Path, light: bool, set: &[(&str, serde_json::Value)]) -> State {
    let paths = Paths::custom(dir);
    paths.create_dirs().unwrap();
    let mut settings = Settings::load(&paths);
    // drawing starts at once unless a test wants the delay
    settings.set("doodle.solo_delay_ms", serde_json::json!(0));
    for (key, value) in set {
        settings.set(key, value.clone());
    }
    let services = Services::detached(paths.data.clone());
    let mut module = DoodleModule::new(&services);
    let events = EventBus::new();
    let mut cx =
        ModuleCx { settings: &mut settings, events: &events, notifier: &LogNotifier, safe_mode: false };
    lum_core::module::Module::start(&mut module, &mut cx).unwrap();
    State { module, settings, services, light, ready: false }
}

fn harness(state: State, render: bool) -> Harness<'static, State> {
    let builder = Harness::builder().with_size([W * 2.0, H]).with_pixels_per_point(1.0);
    let builder = if render && cfg!(target_os = "linux") { builder.wgpu() } else { builder };
    let mut h = builder.build_ui_state(
        |ui, state: &mut State| {
            if !state.ready {
                lum_ui::fonts::install(ui.ctx());
                lum_ui::Motion::off().install(ui.ctx());
                if state.light { Tokens::light() } else { Tokens::dark() }.install(ui.ctx());
                window::install_capture(ui.ctx(), Arc::new(Fake::default()));
                window::install_screens(
                    ui.ctx(),
                    vec![
                        Screen::new("DP-1", Rect::from_min_size(pos2(0.0, 0.0), vec2(W, H))),
                        Screen::new("DP-2", Rect::from_min_size(pos2(W, 0.0), vec2(W, H))).with_scale(2.0),
                    ],
                );
                state.ready = true;
            }
            let ctx = ui.ctx().clone();
            let mut cx = UiCx::new(&ctx, &state.settings);
            state.module.show(&mut cx);
        },
        state,
    );
    h.run();
    h
}

fn hold(h: &mut Harness<'_, State>) {
    h.state_mut().module.hold_event(HoldEvent::Down);
    h.run();
}

fn let_go(h: &mut Harness<'_, State>) {
    h.state_mut().module.hold_event(HoldEvent::Up);
    h.run();
}

fn button(h: &mut Harness<'_, State>, at: Pos2, button: egui::PointerButton, pressed: bool) {
    h.event(egui::Event::PointerButton { pos: at, button, pressed, modifiers: egui::Modifiers::NONE });
    h.run();
}

/// A drag through `points` with a button held.
fn stroke(h: &mut Harness<'_, State>, points: &[Pos2], which: egui::PointerButton) {
    h.hover_at(points[0]);
    h.run();
    button(h, points[0], which, true);
    for p in &points[1..] {
        h.hover_at(*p);
        h.run();
    }
    button(h, *points.last().unwrap(), which, false);
}

fn left(h: &mut Harness<'_, State>, points: &[Pos2]) {
    stroke(h, points, egui::PointerButton::Primary);
}

fn photograph(h: &mut Harness<'_, State>, name: &str) {
    h.run();
    if cfg!(target_os = "linux") {
        let options = SnapshotOptions::new().threshold(1.5).max_failed_pixels(800);
        h.snapshot_options(name, &options);
    }
}

fn notes(state: &State) -> Vec<String> {
    state
        .services
        .take()
        .into_iter()
        .filter_map(|r| match r {
            Request::Notify { text, .. } => Some(text),
            _ => None,
        })
        .collect()
}

/// A drawing with every tool, on both screens.
fn draw_everything(h: &mut Harness<'_, State>) {
    left(h, &[pos2(60.0, 300.0), pos2(120.0, 250.0), pos2(180.0, 320.0), pos2(260.0, 260.0)]);
    for (tool, from, to) in [
        ("Rectangle", pos2(300.0, 150.0), pos2(420.0, 230.0)),
        ("Circle / ellipse", pos2(450.0, 150.0), pos2(600.0, 240.0)),
        ("Arrow", pos2(60.0, 120.0), pos2(240.0, 180.0)),
        ("Highlighter", pos2(W + 60.0, 300.0), pos2(W + 400.0, 300.0)),
        ("Line", pos2(W + 60.0, 340.0), pos2(W + 500.0, 370.0)),
    ] {
        h.get_by_label(tool).click();
        h.run();
        left(h, &[from, (from + to.to_vec2()) / 2.0, to]);
    }
    h.get_by_label("Text").click();
    h.run();
    h.hover_at(pos2(W + 100.0, 200.0));
    h.run();
    button(h, pos2(W + 100.0, 200.0), egui::PointerButton::Primary, true);
    button(h, pos2(W + 100.0, 200.0), egui::PointerButton::Primary, false);
    h.get_by_role(egui::Role::TextInput).type_text("Look here");
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
}

#[test]
fn drawing_with_every_tool_and_the_strip_dark() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), false, &[]), true);
    hold(&mut h);
    assert!(h.state().module.is_active());
    draw_everything(&mut h);
    assert_eq!(h.state().module.marks(), [4, 3]);
    h.hover_at(pos2(500.0, 330.0));
    photograph(&mut h, "doodle_drawing_dark");
}

#[test]
fn drawing_with_every_tool_and_the_strip_light() {
    let dir = tempfile::tempdir().unwrap();
    let set = [("doodle.colour", serde_json::json!("#2563EB"))];
    let mut h = harness(make(dir.path(), true, &set), true);
    hold(&mut h);
    h.get_by_label("Fill shapes").click();
    h.run();
    draw_everything(&mut h);
    assert!(h.state().module.fill());
    photograph(&mut h, "doodle_drawing_light");
}

#[test]
fn the_drawing_stays_after_letting_go_and_the_blackboard_goes_under_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), false, &[]), true);
    hold(&mut h);
    left(&mut h, &[pos2(100.0, 100.0), pos2(300.0, 200.0), pos2(500.0, 120.0)]);
    let_go(&mut h);
    assert!(!h.state().module.is_active());
    assert!(h.query_by_label("Doodle on DP-1").is_some(), "a screen with a drawing keeps its overlay");
    assert!(h.query_by_label("Doodle on DP-2").is_none(), "an empty screen has none");
    h.state_mut().module.bind_pressed("blackboard");
    h.run();
    assert!(h.state().module.blackboard());
    photograph(&mut h, "doodle_blackboard_dark");
}

#[test]
fn alt_shortcuts_during_the_delay_never_start_drawing() {
    let dir = tempfile::tempdir().unwrap();
    let set = [("doodle.solo_delay_ms", serde_json::json!(120))];
    let mut h = harness(make(dir.path(), false, &set), false);
    h.state_mut().module.hold_event(HoldEvent::Down);
    h.step();
    h.state_mut().module.hold_event(HoldEvent::Interrupted("n".into()));
    for _ in 0..20 {
        h.step();
    }
    assert!(!h.state().module.is_active());
    let_go(&mut h);
    h.state_mut().module.hold_event(HoldEvent::Down);
    h.step();
    assert!(!h.state().module.is_active(), "not before the delay");
    for _ in 0..20 {
        h.step();
    }
    assert!(h.state().module.is_active(), "held on its own past the delay");
    // Alt+Tab while drawing is on: out of the way until Alt is let go
    h.state_mut().module.hold_event(HoldEvent::Interrupted("tab".into()));
    h.run();
    assert!(!h.state().module.is_active());
}

#[test]
fn one_undo_history_for_both_screens() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), false, &[]), false);
    hold(&mut h);
    left(&mut h, &[pos2(100.0, 100.0), pos2(200.0, 100.0)]);
    left(&mut h, &[pos2(W + 100.0, 100.0), pos2(W + 200.0, 100.0)]);
    h.state_mut().module.bind_pressed("clear");
    h.run();
    assert_eq!(h.state().module.marks(), [2, 2]);
    h.state_mut().module.bind_pressed("undo");
    h.run();
    assert_eq!(h.state().module.marks(), [1, 1], "the clear came off both screens");
    h.state_mut().module.bind_pressed("undo");
    h.run();
    assert_eq!(h.state().module.marks(), [1, 0], "then the last stroke, on the second screen");
}

#[test]
fn shift_draws_a_straight_line_and_the_right_button_rubs_out() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), false, &[]), false);
    hold(&mut h);
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::SHIFT));
    left(&mut h, &[pos2(100.0, 100.0), pos2(150.0, 180.0), pos2(300.0, 100.0)]);
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    h.run();
    assert!(matches!(
        h.state().module.marks_on(0)[0],
        Mark::Shape { kind: ShapeKind::Line, a: (100.0, 100.0), b: (300.0, 100.0), .. }
    ));
    stroke(&mut h, &[pos2(120.0, 100.0), pos2(180.0, 100.0)], egui::PointerButton::Secondary);
    assert!(matches!(h.state().module.marks_on(0)[1], Mark::Pen { erase: true, width: 12.0, .. }));
}

#[test]
fn the_wheel_changes_the_brush_and_saves_it_quietly() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), false, &[]), false);
    hold(&mut h);
    h.hover_at(pos2(200.0, 200.0));
    h.run();
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: vec2(0.0, 2.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Unknown,
        modifiers: egui::Modifiers::NONE,
    });
    h.run();
    assert_eq!(h.state().module.brush_size(), 6);
    assert!(h.query_by_label("6 PX").is_some());
    for _ in 0..90 {
        h.step();
    }
    let saved: Vec<_> =
        h.state().services.take().into_iter().filter(|r| matches!(r, Request::SetSetting { .. })).collect();
    assert_eq!(saved, [Request::SetSetting { key: "doodle.brush_size".into(), value: serde_json::json!(6) }]);
}

#[test]
fn typing_keeps_drawing_on_after_the_key_is_let_go() {
    let dir = tempfile::tempdir().unwrap();
    let set = [("doodle.start_tool", serde_json::json!("text"))];
    let mut h = harness(make(dir.path(), false, &set), false);
    hold(&mut h);
    assert_eq!(h.state().module.tool(), Tool::Text);
    h.hover_at(pos2(150.0, 150.0));
    h.run();
    button(&mut h, pos2(150.0, 150.0), egui::PointerButton::Primary, true);
    button(&mut h, pos2(150.0, 150.0), egui::PointerButton::Primary, false);
    let_go(&mut h);
    assert!(h.state().module.is_active(), "still on while the text box is open");
    h.get_by_role(egui::Role::TextInput).type_text("Hi");
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    assert!(!h.state().module.is_active());
    assert!(matches!(&h.state().module.marks_on(0)[0], Mark::Text { text, .. } if text == "Hi"));
}

#[test]
fn escape_stops_drawing_until_let_go_and_panic_clears_everything() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), false, &[]), false);
    hold(&mut h);
    left(&mut h, &[pos2(100.0, 100.0), pos2(200.0, 100.0)]);
    h.state_mut().module.bind_pressed("escape");
    h.run();
    assert!(!h.state().module.is_active());
    hold(&mut h);
    assert!(!h.state().module.is_active(), "stays off until the key is let go");
    let_go(&mut h);
    hold(&mut h);
    assert!(h.state().module.is_active());
    h.state_mut().module.bind_pressed("blackboard");
    h.state_mut().module.bind_pressed("panic");
    h.run();
    assert!(!h.state().module.is_active() && !h.state().module.blackboard());
    assert!(h.query_by_label("Doodle on DP-1").is_none(), "nothing left on screen");
}

#[test]
fn the_strip_switches_tools_and_close_stops() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), false, &[]), false);
    hold(&mut h);
    h.get_by_label("Arrow").click();
    h.run();
    assert_eq!(h.state().module.tool(), Tool::Arrow);
    h.get_by_label("Stop drawing").click();
    h.run();
    assert!(!h.state().module.is_active());
    // the next time it starts with the tool used last
    let_go(&mut h);
    hold(&mut h);
    assert_eq!(h.state().module.tool(), Tool::Arrow);
}

#[test]
fn toggle_mode_and_the_keys_it_wants() {
    let dir = tempfile::tempdir().unwrap();
    let set = [("doodle.mode", serde_json::json!("toggle"))];
    let mut h = harness(make(dir.path(), false, &set), false);
    assert_eq!(h.state().module.hold_setting(), None);
    assert!(h.state().module.binds().iter().any(|b| b.id == "toggle"));
    let combos: Vec<_> = h.state().module.fixed_binds().into_iter().map(|b| b.combo).collect();
    assert_eq!(combos, ["esc"]);
    h.state_mut().module.bind_pressed("toggle");
    h.run();
    assert!(h.state().module.is_active());
    h.state_mut().module.bind_pressed("toggle");
    h.run();
    assert!(!h.state().module.is_active());

    let held = make(dir.path(), false, &[]);
    assert_eq!(held.module.hold_setting(), Some("hotkey"));
    let combos: Vec<_> = held.module.fixed_binds().into_iter().map(|b| b.combo).collect();
    assert_eq!(combos, ["esc", "alt+esc"]);
}

#[test]
fn copy_screen_with_the_drawing() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), false, &[]), false);
    hold(&mut h);
    left(&mut h, &[pos2(100.0, 100.0), pos2(200.0, 100.0)]);
    h.state_mut().module.bind_pressed("copy");
    // the screenshot, the picture and the clipboard are on a worker thread: wait for its answer
    let mut said = Vec::new();
    for _ in 0..500 {
        h.step();
        said.extend(notes(h.state()));
        if !said.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(said.len(), 1, "{said:?}");
    // the container may have no clipboard to put it on
    assert!(
        said[0].starts_with("Screenshot with your drawing copied") || said[0].starts_with("Could not copy"),
        "{said:?}"
    );
}
