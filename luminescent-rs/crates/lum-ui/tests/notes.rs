//! Notes: pictures of the panel, a sticky note and the page in Dark and Light, and what clicking, typing, the
//! to-do rows, search and delete with Undo do to `notes.json`. Pictures are compared on Linux only.

#![allow(clippy::unwrap_used)]

use std::path::Path;

use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, SnapshotOptions};
use lum_core::Paths;
use lum_core::events::EventBus;
use lum_core::module::{LogNotifier, Module, ModuleCx};
use lum_core::settings::Settings;
use lum_ui::tools::notes::NotesModule;
use lum_ui::{Request, Services, Tokens, UiCx, UiModule};
use serde_json::{Value, json};

const SAVED: &str = r##"{
  "notes": [
    {"id": "n1", "name": "My First Note", "type": "note",
     "content": "# Ideas\n\nTry the **Rust** build, *really*.\n\n- one\n- two\n\n[docs](https://example.com)",
     "items": [], "pinned": true, "colour": "#7B6CF6", "tags": ["work"]},
    {"id": "t1", "name": "Shopping", "type": "todo", "content": "",
     "items": [
       {"id": "i1", "text": "milk", "checked": true},
       {"id": "i2", "text": "bread", "checked": false, "due": "2099-11-24T09:00"},
       {"id": "i3", "text": "a rather long task that has to wrap onto a second line in a narrow panel", "checked": false}
     ], "pinned": false, "colour": "", "tags": []},
    {"id": "n2", "name": "Holiday", "type": "note", "content": "Passports!", "items": [],
     "pinned": false, "colour": "#30A46C", "tags": ["home", "travel"]}
  ],
  "x": 20, "y": 20, "w": 360, "h": 480, "open": null, "stickies": {}, "preview": false
}"##;

struct State {
    module: NotesModule,
    settings: Settings,
    services: Services,
    light: bool,
    ready: bool,
    page: bool,
}

fn make(dir: &Path, light: bool, page: bool, saved: &str) -> State {
    make_with(dir, light, page, saved, !page)
}

fn make_with(dir: &Path, light: bool, page: bool, saved: &str, panel: bool) -> State {
    let paths = Paths::custom(dir);
    paths.create_dirs().unwrap();
    std::fs::write(paths.data_file("notes.json"), saved).unwrap();
    let mut settings = Settings::load(&paths);
    // the page and the panel would both show a list: pictures of the page show the page alone
    let panels = if panel { json!(["notes"]) } else { json!([]) };
    settings.set("general.start_panels", panels);
    let services = Services::detached(paths.data.clone());
    let mut module = NotesModule::new(&services);
    let events = EventBus::new();
    let mut cx =
        ModuleCx { settings: &mut settings, events: &events, notifier: &LogNotifier, safe_mode: false };
    module.start(&mut cx).unwrap();
    State { module, settings, services, light, ready: false, page }
}

fn harness(state: State, size: [f32; 2], render: bool) -> Harness<'static, State> {
    let builder = Harness::builder().with_size(size).with_pixels_per_point(1.0);
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
                // the overlay's panel is behind a page; without it a light page would float on nothing
                ui.painter().rect_filled(rect, 0.0, Tokens::of(&ctx).p.bg_base);
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
        let options = SnapshotOptions::new().threshold(1.5).max_failed_pixels(400);
        harness.snapshot_options(name, &options);
    }
}

