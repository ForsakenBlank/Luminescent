//! What the Today and Usage pages work out from the activity log and the counts in `usage.json`: the day's
//! summary, its timeline by the hour, the most used things, a bar for each of the last 30 days and how long ago
//! something was. Nothing here draws; `lum-ui` shows it.

use jiff::ToSpan;
use jiff::civil::{Date, DateTime};
use serde_json::{Map, Value};

/// The tags on the Today timeline for the kinds the app writes; any other kind is shown in capitals as it is.
pub fn kind_tag(kind: &str) -> String {
    match kind {
        "run" => "RUN".to_owned(),
        "tool" => "TOOL".to_owned(),
        "timer" => "TIMER".to_owned(),
        other => other.to_uppercase(),
    }
}

/// One line of the activity log whose time could be read.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub at: DateTime,
    pub kind: String,
    pub label: String,
    /// How long it took (a timer that ran); none when the line has no time or it is zero.
    pub secs: Option<f64>,
}

fn secs_of(entry: &Value) -> f64 {
    match entry.get("secs") {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn text_of<'a>(entry: &'a Value, key: &str) -> &'a str {
    entry.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// The timeline: every line with a readable time, in the order given (the log is oldest first).
pub fn entries(values: &[Value]) -> Vec<Entry> {
    values
        .iter()
        .filter_map(|v| {
            let at: DateTime = text_of(v, "t").parse().ok()?;
            let secs = secs_of(v);
            Some(Entry {
                at,
                kind: text_of(v, "kind").to_owned(),
                label: text_of(v, "label").to_owned(),
                secs: (secs != 0.0).then_some(secs),
            })
        })
        .collect()
}

/// The four boxes at the top of the Today page.
#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub runs: usize,
    pub tools: usize,
    /// Seconds timers and stopwatches ran.
    pub timed: f64,
    /// "09:01" of the first and last line, none for an empty day.
    pub first: Option<String>,
    pub last: Option<String>,
}

fn clock_of(entry: &Value) -> String {
    text_of(entry, "t").chars().skip(11).take(5).collect()
}

pub fn summarise(values: &[Value]) -> Summary {
    let count = |kind: &str| values.iter().filter(|v| text_of(v, "kind") == kind).count();
    Summary {
        runs: count("run"),
        tools: count("tool"),
        timed: values.iter().filter(|v| text_of(v, "kind") == "timer").map(secs_of).sum(),
        first: values.first().map(clock_of),
        last: values.last().map(clock_of),
    }
}

/// The things used most that day (dashboard items and tools), most first, ties in the order they first happened.
pub fn most_used(values: &[Value], limit: usize) -> Vec<(String, usize)> {
    let mut counts: Vec<((String, String), usize)> = Vec::new();
    for v in values {
        let kind = text_of(v, "kind");
        if kind != "run" && kind != "tool" {
            continue;
        }
        let key = (kind.to_owned(), text_of(v, "label").to_owned());
        match counts.iter_mut().find(|(k, _)| *k == key) {
            Some((_, n)) => *n += 1,
            None => counts.push((key, 1)),
        }
    }
    counts.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    counts.into_iter().take(limit).map(|((_, label), n)| (label, n)).collect()
}

/// "Today", "Yesterday" or the weekday.
pub fn day_title(day: Date, today: Date) -> String {
    if day == today {
        "Today".to_owned()
    } else if today.checked_sub(1.day()).is_ok_and(|d| d == day) {
        "Yesterday".to_owned()
    } else {
        day.strftime("%A").to_string()
    }
}

/// "27 NOVEMBER 2026", without a leading zero on the day.
pub fn date_line(day: Date) -> String {
    day.strftime("%d %B %Y").to_string().to_uppercase().trim_start_matches('0').to_owned()
}

/// How long ago something happened, the short way: 5m ago, 3h ago, yesterday, 4d ago; empty when `iso` is not a time.
pub fn ago(iso: &str, now: DateTime) -> String {
    let Ok(then) = iso.parse::<DateTime>() else { return String::new() };
    let secs = now.since((jiff::Unit::Second, then)).map_or(0, |s| s.get_seconds()).max(0);
    match secs {
        0..3600 => format!("{}m ago", (secs / 60).max(1)),
        3600..86_400 => format!("{}h ago", secs / 3600),
        86_400..172_800 => "yesterday".to_owned(),
        _ => format!("{}d ago", secs / 86_400),
    }
}

