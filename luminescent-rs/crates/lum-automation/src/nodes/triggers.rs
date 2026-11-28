//! Triggers: nodes that start pulses by themselves.

use std::path::PathBuf;

use jiff::ToSpan;
use jiff::civil::DateTime;
use serde_json::json;

use crate::engine::{Cx, Look, NodeRuntime, Status};
use crate::text::{self, days, hm, left};
use crate::work::{JobMsg, Level, Readings, Watch};

const TICK: u64 = 1;
const STRAIGHT_AWAY: u64 = 2;

/// Every…: fires again and again; its input says how many times it fired.
#[derive(Default)]
pub struct Interval {
    timer: Option<u64>,
    count: u64,
    t0: f64,
    period: f64,
}

impl Interval {
    fn go(&mut self, cx: &mut Cx<'_>) {
        // Python's QTimer took whole milliseconds, never under 100.
        let ms = (cx.num("seconds", "", 5.0) * 1000.0).trunc().max(100.0);
        self.period = ms / 1000.0;
        self.t0 = cx.mono();
        self.timer = Some(cx.after(self.period, TICK));
        if cx.flag("fire_now") {
            cx.after(0.0, STRAIGHT_AWAY);
        }
    }

    fn tick(&mut self, cx: &mut Cx<'_>) {
        self.count += 1;
        self.t0 = cx.mono();
        cx.fire("tick", &self.count.to_string());
    }
}

impl NodeRuntime for Interval {
    fn start(&mut self, cx: &mut Cx<'_>) {
        if cx.flag("autostart") {
            self.go(cx);
        } else {
            cx.set_status("stopped", None);
        }
    }

    fn receive(&mut self, port: &str, _input: &str, cx: &mut Cx<'_>) {
        match port {
            "start" if self.timer.is_none() => self.go(cx),
            "stop" => {
                cx.cancel_all();
                self.timer = None;
                cx.set_status("stopped", None);
            }
            _ => {}
        }
    }

    fn timer(&mut self, token: u64, cx: &mut Cx<'_>) {
        if token == TICK {
            self.timer = Some(cx.after(self.period, TICK));
        }
        self.tick(cx);
    }

    fn value(&self, _port: &str, _input: &str, _cx: &mut Cx<'_>) -> String {
        self.count.to_string()
    }

    fn info(&self, _status: &Status, look: &Look<'_>) -> Status {
        if self.timer.is_none() {
            return Status { text: "stopped".into(), progress: None };
        }
        let left = (self.period - (look.mono - self.t0)).max(0.0);
        let text = if left >= 1.0 { format!("next in {left:.0} s") } else { "now".into() };
        Status { text, progress: Some((1.0 - left / self.period.max(0.1)) as f32) }
    }
}

/// The longest a time trigger sleeps before it looks at the clock again, so a changed clock is noticed (D47).
const AT_MOST: f64 = 60.0;

fn seconds_between(from: DateTime, to: DateTime) -> f64 {
    from.duration_until(to).as_secs_f64()
}

/// Monday is 0, as Python's `weekday()`.
fn weekday(t: DateTime) -> u8 {
    (t.weekday().to_monday_zero_offset()) as u8
}

/// That day at h:m (`hm` only gives back a real time of day).
fn at(day: DateTime, (h, m): (u8, u8)) -> DateTime {
    day.date().at(h as i8, m as i8, 0, 0)
}

/// At a time of day: fires at that time on those days. It sleeps until the next time it could fire (at most a
/// minute) rather than looking every second as Python did.
#[derive(Default)]
pub struct AtTime {
    last: String,
}

impl AtTime {
    /// The next moment it fires after `now` (Python's `_next`, which also counts the minute under way).
    fn next(fields_time: &str, fields_days: &str, now: DateTime, grace: f64) -> Option<DateTime> {
        let hm = hm(fields_time)?;
        let wanted = days(fields_days);
        (0..8).find_map(|d| {
            let day = now.checked_add(d.days()).ok()?;
            let t = at(day, hm);
            (wanted.contains(&weekday(t)) && seconds_between(now, t) > -grace).then_some(t)
        })
    }

