//! When a reminder fires: the schedule maths, how a schedule reads in words, the quick reminders typed into the
//! command palette ("remind me in 20m to stretch") and the little state file that remembers what fired last.
//!
//! A reminder is a JSON object as Settings stores it (`reminders.items`), so a field this version does not know
//! stays where it is. Times are the clock on the wall, without a time zone, as in the Python app.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use fancy_regex::Regex;
use jiff::civil::{Date, DateTime, Time};
use jiff::{SignedDuration, ToSpan};
use lum_core::pyjson::{self, Style};
use lum_core::store::{self, Writer};
use serde_json::{Map, Value, json};

use crate::timeparse::{parse_clock, parse_duration};

pub const DAY_NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// The lengths the pop-up's Snooze menu offers, in minutes.
pub const SNOOZE_CHOICES: [i64; 5] = [5, 10, 15, 30, 60];

const ISO_SECONDS: &str = "%Y-%m-%dT%H:%M:%S";

fn nine() -> Time {
    Time::constant(9, 0, 0, 0)
}

/// `09:30` or `09:30:15`. Anything else gives `default`.
pub fn parse_time(text: &str, default: Time) -> Time {
    let parts: Vec<&str> = text.split(':').collect();
    let number = |i: usize| parts.get(i).and_then(|p| p.trim().parse::<i8>().ok());
    let (Some(h), Some(m)) = (number(0), number(1)) else { return default };
    let s = if parts.len() > 2 { number(2) } else { Some(0) };
    s.and_then(|s| Time::new(h, m, s, 0).ok()).unwrap_or(default)
}

/// `2026-11-23`, as `date.fromisoformat` reads it.
pub fn parse_date(text: &str) -> Option<Date> {
    text.trim().parse::<Date>().ok()
}

fn int(value: Option<&Value>, default: i64) -> i64 {
    match value {
        Some(Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)).unwrap_or(default),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(default),
        Some(Value::Bool(b)) => i64::from(*b),
        _ => default,
    }
}

fn text<'a>(schedule: &'a Value, key: &str, default: &'a str) -> &'a str {
    schedule.get(key).and_then(Value::as_str).unwrap_or(default)
}

fn schedule_of(rem: &Value) -> Value {
    rem.get("schedule").filter(|s| s.is_object()).cloned().unwrap_or_else(|| json!({}))
}

fn at_time(date: Date, time: Time) -> DateTime {
    date.to_datetime(time)
}

fn plus_days(date: Date, days: i64) -> Option<Date> {
    date.checked_add(days.days()).ok()
}

/// The first moment strictly after `after` when `rem` should fire. `None` when it never will.
pub fn next_due(rem: &Value, after: DateTime) -> Option<DateTime> {
    let sch = schedule_of(rem);
    let kind = text(&sch, "type", "daily");
    let t = parse_time(text(&sch, "time", "09:00"), nine());
    match kind {
        // fired when the app starts, not by the clock
        "startup" => None,
        "once" => {
            let due = at_time(parse_date(text(&sch, "date", ""))?, t);
            (due > after).then_some(due)
        }
        "daily" => {
            let due = at_time(after.date(), t);
            if due > after { Some(due) } else { due.checked_add(1.day()).ok() }
        }
        "monthly" => monthly(&sch, t, after),
        "every_days" => every_days(&sch, t, after),
        "weekly" => weekly(&sch, t, after),
        "interval" => interval(&sch, after),
        _ => None,
    }
}

