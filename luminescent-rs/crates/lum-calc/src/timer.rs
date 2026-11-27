//! What a timer or stopwatch is, apart from how it is drawn: elapsed time measured with a monotonic clock, laps,
//! what happens when it runs out, and the state kept in `timers.json` so a restart (or a reboot) carries on
//! counting. The wall clock is only used to work out how long the app was closed.

use std::time::{Duration, Instant};

use jiff::Timestamp;
use jiff::civil::DateTime;
use serde_json::{Map, Value, json};

use crate::timeparse::{parse_clock, parse_duration};

/// "12:48" and ".36": the big display, with hours only when there are some (`1:02:48`).
pub fn fmt_clock(ms: f64, show_ms: bool) -> (String, String) {
    let ms = ms.max(0.0) as u64;
    let total = ms / 1000;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    let main = if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m:02}:{s:02}") };
    (main, if show_ms { format!(".{:02}", (ms % 1000) / 10) } else { String::new() })
}

/// `01:02:48.36`, for the tooltip and the lap list.
pub fn fmt_ms(ms: f64, show_ms: bool) -> String {
    let ms = ms.max(0.0) as u64;
    let total = ms / 1000;
    let text = format!("{:02}:{:02}:{:02}", total / 3600, (total % 3600) / 60, total % 60);
    if show_ms { format!("{text}.{:02}", (ms % 1000) / 10) } else { text }
}

/// What was typed into a panel: the time in milliseconds and a label. `5m tea` is 5 minutes labelled "tea",
/// `at 17:30` runs until then, and nothing at all is `(0, "")`, a stopwatch. `None` when no time can be read.
pub fn parse_entry(text: &str, now: DateTime) -> Option<(f64, String)> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let Some(first) = words.first() else { return Some((0.0, String::new())) };
    if matches!(first.to_lowercase().as_str(), "at" | "until") && words.len() > 1 {
        let when = parse_clock(words[1], now)?;
        let secs = now.until(when).ok()?.total(jiff::Unit::Second).ok()?;
        return Some((secs.max(1.0) * 1000.0, words[2..].join(" ")));
    }
    for k in (1..=words.len()).rev() {
        if let Some(secs) = parse_duration(&words[..k].join(" "), 60)
            && secs > 0.0
        {
            return Some((secs * 1000.0, words[k..].join(" ")));
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Timer,
    Stopwatch,
}

impl Mode {
    pub fn key(self) -> &'static str {
        match self {
            Mode::Timer => "timer",
            Mode::Stopwatch => "stopwatch",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Mode::Timer => "Timer",
            Mode::Stopwatch => "Stopwatch",
        }
    }
}

/// How long it ran for the activity log, once it has run for at least this many seconds.
const LOG_AFTER_SECS: f64 = 5.0;
/// A timer that ran out while the app was closed says so this long after the start, not at once.
const LATE_FINISH: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone)]
pub struct TimerState {
    pub mode: Mode,
    pub show_ms: bool,
    pub duration_ms: i64,
    pub label: String,
    pub laps: Vec<f64>,
    pub finished: bool,
    /// Typed time not asked for yet: a panel opened by the hotkey.
    pub fresh: bool,
    accum_ms: f64,
    started: Option<Instant>,
    finish_at: Option<Instant>,
    /// Every key the file had that is not ours, and the order they came in.
    extra: Map<String, Value>,
}

/// Why a state stopped counting: time to put in the activity log.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Logged(pub Option<f64>);

impl TimerState {
    pub fn new(mode: Mode, duration_ms: i64, label: &str) -> TimerState {
        let mut state = TimerState::blank();
        state.mode = mode;
        state.duration_ms = duration_ms;
        state.label = label.to_owned();
        state
    }

    /// A panel opened by the hotkey, waiting for its time.
    pub fn asking() -> TimerState {
        TimerState { fresh: true, ..TimerState::blank() }
    }

    fn blank() -> TimerState {
        TimerState {
            mode: Mode::Stopwatch,
            show_ms: false,
            duration_ms: 0,
            label: String::new(),
            laps: Vec::new(),
            finished: false,
            fresh: false,
            accum_ms: 0.0,
            started: None,
            finish_at: None,
            extra: Map::new(),
        }
    }

    pub fn is_running(&self) -> bool {
        self.started.is_some()
    }

