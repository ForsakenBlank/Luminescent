//! Keeping the base running when one part fails, as `core/guard.py`.
//!
//! * [`safely`] runs a step and turns a panic into a log line instead of taking the app down.
//! * [`Faults`] counts failures per part (a module, the overlay...) so a part that keeps failing can be
//!   switched off for the session.
//! * [`StartGuard`] notices a start that never got going; two in a row and the next start is safe mode.
//! * [`install_panic_hook`] writes every panic to `data/crash.log` as well as the log.

use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::panic::{self, AssertUnwindSafe, PanicHookInfo};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::pyjson::{self, Style};

/// Runs `step`; a panic is logged as "<what> failed" and gives `None`. The panic hook has already logged the
/// panic itself with its backtrace.
pub fn safely<T>(what: &str, step: impl FnOnce() -> T) -> Option<T> {
    match panic::catch_unwind(AssertUnwindSafe(step)) {
        Ok(value) => Some(value),
        Err(payload) => {
            tracing::error!("{what} failed: {}", panic_message(payload.as_ref()));
            None
        }
    }
}

pub fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "a panic without a message".to_owned()
    }
}

/// Errors per part within a sliding window. [`Faults::add`] gives true once a part has failed too often.
#[derive(Debug)]
pub struct Faults {
    limit: usize,
    window: Duration,
    seen: HashMap<String, Vec<Instant>>,
    tripped: HashSet<String>,
}

impl Default for Faults {
    fn default() -> Faults {
        Faults::new(6, Duration::from_secs(60))
    }
}

impl Faults {
    pub fn new(limit: usize, window: Duration) -> Faults {
        Faults { limit, window, seen: HashMap::new(), tripped: HashSet::new() }
    }

    pub fn add(&mut self, part: &str) -> bool {
        self.add_at(part, Instant::now())
    }

    pub fn add_at(&mut self, part: &str, now: Instant) -> bool {
        if part.is_empty() || self.tripped.contains(part) {
            return false;
        }
        let hits = self.seen.entry(part.to_owned()).or_default();
        hits.retain(|t| now.saturating_duration_since(*t) < self.window);
        hits.push(now);
        if hits.len() >= self.limit {
            self.tripped.insert(part.to_owned());
            return true;
        }
        false
    }

    pub fn is_tripped(&self, part: &str) -> bool {
        self.tripped.contains(part)
    }

    pub fn forgive(&mut self, part: &str) {
        self.seen.remove(part);
        self.tripped.remove(part);
    }
}

/// A small file that says "a start is under way". It is removed once the app has run for a while; if it is
/// still there at the next start, that start crashed or hung. Two such starts in a row and the next one runs in
/// safe mode (no modules, no page boxes, automations paused) so you can still get in and fix things.
#[derive(Debug)]
pub struct StartGuard {
    path: PathBuf,
    pub failed_starts: i64,
    forced: bool,
}

impl StartGuard {
    /// Notes a new start. `forced` is safe mode asked for (`--safe-mode`, `LUMINESCENT_SAFE_MODE`).
    pub fn begin(path: &Path, now_ts: f64, forced: bool) -> StartGuard {
        let failed_starts = read_failed(path).map_or(0, |n| n + 1);
        let body = json!({"failed": failed_starts, "at": now_ts});
        if let Err(e) = fs::write(path, pyjson::to_string(&body, Style::COMPACT)) {
            tracing::warn!("could not write {}: {e}", path.display());
        }
        StartGuard { path: path.to_owned(), failed_starts, forced }
    }

    pub fn safe_mode(&self) -> bool {
        self.failed_starts >= 2 || self.forced
    }

