//! The Dashboard on the overlay: pictures in Dark and Light, and checks of dragging, folding, selecting,
//! tidying up, searching, the menus and the item dialog. Pictures are compared on Linux only, like `snapshots.rs`.

#![allow(clippy::unwrap_used)]

use std::time::Instant;

use egui::{Key, Modifiers, Pos2, Rect, pos2, vec2};
use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, SnapshotOptions};
use lum_core::dashboard::DashboardStore;
use lum_ui::dashboard::{DashboardUi, Env, Out as DashOut};
use lum_ui::overlay::{ChromeData, Config, MainOverlay, PageFrame, PageSlot, pages};
use lum_ui::themes::demo::{Demo, Look};
use lum_ui::window::Screen;
use serde_json::Map;

const SIZE: [f32; 2] = [1600.0, 1000.0];

struct State {
    overlay: MainOverlay,
    config: Config,
    demo: Demo,
    store: DashboardStore,
    ui: DashboardUi,
    outs: Vec<DashOut>,
    running: Vec<String>,
    snap: i64,
}

fn wall() -> String {
    format!("{}/../../fixtures/lumtheme/wall.png", env!("CARGO_MANIFEST_DIR"))
}

fn layout() -> String {
    let wall = wall();
    format!(
        r#"{{"layout": 3, "active": "main", "boards": [{{"id": "main", "name": "Main", "tabs": [
  {{"id": "apps", "title": "Apps", "x": 96, "y": 232, "items": [
     {{"id": "ff", "name": "Firefox", "path": "/usr/bin/firefox", "args": "--private-window", "hotkey": "ctrl+alt+f"}},
     {{"id": "h1", "kind": "heading", "name": "Work"}},
     {{"id": "code", "name": "Editor", "path": "code", "admin": true}},
     {{"id": "sep", "kind": "separator"}},
     {{"id": "grp", "kind": "group", "name": "Morning", "group": ["ff", "code"]}},
     {{"id": "url", "name": "Luminescent", "path": "https://example.org"}}]}},
  {{"id": "games", "title": "Games", "x": 392, "y": 232, "items": [
     {{"id": "g1", "name": "Chess", "path": "~/Games/chess"}}, {{"id": "g2", "name": "Cards", "path": "~/Games/cards"}}]}},
  {{"id": "empty", "title": "Empty", "x": 688, "y": 232, "collapsed": false, "items": []}},
  {{"id": "page", "kind": "page", "page": "themes", "title": "Themes", "x": 96, "y": 560, "w": 720, "h": 300}},
  {{"id": "pic", "kind": "picture", "title": "Wall", "path": "{wall}", "x": 1100, "y": 232, "w": 300, "h": 200}}
]}}]}}"#
    )
}

fn many(boxes: usize, per_box: usize) -> String {
    let tabs: Vec<String> = (0..boxes)
        .map(|b| {
            let items: Vec<String> = (0..per_box)
                .map(|i| format!(r#"{{"id": "i{b}_{i}", "name": "Item {b} {i}", "path": "~/x{b}_{i}"}}"#))
                .collect();
            format!(
                r#"{{"id": "t{b}", "title": "Box {b}", "x": {}, "y": {}, "items": [{}]}}"#,
                96 + (b % 5) * 300,
                140 + (b / 5) * 140,
                items.join(",")
            )
        })
        .collect();
    format!(
        r#"{{"layout": 3, "active": "main", "boards": [{{"id": "main", "name": "Main", "tabs": [{}]}}]}}"#,
        tabs.join(",")
    )
}

fn harness(dir: &tempfile::TempDir, look: Look, json: &str) -> Harness<'static, State> {
    build(dir, look, json, false)
}

fn shot_harness(dir: &tempfile::TempDir, look: Look, json: &str) -> Harness<'static, State> {
    build(dir, look, json, true)
}