fn monthly(sch: &Value, t: Time, after: DateTime) -> Option<DateTime> {
    let day = int(sch.get("monthday"), 1).clamp(1, 31) as i8;
    let (mut year, mut month) = (after.year(), after.month());
    for _ in 0..14 {
        let first = Date::new(year, month, 1).ok()?;
        let due = at_time(Date::new(year, month, day.min(first.days_in_month())).ok()?, t);
        if due > after {
            return Some(due);
        }
        (year, month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    }
    None
}

fn every_days(sch: &Value, t: Time, after: DateTime) -> Option<DateTime> {
    let every = int(sch.get("every_days"), 2).max(1);
    let start = parse_date(text(sch, "date", "")).unwrap_or(after.date());
    let first = at_time(start, t);
    if first > after {
        return Some(first);
    }
    // whole days between, as timedelta.days counts them
    let whole = (after - first).get_days() as i64;
    let steps = whole / every;
    let mut due = first.checked_add((steps * every).days()).ok()?;
    while due <= after {
        due = due.checked_add(every.days()).ok()?;
    }
    Some(due)
}

fn weekly(sch: &Value, t: Time, after: DateTime) -> Option<DateTime> {
    let days: Vec<i64> = sch.get("days")?.as_array()?.iter().filter_map(Value::as_i64).collect();
    if days.is_empty() {
        return None;
    }
    for i in 0..8 {
        let date = plus_days(after.date(), i)?;
        if days.contains(&i64::from(date.weekday().to_monday_zero_offset())) {
            let due = at_time(date, t);
            if due > after {
                return Some(due);
            }
        }
    }
    None
}

fn interval(sch: &Value, after: DateTime) -> Option<DateTime> {
    let every = int(sch.get("every_minutes"), 60).max(1);
    let mut due = after.checked_add(every.minutes()).ok()?;
    if sch.get("window").is_some_and(truthy) {
        let start = parse_time(text(sch, "from", "09:00"), nine());
        let end = parse_time(text(sch, "to", "18:00"), nine());
        if start <= end {
            if due.time() < start {
                due = at_time(due.date(), start);
            } else if due.time() > end {
                due = at_time(plus_days(due.date(), 1)?, start);
            }
        } else if end < due.time() && due.time() < start {
            // a window that wraps midnight, like 22:00 to 02:00
            due = at_time(due.date(), start);
        }
    }
    Some(due)
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
        Value::Null => false,
    }
}

fn ordinal(day: i64) -> &'static str {
    if (11..=13).contains(&(day % 100)) {
        return "th";
    }
    match day % 10 {
        1 => "st",
        2 => "nd",
        3 => "rd",
        _ => "th",
    }
}

/// The schedule in words: "Every day at 09:00".
pub fn describe(rem: &Value) -> String {
    let sch = schedule_of(rem);
    let kind = text(&sch, "type", "daily");
    let t = text(&sch, "time", "09:00");
    match kind {
        "startup" => format!("When the launcher starts (after {} s)", int(sch.get("startup_delay"), 10)),
        "once" => format!("Once on {} at {t}", text(&sch, "date", "?")),
        "daily" => format!("Every day at {t}"),
        "weekly" => {
            let mut days: Vec<i64> = sch
                .get("days")
                .and_then(Value::as_array)
                .map(|l| l.iter().filter_map(Value::as_i64).collect())
                .unwrap_or_default();
            days.sort_unstable();
            let names: Vec<&str> =
                days.iter().filter_map(|d| DAY_NAMES.get(usize::try_from(*d).ok()?).copied()).collect();
            let list = if names.is_empty() { "No days".to_owned() } else { names.join(", ") };
            format!("{list} at {t}")
        }
        "monthly" => {
            let d = int(sch.get("monthday"), 1);
            format!("Every month on the {d}{} at {t}", ordinal(d))
        }
        "every_days" => {
            format!("Every {} days at {t} (from {})", int(sch.get("every_days"), 2), text(&sch, "date", "?"))
        }
        "interval" => {
            let minutes = sch.get("every_minutes").map_or_else(|| "60".to_owned(), py_text);
            let mut line = format!("Every {minutes} min");
            if sch.get("window").is_some_and(truthy) {
                line.push_str(&format!(
                    " ({}\u{2013}{})",
                    text(&sch, "from", "09:00"),
                    text(&sch, "to", "18:00")
                ));
            }
            line
        }
        other => other.to_owned(),
    }
}