    fn tick(&mut self, cx: &mut Cx<'_>) {
        let now = cx.local();
        let time = cx.raw_str("time");
        if let Some((h, m)) = hm(&time) {
            let key = now.strftime("%Y-%m-%d %H:%M").to_string();
            let here = (now.hour(), now.minute()) == (h as i8, m as i8);
            if here && days(&cx.raw_str("days")).contains(&weekday(now)) && key != self.last {
                self.last = key;
                cx.fire("fire", &now.strftime("%H:%M").to_string());
            }
        }
        let wait = match AtTime::next(&time, &cx.raw_str("days"), now, 0.0) {
            Some(t) => seconds_between(now, t).clamp(0.0, AT_MOST),
            None => AT_MOST,
        };
        cx.after(wait, TICK);
    }
}

impl NodeRuntime for AtTime {
    fn start(&mut self, cx: &mut Cx<'_>) {
        self.tick(cx);
    }

    fn timer(&mut self, _token: u64, cx: &mut Cx<'_>) {
        self.tick(cx);
    }

    fn info(&self, _status: &Status, look: &Look<'_>) -> Status {
        let Some(t) = AtTime::next(&look.raw_str("time"), &look.raw_str("days"), look.local, 59.0) else {
            return Status { text: "⚠ write the time like 16:00".into(), progress: None };
        };
        let secs = seconds_between(look.local, t);
        if secs < 60.0 {
            return Status { text: "now".into(), progress: None };
        }
        let today = look.local.date();
        let day = if t.date() == today {
            "today".to_owned()
        } else if today.tomorrow().ok() == Some(t.date()) {
            "tomorrow".to_owned()
        } else {
            t.strftime("%a").to_string()
        };
        Status { text: format!("next: {day} {} (in {})", t.strftime("%H:%M"), left(secs)), progress: None }
    }
}

/// On a date: fires once, on that day at that time.
#[derive(Default)]
pub struct OnDate;

impl OnDate {
    fn target(date: &str, time: &str) -> Option<DateTime> {
        let (h, m) = hm(time).unwrap_or((0, 0));
        let day: jiff::civil::Date =
            jiff::fmt::strtime::parse("%Y-%m-%d", date.trim()).ok()?.to_date().ok()?;
        Some(day.at(h as i8, m as i8, 0, 0))
    }

    fn iso(t: DateTime) -> String {
        t.strftime("%Y-%m-%dT%H:%M:%S").to_string()
    }

    fn tick(&mut self, cx: &mut Cx<'_>) {
        let Some(t) = OnDate::target(&cx.raw_str("date"), &cx.raw_str("time")) else { return };
        let iso = OnDate::iso(t);
        if cx.state().get("fired").and_then(|v| v.as_str()) == Some(iso.as_str()) {
            return;
        }
        let now = cx.local();
        let late = -seconds_between(now, t);
        if late >= 0.0 && (late < 90.0 || cx.flag("catch_up")) {
            cx.save_state("fired", json!(iso));
            cx.fire("fire", &t.strftime("%Y-%m-%d %H:%M").to_string());
        } else if late < 0.0 {
            cx.after((-late).min(AT_MOST), TICK);
        }
        // a date that went by without catching up never fires: nothing left to wait for
    }
}

impl NodeRuntime for OnDate {
    fn start(&mut self, cx: &mut Cx<'_>) {
        self.tick(cx);
    }

    fn timer(&mut self, _token: u64, cx: &mut Cx<'_>) {
        self.tick(cx);
    }

    fn info(&self, _status: &Status, look: &Look<'_>) -> Status {
        let text = match OnDate::target(&look.raw_str("date"), &look.raw_str("time")) {
            None => "⚠ pick a date".to_owned(),
            Some(t)
                if look.state().get("fired").and_then(|v| v.as_str()) == Some(OnDate::iso(t).as_str()) =>
            {
                "done (fired once)".to_owned()
            }
            Some(t) => match seconds_between(look.local, t) {
                secs if secs > 0.0 => format!("in {}", left(secs)),
                _ => "now".to_owned(),
            },
        };
        Status { text, progress: None }
    }
}

/// Luminescent event: fires when something happens in Luminescent.
pub struct EventTrigger;

impl NodeRuntime for EventTrigger {
    fn event(&mut self, name: &str, text: &str, cx: &mut Cx<'_>) {
        if name == cx.raw_str("event") && text.to_lowercase().contains(&cx.raw_str("filter").to_lowercase()) {
            cx.fire("fire", text);
        }
    }
}

fn yes_no(b: bool) -> String {
    if b { "yes".into() } else { "no".into() }
}

/// Program starts / stops: watches for a program by its process name.
#[derive(Default)]
pub struct Process {
    up: Option<bool>,
}