    pub fn elapsed_ms(&self, now: Instant) -> f64 {
        match self.started {
            Some(at) => self.accum_ms + now.saturating_duration_since(at).as_secs_f64() * 1000.0,
            None => self.accum_ms,
        }
    }

    /// What the big display shows: what is left for a timer, what has passed for a stopwatch.
    pub fn display_ms(&self, now: Instant) -> f64 {
        match self.mode {
            Mode::Timer => (self.duration_ms as f64 - self.elapsed_ms(now)).max(0.0),
            Mode::Stopwatch => self.elapsed_ms(now),
        }
    }

    /// How far a countdown has got, 0 to 1.
    pub fn progress(&self, now: Instant) -> f64 {
        (self.elapsed_ms(now) / self.duration_ms.max(1) as f64).clamp(0.0, 1.0)
    }

    pub fn name(&self) -> String {
        if self.label.is_empty() { self.mode.title().to_owned() } else { self.label.clone() }
    }

    fn log(&self, now: Instant) -> Logged {
        let secs = self.started.map_or(0.0, |at| now.saturating_duration_since(at).as_secs_f64());
        Logged((secs >= LOG_AFTER_SECS).then_some(secs))
    }

    /// Starts, or pauses. A finished timer starts again from its full time.
    pub fn play_pause(&mut self, now: Instant) -> Logged {
        if self.is_running() {
            let logged = self.log(now);
            self.accum_ms = self.elapsed_ms(now);
            self.started = None;
            return logged;
        }
        if self.mode == Mode::Timer && (self.finished || self.duration_ms <= 0) {
            self.reset(now);
            if self.duration_ms <= 0 {
                return Logged(None);
            }
        }
        self.started = Some(now);
        Logged(None)
    }

    pub fn reset(&mut self, now: Instant) -> Logged {
        let logged = if self.is_running() { self.log(now) } else { Logged(None) };
        self.started = None;
        self.finish_at = None;
        self.finished = false;
        self.accum_ms = 0.0;
        self.laps.clear();
        logged
    }

    pub fn switch_mode(&mut self, now: Instant) -> Logged {
        self.mode = if self.mode == Mode::Stopwatch { Mode::Timer } else { Mode::Stopwatch };
        self.reset(now)
    }

    /// Takes a typed time: sets the mode, duration and label, and starts counting. `None` when it is an empty
    /// entry on a timer that already exists (nothing to change); otherwise what the old run adds to the log.
    pub fn set_time(&mut self, ms: f64, label: &str, now: Instant) -> Option<Logged> {
        if ms <= 0.0 && !self.fresh {
            return None;
        }
        self.fresh = false;
        self.extra.remove("ask");
        if !label.is_empty() {
            self.label = label.to_owned();
        }
        self.mode = if ms > 0.0 { Mode::Timer } else { Mode::Stopwatch };
        self.duration_ms = ms as i64;
        let logged = self.reset(now);
        self.play_pause(now);
        Some(logged)
    }

    pub fn lap(&mut self, now: Instant) -> bool {
        if self.mode != Mode::Stopwatch || (!self.is_running() && self.elapsed_ms(now) <= 0.0) {
            return false;
        }
        self.laps.push(self.elapsed_ms(now));
        true
    }

    /// Has a countdown run out? Gives back the time to log and what to say when it has (`None`: still going).
    pub fn tick(&mut self, now: Instant) -> Option<Finish> {
        if let Some(at) = self.finish_at
            && now >= at
        {
            self.finish_at = None;
            return Some(self.finish(now));
        }
        let ran_out =
            self.mode == Mode::Timer && self.is_running() && self.elapsed_ms(now) >= self.duration_ms as f64;
        ran_out.then(|| self.finish(now))
    }

    /// The next moment `tick` has something to do for a state that is not counting (a late finish).
    pub fn wake_at(&self) -> Option<Instant> {
        self.finish_at
    }

    fn finish(&mut self, now: Instant) -> Finish {
        let logged = self.log(now);
        self.started = None;
        self.finished = true;
        self.accum_ms = self.duration_ms as f64;
        let text = if self.label.is_empty() {
            fmt_clock(self.duration_ms as f64, false).0
        } else {
            self.label.clone()
        };
        Finish { text, logged }
    }