/// The bars of the Usage page: uses on each of the last 30 days, oldest first, today last.
pub fn day_series(days: &Map<String, Value>, today: Date) -> Vec<(Date, i64)> {
    (0..30i64)
        .rev()
        .filter_map(|back| today.checked_sub(back.days()).ok())
        .map(|d| (d, days.get(&d.to_string()).map_or(0, count_of)))
        .collect()
}

fn count_of(value: &Value) -> i64 {
    match value {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)).unwrap_or(0),
        Value::String(s) => s.trim().parse().unwrap_or(0),
        Value::Bool(b) => i64::from(*b),
        _ => 0,
    }
}

/// One row of a Usage column.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub key: String,
    pub label: String,
    pub uses: i64,
    pub last: String,
    /// Uses as a part of the column's biggest, for the bar under the name.
    pub fraction: f32,
}

/// The most used 25 of one column (`items`, `hotkeys` or `tools`), most first; ties keep the file's order.
pub fn top_rows(bucket: Option<&Value>, limit: usize) -> Vec<Row> {
    let Some(Value::Object(map)) = bucket else { return Vec::new() };
    let mut rows: Vec<Row> = map
        .iter()
        .map(|(key, v)| Row {
            key: key.clone(),
            label: Some(text_of(v, "label")).filter(|l| !l.is_empty()).unwrap_or(key).to_owned(),
            uses: v.get("n").map_or(0, count_of),
            last: text_of(v, "last").to_owned(),
            fraction: 0.0,
        })
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.uses));
    rows.truncate(limit);
    let top = rows.iter().map(|r| r.uses).max().unwrap_or(1).max(1);
    for row in &mut rows {
        row.fraction = row.uses as f32 / top as f32;
    }
    rows
}

/// What a hotkey row says: the keys, then what the hotkey was for (the part of its label after "Module: ").
pub fn hotkey_label(pretty_keys: &str, label: &str) -> String {
    format!("{pretty_keys}  ·  {}", label.rsplit(": ").next().unwrap_or(label))
}

