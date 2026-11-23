//! Usage stats and the day's activity log, as `core/usage.py`.
//!
//! * `data/usage.json`: how often each dashboard item, hotkey and tool was used, and when last:
//!   `{"items": {key: {"label", "n", "last"}}, "hotkeys": {...}, "tools": {...}, "days": {"2026-10-26": n}}`
//! * `data/activity.jsonl`: one line per thing you did, `{"t", "kind", "label", "secs"?}`. kind is `run` (a
//!   dashboard item), `tool`, or `timer` (a timer or stopwatch that ran for `secs`). The Today page reads it.
//!
//! Recording only changes memory; the app calls [`Usage::flush`] a few seconds after the last use (the Python
//! app's 4 s timer), so a hotkey costs a map update. Nothing leaves the machine.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use jiff::ToSpan;
use jiff::civil::{Date, DateTime};
use serde_json::{Map, Value, json};

use crate::clock::Clock;
use crate::paths::Paths;
use crate::pyjson::{self, Style};
use crate::store;

pub const KEEP_DAYS: i64 = 120;
/// How long after the last use the app should flush.
pub const FLUSH_AFTER_MS: u64 = 4000;
const TRIM_FROM_BYTES: u64 = 2_000_000;
const BUCKETS: [&str; 4] = ["items", "hotkeys", "tools", "days"];

pub struct Usage {
    usage_path: PathBuf,
    activity_path: PathBuf,
    clock: Arc<dyn Clock>,
    data: Option<Map<String, Value>>,
    pending: Vec<String>,
    dirty: bool,
}

impl Usage {
    pub fn new(paths: &Paths, clock: Arc<dyn Clock>) -> Usage {
        Usage {
            usage_path: paths.usage_file(),
            activity_path: paths.activity_file(),
            clock,
            data: None,
            pending: Vec::new(),
            dirty: false,
        }
    }

    fn data(&mut self) -> &mut Map<String, Value> {
        self.data.get_or_insert_with(|| {
            let mut data = match store::load_json(&self.usage_path) {
                Some(Value::Object(map)) => map,
                _ => Map::new(),
            };
            for bucket in BUCKETS {
                if !data.get(bucket).is_some_and(Value::is_object) {
                    data.insert(bucket.to_owned(), json!({}));
                }
            }
            data
        })
    }

    /// Something changed since the last flush.
    pub fn needs_flush(&self) -> bool {
        self.dirty
    }

    /// Counts one use. `kind` is `items`, `hotkeys` or `tools`.
    pub fn record(&mut self, kind: &str, key: &str, label: &str) {
        if key.is_empty() {
            return;
        }
        let now = self.clock.now();
        let data = self.data();
        let bucket = object_at(data, kind);
        let entry = bucket.entry(key).or_insert_with(
            || json!({"label": if label.is_empty() { key } else { label }, "n": 0, "last": ""}),
        );
        if !entry.is_object() {
            *entry = json!({"label": key, "n": 0, "last": ""});
        }
        let count = entry.get("n").map_or(0, python_int) + 1;
        entry["n"] = count.into();
        entry["last"] = iso_seconds(now).into();
        if !label.is_empty() {
            entry["label"] = label.into();
        }
        let days = object_at(data, "days");
        let today = now.date().to_string();
        let seen = days.get(&today).map_or(0, python_int);
        days.insert(today, (seen + 1).into());
        self.dirty = true;
    }

    /// Adds a line to the activity log (the Today page).
    pub fn activity(&mut self, kind: &str, label: &str, secs: Option<f64>, when: Option<DateTime>) {
        let when = when.unwrap_or_else(|| self.clock.now());
        let mut entry = json!({"t": iso_seconds(when), "kind": kind, "label": label});
        if let Some(secs) = secs {
            entry["secs"] = round_to_tenth(secs).into();
        }
        self.pending.push(pyjson::to_string(&entry, Style { indent: None, ensure_ascii: false }));
        self.dirty = true;
    }

    pub fn flush(&mut self) {
        self.dirty = false;
        let cutoff = self.cutoff();
        if let Some(data) = self.data.as_mut() {
            object_at(data, "days").retain(|day, _| day.as_str() >= cutoff.as_str());
            if let Err(e) =
                store::save_json(&self.usage_path, &Value::Object(data.clone()), Style::indented(1))
            {
                tracing::error!("could not save usage.json: {e}");
            }
        }
        if !self.pending.is_empty() {
            let mut text = self.pending.join("\n");
            text.push('\n');
            self.pending.clear();
            let appended = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.activity_path)
                .and_then(|mut f| f.write_all(text.as_bytes()));
            if let Err(e) = appended {
                tracing::error!("could not write activity.jsonl: {e}");
            }
            self.trim_activity(&cutoff);
        }
    }

    fn cutoff(&self) -> String {
        let today = self.clock.now().date();
        today.checked_sub(KEEP_DAYS.days()).unwrap_or(today).to_string()
    }

    /// Keeps about KEEP_DAYS of activity once the file grows past 2 MB.
    fn trim_activity(&self, cutoff: &str) {
        let big = fs::metadata(&self.activity_path).is_ok_and(|m| m.len() >= TRIM_FROM_BYTES);
        if !big {
            return;
        }
        let Ok(bytes) = fs::read(&self.activity_path) else { return };
        let text = String::from_utf8_lossy(&bytes);
        // the date sits at a fixed place: {"t": "2026-11-23T...
        let keep: String = text
            .split_inclusive('\n')
            .filter(|line| line.get(7..17).is_some_and(|day| day >= cutoff))
            .collect();
        let _ = store::write_atomically(&self.activity_path, keep.as_bytes());
    }

    pub fn stats(&mut self) -> &Map<String, Value> {
        self.data()
    }

    pub fn reset(&mut self) {
        let empty = BUCKETS.iter().map(|b| (b.to_string(), json!({}))).collect();
        self.data = Some(empty);
        self.flush();
    }

    /// Every activity entry of one day (today by default), oldest first, including unsaved ones.
    pub fn day_activity(&self, day: Option<Date>) -> Vec<Value> {
        let day = day.unwrap_or_else(|| self.clock.now().date()).to_string();
        let mut out = Vec::new();
        if let Ok(bytes) = fs::read(&self.activity_path) {
            let text = String::from_utf8_lossy(&bytes);
            for line in text.lines() {
                let head = line.char_indices().nth(20).map_or(line, |(i, _)| &line[..i]);
                if head.contains(&day)
                    && let Ok(entry) = serde_json::from_str::<Value>(line)
                {
                    out.push(entry);
                }
            }
        }
        for line in &self.pending {
            if let Ok(entry) = serde_json::from_str::<Value>(line)
                && entry.get("t").and_then(Value::as_str).is_some_and(|t| t.starts_with(&day))
            {
                out.push(entry);
            }
        }
        out.sort_by(|a, b| time_of(a).cmp(time_of(b)));
        out
    }
}