    /// The line for the tray's tooltip: "Focus: 12:30 left", "Timer: DONE", "Stopwatch: 00:01:05".
    pub fn status_line(&self, now: Instant) -> String {
        let name = self.name();
        match self.mode {
            Mode::Timer if self.finished => format!("{name}: DONE"),
            Mode::Timer => format!(
                "{name}: {} left",
                fmt_ms((self.duration_ms as f64 - self.elapsed_ms(now)).max(0.0), false)
            ),
            Mode::Stopwatch => format!("{name}: {}", fmt_ms(self.elapsed_ms(now), false)),
        }
    }

    /// Reads a state from `timers.json`. A timer that was running carries on from where the wall clock says it
    /// got to, and one that ran out while the app was closed finishes a moment after the start.
    pub fn from_json(cfg: &Map<String, Value>, wall_now: f64, now: Instant) -> TimerState {
        let mut state = TimerState::blank();
        state.extra = cfg.clone();
        state.mode = if cfg.get("mode").and_then(Value::as_str) == Some("timer") {
            Mode::Timer
        } else {
            Mode::Stopwatch
        };
        state.show_ms = cfg.get("show_ms").and_then(Value::as_bool).unwrap_or(false);
        state.duration_ms = cfg.get("duration_ms").and_then(Value::as_f64).unwrap_or(0.0) as i64;
        state.label = cfg.get("label").and_then(Value::as_str).unwrap_or_default().to_owned();
        state.laps = cfg
            .get("laps")
            .and_then(Value::as_array)
            .map(|l| l.iter().filter_map(Value::as_f64).collect())
            .unwrap_or_default();
        state.fresh = cfg.get("ask").and_then(Value::as_bool).unwrap_or(false);
        let mut elapsed = cfg.get("elapsed_ms").and_then(Value::as_f64).unwrap_or(0.0).max(0.0);
        let running = cfg.get("running").and_then(Value::as_bool).unwrap_or(false);
        if running && let Some(saved) = cfg.get("saved_at").and_then(Value::as_f64) {
            elapsed += (wall_now - saved).max(0.0) * 1000.0;
        }
        state.accum_ms = elapsed;
        if cfg.get("finished").and_then(Value::as_bool).unwrap_or(false) && state.mode == Mode::Timer {
            state.finished = true;
            state.accum_ms = state.duration_ms as f64;
        } else if running {
            if state.mode == Mode::Timer && elapsed >= state.duration_ms as f64 {
                state.accum_ms = state.duration_ms as f64;
                state.finish_at = Some(now + LATE_FINISH);
            } else {
                state.started = Some(now);
            }
        }
        state
    }

    /// What goes into `timers.json`: the keys the file had, then ours.
    pub fn to_json(&self, geometry: [i64; 4], wall_now: f64, now: Instant) -> Value {
        let mut out = self.extra.clone();
        if !self.label.is_empty() {
            out.insert("label".to_owned(), json!(self.label));
        }
        let mut put = |key: &str, value: Value| {
            out.insert(key.to_owned(), value);
        };
        put("mode", json!(self.mode.key()));
        put("show_ms", json!(self.show_ms));
        put("duration_ms", json!(self.duration_ms));
        for (key, v) in ["x", "y", "w", "h"].into_iter().zip(geometry) {
            put(key, json!(v));
        }
        put("laps", json!(self.laps));
        put("elapsed_ms", json!(self.elapsed_ms(now)));
        put("running", json!(self.is_running()));
        put("finished", json!(self.finished));
        if self.is_running() {
            // the wall clock, not the monotonic one: it has to mean the same thing after a reboot
            put("saved_at", json!(wall_now));
        }
        Value::Object(out)
    }
}

/// A countdown reached zero.
#[derive(Debug, Clone, PartialEq)]
pub struct Finish {
    /// The label, or the time it was set for: what the event says.
    pub text: String,
    pub logged: Logged,
}

