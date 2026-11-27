//! Reading times people type: `5m`, `1h30`, `1h 30m`, `90s`, `1:30:00`, `2.5h`, `17:30`, `5:30pm`. Used by the
//! timer ("type the time straight into the panel"), quick reminders in the command palette and the calendar.

use std::sync::LazyLock;

use jiff::ToSpan;
use jiff::civil::DateTime;
use regex::Regex;

const UNITS: &[(&str, u64)] = &[
    ("s", 1),
    ("sec", 1),
    ("secs", 1),
    ("second", 1),
    ("seconds", 1),
    ("m", 60),
    ("min", 60),
    ("mins", 60),
    ("minute", 60),
    ("minutes", 60),
    ("h", 3600),
    ("hr", 3600),
    ("hrs", 3600),
    ("hour", 3600),
    ("hours", 3600),
    ("d", 86400),
    ("day", 86400),
    ("days", 86400),
    ("w", 604800),
    ("wk", 604800),
    ("week", 604800),
    ("weeks", 604800),
];

fn unit_seconds(unit: &str) -> Option<u64> {
    UNITS.iter().find(|(name, _)| *name == unit).map(|(_, secs)| *secs)
}

/// What a bare number after a unit means: "1h30" is 1 h 30 m.
fn next_smaller(secs: u64) -> u64 {
    match secs {
        3600 => 60,
        60 => 1,
        86400 => 3600,
        604800 => 86400,
        _ => 1,
    }
}

/// Seconds in a duration like `1h30`, `1h 30m`, `90s`, `2.5h`, `1:30:00` (h:m:s) or `25:00` (m:s). A bare number
/// counts in `bare_unit` seconds (minutes by default). `None` when it is not a duration.
pub fn parse_duration(text: &str, bare_unit: u64) -> Option<f64> {
    static CLOCKISH: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\d+:\d{1,2}(:\d{1,2})?$").expect("valid"));
    static PART: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)(\d+(?:[.,]\d+)?)\s*([a-z]+)?").expect("valid"));
    let t = text.trim().to_lowercase();
    if t.is_empty() {
        return None;
    }
    if CLOCKISH.is_match(&t) {
        let parts: Vec<f64> = t.split(':').filter_map(|p| p.parse().ok()).collect();
        return Some(match parts.as_slice() {
            [h, m, s] => h * 3600.0 + m * 60.0 + s,
            [m, s] => m * 60.0 + s,
            _ => return None,
        });
    }
    let t = t.replace(" and ", " ");
    let (mut total, mut pos, mut last_unit) = (0.0, 0, 0);
    for m in PART.captures_iter(&t) {
        let whole = m.get(0)?;
        if !t[pos..whole.start()].trim_matches([' ', ',']).is_empty() {
            return None;
        }
        let number: f64 = m[1].replace(',', ".").parse().ok()?;
        let secs = match m.get(2) {
            Some(unit) => unit_seconds(unit.as_str())?,
            None if last_unit != 0 => next_smaller(last_unit),
            None => bare_unit,
        };
        total += number * secs as f64;
        last_unit = secs;
        pos = whole.end();
    }
    if pos == 0 || !t[pos..].trim().is_empty() {
        return None;
    }
    Some(total)
}

/// The next time a clock time comes round: `17:30`, `5:30pm`, `9am`, `9` (today, or tomorrow when it has gone).
pub fn parse_clock(text: &str, now: DateTime) -> Option<DateTime> {
    static CLOCK: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(\d{1,2})(?:[:.](\d{2}))?\s*(am|pm)?$").expect("valid"));
    let lower = text.trim().to_lowercase();
    let m = CLOCK.captures(&lower)?;
    let mut hour: i8 = m[1].parse().ok()?;
    let minute: i8 = m.get(2).map_or(Some(0), |g| g.as_str().parse().ok())?;
    if let Some(suffix) = m.get(3) {
        if !(1..=12).contains(&hour) {
            return None;
        }
        hour = (hour % 12) + if suffix.as_str() == "pm" { 12 } else { 0 };
    }
    if hour > 23 || minute > 59 {
        return None;
    }
    let due = now.date().at(hour, minute, 0, 0);
    Some(if due > now { due } else { due.checked_add(1.day()).ok()? })
}

/// `90` becomes `1m 30s` and `5400` becomes `1h 30m`.
pub fn fmt_secs(secs: f64) -> String {
    let secs = secs.round_ties_even() as i64;
    // Python's divmod rounds down, so a negative time reads oddly (-5 is -1h 59m); kept as it was
    let (h, rest) = (secs.div_euclid(3600), secs.rem_euclid(3600));
    let (m, s) = (rest / 60, rest % 60);
    if h != 0 {
        if m > 0 { format!("{h}h {m:02}m") } else { format!("{h}h") }
    } else if m > 0 {
        if s > 0 { format!("{m}m {s:02}s") } else { format!("{m}m") }
    } else {
        format!("{s}s")
    }
}