impl NodeRuntime for Process {
    fn start(&mut self, cx: &mut Cx<'_>) {
        cx.watch(Watch::Programs);
    }

    fn reading(&mut self, readings: &Readings, cx: &mut Cx<'_>) {
        let Some(names) = &readings.programs else { return };
        let want = cx.raw_str("name").trim().to_lowercase();
        if want.is_empty() {
            cx.set_status("⚠ type a program name", None);
            return;
        }
        let bare = want.strip_suffix(".exe").unwrap_or(&want);
        let up = names.contains(&want) || names.contains(&format!("{want}.exe")) || names.contains(bare);
        if self.up.is_some_and(|was| was != up) {
            cx.fire(if up { "started" } else { "stopped" }, &want);
        }
        if self.up != Some(up) {
            cx.set_status(if up { "running" } else { "not running" }, None);
        }
        self.up = Some(up);
    }

    fn value(&self, _port: &str, _input: &str, _cx: &mut Cx<'_>) -> String {
        yes_no(self.up == Some(true))
    }
}

/// File or folder changes: fires when the file, or anything directly in the folder, changes.
#[derive(Default)]
pub struct FileWatch {
    watcher: Option<notify::RecommendedWatcher>,
    path: PathBuf,
    settle: Option<u64>,
}

const HIT: u64 = 1;
const SETTLE: u64 = 2;

impl NodeRuntime for FileWatch {
    fn start(&mut self, cx: &mut Cx<'_>) {
        use notify::Watcher;

        let raw = cx.raw_str("path").trim().to_owned();
        self.path = text::expand_user(&raw);
        if raw.is_empty() {
            cx.set_status("pick a file or folder", None);
            return;
        }
        if !self.path.exists() {
            cx.set_status("⚠ that path does not exist", None);
            return;
        }
        // A file is watched through its folder: editors save by writing a new file in its place, which a watch
        // on the old file would never see.
        let (folder, name) = if self.path.is_dir() {
            (self.path.clone(), None)
        } else {
            let folder = self.path.parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
            (folder, self.path.file_name().map(|n| n.to_owned()))
        };
        let job = cx.job(HIT);
        let made = notify::recommended_watcher(move |got: notify::Result<notify::Event>| {
            let Ok(event) = got else { return };
            if job.stopped() || matches!(event.kind, notify::EventKind::Access(_)) {
                return;
            }
            if let Some(name) = &name
                && !event.paths.iter().any(|p| p.file_name() == Some(name.as_os_str()))
            {
                return;
            }
            job.send(JobMsg::Line(String::new()));
        });
        let watching =
            made.and_then(|mut w| w.watch(&folder, notify::RecursiveMode::NonRecursive).map(|_| w));
        match watching {
            Ok(w) => {
                self.watcher = Some(w);
                cx.set_status("watching", None);
            }
            Err(e) => cx.set_status(format!("⚠ {e}"), None),
        }
    }

    fn job(&mut self, _token: u64, _msg: JobMsg, cx: &mut Cx<'_>) {
        // a save is often several changes in a row: fire once they settle
        if let Some(t) = self.settle.take() {
            cx.cancel(t);
        }
        self.settle = Some(cx.after(0.4, SETTLE));
    }

    fn timer(&mut self, _token: u64, cx: &mut Cx<'_>) {
        self.settle = None;
        cx.fire("changed", &self.path.to_string_lossy());
    }

    fn stop(&mut self, _cx: &mut Cx<'_>) {
        self.watcher = None;
    }
}

/// Clipboard changes: fires when something new is copied.
pub struct ClipTrigger;

impl NodeRuntime for ClipTrigger {
    fn start(&mut self, cx: &mut Cx<'_>) {
        cx.watch(Watch::Clipboard);
    }

    fn reading(&mut self, readings: &Readings, cx: &mut Cx<'_>) {
        if let Some(text) = &readings.clipboard {
            cx.fire("changed", text);
        }
    }
}

/// Idle / back: fires when the PC has not been used for that long, and again when you are back.
#[derive(Default)]
pub struct Idle {
    away: bool,
}

impl NodeRuntime for Idle {
    fn start(&mut self, cx: &mut Cx<'_>) {
        cx.watch(Watch::Idle);
    }