    /// The start went fine: a crash later on is not a start-up problem.
    pub fn ok(&self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// `int(json.load(f).get("failed", 0))`, or `None` when Python's version would have raised.
fn read_failed(path: &Path) -> Option<i64> {
    let text = fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    match value.as_object()?.get("failed") {
        None => Some(0),
        Some(Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Some(Value::String(s)) => s.trim().parse().ok(),
        Some(Value::Bool(b)) => Some(i64::from(*b)),
        Some(_) => None,
    }
}

/// Appends the start line the Python app writes to crash.log on every start.
pub fn mark_start_in_crash_log(path: &Path, version: &str, started: &str) {
    let line = format!("\n=== {version} started {started} ===\n");
    let written = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| f.write_all(line.as_bytes()));
    if let Err(e) = written {
        tracing::warn!("could not write {}: {e}", path.display());
    }
}

/// Every panic goes to the log (as "unhandled", like the Python excepthook) and to crash.log with a backtrace.
/// The default hook still runs too, so a panic shows on the terminal when there is one.
pub fn install_panic_hook(crash_log: PathBuf) {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let text = describe_panic(info);
        tracing::error!(target: "unhandled", "{text}");
        // if crash.log can not be written, the log line above is all there is
        let _ = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&crash_log)
            .and_then(|mut f| f.write_all(format!("{text}\n").as_bytes()));
        previous(info);
    }));
}

fn describe_panic(info: &PanicHookInfo<'_>) -> String {
    let thread = std::thread::current();
    let name = thread.name().unwrap_or("unnamed");
    let place = info.location().map(|l| format!(" at {}:{}", l.file(), l.line())).unwrap_or_default();
    let backtrace = std::backtrace::Backtrace::force_capture();
    format!("panic in thread '{name}'{place}: {}\n{backtrace}", panic_message(info.payload()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn six_faults_in_a_minute_trip_a_part() {
        let mut faults = Faults::default();
        let t0 = Instant::now();
        for i in 0..5 {
            assert!(!faults.add_at("module:notes", t0 + Duration::from_secs(i)));
        }
        assert!(faults.add_at("module:notes", t0 + Duration::from_secs(5)));
        assert!(faults.is_tripped("module:notes"));
        assert!(!faults.add_at("module:notes", t0 + Duration::from_secs(6)), "only trips once");
        faults.forgive("module:notes");
        assert!(!faults.is_tripped("module:notes"));
        assert!(!faults.add_at("", t0), "errors outside any part are not counted");
    }

    #[test]
    fn old_faults_fall_out_of_the_window() {
        let mut faults = Faults::default();
        let t0 = Instant::now();
        for i in 0..10 {
            assert!(
                !faults.add_at("overlay", t0 + Duration::from_secs(i * 15)),
                "one every 15 s never trips"
            );
        }
    }

    #[test]
    fn safely_catches_panics() {
        assert_eq!(safely("adding", || 1 + 1), Some(2));
        assert_eq!(safely("exploding", || -> i32 { panic!("boom") }), None);
    }

    #[test]
    fn two_failed_starts_mean_safe_mode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("starting.json");
        let first = StartGuard::begin(&path, 1795444200.0, false);
        assert_eq!(first.failed_starts, 0);
        assert_eq!(fs::read_to_string(&path).unwrap(), r#"{"failed": 0, "at": 1795444200.0}"#);
        // that start never called ok(): it crashed
        let second = StartGuard::begin(&path, 1795444201.5, false);
        assert_eq!(second.failed_starts, 1);
        assert!(!second.safe_mode());
        let third = StartGuard::begin(&path, 1795444203.0, false);
        assert!(third.safe_mode());
        third.ok();
        assert!(!path.exists());
        let fine = StartGuard::begin(&path, 1795444210.0, false);
        assert!(!fine.safe_mode());
        assert!(StartGuard::begin(&path, 1795444210.0, true).safe_mode(), "asked for");
    }

    #[test]
    fn reads_the_python_start_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("starting.json");
        fs::copy(lum_fixtures::path("sample_home/data/starting.json"), &path).unwrap();
        assert_eq!(StartGuard::begin(&path, 0.0, false).failed_starts, 2);
        fs::write(&path, "garbage").unwrap();
        assert_eq!(StartGuard::begin(&path, 0.0, false).failed_starts, 0);
    }
}