/// A JSON value as Python's `str()` writes it in an f-string.
fn py_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// A new reminder as the Settings editor's Add button makes it.
pub fn new_reminder(today: Date, id: &str) -> Value {
    json!({
        "id": id,
        "title": "New reminder",
        "message": "",
        "enabled": true,
        "schedule": {
            "type": "daily", "time": "09:00", "date": today.to_string(), "days": [0, 1, 2, 3, 4],
            "every_minutes": 60, "window": false, "from": "09:00", "to": "18:00", "startup_delay": 10,
            "monthday": today.day(), "every_days": 2
        },
        "actions": {
            "popup": true, "position": "default", "sound": "default", "sound_file": "", "sound_repeat": 1,
            "toast": true
        }
    })
}

const UNIT: &str = r"(?:seconds?|secs?|s|minutes?|mins?|m|hours?|hrs?|h|days?|d|weeks?|w)(?![a-z])";

fn duration_pattern() -> String {
    format!(r"[\d.,:]+\s*(?:{UNIT})?(?:\s*\d+\s*(?:{UNIT})?)?")
}

static QUICK: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    let dur = duration_pattern();
    let clock = r"\d{1,2}(?:[:.]\d{2})?\s*(?:am|pm)?";
    let patterns = [
        // remind me in 20m to stretch / in 1h30 call mum
        format!(
            r"(?i)^(?:remind(?:\s+me)?\s+)?in\s+(?P<dur>{dur})\s+(?:to\s+|that\s+|about\s+)?(?P<what>.+)$"
        ),
        // remind me to stretch in 20m
        format!(r"(?i)^remind(?:\s+me)?\s+(?:to\s+|that\s+|about\s+)?(?P<what>.+?)\s+in\s+(?P<dur>{dur})$"),
        // remind me at 17:30 to call mum / at 9am standup / tomorrow at 9 dentist
        format!(
            r"(?i)^(?:remind(?:\s+me)?\s+)?(?P<tmrw>tomorrow\s+)?at\s+(?P<clock>{clock})\s+(?:to\s+|that\s+|about\s+)?(?P<what>.+)$"
        ),
        // remind me to call mum at 17:30
        format!(
            r"(?i)^remind(?:\s+me)?\s+(?:to\s+|that\s+|about\s+)?(?P<what>.+?)\s+(?P<tmrw>tomorrow\s+)?at\s+(?P<clock>{clock})$"
        ),
    ];
    patterns.iter().map(|p| Regex::new(p).expect("the quick reminder patterns are valid")).collect()
});

static BARE_UNIT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!("^(?:{UNIT})$")).expect("the unit pattern is valid"));

fn capitalise(what: &str) -> String {
    let mut chars = what.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// `(when, what)` for "remind me in 20m to stretch", "at 17:30 call mum"... `None` when it is not one.
pub fn parse_quick(text: &str, now: DateTime) -> Option<(DateTime, String)> {
    let t = text.trim();
    if t.chars().count() < 6 {
        return None;
    }
    for rx in QUICK.iter() {
        let Ok(Some(m)) = rx.captures(t) else { continue };
        let what = m["what"].trim().trim_end_matches('.').to_owned();
        if what.is_empty() || BARE_UNIT.is_match(&what.to_lowercase()).unwrap_or(false) {
            continue;
        }
        if let Some(dur) = m.name("dur") {
            let secs = match parse_duration(dur.as_str(), 60) {
                Some(s) if s >= 1.0 => s,
                _ => continue,
            };
            let when = now.checked_add(SignedDuration::try_from_secs_f64(secs).ok()?).ok()?;
            return Some((when, capitalise(&what)));
        }
        let Some(mut when) = m.name("clock").and_then(|c| parse_clock(c.as_str(), now)) else { continue };
        if m.name("tmrw").is_some() {
            when = at_time(plus_days(now.date(), 1)?, when.time());
        }
        return Some((when, capitalise(&what)));
    }
    None
}

/// What the state file keeps for each reminder: when it last fired and until when it is snoozed.
#[derive(Debug)]
pub struct State {
    path: PathBuf,
    map: Map<String, Value>,
    writer: Writer,
}

/// One that went off: its id, and whether it was a "once" reminder (which then turns itself off).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fired {
    pub id: String,
    pub once: bool,
}