fn saved(dir: &Path) -> Value {
    let text = std::fs::read_to_string(dir.join("data/notes.json")).unwrap();
    serde_json::from_str(&text).unwrap()
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

fn with_open(note: &str) -> String {
    SAVED.replace("\"open\": null", &format!("\"open\": {note}"))
}

const TALL: [f32; 2] = [420.0, 560.0];

#[test]
fn panel_list_dark() {
    let dir = tempfile::tempdir().unwrap();
    photograph(&mut harness(make(dir.path(), false, false, SAVED), TALL, true), "notes_list_dark");
}

#[test]
fn panel_list_light() {
    let dir = tempfile::tempdir().unwrap();
    photograph(&mut harness(make(dir.path(), true, false, SAVED), TALL, true), "notes_list_light");
}

#[test]
fn panel_todo_dark() {
    let dir = tempfile::tempdir().unwrap();
    let state = make(dir.path(), false, false, &with_open("1"));
    photograph(&mut harness(state, TALL, true), "notes_todo_dark");
}

#[test]
fn panel_todo_light() {
    let dir = tempfile::tempdir().unwrap();
    let state = make(dir.path(), true, false, &with_open("1"));
    photograph(&mut harness(state, TALL, true), "notes_todo_light");
}

#[test]
fn panel_text_dark() {
    let dir = tempfile::tempdir().unwrap();
    let state = make(dir.path(), false, false, &with_open("0"));
    photograph(&mut harness(state, TALL, true), "notes_text_dark");
}

#[test]
fn panel_preview_dark() {
    let dir = tempfile::tempdir().unwrap();
    let saved = with_open("0").replace("\"preview\": false", "\"preview\": true");
    photograph(&mut harness(make(dir.path(), false, false, &saved), TALL, true), "notes_preview_dark");
}

#[test]
fn panel_preview_light() {
    let dir = tempfile::tempdir().unwrap();
    let saved = with_open("0").replace("\"preview\": false", "\"preview\": true");
    photograph(&mut harness(make(dir.path(), true, false, &saved), TALL, true), "notes_preview_light");
}

#[test]
fn a_sticky_note_dark() {
    let dir = tempfile::tempdir().unwrap();
    let saved = SAVED
        .replace("\"stickies\": {}", "\"stickies\": {\"t1\": [20, 20, 300, 300]}")
        .replace("\"x\": 20, \"y\": 20", "\"x\": 700, \"y\": 700");
    let state = make_with(dir.path(), false, false, &saved, false);
    photograph(&mut harness(state, [340.0, 340.0], true), "notes_sticky_dark");
}

#[test]
fn panel_in_edit_mode_dark() {
    let dir = tempfile::tempdir().unwrap();
    // lower on the screen than usual, so the anchor bar at the top does not hide the header
    let saved = with_open("0").replace("\"x\": 20, \"y\": 20", "\"x\": 20, \"y\": 90");
    let state = make(dir.path(), false, false, &saved);
    let mut h = harness(state, TALL, true);
    h.run();
    h.state_mut().module.bind_pressed("edit");
    photograph(&mut h, "notes_edit_dark");
}

#[test]
fn page_dark() {
    let dir = tempfile::tempdir().unwrap();
    photograph(&mut harness(make(dir.path(), false, true, SAVED), [920.0, 560.0], true), "notes_page_dark");
}

#[test]
fn page_light() {
    let dir = tempfile::tempdir().unwrap();
    photograph(&mut harness(make(dir.path(), true, true, SAVED), [920.0, 560.0], true), "notes_page_light");
}

#[test]
fn clicking_a_card_opens_the_note_and_back_returns_to_the_list() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), false, false, SAVED), TALL, false);
    h.run();
    h.get_by_label("Note Holiday").click();
    h.run();
    assert!(h.query_by_label("Back to all notes").is_some(), "inside a note the header has a back arrow");
    assert!(h.query_by_label("Note Holiday").is_none(), "the list is gone");
    h.get_by_label("Back to all notes").click();
    h.run();
    assert!(h.query_by_label("Note Holiday").is_some());
    finish(h);
    assert_eq!(saved(dir.path())["open"], Value::Null);
}

#[test]
fn the_panel_remembers_the_open_note_as_an_index() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), false, false, SAVED), TALL, false);
    h.run();
    h.get_by_label("Note Shopping").click();
    h.run();
    finish(h);
    assert_eq!(saved(dir.path())["open"], json!(1));
}

#[test]
fn search_filters_by_text_and_by_tag() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), false, false, SAVED), TALL, false);
    h.run();
    let search = h.get_by_role_and_label(egui::Role::TextInput, "Search notes (#tag for a tag)");
    search.focus();
    h.run();
    h.get_by_role_and_label(egui::Role::TextInput, "Search notes (#tag for a tag)").type_text("#tra");
    h.run();
    assert!(h.query_by_label("Note Holiday").is_some());
    assert!(h.query_by_label("Note Shopping").is_none());
    assert!(h.query_by_label("Note My First Note").is_none());
    finish(h);
}