fn time_of(entry: &Value) -> &str {
    entry.get("t").and_then(Value::as_str).unwrap_or_default()
}

fn object_at<'a>(data: &'a mut Map<String, Value>, key: &str) -> &'a mut Map<String, Value> {
    let slot = data.entry(key).or_insert_with(|| json!({}));
    if !slot.is_object() {
        *slot = json!({});
    }
    slot.as_object_mut().expect("just made an object")
}

/// Python's `int(x)` on what a counter might hold.
fn python_int(v: &Value) -> i64 {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)).unwrap_or(0),
        Value::String(s) => s.trim().parse().unwrap_or(0),
        Value::Bool(b) => i64::from(*b),
        _ => 0,
    }
}

/// `datetime.isoformat(timespec="seconds")`.
pub fn iso_seconds(at: DateTime) -> String {
    at.strftime("%Y-%m-%dT%H:%M:%S").to_string()
}

/// Python's `round(x, 1)`: the nearest tenth of the exact value, ties to even.
fn round_to_tenth(x: f64) -> f64 {
    format!("{x:.1}").parse().unwrap_or(x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FixedClock;

    fn usage_at(dir: &std::path::Path) -> (Usage, Arc<FixedClock>) {
        let clock = Arc::new(FixedClock::new(DateTime::constant(2026, 11, 23, 14, 30, 0, 0)));
        let paths = Paths::at(dir);
        fs::create_dir_all(&paths.data).unwrap();
        (Usage::new(&paths, clock.clone()), clock)
    }

    #[test]
    fn counts_and_writes_like_python() {
        let dir = tempfile::tempdir().unwrap();
        let (mut usage, _clock) = usage_at(dir.path());
        usage.record("tools", "notes:new_note", "");
        usage.record("tools", "notes:new_note", "Notes: New note");
        usage.record("items", "", "ignored");
        assert!(usage.needs_flush());
        usage.flush();
        assert!(!usage.needs_flush());
        let text = fs::read_to_string(dir.path().join("data/usage.json")).unwrap();
        assert_eq!(
            text,
            "{\n \"items\": {},\n \"hotkeys\": {},\n \"tools\": {\n  \"notes:new_note\": {\n   \"label\": \
             \"Notes: New note\",\n   \"n\": 2,\n   \"last\": \"2026-11-23T14:30:00\"\n  }\n },\n \"days\": {\n  \
             \"2026-11-23\": 2\n }\n}"
        );
    }

    #[test]
    fn activity_lines_and_today() {
        let dir = tempfile::tempdir().unwrap();
        let (mut usage, clock) = usage_at(dir.path());
        usage.activity("timer", "Focus → deep", Some(1499.96), None);
        clock.set(DateTime::constant(2026, 11, 23, 9, 0, 0, 0));
        usage.activity("run", "Firefox", None, None);
        let today = usage.day_activity(None);
        assert_eq!(today.len(), 2, "unsaved lines count too");
        assert_eq!(today[0]["label"], "Firefox");
        usage.flush();
        let text = fs::read_to_string(dir.path().join("data/activity.jsonl")).unwrap();
        assert_eq!(
            text,
            "{\"t\": \"2026-11-23T14:30:00\", \"kind\": \"timer\", \"label\": \"Focus → deep\", \"secs\": 1500.0}\n\
             {\"t\": \"2026-11-23T09:00:00\", \"kind\": \"run\", \"label\": \"Firefox\"}\n"
        );
        assert_eq!(usage.day_activity(None).len(), 2);
        assert!(usage.day_activity(Some(Date::constant(2026, 11, 22))).is_empty());
    }

    #[test]
    fn old_days_are_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let (mut usage, clock) = usage_at(dir.path());
        clock.set(DateTime::constant(2026, 1, 1, 12, 0, 0, 0));
        usage.record("hotkeys", "alt+n", "notes: hotkey");
        clock.set(DateTime::constant(2026, 11, 23, 12, 0, 0, 0));
        usage.record("hotkeys", "alt+n", "");
        usage.flush();
        let days = usage.stats()["days"].clone();
        assert_eq!(days, json!({"2026-11-23": 1}));
    }

    #[test]
    fn rounding_matches_python() {
        assert_eq!(round_to_tenth(0.25), 0.2);
        assert_eq!(round_to_tenth(0.35), 0.3);
        assert_eq!(round_to_tenth(12.0), 12.0);
        assert_eq!(round_to_tenth(1499.96), 1500.0);
    }
}