fn iso(at: DateTime) -> String {
    at.strftime(ISO_SECONDS).to_string()
}

fn read_iso(value: Option<&Value>) -> Option<DateTime> {
    value?.as_str()?.trim().parse().ok()
}

/// A schedule as the state file records it, so an edit to one is noticed: Python's
/// `json.dumps(schedule, sort_keys=True)`.
pub fn signature(schedule: &Value) -> String {
    fn sorted(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                Value::Object(keys.into_iter().map(|k| (k.clone(), sorted(&map[k]))).collect())
            }
            Value::Array(list) => Value::Array(list.iter().map(sorted).collect()),
            other => other.clone(),
        }
    }
    pyjson::to_string(&sorted(schedule), Style::COMPACT)
}

fn items_with_ids(items: &[Value]) -> impl Iterator<Item = &Value> {
    items.iter().filter(|r| r.get("id").and_then(Value::as_str).is_some_and(|s| !s.is_empty()))
}

fn id_of(rem: &Value) -> &str {
    rem.get("id").and_then(Value::as_str).unwrap_or_default()
}

fn enabled(rem: &Value) -> bool {
    rem.get("enabled").is_none_or(truthy)
}

impl State {
    pub fn load(path: &Path) -> State {
        let map = match store::load_json(path) {
            Some(Value::Object(map)) => map,
            _ => Map::new(),
        };
        State { path: path.to_owned(), map, writer: Writer::new("reminders-state") }
    }

    fn entry(&mut self, id: &str) -> &mut Map<String, Value> {
        let slot = self.map.entry(id.to_owned()).or_insert_with(|| Value::Object(Map::new()));
        if !slot.is_object() {
            *slot = Value::Object(Map::new());
        }
        match slot {
            Value::Object(map) => map,
            _ => unreachable!("the slot was made an object just above"),
        }
    }

    fn entry_ref(&self, id: &str) -> Option<&Map<String, Value>> {
        self.map.get(id)?.as_object()
    }

    pub fn last(&self, id: &str) -> Option<DateTime> {
        read_iso(self.entry_ref(id)?.get("last"))
    }

    pub fn snoozed_until(&self, id: &str) -> Option<DateTime> {
        read_iso(self.entry_ref(id)?.get("snoozed_until"))
    }

    /// A reminder never seen before starts its clock now, so "every 30 minutes" fires in 30 minutes and not at
    /// once. The same goes for one whose schedule was just edited: moving 21:30 to 08:00 at 10:00 used to fire it
    /// straight away as a missed 08:00 catch-up.
    pub fn prime(&mut self, items: &[Value], now: DateTime) {
        for rem in items_with_ids(items) {
            let sig = signature(&schedule_of(rem));
            let entry = self.entry(id_of(rem));
            let edited = entry.get("sched").and_then(Value::as_str).is_some_and(|old| old != sig);
            if !entry.contains_key("last") || edited {
                entry.insert("last".into(), json!(iso(now)));
            }
            entry.insert("sched".into(), json!(sig));
        }
    }

    /// Starts the clock of a reminder added by another module, and forgets any snooze.
    pub fn started(&mut self, rem: &Value, now: DateTime) {
        let sig = signature(&schedule_of(rem));
        let entry = self.entry(id_of(rem));
        entry.insert("last".into(), json!(iso(now)));
        entry.insert("sched".into(), json!(sig));
        entry.remove("snoozed_until");
    }

    pub fn forget(&mut self, id: &str) {
        self.map.remove(id);
    }

    pub fn snooze(&mut self, id: &str, until: DateTime) {
        self.entry(id).insert("snoozed_until".into(), json!(iso(until)));
    }

