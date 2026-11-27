//! Today, Usage and Calendar: pictures of each page in Dark and Light and as a small box, what the pages read from
//! the activity log, the usage counts and the stored Atlas calendar, and what their buttons do.
//! Pictures are compared on Linux only.

#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::sync::Arc;

use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, SnapshotOptions};
use jiff::civil::DateTime;
use lum_core::Paths;
use lum_core::clock::FixedClock;
use lum_core::usage::Usage;
use lum_ui::Tokens;
use lum_ui::tools::overview::{Env, Overview};
use serde_json::{Value, json};

const NOW: &str = "2026-11-23T15:00:00";

struct State {
    overview: Overview,
    usage: Usage,
    paths: Paths,
    reminders: Option<Vec<Value>>,
    atlas_on: bool,
    key: &'static str,
    light: bool,
    ready: bool,
}

fn at(text: &str) -> DateTime {
    text.parse().unwrap()
}

/// An Atlas calendar that sent two events (one repeating on weekdays, one all-day over two days) and two tasks.
fn atlas_file(path: &Path) {
    let ms = |text: &str| -> i64 { format!("{text}Z").parse::<jiff::Timestamp>().unwrap().as_millisecond() };
    let stored = json!({
        "peers": {"phone": {"name": "Pixel", "kind": "Phone", "last": ms("2026-11-23T08:30:00")}},
        "shadow": {
            "event/e1": {"kind": "event", "uid": "e1", "fields": {
                "title": "Stand-up", "startsAt": ms("2026-11-23T09:30:00"), "endsAt": ms("2026-11-23T09:45:00"),
                "repeatRule": "WEEKDAYS", "color": -1750963}},
            "event/e2": {"kind": "event", "uid": "e2", "fields": {
                "title": "Holiday in Wales", "allDay": true, "startsAt": ms("2026-11-25T00:00:00"),
                "endsAt": ms("2026-11-27T00:00:00"), "color": -13_332_010_i64}},
            "task/t1": {"kind": "task", "uid": "t1", "fields": {"title": "Pay rent", "due": ms("2026-11-23T12:00:00"), "done": false}},
            "task/t2": {"kind": "task", "uid": "t2", "fields": {"title": "Book dentist", "due": ms("2026-11-23T10:00:00"), "done": true}}
        }
    });
    std::fs::write(path, stored.to_string()).unwrap();
}

fn make(dir: &Path, key: &'static str, light: bool) -> State {
    let paths = Paths::custom(dir);
    paths.create_dirs().unwrap();
    for name in ["activity.jsonl", "usage.json"] {
        let from = lum_fixtures::path(&format!("sample_home/data/{name}"));
        std::fs::copy(from, paths.data_file(name)).unwrap();
    }
    atlas_file(&paths.data_file("atlas_sync.json"));
    let settings = lum_fixtures::read_json("sample_home/settings.json");
    let reminders = settings["reminders"]["items"].as_array().cloned();
    let usage = Usage::new(&paths, Arc::new(FixedClock::new(at(NOW))));
    State { overview: Overview::new(), usage, paths, reminders, atlas_on: true, key, light, ready: false }
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
            let rect = ui.max_rect();
            ui.painter().rect_filled(rect, 0.0, Tokens::of(ui.ctx()).p.bg_base);
            let mut env = Env {
                usage: &mut state.usage,
                today: at(NOW).date(),
                now: at(NOW),
                reminders: state.reminders.as_deref(),
                atlas_on: state.atlas_on,
                atlas_file: state.paths.data_file("atlas_sync.json"),
            };
            state.overview.show(state.key, ui, rect, &mut env);
        },
        state,
    )
}

fn photograph(h: &mut Harness<'_, State>, name: &str) {
    h.run();
    h.run();
    if cfg!(target_os = "linux") {
        let options = SnapshotOptions::new().threshold(1.5).max_failed_pixels(600);
        h.snapshot_options(name, &options);
    }
}

const PAGE: [f32; 2] = [980.0, 560.0];
const BOX: [f32; 2] = [420.0, 560.0];

fn picture(key: &'static str, light: bool, size: [f32; 2], name: &str) {
    let dir = tempfile::tempdir().unwrap();
    photograph(&mut harness(make(dir.path(), key, light), size, true), name);
}

#[test]
fn today_dark() {
    picture("today", false, PAGE, "today_dark");
}

#[test]
fn today_light() {
    picture("today", true, PAGE, "today_light");
}

#[test]
fn today_as_a_small_box() {
    picture("today", false, BOX, "today_box_dark");
}

#[test]
fn usage_dark() {
    picture("stats", false, PAGE, "usage_dark");
}

#[test]
fn usage_light() {
    picture("stats", true, PAGE, "usage_light");
}

#[test]
fn usage_as_a_small_box() {
    picture("stats", false, BOX, "usage_box_dark");
}

#[test]
fn calendar_dark() {
    picture("calendar", false, PAGE, "calendar_dark");
}

#[test]
fn calendar_light() {
    picture("calendar", true, PAGE, "calendar_light");
}

#[test]
fn calendar_as_a_small_box() {
    picture("calendar", false, BOX, "calendar_box_dark");
}

#[test]
fn calendar_without_atlas_sync() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make(dir.path(), "calendar", false);
    state.atlas_on = false;
    photograph(&mut harness(state, PAGE, true), "calendar_off_dark");
}

