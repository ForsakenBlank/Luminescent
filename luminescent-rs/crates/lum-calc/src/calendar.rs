//! Atlas's calendar as the Calendar page shows it: the events and tasks kept in `atlas_sync.json` as of the last
//! sync (read only, Atlas owns them), when an event with a repeat rule happens, and what a day holds including
//! Luminescent's own reminders. Phase 13 brings the live sync; the data it keeps is the same.

use std::path::Path;

use jiff::civil::{Date, DateTime, date};
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Span, Timestamp, ToSpan};
use serde_json::Value;

use crate::reminder;

const DAY_MS: i64 = 86_400_000;

/// The events and tasks Atlas last sent, and when that was.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Calendar {
    pub events: Vec<Value>,
    pub tasks: Vec<Value>,
    /// The newest sync with any device, in milliseconds since 1970; 0 when there has not been one.
    pub last_sync_ms: i64,
}

fn int(value: Option<&Value>) -> i64 {
    match value {
        Some(Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)).unwrap_or(0),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0),
        Some(Value::Bool(b)) => i64::from(*b),
        _ => 0,
    }
}

fn int_opt(value: Option<&Value>) -> Option<i64> {
    match value {
        None | Some(Value::Null) => None,
        other => Some(int(other)),
    }
}

fn truthy(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
        _ => false,
    }
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

impl Calendar {
    /// Reads the shadow store of `atlas_sync.json`. A missing or unreadable file is an empty calendar.
    pub fn load(path: &Path) -> Calendar {
        let Some(root) =
            std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok())
        else {
            return Calendar::default();
        };
        let mut calendar = Calendar::default();
        if let Some(shadow) = root.get("shadow").and_then(Value::as_object) {
            for record in shadow.values() {
                let Some(fields) = record.get("fields").and_then(Value::as_object) else { continue };
                let mut merged = fields.clone();
                merged.insert("uid".into(), record.get("uid").cloned().unwrap_or(Value::Null));
                match text(record, "kind") {
                    "event" => calendar.events.push(Value::Object(merged)),
                    "task" => calendar.tasks.push(Value::Object(merged)),
                    _ => {}
                }
            }
        }
        calendar.events.sort_by_key(|e| int(e.get("startsAt")));
        calendar.tasks.sort_by_key(|t| (t.get("due").is_none_or(Value::is_null), int(t.get("due"))));
        if let Some(peers) = root.get("peers").and_then(Value::as_object) {
            calendar.last_sync_ms = peers.values().map(|p| int(p.get("last"))).max().unwrap_or(0);
        }
        calendar
    }
}

/// `#RRGGBB` of an Atlas colour (a signed 32 bit number whose low 24 bits are the colour); empty for none.
pub fn int_to_colour(value: Option<&Value>) -> String {
    match int_opt(value) {
        None => String::new(),
        Some(n) => format!("#{:06X}", n & 0xFF_FFFF),
    }
}

fn local(ms: i64, tz: &TimeZone) -> Option<DateTime> {
    Timestamp::from_millisecond(ms).ok().map(|t| t.to_zoned_datetime(tz.clone()).datetime())
}

fn to_ms(at: DateTime, tz: &TimeZone) -> Option<i64> {
    tz.to_zoned_datetime(at).ok().map(|z| z.timestamp().as_millisecond())
}

fn days_since_epoch(day: Date) -> i64 {
    day.since(date(1970, 1, 1)).map_or(0, |s| i64::from(s.get_days()))
}

