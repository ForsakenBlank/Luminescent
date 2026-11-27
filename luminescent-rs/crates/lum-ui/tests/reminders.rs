//! Reminders: pictures of the pop-up in Dark and Light, and what happens when one is due: the event, the pop-up,
//! Snooze and Done, a "once" reminder switching itself off, quick reminders from the palette and to-do due dates.

#![allow(clippy::unwrap_used)]

use std::path::Path;

use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, SnapshotOptions};
use jiff::{ToSpan, ZonedDateTime};
use lum_calc::reminder::{Edit, signature};
use lum_core::Paths;
use lum_core::events::EventBus;
use lum_core::module::{LogNotifier, Module, ModuleCx};
use lum_core::notes::TodoSync;
use lum_core::settings::Settings;
use lum_ui::tools::reminders::RemindersModule;
use lum_ui::{Request, Services, Tokens, UiCx, UiModule};
use serde_json::{Value, json};

struct State {
    module: RemindersModule,
    settings: Settings,
    services: Services,
    light: bool,
    ready: bool,
}

fn stamp(at: jiff::civil::DateTime) -> String {
    at.strftime("%Y-%m-%dT%H:%M:%S").to_string()
}

fn once_due(minutes_ago: i64) -> Value {
    let due = ZonedDateTime::now().datetime().checked_sub(minutes_ago.minutes()).unwrap();
    json!({
        "id": "r1", "title": "Stretch", "message": "Stand up and walk about for a bit.", "enabled": true,
        "schedule": {"type": "once", "date": due.date().to_string(), "time": due.strftime("%H:%M:%S").to_string()},
        "actions": {"popup": true, "position": "default", "sound": "none", "sound_file": "", "sound_repeat": 1, "toast": true}
    })
}

fn weekly_rem() -> Value {
    json!({
        "id": "r2", "title": "Put the bins out", "message": "", "enabled": true,
        "schedule": {"type": "weekly", "time": "19:30", "days": [1, 3]},
        "actions": {"popup": true, "position": "default", "sound": "none", "sound_file": "", "sound_repeat": 1, "toast": false}
    })
}

/// A state that last saw the reminder ten minutes ago, so what was due in between is "missed" and fires.
fn make(dir: &Path, light: bool, items: Vec<Value>) -> State {
    let paths = Paths::custom(dir);
    paths.create_dirs().unwrap();
    let before = ZonedDateTime::now().datetime().checked_sub(10.minutes()).unwrap();
    let mut state = serde_json::Map::new();
    for rem in &items {
        let id = rem["id"].as_str().unwrap();
        state.insert(id.into(), json!({"last": stamp(before), "sched": signature(&rem["schedule"])}));
    }
    std::fs::write(paths.data_file("reminders_state.json"), Value::Object(state).to_string()).unwrap();
    let mut settings = Settings::load(&paths);
    settings.set("reminders.items", Value::Array(items));
    // the top left corner of the test window, where the pop-up is easy to see
    settings.set("reminders.popup_position", json!("tl"));
    let services = Services::detached(paths.data.clone());
    let mut module = RemindersModule::new(&services);
    let events = EventBus::new();
    let mut cx =
        ModuleCx { settings: &mut settings, events: &events, notifier: &LogNotifier, safe_mode: false };
    module.start(&mut cx).unwrap();
    State { module, settings, services, light, ready: false }
}

fn harness(state: State, render: bool) -> Harness<'static, State> {
    let builder = Harness::builder().with_size([440.0, 280.0]).with_pixels_per_point(1.0);
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

fn photograph(harness: &mut Harness<'_, State>, name: &str) {
    harness.run();
    if cfg!(target_os = "linux") {
        // the time on the pop-up is the real one, so a few pixels of the digits may differ
        let options = SnapshotOptions::new().threshold(1.5).max_failed_pixels(700);
        harness.snapshot_options(name, &options);
    }
}

fn state_file(dir: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join("data/reminders_state.json")).unwrap()).unwrap()
}

#[test]
fn popup_dark() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make(dir.path(), false, vec![weekly_rem()]);
    state.module.preview_reminder(&weekly_rem());
    photograph(&mut harness(state, true), "reminders_popup_dark");
}

#[test]
fn popup_light() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make(dir.path(), true, vec![once_due(0)]);
    state.module.preview_reminder(&once_due(0));
    photograph(&mut harness(state, true), "reminders_popup_light");
}