#[test]
fn typing_in_a_note_is_saved() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), false, false, &with_open("2")), TALL, false);
    h.run();
    let editor = h.get_by_role(egui::Role::MultilineTextInput);
    editor.focus();
    h.run();
    h.get_by_role(egui::Role::MultilineTextInput).type_text(" Tickets.");
    h.run();
    finish(h);
    let notes = saved(dir.path());
    let content = notes["notes"][2]["content"].as_str().unwrap();
    assert!(content.contains("Tickets."), "got {content:?}");
    assert_eq!(notes["notes"][2]["colour"], "#30A46C", "the rest of the note is untouched");
}

#[test]
fn ticking_a_task_and_clearing_the_done_ones_changes_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), false, false, &with_open("1")), [420.0, 640.0], false);
    h.run();
    h.get_by_role_and_label(egui::Role::CheckBox, "bread").click();
    h.run();
    h.get_by_label("Clear done").click();
    h.run();
    finish(h);
    let items = saved(dir.path())["notes"][1]["items"].clone();
    let texts: Vec<&str> = items.as_array().unwrap().iter().map(|i| i["text"].as_str().unwrap()).collect();
    assert_eq!(texts.len(), 1, "milk and bread were done");
    assert!(texts[0].starts_with("a rather long task"));
}

#[test]
fn a_new_task_goes_in_with_enter() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), false, false, &with_open("1")), [420.0, 640.0], false);
    h.run();
    let add = h.get_by_role_and_label(egui::Role::TextInput, "Add a task…");
    add.focus();
    h.run();
    h.get_by_role_and_label(egui::Role::TextInput, "Add a task…").type_text("eggs");
    h.key_press(egui::Key::Enter);
    h.run();
    finish(h);
    let items = saved(dir.path())["notes"][1]["items"].clone();
    let last = items.as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["text"], "eggs");
    assert_eq!(last["checked"], false);
    assert!(last.get("due").is_none(), "a new task has no due key, as in Python");
}

#[test]
fn deleting_a_note_in_edit_mode_offers_undo_for_a_while() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), false, false, &with_open("2")), TALL, false);
    h.run();
    h.state_mut().module.bind_pressed("edit");
    h.run();
    h.get_by_label("Delete this note").click();
    h.run();
    assert!(h.query_by_label("Note Holiday").is_none());
    assert!(h.query_by_label("Undo").is_some(), "the list says what was deleted and offers Undo");
    h.get_by_label("Undo").click();
    h.run();
    assert!(h.query_by_label("Note Holiday").is_some(), "back in the list");
    finish(h);
    assert_eq!(saved(dir.path())["notes"].as_array().unwrap().len(), 3);
}

#[test]
fn deleting_a_to_do_note_with_a_due_date_drops_its_reminder() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), false, false, &with_open("1")), TALL, false);
    h.run();
    h.state_mut().module.bind_pressed("edit");
    h.run();
    h.get_by_label("Delete this note").click();
    h.run();
    let asked = h.state().services.take();
    assert!(
        asked.iter().any(
            |r| matches!(r, Request::TodoReminder(lum_core::notes::TodoSync::Drop { id }) if id == "todo-i2")
        ),
        "the reminder of the dated task is dropped: {asked:?}"
    );
    finish(h);
}

#[test]
fn the_palette_lists_every_note_with_its_tags() {
    let dir = tempfile::tempdir().unwrap();
    let h = harness(make(dir.path(), false, false, SAVED), TALL, false);
    let rows = h.state().module.palette_entries();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].label, "📌 My First Note");
    assert!(rows[2].hint.starts_with("#home #travel"));
    finish(h);
}

#[test]
fn the_page_opens_the_pinned_note_and_shows_its_text() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), false, true, SAVED), [920.0, 560.0], false);
    h.run();
    assert!(
        h.query_by_role(egui::Role::MultilineTextInput).is_some(),
        "the first pinned note is a text note"
    );
    h.get_by_label("Note Shopping").click();
    h.run();
    assert!(h.query_by_role_and_label(egui::Role::TextInput, "Add a task…").is_some());
    finish(h);
}

#[test]
fn an_unreadable_file_is_kept_and_notes_starts_with_the_first_note() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), false, false, "{broken"), TALL, false);
    h.run();
    assert!(h.query_by_label("Note My First Note").is_some());
    assert!(dir.path().join("data/notes.json.bad").exists());
    finish(h);
}