/// Start times (ms) of an event between `start_ms` and `end_ms`, following its repeat rule. Days are worked out in
/// `tz`, the way the phone shows them: a daily 09:00 stays at 09:00 across a clock change.
pub fn occurrences(event: &Value, start_ms: i64, end_ms: i64, tz: &TimeZone) -> Vec<i64> {
    let first = int(event.get("startsAt"));
    let rule = match event.get("repeatRule") {
        Some(Value::Null) | None => "NONE".to_owned(),
        Some(Value::String(s)) if s.is_empty() => "NONE".to_owned(),
        Some(Value::String(s)) => s.to_uppercase(),
        Some(other) => other.to_string().to_uppercase(),
    };
    let until = truthy(event.get("repeatUntil")).then(|| int(event.get("repeatUntil")));
    // days taken out of a repeat in Atlas, as days since 1970
    let skipped: Vec<i64> = event
        .get("skippedDays")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .split(',')
        .filter_map(|d| d.trim().parse().ok())
        .collect();
    let step: Option<Span> = match rule.as_str() {
        "DAILY" | "WEEKDAYS" => Some(1.day()),
        "WEEKLY" => Some(1.week()),
        "FORTNIGHTLY" => Some(2.weeks()),
        _ => None,
    };
    let Some(mut t) = local(first, tz) else { return Vec::new() };
    let mut out = Vec::new();
    for _ in 0..4000 {
        let Some(ms) = to_ms(t, tz) else { break };
        if ms > end_ms || until.is_some_and(|u| ms > u) {
            break;
        }
        let weekend = t.date().weekday().to_monday_zero_offset() >= 5;
        if ms >= start_ms
            && !(rule == "WEEKDAYS" && weekend)
            && !skipped.contains(&days_since_epoch(t.date()))
        {
            out.push(ms);
        }
        if rule == "NONE" {
            break;
        }
        let next = if let Some(step) = step {
            t.checked_add(step).ok()
        } else if rule == "MONTHLY" {
            // past the 28th a month is 31 days, so the day drifts rather than falling off a short month
            if t.day() <= 28 {
                let (year, month) =
                    if t.month() == 12 { (t.year() + 1, 1) } else { (t.year(), t.month() + 1) };
                t.with().year(year).month(month).build().ok()
            } else {
                t.checked_add(31.days()).ok()
            }
        } else if rule == "YEARLY" {
            if t.month() == 2 && t.day() == 29 {
                t.checked_add(365.days()).ok()
            } else {
                t.with().year(t.year() + 1).build().ok()
            }
        } else {
            None
        };
        match next {
            Some(next) => t = next,
            None => break,
        }
    }
    out
}

/// What kind of line a day's agenda row is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Event,
    Task,
    Done,
    Reminder,
}