    /// Which reminders are due. The state is brought up to date; `true` in the second place says it changed.
    ///
    /// A reminder missed while the app was closed still fires if it was due within `catch_up_minutes` (at least
    /// two); older ones are let go.
    pub fn tick(
        &mut self,
        items: &[Value],
        now: DateTime,
        catch_up_minutes: i64,
        snooze_minutes: i64,
    ) -> (Vec<Fired>, bool) {
        let grace = catch_up_minutes.max(2).minutes();
        let mut fired = Vec::new();
        let mut changed = false;
        for rem in items_with_ids(items).filter(|r| enabled(r)) {
            let id = id_of(rem).to_owned();
            if self.entry_ref(&id).is_some_and(|e| e.contains_key("snoozed_until")) {
                changed |= self.tick_snoozed(&id, now, snooze_minutes, &mut fired);
                continue;
            }
            let mut last = self.last(&id).unwrap_or(now);
            if last > now.checked_add(5.minutes()).unwrap_or(now) {
                // the clock was ahead when this was saved: count from now instead
                last = now;
                self.entry(&id).insert("last".into(), json!(iso(now)));
                changed = true;
            }
            let earliest = now.checked_sub(grace).unwrap_or(now);
            let Some(due) = next_due(rem, last.max(earliest)).filter(|due| *due <= now) else { continue };
            self.entry(&id).insert("last".into(), json!(iso(due)));
            changed = true;
            let once = text(&schedule_of(rem), "type", "daily") == "once";
            fired.push(Fired { id, once });
        }
        (fired, changed)
    }

    fn tick_snoozed(&mut self, id: &str, now: DateTime, snooze_minutes: i64, fired: &mut Vec<Fired>) -> bool {
        let Some(until) = self.snoozed_until(id) else {
            self.entry(id).remove("snoozed_until");
            return true;
        };
        // saved while the clock was ahead
        let limit = now.checked_add((snooze_minutes + 5).minutes()).unwrap_or(now);
        let until = if until > limit { now } else { until };
        if until > now {
            return false;
        }
        let entry = self.entry(id);
        entry.remove("snoozed_until");
        // the snoozed firing replaces the one it postponed: without this an "every N min" reminder fired twice
        // in a row when the snooze was at least N minutes long
        entry.insert("last".into(), json!(iso(now)));
        fired.push(Fired { id: id.to_owned(), once: false });
        true
    }

    /// The next reminder that will fire, for the overlay's status chips.
    pub fn next_reminder(&self, items: &[Value], now: DateTime) -> Option<(DateTime, String)> {
        let mut best: Option<(DateTime, String)> = None;
        for rem in items_with_ids(items).filter(|r| enabled(r)) {
            let due = self.snoozed_until(id_of(rem)).or_else(|| next_due(rem, now));
            let title =
                rem.get("title").and_then(Value::as_str).filter(|t| !t.is_empty()).unwrap_or("Reminder");
            if let Some(due) = due
                && best.as_ref().is_none_or(|(soonest, _)| due < *soonest)
            {
                best = Some((due, title.to_owned()));
            }
        }
        best
    }

    /// How long until anything needs a look: the soonest a reminder is due or a snooze ends.
    pub fn next_wake(&self, items: &[Value], now: DateTime, catch_up_minutes: i64) -> Option<DateTime> {
        let mut soonest: Option<DateTime> = None;
        for rem in items_with_ids(items).filter(|r| enabled(r)) {
            let id = id_of(rem);
            let due = match self.snoozed_until(id) {
                Some(until) => Some(until),
                None => {
                    let earliest = now.checked_sub(catch_up_minutes.max(2).minutes()).unwrap_or(now);
                    next_due(rem, self.last(id).unwrap_or(now).max(earliest))
                }
            };
            if let Some(due) = due
                && soonest.is_none_or(|s| due < s)
            {
                soonest = Some(due);
            }
        }
        soonest
    }

    pub fn to_text(&self) -> String {
        pyjson::to_string(&Value::Object(self.map.clone()), Style::indented(2))
    }

    pub fn save(&self) {
        self.writer.save(&self.path, self.to_text());
    }

    pub fn flush(&self) {
        self.writer.flush();
    }
}

