#![allow(clippy::unwrap_used)]
//! Every reminder case the Python app recorded (phase 0): when each kind of schedule fires next, how it reads in
//! words, what the palette makes of "remind me in 20m to stretch", and what a new reminder looks like.

use jiff::civil::DateTime;
use lum_calc::reminder::{self, Fired, SNOOZE_CHOICES, State, describe, new_reminder, next_due, parse_quick};
use lum_fixtures::{read_json, read_text};
use serde_json::{Value, json};

fn at(text: &str) -> DateTime {
    text.parse().unwrap()
}

fn show(when: Option<DateTime>) -> Value {
    when.map_or(Value::Null, |d| json!(d.strftime("%Y-%m-%dT%H:%M:%S").to_string()))
}

#[test]
fn every_next_due_case_comes_out_the_same() {
    let cases = read_json("reminder_cases.json");
    let cases = cases["next_due"].as_array().unwrap();
    assert!(cases.len() >= 150);
    for case in cases {
        let rem = json!({"schedule": case["schedule"]});
        let got = show(next_due(&rem, at(case["after"].as_str().unwrap())));
        assert_eq!(got, case["due"], "{} after {}", case["schedule"], case["after"]);
    }
}

#[test]
fn every_schedule_reads_the_same_in_words() {
    let cases = read_json("reminder_cases.json");
    for case in cases["describe"].as_array().unwrap() {
        let rem = json!({"schedule": case["schedule"]});
        assert_eq!(describe(&rem), case["text"].as_str().unwrap(), "{}", case["schedule"]);
    }
}

#[test]
fn every_quick_reminder_is_read_the_same() {
    let cases = read_json("reminder_cases.json");
    for case in cases["parse_quick"].as_array().unwrap() {
        let text = case["text"].as_str().unwrap();
        let got = parse_quick(text, at(case["now"].as_str().unwrap()));
        let got = got.map_or(Value::Null, |(when, what)| json!([show(Some(when)), what]));
        assert_eq!(got, case["result"], "{text}");
    }
}

#[test]
fn a_new_reminder_has_the_fields_the_editor_expects() {
    let cases = read_json("reminder_cases.json");
    let want = &cases["new_reminder"];
    let today = at("2026-11-23T00:00:00").date();
    let mut got = new_reminder(today, "x");
    got["id"] = want["id"].clone();
    assert_eq!(got, *want);
    assert_eq!(cases["snooze_choices"].as_array().unwrap().len(), 5);
    assert_eq!(&SNOOZE_CHOICES[..3], [5, 10, 15]);
}

#[test]
fn the_sample_state_file_is_saved_back_unchanged() {
    let original = read_text("sample_home/data/reminders_state.json");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reminders_state.json");
    std::fs::write(&path, &original).unwrap();
    let state = State::load(&path);
    assert_eq!(state.to_text(), original.trim_end());
}

#[test]
fn the_signature_is_what_python_wrote_into_the_sample() {
    let sample: Value = serde_json::from_str(&read_text("sample_home/data/reminders_state.json")).unwrap();
    let settings: Value = serde_json::from_str(&read_text("sample_home/settings.json")).unwrap();
    let items = settings["reminders"]["items"].as_array().unwrap();
    let water = items.iter().find(|r| r["id"] == "r-water").unwrap();
    assert_eq!(reminder::signature(&water["schedule"]), sample["r-water"]["sched"].as_str().unwrap());
}

fn daily(id: &str, time: &str) -> Value {
    json!({"id": id, "title": id, "enabled": true, "schedule": {"type": "daily", "time": time}})
}

fn state_in(dir: &tempfile::TempDir) -> State {
    State::load(&dir.path().join("reminders_state.json"))
}

#[test]
fn a_reminder_seen_for_the_first_time_starts_its_clock_now() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(&dir);
    let items = [daily("a", "08:00")];
    let now = at("2026-11-23T10:00:00");
    state.prime(&items, now);
    assert_eq!(state.last("a"), Some(now));
    let (fired, _) = state.tick(&items, now, 120, 10);
    assert!(fired.is_empty(), "08:00 had gone before it was made, so it does not fire as a catch-up");
}

#[test]
fn an_edited_schedule_does_not_fire_as_a_missed_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(&dir);
    let before = [daily("a", "21:30")];
    state.prime(&before, at("2026-11-23T09:00:00"));
    let edited = [daily("a", "08:00")];
    let now = at("2026-11-23T10:00:00");
    state.prime(&edited, now);
    assert_eq!(state.last("a"), Some(now));
    assert!(state.tick(&edited, now, 120, 10).0.is_empty());
}