impl Kind {
    /// The small word at the right of the row.
    pub fn label(self) -> &'static str {
        match self {
            Kind::Event => "ATLAS",
            Kind::Task | Kind::Done => "TASK",
            Kind::Reminder => "REMINDER",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// Rows are in this order: all-day events, then events and reminders by time, then tasks.
    pub sort: i64,
    pub when: String,
    pub title: String,
    pub kind: Kind,
    /// The event's own colour as `#RRGGBB`; none for everything else (the page picks a theme colour).
    pub colour: Option<String>,
}

fn clock(ms: i64, tz: &TimeZone) -> String {
    local(ms, tz).map(|d| d.strftime("%H:%M").to_string()).unwrap_or_default()
}

/// Everything on one day: Atlas events (including a repeat that started earlier and one still going from the
/// day before), tasks due that day, and Luminescent's reminders when `reminders` is given (Reminders is on).
pub fn items_on(day: Date, calendar: &Calendar, reminders: Option<&[Value]>, tz: &TimeZone) -> Vec<Item> {
    let Some(start) = to_ms(day.at(0, 0, 0, 0), tz) else { return Vec::new() };
    let end = start + DAY_MS - 1;
    let mut out = Vec::new();
    for event in &calendar.events {
        let span = (int(event.get("endsAt")) - int(event.get("startsAt"))).max(0);
        let all_day = truthy(event.get("allDay"));
        for t in occurrences(event, start - span, end, tz) {
            if t + span < start {
                continue;
            }
            let title = Some(text(event, "title")).filter(|s| !s.is_empty()).unwrap_or("Event");
            out.push(Item {
                sort: if all_day { 0 } else { t },
                when: if all_day { "All day".into() } else { clock(t.max(start), tz) },
                title: title.to_owned(),
                kind: Kind::Event,
                colour: Some(int_to_colour(event.get("color"))).filter(|c| !c.is_empty()),
            });
        }
    }
    for task in &calendar.tasks {
        let Some(due) = int_opt(task.get("due")) else { continue };
        if local(due, tz).map(|d| d.date()) != Some(day) {
            continue;
        }
        let done = truthy(task.get("done"));
        let title = Some(text(task, "title")).filter(|s| !s.is_empty()).unwrap_or("Task");
        out.push(Item {
            sort: 1,
            when: if done { "Done" } else { "Task" }.to_owned(),
            title: title.to_owned(),
            kind: if done { Kind::Done } else { Kind::Task },
            colour: None,
        });
    }
    let before = day.at(0, 0, 0, 0).checked_sub(SignedDuration::from_secs(1)).ok();
    for rem in
        reminders.unwrap_or_default().iter().filter(|r| r.get("enabled").is_none_or(|e| truthy(Some(e))))
    {
        let Some(due) = before.and_then(|b| reminder::next_due(rem, b)) else { continue };
        if due.date() != day {
            continue;
        }
        let title = Some(text(rem, "title")).filter(|s| !s.is_empty()).unwrap_or("Reminder");
        out.push(Item {
            sort: to_ms(due, tz).unwrap_or(0),
            when: due.strftime("%H:%M").to_string(),
            title: title.to_owned(),
            kind: Kind::Reminder,
            colour: None,
        });
    }
    out.sort_by_key(|i| i.sort);
    out
}

/// The weeks of a month for the grid, Monday first: from the Monday on or before the 1st to the Sunday on or
/// after the last day, so the days of the months either side fill the first and last row.
pub fn month_weeks(first_of_month: Date) -> Vec<[Date; 7]> {
    let first = first_of_month.first_of_month();
    let last = first.last_of_month();
    let lead = i64::from(first.weekday().to_monday_zero_offset());
    let trail = 6 - i64::from(last.weekday().to_monday_zero_offset());
    let end = last.checked_add(trail.days()).unwrap_or(last);
    let mut weeks = Vec::new();
    let mut monday = first.checked_sub(lead.days()).unwrap_or(first);
    while monday <= end {
        let mut week = [monday; 7];
        for (i, slot) in week.iter_mut().enumerate() {
            *slot = monday.checked_add((i as i64).days()).unwrap_or(monday);
        }
        weeks.push(week);
        monday = monday.checked_add(7.days()).unwrap_or(end.checked_add(1.day()).unwrap_or(end));
    }
    weeks
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cases() -> Value {
        lum_fixtures::read_json("atlas_cases.json")
    }

    #[test]
    fn every_occurrence_case_of_the_python_app_passes() {
        let cases = cases();
        let list = cases["occurrences"].as_array().expect("a list");
        assert_eq!(list.len(), 12);
        for (i, case) in list.iter().enumerate() {
            let (from, to) = (case["from"].as_i64().expect("from"), case["to"].as_i64().expect("to"));
            let want: Vec<i64> =
                case["occurrences"].as_array().expect("list").iter().filter_map(Value::as_i64).collect();
            let mut got = occurrences(&case["event"], from, to, &TimeZone::UTC);
            // the fixture keeps only the first 30 of a long repeat
            if want.len() == 30 && got.len() > 30 {
                got.truncate(30);
            }
            assert!(
                got == want,
                "case {i}: {} gave {} times, expected {}",
                case["event"],
                got.len(),
                want.len()
            );
        }
    }

    #[test]
    fn every_colour_case_of_the_python_app_passes() {
        let cases = cases();
        for case in cases["int_to_colour"].as_array().expect("a list") {
            assert_eq!(int_to_colour(Some(&case["int"])), case["colour"].as_str().expect("text"), "{case}");
        }
    }

    fn at_utc(day: &str, time: &str) -> i64 {
        let stamp: Timestamp = format!("{day}T{time}Z").parse().expect("a time");
        stamp.as_millisecond()
    }

    fn calendar() -> Calendar {
        Calendar {
            events: vec![
                json!({"uid": "e1", "title": "Stand-up", "startsAt": at_utc("2026-11-23", "09:30:00"),
                       "endsAt": at_utc("2026-11-23", "09:45:00"), "repeatRule": "WEEKDAYS", "color": -1750963}),
                json!({"uid": "e2", "title": "Holiday", "allDay": true, "startsAt": at_utc("2026-11-26", "00:00:00"),
                       "endsAt": at_utc("2026-11-28", "00:00:00")}),
                json!({"uid": "e3", "startsAt": at_utc("2026-11-26", "22:00:00"), "endsAt": at_utc("2026-11-27", "01:00:00")}),
            ],
            tasks: vec![
                json!({"uid": "t1", "title": "Pay rent", "due": at_utc("2026-11-27", "12:00:00"), "done": false}),
                json!({"uid": "t2", "title": "Book dentist", "due": at_utc("2026-11-27", "08:00:00"), "done": true}),
                json!({"uid": "t3", "title": "No date", "due": null}),
            ],
            last_sync_ms: 0,
        }
    }

    fn day(text: &str) -> Date {
        text.parse().expect("a date")
    }

    #[test]
    fn a_day_lists_events_tasks_and_what_runs_over_from_the_night_before() {
        let items = items_on(day("2026-11-27"), &calendar(), None, &TimeZone::UTC);
        let rows: Vec<_> = items.iter().map(|i| (i.when.as_str(), i.title.as_str(), i.kind)).collect();
        // all-day first, then tasks (their key is 1), then timed events: the order the Python page has
        assert_eq!(
            rows,
            vec![
                ("All day", "Holiday", Kind::Event),
                ("Task", "Pay rent", Kind::Task),
                ("Done", "Book dentist", Kind::Done),
                ("00:00", "Event", Kind::Event),
                ("09:30", "Stand-up", Kind::Event),
            ]
        );
        assert_eq!(items[4].colour.as_deref(), Some("#E5484D"));
        assert_eq!(items[0].colour, None);
    }

    #[test]
    fn a_weekend_has_no_standup_and_a_day_with_nothing_is_empty() {
        let saturday = items_on(day("2026-11-28"), &calendar(), None, &TimeZone::UTC);
        let titles: Vec<_> = saturday.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, ["Holiday"], "the two-day holiday reaches Saturday morning, nothing else");
        assert!(items_on(day("2026-12-26"), &calendar(), None, &TimeZone::UTC).is_empty());
    }

