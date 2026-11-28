//! The Automation page: pictures of the first-start example in Dark and Light, and wiring, the picker, fields,
//! ▶ and Delete reaching the engine that runs on its own thread. Pictures are compared on Linux only.

#![allow(clippy::unwrap_used)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Key, Modifiers, Pos2, pos2};
use egui_kittest::{Harness, SnapshotOptions};
use lum_automation::{Automation, Config, Engine, Host, Out, SystemClock, doc};
use lum_ui::Tokens;
use lum_ui::automation::{AutomationUi, Env};
use serde_json::{Value, json};

struct Quiet;

impl Host for Quiet {
    fn send(&self, _out: Out) {}
}

struct State {
    page: AutomationUi,
    auto: Automation,
    light: bool,
    ready: bool,
}

fn make(dir: &std::path::Path, saved: Option<Value>, light: bool) -> State {
    let engine = Engine::new(saved, Arc::new(SystemClock::default()), Arc::new(Quiet), Config::default());
    let auto = Automation::start(engine, dir.join("automation.json"), || {});
    let mut page = AutomationUi::new();
    page.set_engine(auto.handle());
    State { page, auto, light, ready: false }
}

fn harness(state: State, render: bool) -> Harness<'static, State> {
    let builder = Harness::builder().with_size([1280.0, 800.0]).with_pixels_per_point(1.0);
    let builder = if render && cfg!(target_os = "linux") { builder.wgpu() } else { builder };
    let harness = builder.build_ui_state(
        |ui, state: &mut State| {
            if !state.ready {
                lum_ui::fonts::install(ui.ctx());
                lum_ui::Motion::off().install(ui.ctx());
                if state.light { Tokens::light() } else { Tokens::dark() }.install(ui.ctx());
                state.ready = true;
            }
            let rect = ui.max_rect();
            let choices = |name: &str| match name {
                "_themes" => vec![("next".to_owned(), "The next theme".to_owned())],
                _ => Vec::new(),
            };
            let env = Env { choices: &choices, snap: 8.0 };
            state.page.page(ui, rect, &env);
        },
        state,
    );
    // the harness steps a quarter of a second a frame, which is slower than any double click
    harness.ctx.options_mut(|o| o.input_options.max_double_click_delay = 5.0);
    harness
}

fn photograph(harness: &mut Harness<'_, State>, name: &str) {
    harness.run();
    harness.run();
    if cfg!(target_os = "linux") {
        let options = SnapshotOptions::new().threshold(1.5).max_failed_pixels(300);
        harness.snapshot_options(name, &options);
    }
}

/// Waits (for real) for the engine's thread.
fn wait_for(h: &mut Harness<'_, State>, what: impl Fn(&State) -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(10);
    while Instant::now() < end {
        if what(h.state()) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
        h.step();
    }
    false
}

/// A screen point for a canvas point, with the view where a new page starts (0, 0 in the middle, 100 %).
fn at(x: f32, y: f32) -> Pos2 {
    pos2(640.0 + x, 400.0 + y)
}

fn button(h: &mut Harness<'_, State>, at: Pos2, pressed: bool, button: egui::PointerButton) {
    h.event(egui::Event::PointerButton { pos: at, button, pressed, modifiers: Modifiers::NONE });
}

fn drag(h: &mut Harness<'_, State>, from: Pos2, to: Pos2) {
    h.hover_at(from);
    button(h, from, true, egui::PointerButton::Primary);
    h.step();
    for step in 1..=6 {
        h.hover_at(from + (to - from) * (step as f32 / 6.0));
        h.step();
    }
    button(h, to, false, egui::PointerButton::Primary);
    h.step();
    h.step();
}

fn click(h: &mut Harness<'_, State>, at: Pos2) {
    h.hover_at(at);
    button(h, at, true, egui::PointerButton::Primary);
    button(h, at, false, egui::PointerButton::Primary);
    h.step();
    h.step();
}

fn workspace(state: &State) -> Value {
    state.auto.read(|e| e.workspace(&e.active()).cloned().unwrap())
}

fn two_nodes() -> Value {
    json!({"version": 1, "vars": {}, "workspaces": [{"id": "w", "name": "Wiring", "enabled": true, "nodes": [
        {"id": "btn", "type": "manual", "x": -300, "y": -100, "fields": {"label": "Go"}},
        {"id": "show", "type": "show", "x": 100, "y": -100, "fields": {}}
    ], "wires": []}]})
}

#[test]
fn example_dark() {
    let dir = tempfile::tempdir().unwrap();
    photograph(&mut harness(make(dir.path(), None, false), true), "automation_example_dark");
}

#[test]
fn example_light() {
    let dir = tempfile::tempdir().unwrap();
    photograph(&mut harness(make(dir.path(), None, true), true), "automation_example_light");
}

#[test]
fn a_wire_drawn_between_ports_reaches_the_engine_and_run_sends_a_pulse() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), Some(two_nodes()), false), false);
    h.run();
    // the Button's fire output (right edge, first row) onto the Show's input (left edge, first row)
    drag(&mut h, at(-300.0 + 260.0, -100.0 + 49.0), at(100.0, -100.0 + 49.0));
    let wires = workspace(h.state())["wires"].clone();
    assert_eq!(wires.as_array().unwrap().len(), 1, "{wires}");
    assert_eq!(wires[0]["src"], json!(["btn", "out:fire"]));
    assert_eq!(wires[0]["dst"], json!(["show", "in:in"]));
    // ▶ on the Button
    click(&mut h, at(-300.0 + 241.0, -100.0 + 16.0));
    assert!(wait_for(&mut h, |s| s.auto.read(|e| e.info("w", "show").is_some_and(|i| i.text == "(empty)"))));
}