    fn reading(&mut self, readings: &Readings, cx: &mut Cx<'_>) {
        let Some(secs) = readings.idle else { return };
        let limit = cx.num("minutes", "", 5.0).max(0.1) * 60.0;
        if !self.away && secs >= limit {
            self.away = true;
            cx.set_status("away", None);
            cx.fire("idle", &format!("{:.0}", secs / 60.0));
        } else if self.away && secs < 5.0 {
            self.away = false;
            cx.set_status("", None);
            cx.fire("back", "");
        }
    }
}

/// CPU / GPU / temperature / battery: fires when the level crosses your line and stays there, and again when
/// it is back.
#[derive(Default)]
pub struct Threshold {
    since: Option<f64>,
    over: bool,
    level: Option<f64>,
}

impl NodeRuntime for Threshold {
    fn start(&mut self, cx: &mut Cx<'_>) {
        cx.watch(Watch::Levels);
    }

    fn reading(&mut self, readings: &Readings, cx: &mut Cx<'_>) {
        let metric = match cx.raw_str("metric") {
            m if m.is_empty() => "cpu".to_owned(),
            m => m,
        };
        let Some(level) = readings.level(&metric) else { return };
        let v = match level {
            Level::Value(v) => v,
            Level::NotYet => {
                cx.set_status("not available yet", None);
                return;
            }
            Level::Missing => {
                let text = match metric.as_str() {
                    "battery" | "cpu_temp" | "gpu" | "gpu_temp" => "⚠ not available here",
                    _ => "…",
                };
                cx.set_status(text, None);
                return;
            }
        };
        self.level = Some(v);
        let unit = if metric.ends_with("_temp") { " °C" } else { " %" };
        let line = cx.num("value", "", 90.0);
        let bad = if cx.raw_str("op") == "above" { v > line } else { v < line };
        let now = cx.mono();
        if bad && !self.over {
            let since = *self.since.get_or_insert(now);
            if now - since >= cx.num("for_s", "", 10.0) {
                self.over = true;
                cx.fire("crossed", &format!("{v:.0}"));
            }
        } else if !bad {
            self.since = None;
            if self.over {
                self.over = false;
                cx.fire("back", &format!("{v:.0}"));
            }
        }
        cx.set_status(format!("{v:.0}{unit}"), None);
    }

    fn value(&self, _port: &str, _input: &str, _cx: &mut Cx<'_>) -> String {
        self.level.map(|v| format!("{v:.0}")).unwrap_or_default()
    }
}

/// Window focused: fires when you switch to a window whose app or title contains the text, and when you leave.
#[derive(Default)]
pub struct Window {
    inside: Option<bool>,
    title: String,
}

impl NodeRuntime for Window {
    fn start(&mut self, cx: &mut Cx<'_>) {
        cx.watch(Watch::Window);
    }

    fn reading(&mut self, readings: &Readings, cx: &mut Cx<'_>) {
        let Some((app, title)) = &readings.window else { return };
        let want = cx.raw_str("match").trim().to_lowercase();
        if want.is_empty() {
            cx.set_status("⚠ type part of an app name or title", None);
            return;
        }
        let inside = app.to_lowercase().contains(&want) || title.to_lowercase().contains(&want);
        if inside {
            self.title = title.clone();
        }
        if self.inside.is_some_and(|was| was != inside) {
            cx.fire(if inside { "focused" } else { "left" }, title);
        }
        if self.inside != Some(inside) {
            cx.set_status(if inside { "in it" } else { "elsewhere" }, None);
        }
        self.inside = Some(inside);
    }

    fn value(&self, _port: &str, _input: &str, _cx: &mut Cx<'_>) -> String {
        self.title.clone()
    }
}

/// Internet on / off: fires when the internet goes away and when it comes back.
#[derive(Default)]
pub struct Network {
    up: Option<bool>,
}

impl NodeRuntime for Network {
    fn start(&mut self, cx: &mut Cx<'_>) {
        cx.watch(Watch::Internet);
    }

    fn reading(&mut self, readings: &Readings, cx: &mut Cx<'_>) {
        let Some(Some(up)) = readings.online else { return };
        if self.up.is_some_and(|was| was != up) {
            cx.fire(if up { "online" } else { "offline" }, "");
        }
        if self.up != Some(up) {
            cx.set_status(if up { "online" } else { "offline" }, None);
        }
        self.up = Some(up);
    }

    fn value(&self, _port: &str, _input: &str, _cx: &mut Cx<'_>) -> String {
        yes_no(self.up == Some(true))
    }
}