#[test]
fn a_reminder_missed_a_little_while_ago_still_fires_once_and_older_ones_do_not() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(&dir);
    let items = [daily("a", "09:00")];
    state.prime(&items, at("2026-11-22T20:00:00"));
    let (fired, changed) = state.tick(&items, at("2026-11-23T09:30:00"), 120, 10);
    assert_eq!(fired, vec![Fired { id: "a".into(), once: false }]);
    assert!(changed);
    assert_eq!(state.last("a"), Some(at("2026-11-23T09:00:00")));
    assert!(state.tick(&items, at("2026-11-23T09:31:00"), 120, 10).0.is_empty(), "not twice");

    let mut late = state_in(&dir);
    late.prime(&items, at("2026-11-22T20:00:00"));
    let (fired, _) = late.tick(&items, at("2026-11-23T14:00:00"), 120, 10);
    assert!(fired.is_empty(), "five hours late is more than the two it forgives");
}

#[test]
fn a_once_reminder_fires_and_says_it_was_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(&dir);
    let once = json!({"id": "o", "enabled": true, "schedule": {"type": "once", "date": "2026-11-23", "time": "10:00"}});
    let items = [once];
    state.prime(&items, at("2026-11-23T09:00:00"));
    let (fired, _) = state.tick(&items, at("2026-11-23T10:00:05"), 120, 10);
    assert_eq!(fired, vec![Fired { id: "o".into(), once: true }]);
}

#[test]
fn a_disabled_reminder_never_fires() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(&dir);
    let mut rem = daily("a", "09:00");
    rem["enabled"] = json!(false);
    let items = [rem];
    state.prime(&items, at("2026-11-22T20:00:00"));
    assert!(state.tick(&items, at("2026-11-23T09:30:00"), 120, 10).0.is_empty());
}

#[test]
fn a_last_time_in_the_future_is_counted_from_now() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reminders_state.json");
    std::fs::write(&path, r#"{"a": {"last": "2026-12-25T00:00:00"}}"#).unwrap();
    let mut state = State::load(&path);
    let now = at("2026-11-23T09:30:00");
    let (fired, changed) = state.tick(&[daily("a", "09:00")], now, 120, 10);
    assert!(fired.is_empty() && changed);
    assert_eq!(state.last("a"), Some(now));
}

#[test]
fn a_snoozed_reminder_fires_when_the_snooze_ends_and_replaces_the_one_it_postponed() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(&dir);
    let items = [daily("a", "09:00")];
    state.prime(&items, at("2026-11-22T20:00:00"));
    state.snooze("a", at("2026-11-23T09:45:00"));
    assert!(state.tick(&items, at("2026-11-23T09:30:00"), 120, 10).0.is_empty(), "still snoozed");
    let (fired, changed) = state.tick(&items, at("2026-11-23T09:46:00"), 120, 10);
    assert_eq!(fired.len(), 1);
    assert!(changed);
    assert_eq!(state.snoozed_until("a"), None);
    assert_eq!(state.last("a"), Some(at("2026-11-23T09:46:00")));
}

#[test]
fn a_snooze_far_in_the_future_is_cut_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(&dir);
    let items = [daily("a", "09:00")];
    state.prime(&items, at("2026-11-23T08:00:00"));
    state.snooze("a", at("2027-01-01T00:00:00"));
    let (fired, _) = state.tick(&items, at("2026-11-23T09:30:00"), 120, 10);
    assert_eq!(fired.len(), 1, "saved while the clock was ahead, so it goes off now");
}

#[test]
fn the_next_reminder_is_the_soonest_and_a_snooze_counts() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(&dir);
    let items = [daily("a", "18:00"), daily("b", "16:00"), daily("c", "12:00")];
    let now = at("2026-11-23T14:30:00");
    state.prime(&items, now);
    assert_eq!(state.next_reminder(&items, now), Some((at("2026-11-23T16:00:00"), "b".to_owned())));
    state.snooze("a", at("2026-11-23T14:45:00"));
    assert_eq!(state.next_reminder(&items, now), Some((at("2026-11-23T14:45:00"), "a".to_owned())));
    assert_eq!(state.next_wake(&items, now, 120), Some(at("2026-11-23T14:45:00")));
}