#[test]
fn dragging_out_of_an_input_picks_its_wire_up_again() {
    let dir = tempfile::tempdir().unwrap();
    let mut saved = two_nodes();
    saved["workspaces"][0]["wires"] =
        json!([{"id": "w1", "src": ["btn", "out:fire"], "dst": ["show", "in:in"]}]);
    let mut h = harness(make(dir.path(), Some(saved), false), false);
    h.run();
    // dropped on empty space far away: the wire is gone and the picker offers a node to plug it into
    drag(&mut h, at(100.0, -100.0 + 49.0), at(100.0, 250.0));
    assert!(workspace(h.state())["wires"].as_array().unwrap().is_empty());
    // words only Show's help has
    h.event(egui::Event::Text("wire carries".into()));
    h.step();
    h.key_press(Key::Enter);
    h.step();
    h.step();
    let ws = workspace(h.state());
    let nodes = ws["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 3, "{ws}");
    let new = doc::str_of(&nodes[2], "id");
    assert_eq!(ws["wires"][0]["src"], json!(["btn", "out:fire"]));
    assert_eq!(ws["wires"][0]["dst"], json!([new, "in:in"]));
}

#[test]
fn the_picker_adds_the_node_you_search_for_on_the_grid() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), Some(two_nodes()), false), false);
    h.run();
    let spot = at(261.0, 203.0);
    click(&mut h, spot);
    assert!(!h.state().page.picking());
    click(&mut h, spot);
    assert!(h.state().page.picking(), "a double-click on the open space opens the picker");
    h.event(egui::Event::Text("notification".into()));
    h.step();
    h.key_press(Key::Enter);
    h.step();
    let ws = workspace(h.state());
    let added = ws["nodes"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(added["type"], "notify");
    // snapped to the 8 px grid as Python's round() does it: 261 / 8 = 32.6 → 264, 203 / 8 = 25.4 → 200
    assert_eq!((added["x"].clone(), added["y"].clone()), (json!(264), json!(200)));
    assert_eq!(added["fields"]["title"], "Luminescent");
}

#[test]
fn moving_selecting_and_deleting_nodes() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), Some(two_nodes()), false), false);
    h.run();
    // the Show's header, dragged 50 right and 30 down: it lands on the grid
    drag(&mut h, at(100.0 + 120.0, -100.0 + 16.0), at(100.0 + 170.0, -100.0 + 46.0));
    let ws = workspace(h.state());
    let show = doc::node_in(&ws, "show").unwrap();
    assert_eq!((show["x"].as_f64(), show["y"].as_f64()), (Some(152.0), Some(-72.0)));
    assert_eq!(h.state().page.selection(), ["show"]);
    h.key_press(Key::Escape);
    h.step();
    assert!(h.state().page.selection().is_empty());
    click(&mut h, at(-300.0 + 120.0, -100.0 + 16.0));
    assert_eq!(h.state().page.selection(), ["btn"]);
    h.key_press(Key::Delete);
    h.step();
    let ws = workspace(h.state());
    assert_eq!(ws["nodes"].as_array().unwrap().len(), 1);
    assert!(doc::node_in(&ws, "btn").is_none());
}

#[test]
fn the_wheel_zooms_around_the_mouse_and_f_fits() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), Some(two_nodes()), false), false);
    h.run();
    let spot = at(200.0, 100.0);
    h.hover_at(spot);
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, 1.0),
        modifiers: Modifiers::NONE,
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Unknown,
    });
    h.step();
    let view = h.state().page.view();
    assert!((view.zoom - 1.15).abs() < 1e-4, "{view:?}");
    // the canvas point under the mouse stays under it
    let under = view.center + (spot - pos2(640.0, 400.0)) / view.zoom;
    assert!((under - pos2(200.0, 100.0)).length() < 0.01, "{under:?}");
    h.key_press(Key::F);
    h.step();
    let fitted = h.state().page.view();
    assert!(fitted.zoom <= 1.2 && fitted.zoom > 0.5, "{fitted:?}");
}

#[test]
fn the_editor_has_every_character_it_draws() {
    let dir = tempfile::tempdir().unwrap();
    let mut saved = two_nodes();
    saved["workspaces"][0]["nodes"] = json!([
        {"id": "at", "type": "at_time", "x": -300, "y": -100, "fields": {"time": "25:00"}},
        {"id": "s", "type": "show", "x": 100, "y": -100, "fields": {}}
    ]);
    let mut h = harness(make(dir.path(), Some(saved), false), true);
    // ⚠ in a status line, ▶ on the nodes, ⏎ once Show passes on more than one line
    h.state().auto.with(|e| e.run_node("w", "s", "one\ntwo"));
    assert!(wait_for(&mut h, |s| s.auto.read(|e| e.info("w", "s").is_some_and(|i| i.text == "one ⏎ two"))));
    assert!(wait_for(&mut h, |s| s
        .auto
        .read(|e| e.info("w", "at").is_some_and(|i| i.text.starts_with('⚠')))));
    h.run();
}