fn build(dir: &tempfile::TempDir, look: Look, json: &str, render: bool) -> Harness<'static, State> {
    let path = dir.path().join("dashboard.json");
    std::fs::write(&path, json).unwrap();
    let mut overlay = MainOverlay::default();
    overlay.open(
        "dashboard",
        vec![Screen::new("screen", Rect::from_min_size(pos2(0.0, 0.0), vec2(SIZE[0], SIZE[1])))],
        0,
    );
    let state = State {
        overlay,
        config: Config::default(),
        demo: Demo::new(dir.path(), look),
        store: DashboardStore::load(&path, &[], &Map::new()),
        ui: DashboardUi::default(),
        outs: Vec::new(),
        running: vec!["code".into()],
        snap: 8,
    };
    let builder = Harness::builder().with_size(SIZE).with_pixels_per_point(1.0);
    let builder = if render && cfg!(target_os = "linux") { builder.wgpu() } else { builder };
    let harness = builder.build_ui_state(
        |ui, state: &mut State| {
            let ctx = ui.ctx().clone();
            state.demo.ready(&ctx);
            let State { overlay, config, demo, store, ui: dash, outs, running, snap } = state;
            let is_running = |id: &str| running.iter().any(|r| r == id);
            let owner = |_: &str| None::<String>;
            let env = Env {
                snap: *snap,
                tidy_style: "columns",
                tidy_order: "position",
                show_icons: false,
                safe_mode: false,
                is_running: &is_running,
                key_owner: &owner,
                tools: &[],
            };
            let mut page = |ui: &mut egui::Ui, slot: &PageSlot<'_>| {
                let mut sub = |ui: &mut egui::Ui, key: &str, rect: Rect| {
                    if key == "themes" {
                        demo.page(ui, rect);
                    } else {
                        pages::placeholder(ui, key);
                    }
                    PageFrame::default()
                };
                dash.show(ui, slot, store, &env, &mut sub)
            };
            let data = ChromeData {
                time: "14:30".into(),
                date: "MONDAY 23 NOVEMBER".into(),
                hour: 14,
                ..Default::default()
            };
            overlay.show(&ctx, config, &data, &mut page);
            outs.push(dash.take_out());
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
        let options = SnapshotOptions::new().threshold(1.5).max_failed_pixels(400);
        harness.snapshot_options(name, &options);
    }
}

fn press(h: &mut Harness<'_, State>, at: Pos2, mods: Modifiers) {
    h.hover_at(at);
    h.event(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: mods,
    });
}

fn release(h: &mut Harness<'_, State>, at: Pos2, mods: Modifiers) {
    h.event(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: mods,
    });
}

fn click(h: &mut Harness<'_, State>, at: Pos2) {
    press(h, at, Modifiers::NONE);
    release(h, at, Modifiers::NONE);
    h.run();
}

fn right_click(h: &mut Harness<'_, State>, at: Pos2) {
    h.hover_at(at);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    h.run();
}

/// Drags in steps, running a frame between them like a real mouse.
fn drag(h: &mut Harness<'_, State>, from: Pos2, to: Pos2) {
    press(h, from, Modifiers::NONE);
    h.step();
    for step in 1..=6 {
        let at = from + (to - from) * (step as f32 / 6.0);
        h.hover_at(at);
        // a drag asks for a frame after every move, so it never settles: `run` would fail
        h.step();
    }
    release(h, to, Modifiers::NONE);
    h.run();
    h.run();
}

fn tab_xy(h: &Harness<'_, State>, id: &str) -> (i64, i64) {
    let t = h.state().store.tab(id).unwrap();
    (t.x(), t.y())
}

#[test]
fn dashboard_dark() {
    let dir = tempfile::tempdir().unwrap();
    photograph(&mut shot_harness(&dir, Look::Dark, &layout()), "dashboard_dark");
}

#[test]
fn dashboard_light() {
    let dir = tempfile::tempdir().unwrap();
    photograph(&mut shot_harness(&dir, Look::Light, &layout()), "dashboard_light");
}

#[test]
fn an_empty_dashboard_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = shot_harness(
        &dir,
        Look::Dark,
        r#"{"layout": 3, "boards": [{"id": "main", "name": "Main", "tabs": []}]}"#,
    );
    photograph(&mut h, "dashboard_empty");
}

#[test]
fn it_is_quiet_once_it_is_up() {
    // `run` fails when something asks for a frame every frame
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(&dir, Look::Dark, &layout());
    h.run();
}

#[test]
fn a_header_drags_its_box_onto_the_snap_grid() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(&dir, Look::Dark, &layout());
    h.run();
    h.run();
    // the Games header: x 392..656, y 232..266
    drag(&mut h, pos2(500.0, 249.0), pos2(500.0 + 53.0, 249.0 + 29.0));
    assert_eq!(tab_xy(&h, "games"), (448, 264), "53 and 29 px snapped to the 8 px grid");
}

#[test]
fn snap_off_moves_pixel_by_pixel_and_a_lock_stops_the_drag() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(&dir, Look::Dark, &layout());
    h.state_mut().snap = 0;
    h.run();
    drag(&mut h, pos2(500.0, 249.0), pos2(537.0, 261.0));
    assert_eq!(tab_xy(&h, "games"), (429, 244));
    h.state_mut().store.set_locked(true);
    h.run();
    drag(&mut h, pos2(540.0, 261.0), pos2(700.0, 400.0));
    assert_eq!(tab_xy(&h, "games"), (429, 244), "locked");
}