/// A change another module asks for in the reminders list.
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    /// Add a reminder, or replace the one with the same id.
    Add(Value),
    Remove(String),
    /// A "once" reminder that has gone off turns itself off.
    Disable(String),
}

/// A one-off reminder at `when`, as the palette's "remind me" and a to-do's due date make it.
pub fn quick_reminder(title: &str, message: &str, when: DateTime, id: &str, new_id: &str) -> Value {
    let mut rem = new_reminder(when.date(), if id.is_empty() { new_id } else { id });
    rem["title"] = json!(title);
    rem["message"] = json!(message);
    rem["schedule"]["type"] = json!("once");
    rem["schedule"]["date"] = json!(when.date().to_string());
    rem["schedule"]["time"] = json!(when.strftime("%H:%M:%S").to_string());
    rem["quick"] = json!(true);
    rem
}

/// Applies an edit to the list. Adding also tidies away finished quick reminders older than a day.
pub fn apply_edit(items: &mut Vec<Value>, edit: &Edit, today: Date) {
    match edit {
        Edit::Add(rem) => {
            let id = id_of(rem).to_owned();
            let cutoff = plus_days(today, -1).map(|d| d.to_string()).unwrap_or_default();
            items.retain(|r| r.is_object() && id_of(r) != id);
            items.retain(|r| {
                let old_quick = r.get("quick").is_some_and(truthy)
                    && !enabled(r)
                    && r["schedule"].get("date").and_then(Value::as_str).unwrap_or_default()
                        < cutoff.as_str();
                !old_quick
            });
            items.push(rem.clone());
        }
        Edit::Remove(id) => items.retain(|r| id_of(r) != id),
        Edit::Disable(id) => {
            for rem in items.iter_mut().filter(|r| id_of(r) == id) {
                rem["enabled"] = json!(false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> DateTime {
        text.parse().expect("a date and time")
    }

    #[test]
    fn a_quick_reminder_is_a_once_reminder_with_seconds() {
        let rem = quick_reminder("Stretch", "", at("2026-11-23T14:50:07"), "", "abc");
        assert_eq!(rem["id"], "abc");
        assert_eq!(rem["schedule"]["type"], "once");
        assert_eq!(rem["schedule"]["date"], "2026-11-23");
        assert_eq!(rem["schedule"]["time"], "14:50:07");
        assert_eq!(rem["quick"], true);
        assert_eq!(quick_reminder("x", "", at("2026-11-23T14:50:07"), "todo-1", "abc")["id"], "todo-1");
    }

    #[test]
    fn adding_replaces_by_id_and_tidies_old_finished_quick_reminders() {
        let today = at("2026-11-23T00:00:00").date();
        let old = json!({"id": "old", "quick": true, "enabled": false, "schedule": {"date": "2026-11-20"}});
        let recent =
            json!({"id": "recent", "quick": true, "enabled": false, "schedule": {"date": "2026-11-23"}});
        let mine = json!({"id": "mine", "title": "A", "enabled": true, "schedule": {"date": "2026-11-01"}});
        let mut items = vec![old, recent, mine];
        let again = json!({"id": "mine", "title": "B"});
        apply_edit(&mut items, &Edit::Add(again), today);
        let ids: Vec<&str> = items.iter().map(id_of).collect();
        assert_eq!(ids, ["recent", "mine"]);
        assert_eq!(items[1]["title"], "B");
    }

    #[test]
    fn removing_and_disabling_find_their_reminder() {
        let today = at("2026-11-23T00:00:00").date();
        let mut items = vec![json!({"id": "a", "enabled": true}), json!({"id": "b", "enabled": true})];
        apply_edit(&mut items, &Edit::Disable("a".into()), today);
        assert_eq!(items[0]["enabled"], false);
        apply_edit(&mut items, &Edit::Remove("b".into()), today);
        assert_eq!(items.len(), 1);
        apply_edit(&mut items, &Edit::Remove("nobody".into()), today);
        assert_eq!(items.len(), 1);
    }
}