/// The total of the last 30 days' counts as the page's header says it ("123 USES COUNTED"), empty for none.
pub fn total_line(days: &Map<String, Value>) -> String {
    let total: i64 = days.values().map(count_of).sum();
    if total > 0 { format!("{total} USES COUNTED") } else { String::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn log() -> Vec<Value> {
        lum_fixtures::read_text("sample_home/data/activity.jsonl")
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }

    #[test]
    fn the_sample_log_is_summed_up_as_the_python_page_does() {
        let day: Vec<Value> =
            log().into_iter().filter(|v| text_of(v, "t").starts_with("2026-11-23")).collect();
        assert!(!day.is_empty());
        let summary = summarise(&day);
        assert_eq!((summary.runs, summary.tools), (1, 0));
        assert_eq!(summary.timed, 1500.0);
        assert_eq!(summary.first.as_deref(), Some("09:01"));
        assert_eq!(summary.last.as_deref(), Some("11:00"));
    }

    #[test]
    fn an_empty_day_has_no_first_and_last() {
        let summary = summarise(&[]);
        assert_eq!(
            (summary.runs, summary.tools, summary.timed, summary.first, summary.last),
            (0, 0, 0.0, None, None)
        );
    }

    #[test]
    fn the_timeline_skips_lines_it_can_not_read_and_keeps_unknown_kinds() {
        let values = vec![
            json!({"t": "2026-11-23T09:01:00", "kind": "run", "label": "Firefox"}),
            json!({"kind": "run", "label": "no time"}),
            json!({"t": "not a time", "kind": "tool", "label": "bad"}),
            json!({"t": "2026-11-23T11:00:00", "kind": "reminder", "label": "Drink some water"}),
            json!({"t": "2026-11-23T11:30:00", "kind": "timer", "label": "Focus", "secs": 90.5}),
            json!({"t": "2026-11-23T11:31:00", "kind": "timer", "label": "Zero", "secs": 0}),
        ];
        let timeline = entries(&values);
        let seen: Vec<_> = timeline.iter().map(|e| (e.label.as_str(), e.secs)).collect();
        assert_eq!(
            seen,
            [("Firefox", None), ("Drink some water", None), ("Focus", Some(90.5)), ("Zero", None)]
        );
        assert_eq!(kind_tag("reminder"), "REMINDER");
        assert_eq!(kind_tag("tool"), "TOOL");
    }

    #[test]
    fn the_most_used_list_counts_runs_and_tools_and_keeps_first_seen_order_for_ties() {
        let line =
            |kind: &str, label: &str| json!({"t": "2026-11-23T09:00:00", "kind": kind, "label": label});
        let values = vec![
            line("run", "Firefox"),
            line("tool", "Calculator"),
            line("run", "Terminal"),
            line("run", "Terminal"),
            line("timer", "Focus"),
            line("run", "Firefox"),
        ];
        assert_eq!(
            most_used(&values, 8),
            vec![("Firefox".to_owned(), 2), ("Terminal".to_owned(), 2), ("Calculator".to_owned(), 1)]
        );
        assert_eq!(most_used(&values, 1).len(), 1);
    }

    #[test]
    fn the_title_and_date_line_read_like_the_python_page() {
        let today: Date = "2026-11-27".parse().expect("a date");
        assert_eq!(day_title(today, today), "Today");
        assert_eq!(day_title("2026-11-26".parse().expect("a date"), today), "Yesterday");
        assert_eq!(day_title("2026-11-23".parse().expect("a date"), today), "Monday");
        assert_eq!(date_line(today), "27 NOVEMBER 2026");
        assert_eq!(date_line("2026-11-05".parse().expect("a date")), "5 NOVEMBER 2026");
    }

    #[test]
    fn how_long_ago_is_short() {
        let now: DateTime = "2026-11-27T12:00:00".parse().expect("a time");
        assert_eq!(ago("2026-11-27T11:59:30", now), "1m ago");
        assert_eq!(ago("2026-11-27T11:30:00", now), "30m ago");
        assert_eq!(ago("2026-11-27T09:00:00", now), "3h ago");
        assert_eq!(ago("2026-11-26T13:00:00", now), "23h ago");
        assert_eq!(ago("2026-11-26T11:00:00", now), "yesterday");
        assert_eq!(ago("2026-11-23T12:00:00", now), "4d ago");
        assert_eq!(ago("2026-11-27T13:00:00", now), "1m ago", "a clock that ran ahead is never negative");
        assert_eq!(ago("", now), "");
        assert_eq!(ago("yesterday", now), "");
    }

    #[test]
    fn thirty_days_of_bars_end_today() {
        let today: Date = "2026-11-27".parse().expect("a date");
        let counts = json!({"2026-11-22": 30, "2026-11-23": "25", "2025-01-01": 9});
        let series = day_series(counts.as_object().expect("object"), today);
        assert_eq!(series.len(), 30);
        assert_eq!(series[29], (today, 0));
        assert_eq!(series[0].0.to_string(), "2026-10-29");
        assert!(series.contains(&("2026-11-22".parse().expect("a date"), 30)));
        assert!(series.contains(&("2026-11-23".parse().expect("a date"), 25)));
        assert_eq!(total_line(counts.as_object().expect("object")), "64 USES COUNTED");
        assert_eq!(total_line(&Map::new()), "");
    }

    #[test]
    fn a_usage_column_is_sorted_cut_and_scaled() {
        let bucket = json!({
            "a": {"label": "Alpha", "n": 2, "last": "2026-11-23T09:00:00"},
            "b": {"label": "", "n": 10},
            "c": {"label": "Gamma", "n": 10, "last": "x"},
            "d": {"label": "Delta", "n": "5"}
        });
        let rows = top_rows(Some(&bucket), 3);
        let seen: Vec<_> = rows.iter().map(|r| (r.label.as_str(), r.uses)).collect();
        assert_eq!(seen, [("b", 10), ("Gamma", 10), ("Delta", 5)], "an unnamed one shows its key");
        assert_eq!(rows[0].fraction, 1.0);
        assert_eq!(rows[2].fraction, 0.5);
        assert!(top_rows(None, 25).is_empty());
        assert!(top_rows(Some(&json!([1])), 25).is_empty());
    }

    #[test]
    fn the_sample_usage_file_gives_the_rows_the_page_shows() {
        let usage = lum_fixtures::read_json("sample_home/data/usage.json");
        let items = top_rows(usage.get("items"), 25);
        assert_eq!((items[0].label.as_str(), items[0].uses), ("Firefox", 12));
        assert_eq!(
            hotkey_label("Ctrl+Shift+Space", "Open Luminescent"),
            "Ctrl+Shift+Space  ·  Open Luminescent"
        );
        assert_eq!(hotkey_label("Ctrl+B", "Snippets: open snippets"), "Ctrl+B  ·  open snippets");
    }
}