    #[test]
    fn reminders_join_the_day_when_reminders_is_on() {
        let reminders = vec![
            json!({"title": "Water the plants", "enabled": true, "schedule": {"type": "daily", "time": "08:15"}}),
            json!({"title": "Switched off", "enabled": false, "schedule": {"type": "daily", "time": "07:00"}}),
            json!({"title": "No flag means on", "schedule": {"type": "once", "date": "2026-11-27", "time": "17:00"}}),
            json!({"title": "Next week", "schedule": {"type": "once", "date": "2026-12-04", "time": "17:00"}}),
        ];
        let items = items_on(day("2026-11-27"), &Calendar::default(), Some(&reminders), &TimeZone::UTC);
        let rows: Vec<_> = items.iter().map(|i| (i.when.as_str(), i.title.as_str(), i.kind)).collect();
        assert_eq!(
            rows,
            vec![
                ("08:15", "Water the plants", Kind::Reminder),
                ("17:00", "No flag means on", Kind::Reminder),
            ]
        );
        assert!(items_on(day("2026-11-27"), &Calendar::default(), None, &TimeZone::UTC).is_empty());
    }

    #[test]
    fn the_stored_file_is_read_into_events_and_tasks_in_order() {
        let dir = tempfile::tempdir().expect("a temp folder");
        let path = dir.path().join("atlas_sync.json");
        let stored = json!({
            "peers": {"phone": {"last": 1_795_444_200_000_i64}, "tablet": {"last": 1_795_000_000_000_i64}},
            "shadow": {
                "event/b": {"kind": "event", "uid": "b", "fields": {"title": "Later", "startsAt": 20}},
                "event/a": {"kind": "event", "uid": "a", "fields": {"title": "Sooner", "startsAt": 10}},
                "task/x": {"kind": "task", "uid": "x", "fields": {"title": "No date", "due": null}},
                "task/y": {"kind": "task", "uid": "y", "fields": {"title": "Dated", "due": 5}},
                "item/n": {"kind": "item", "uid": "n", "fields": {"name": "A note"}}
            }
        });
        std::fs::write(&path, stored.to_string()).expect("written");
        let calendar = Calendar::load(&path);
        let events: Vec<_> = calendar.events.iter().map(|e| text(e, "uid")).collect();
        let tasks: Vec<_> = calendar.tasks.iter().map(|t| text(t, "uid")).collect();
        assert_eq!((events, tasks), (vec!["a", "b"], vec!["y", "x"]));
        assert_eq!(calendar.last_sync_ms, 1_795_444_200_000);
        let sample = Calendar::load(&lum_fixtures::path("sample_home/data/atlas_sync.json"));
        assert!(sample.events.is_empty() && sample.tasks.is_empty());
        assert_eq!(sample.last_sync_ms, 1_795_444_200_000);
        assert_eq!(Calendar::load(&dir.path().join("missing.json")), Calendar::default());
    }

    #[test]
    fn a_month_is_shown_in_whole_weeks_starting_on_monday() {
        let weeks = month_weeks(day("2026-11-01"));
        assert_eq!(weeks.len(), 6);
        assert_eq!(weeks[0][0], day("2026-10-26"));
        assert_eq!(weeks[0][6], day("2026-11-01"));
        assert_eq!(weeks[5][0], day("2026-11-30"));
        assert_eq!(weeks[5][6], day("2026-12-06"));
        let february = month_weeks(day("2027-02-10"));
        assert_eq!(
            february.len(),
            4,
            "a February that starts on a Monday and has 28 days is exactly four weeks"
        );
        assert_eq!(february[0][0], day("2027-02-01"));
        assert_eq!(february[3][6], day("2027-02-28"));
    }
}