#[test]
fn a_click_on_a_header_folds_the_box() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(&dir, Look::Dark, &layout());
    h.run();
    click(&mut h, pos2(500.0, 249.0));
    h.run();
    assert!(h.state().store.tab("games").unwrap().collapsed());
    click(&mut h, pos2(500.0, 249.0));
    h.run();
    assert!(!h.state().store.tab("games").unwrap().collapsed());
}

#[test]
fn double_clicking_a_row_runs_it_and_hides_the_overlay_for_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(&dir, Look::Dark, &layout());
    h.run();
    // both clicks in one frame, so they count as a double click
    h.get_by_label("Chess").click();
    h.get_by_label("Chess").click();
    h.run();
    let runs: Vec<_> = h.state().outs.iter().flat_map(|o| o.run.clone()).collect();
    assert_eq!(runs, [("g1".to_owned(), None)]);
    assert!(h.state().outs.iter().any(|o| o.hide_overlay));
}

#[test]
fn a_selection_band_picks_the_boxes_it_touches_and_delete_asks_first() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(&dir, Look::Dark, &layout());
    h.run();
    h.run();
    drag(&mut h, pos2(60.0, 200.0), pos2(700.0, 280.0));
    h.key_press_modifiers(Modifiers::NONE, Key::Delete);
    h.run();
    h.run();
    h.get_by_label_contains("Delete 3 box(es)");
}

#[test]
fn tidy_up_lines_the_boxes_up_and_undo_puts_them_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(&dir, Look::Dark, &layout());
    h.run();
    let before = tab_xy(&h, "games");
    h.get_by_label("Tidy up").click();
    h.run();
    h.run();
    assert_ne!(tab_xy(&h, "games"), before);
    h.get_by_label_contains("Undo").click();
    h.run();
    h.run();
    assert_eq!(tab_xy(&h, "games"), before);
}

#[test]
fn the_search_box_runs_the_first_match() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(&dir, Look::Dark, &layout());
    h.run();
    h.get_by_label("Find an item").focus();
    h.run();
    h.get_by_label("Find an item").type_text("chess");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    h.run();
    let runs: Vec<_> = h.state().outs.iter().flat_map(|o| o.run.clone()).collect();
    assert_eq!(runs, [("g1".to_owned(), None)], "Enter runs the first match");
}

#[test]
fn dragging_a_row_onto_another_box_moves_the_item() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(&dir, Look::Dark, &layout());
    h.run();
    h.run();
    // the Chess row of Games, dropped on the empty box
    drag(&mut h, pos2(500.0, 232.0 + 34.0 + 4.0 + 16.0), pos2(780.0, 232.0 + 60.0));
    let empty: Vec<String> =
        h.state().store.tab("empty").unwrap().items().map(|i| i.name().to_owned()).collect();
    assert_eq!(empty, ["Chess"]);
}

#[test]
fn the_row_menu_opens_the_item_dialog_and_saving_renames() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(&dir, Look::Dark, &layout());
    h.run();
    right_click(&mut h, pos2(500.0, 232.0 + 34.0 + 4.0 + 16.0));
    h.run();
    h.get_by_label("Edit…").click();
    h.run();
    h.run();
    let name = h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Name");
    name.focus();
    h.run();
    h.event(egui::Event::Text(" 2".into()));
    h.run();
    h.get_by_label("Save").click();
    h.run();
    h.run();
    assert_eq!(h.state().store.find_item("g1").unwrap().1.name(), "Chess 2");
}

#[test]
fn dragging_thirty_boxes_with_three_hundred_items_stays_smooth() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(&dir, Look::Dark, &many(30, 10));
    h.run();
    h.run();
    let start = Instant::now();
    let frames = 60;
    press(&mut h, pos2(150.0, 150.0), Modifiers::NONE);
    h.run();
    for step in 0..frames {
        h.hover_at(pos2(150.0 + step as f32 * 3.0, 150.0 + step as f32 * 2.0));
        h.step();
    }
    release(&mut h, pos2(330.0, 270.0), Modifiers::NONE);
    h.run();
    let per_frame = start.elapsed().as_secs_f64() * 1000.0 / f64::from(frames);
    println!("dragging 30 boxes with 300 items: {per_frame:.2} ms a frame");
    let limit = if cfg!(debug_assertions) { 120.0 } else { 16.0 };
    assert!(per_frame < limit, "{per_frame:.2} ms a frame");
}