#[test]
fn today_lists_what_the_log_has_for_the_day() {
    let dir = tempfile::tempdir().unwrap();
    // the sample log is of the 23rd; the page opens on the day it is told is today
    let mut h = harness(make(dir.path(), "today", false), PAGE, false);
    h.run();
    assert!(h.query_by_label("09:01 RUN Firefox").is_some());
    assert!(h.query_by_label("10:30 TIMER Focus").is_some());
    assert!(
        h.query_by_label("11:00 REMINDER Drink some water").is_some(),
        "a kind it does not know is still shown"
    );
}

#[test]
fn the_arrows_step_to_an_earlier_day_and_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), "today", false), PAGE, false);
    h.run();
    h.get_by_label("The day before").click();
    h.run();
    h.run();
    assert!(h.query_by_label("09:01 RUN Firefox").is_none(), "the 22nd has nothing in the log");
    h.get_by_label("The day after").click();
    h.run();
    h.run();
    assert!(h.query_by_label("09:01 RUN Firefox").is_some());
}

#[test]
fn a_line_added_while_the_page_was_away_shows_when_it_comes_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), "today", false), PAGE, false);
    h.run();
    h.state_mut().usage.activity("tool", "Calculator", None, Some(at("2026-11-23T12:30:00")));
    h.state_mut().key = "stats";
    h.run();
    h.state_mut().key = "today";
    h.run();
    h.run();
    assert!(h.query_by_label("12:30 TOOL Calculator").is_some());
}

#[test]
fn usage_lists_items_hotkeys_and_tools_with_their_counts() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), "stats", false), PAGE, false);
    h.run();
    assert!(h.query_by_label("Firefox").is_some());
    assert!(h.query_by_label("Calculator").is_some());
    let keys = h.query_all_by_label_contains("Open Luminescent").count();
    assert_eq!(keys, 1, "the hotkey row says the keys and what they are for");
}

#[test]
fn reset_asks_first_and_then_forgets_every_count() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), "stats", false), PAGE, false);
    h.run();
    h.get_by_label("Reset…").click();
    h.run();
    assert!(
        h.query_by_label("Reset usage").is_some()
            || h.query_by_label("Forget every count (the Today log stays)?").is_some()
    );
    assert!(h.query_by_label("Firefox").is_some(), "nothing is forgotten before the answer");
    h.get_by_label("Reset").click();
    h.run();
    h.run();
    assert!(h.query_by_label("Firefox").is_none());
    assert_eq!(h.state_mut().usage.stats()["items"], json!({}));
}

#[test]
fn cancelling_the_question_keeps_the_counts() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), "stats", false), PAGE, false);
    h.run();
    h.get_by_label("Reset…").click();
    h.run();
    h.get_by_label("Cancel").click();
    h.run();
    h.run();
    assert!(h.query_by_label("Firefox").is_some());
}

#[test]
fn the_calendar_opens_on_today_with_atlas_events_tasks_and_reminders() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), "calendar", false), PAGE, false);
    h.run();
    assert!(h.query_by_label_contains("Stand-up").is_some());
    assert!(h.query_by_label_contains("Pay rent").is_some());
    assert!(h.query_by_label_contains("Book dentist").is_some());
    assert!(h.query_by_label_contains("Holiday in Wales").is_none(), "that starts on the 25th");
}

#[test]
fn picking_a_day_shows_that_days_agenda() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), "calendar", false), PAGE, false);
    h.run();
    h.get_by_label("Wednesday 25 November").click();
    h.run();
    h.run();
    assert!(h.query_by_label_contains("Holiday in Wales").is_some());
    assert!(h.query_by_label_contains("Pay rent").is_none());
    h.get_by_label("Today").click();
    h.run();
    h.run();
    assert!(h.query_by_label_contains("Pay rent").is_some());
}

#[test]
fn the_month_arrows_change_the_month_and_keep_the_chosen_day() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), "calendar", false), PAGE, false);
    h.run();
    h.get_by_label("Month after").click();
    h.run();
    h.run();
    assert!(h.query_by_label("Tuesday 1 December").is_some());
    assert!(h.query_by_label_contains("Pay rent").is_some(), "the chosen day is still the 23rd");
    h.get_by_label("Month before").click();
    h.run();
    h.run();
    assert!(h.query_by_label("Monday 23 November").is_some());
}

#[test]
fn without_atlas_sync_only_the_reminders_show() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make(dir.path(), "calendar", false);
    state.atlas_on = false;
    let mut h = harness(state, PAGE, false);
    h.run();
    assert!(h.query_by_label_contains("Stand-up").is_none());
    assert!(h.query_by_label_contains("Pay rent").is_none());
}

#[test]
fn without_reminders_running_their_lines_are_left_out() {
    let dir = tempfile::tempdir().unwrap();
    let mut with = make(dir.path(), "calendar", false);
    with.reminders = Some(vec![
        json!({"title": "Bins out", "enabled": true, "schedule": {"type": "once", "date": "2026-11-23", "time": "19:30"}}),
    ]);
    let mut h = harness(with, PAGE, false);
    h.run();
    assert!(h.query_by_label_contains("Bins out").is_some());
    h.state_mut().reminders = None;
    h.state_mut().key = "stats";
    h.run();
    h.state_mut().key = "calendar";
    h.run();
    h.run();
    assert!(h.query_by_label_contains("Bins out").is_none());
}

#[test]
fn sync_with_atlas_opens_its_settings() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(make(dir.path(), "calendar", false), PAGE, false);
    h.run();
    h.get_by_label("Sync with Atlas").click();
    h.run();
    let out = h.state_mut().overview.take_out();
    assert_eq!(out.open_settings.as_deref(), Some("atlas_sync"));
    assert_eq!(h.state_mut().overview.take_out().open_settings, None, "asked for once");
}