/// Seconds since 1970, as `time.time()` gives them.
pub fn wall_seconds() -> f64 {
    Timestamp::now().as_millisecond() as f64 / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: u64, base: Instant) -> Instant {
        base + Duration::from_secs(secs)
    }

    #[test]
    fn the_big_display_shows_hours_only_when_there_are_some() {
        assert_eq!(fmt_clock(65_000.0, false), ("01:05".into(), String::new()));
        assert_eq!(fmt_clock(3_725_360.0, true), ("1:02:05".into(), ".36".into()));
        assert_eq!(fmt_ms(3_725_360.0, true), "01:02:05.36");
    }

    #[test]
    fn typed_entries_are_a_time_and_a_label() {
        let now = DateTime::constant(2026, 11, 23, 14, 30, 0, 0);
        assert_eq!(parse_entry("5m tea", now), Some((300_000.0, "tea".into())));
        assert_eq!(parse_entry("25m focus time", now), Some((1_500_000.0, "focus time".into())));
        assert_eq!(parse_entry("", now), Some((0.0, String::new())));
        assert_eq!(parse_entry("at 17:30 call mum", now), Some((10_800_000.0, "call mum".into())));
        assert_eq!(parse_entry("tea", now), None);
    }

    #[test]
    fn a_countdown_runs_out_once_and_starts_again_from_the_top() {
        let t0 = Instant::now();
        let mut timer = TimerState::new(Mode::Timer, 10_000, "Tea");
        timer.play_pause(t0);
        assert!(timer.tick(at(9, t0)).is_none());
        let done = timer.tick(at(10, t0)).unwrap();
        assert_eq!(done.text, "Tea");
        assert_eq!(done.logged, Logged(Some(10.0)));
        assert!(timer.finished && !timer.is_running());
        assert!(timer.tick(at(11, t0)).is_none());
        timer.play_pause(at(12, t0));
        assert!(timer.is_running() && !timer.finished);
        assert_eq!(timer.elapsed_ms(at(14, t0)), 2000.0);
    }

    #[test]
    fn pausing_keeps_the_time_and_short_runs_are_not_logged() {
        let t0 = Instant::now();
        let mut watch = TimerState::new(Mode::Stopwatch, 0, "");
        watch.play_pause(t0);
        assert_eq!(watch.play_pause(at(3, t0)), Logged(None));
        assert_eq!(watch.elapsed_ms(at(100, t0)), 3000.0);
        watch.play_pause(at(100, t0));
        assert_eq!(watch.play_pause(at(110, t0)), Logged(Some(10.0)));
        assert_eq!(watch.elapsed_ms(at(500, t0)), 13_000.0);
    }

    #[test]
    fn laps_belong_to_a_stopwatch_that_has_time() {
        let t0 = Instant::now();
        let mut watch = TimerState::new(Mode::Stopwatch, 0, "");
        assert!(!watch.lap(t0));
        watch.play_pause(t0);
        assert!(watch.lap(at(4, t0)));
        assert_eq!(watch.laps, vec![4000.0]);
        let mut timer = TimerState::new(Mode::Timer, 5000, "");
        timer.play_pause(t0);
        assert!(!timer.lap(at(1, t0)));
    }

    #[test]
    fn a_running_timer_carries_on_after_a_restart_and_a_late_one_finishes() {
        let cfg = |elapsed: f64, saved: f64| -> Map<String, Value> {
            serde_json::from_value(json!({"mode": "timer", "duration_ms": 1_500_000, "elapsed_ms": elapsed,
                "running": true, "finished": false, "saved_at": saved}))
            .unwrap()
        };
        let t0 = Instant::now();
        let state = TimerState::from_json(&cfg(300_000.0, 1000.0), 1060.0, t0);
        assert!(state.is_running());
        assert_eq!(state.elapsed_ms(t0), 360_000.0);
        let late = TimerState::from_json(&cfg(300_000.0, 1000.0), 5000.0, t0);
        assert!(!late.is_running());
        assert_eq!(late.wake_at(), Some(t0 + LATE_FINISH));
        let mut late = late;
        assert!(late.tick(t0).is_none());
        let done = late.tick(t0 + LATE_FINISH).unwrap();
        assert_eq!(done.text, "25:00");
        assert!(late.finished);
    }

    #[test]
    fn the_file_keeps_unknown_keys_and_writes_ours_in_place() {
        let cfg: Map<String, Value> = serde_json::from_value(json!({
            "label": "Focus", "mode": "timer", "show_ms": false, "duration_ms": 1_500_000, "x": 1, "y": 2,
            "w": 3, "h": 4, "laps": [], "elapsed_ms": 0.0, "running": false, "finished": false, "colour": "red"
        }))
        .unwrap();
        let t0 = Instant::now();
        let state = TimerState::from_json(&cfg, 0.0, t0);
        let out = state.to_json([1, 2, 3, 4], 0.0, t0);
        assert_eq!(out, Value::Object(cfg));
        let keys: Vec<_> = out.as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys.first().map(String::as_str), Some("label"));
        assert_eq!(keys.last().map(String::as_str), Some("colour"));
    }
}