#[test]
fn a_reminder_that_was_due_a_few_minutes_ago_goes_off() {
    let dir = tempfile::tempdir().unwrap();
    let state = make(dir.path(), false, vec![once_due(3)]);
    let mut h = harness(state, false);
    h.step();
    // the first look is a moment after the start
    std::thread::sleep(std::time::Duration::from_millis(1600));
    h.run();
    assert!(h.query_by_label("Done").is_some(), "the pop-up is up");
    assert!(h.query_by_label("Snooze 10 min").is_some());
    let asked = h.state().services.take();
    assert!(
        asked.iter().any(
            |r| matches!(r, Request::Emit { name, text } if name == "reminder.fired" && text == "Stretch")
        ),
        "{asked:?}"
    );
    assert!(
        asked.iter().any(|r| matches!(r, Request::ReminderEdit(Edit::Disable(id)) if id == "r1")),
        "once means once"
    );
    assert!(
        asked.iter().any(|r| matches!(r, Request::SystemNotice { .. })),
        "the desktop notification, no card"
    );
    h.get_by_label("Done").click();
    h.run();
    assert!(h.query_by_label("Done").is_none(), "Done closes it");
    finish(h);
}

#[test]
fn snooze_remembers_when_to_come_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make(dir.path(), false, vec![weekly_rem()]);
    state.module.preview_reminder(&weekly_rem());
    let mut h = harness(state, false);
    h.run();
    h.get_by_label("Snooze 10 min").click();
    h.run();
    assert!(h.query_by_label("Done").is_none());
    finish(h);
    let saved = state_file(dir.path());
    let until = saved["r2"]["snoozed_until"].as_str().unwrap();
    let then: jiff::civil::DateTime = until.parse().unwrap();
    let wait = (then - ZonedDateTime::now().datetime()).total(jiff::Unit::Minute).unwrap();
    assert!((9.0..=10.1).contains(&wait), "snoozed for {wait} minutes");
}

#[test]
fn a_reminder_far_in_the_future_stays_quiet_and_shows_in_the_status_strip() {
    let dir = tempfile::tempdir().unwrap();
    let mut rem = once_due(0);
    rem["schedule"]["date"] = json!("2999-01-01");
    let state = make(dir.path(), false, vec![rem]);
    let mut h = harness(state, false);
    std::thread::sleep(std::time::Duration::from_millis(1600));
    h.run();
    assert!(h.query_by_label("Done").is_none());
    let chips = h.state().module.chips();
    assert_eq!(chips.len(), 1);
    assert!(chips[0].1.ends_with("09:00  Stretch") || chips[0].1.contains("Stretch"), "{chips:?}");
    finish(h);
}

#[test]
fn the_palette_turns_remind_me_into_a_quick_reminder() {
    let dir = tempfile::tempdir().unwrap();
    let state = make(dir.path(), false, Vec::new());
    let mut h = harness(state, false);
    h.run();
    let rows = h.state().module.palette_live("remind me in 20m to stretch");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].label, "Remind me: Stretch");
    assert!(rows[0].hint.starts_with("REMINDER \u{b7} "), "{}", rows[0].hint);
    assert!(h.state().module.palette_live("hello there").is_empty());
    let id = rows[0].id.clone();
    let services = h.state().services.clone();
    assert!(h.state_mut().module.run_palette(&id, &services));
    let asked = services.take();
    let added = asked.iter().find_map(|r| match r {
        Request::ReminderEdit(Edit::Add(rem)) => Some(rem.clone()),
        _ => None,
    });
    let rem = added.expect("the reminder is added");
    assert_eq!(rem["title"], "Stretch");
    assert_eq!(rem["quick"], true);
    assert_eq!(rem["schedule"]["type"], "once");
    assert!(asked.iter().any(|r| matches!(r, Request::Notify { title, .. } if title == "Reminders")));
    finish(h);
}

#[test]
fn a_to_do_due_date_sets_and_drops_a_reminder() {
    let dir = tempfile::tempdir().unwrap();
    let state = make(dir.path(), false, Vec::new());
    let mut h = harness(state, false);
    h.run();
    let when: jiff::civil::DateTime = "2099-11-24T09:00:00".parse().unwrap();
    let set = TodoSync::Set {
        id: "todo-i1".into(),
        title: "To-do: bread".into(),
        when,
        message: "From the note".into(),
    };
    assert!(h.state_mut().module.todo_reminder(&set));
    let asked = h.state().services.take();
    let Some(Request::ReminderEdit(Edit::Add(rem))) = asked.first() else { panic!("{asked:?}") };
    assert_eq!(rem["id"], "todo-i1");
    assert_eq!(rem["message"], "From the note");
    assert!(h.state_mut().module.todo_reminder(&TodoSync::Drop { id: "todo-i1".into() }));
    assert!(
        matches!(h.state().services.take().first(), Some(Request::ReminderEdit(Edit::Remove(id))) if id == "todo-i1")
    );
    finish(h);
}

#[test]
fn a_reminder_with_a_theme_asks_for_the_switch() {
    let dir = tempfile::tempdir().unwrap();
    let mut rem = weekly_rem();
    rem["actions"]["theme"] = json!("light");
    rem["actions"]["popup"] = json!(false);
    let mut state = make(dir.path(), false, vec![rem.clone()]);
    state.module.preview_reminder(&rem);
    let mut h = harness(state, false);
    h.run();
    assert!(h.state().services.take().iter().any(|r| matches!(r, Request::ApplyTheme(id) if id == "light")));
    finish(h);
}
