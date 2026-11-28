#![allow(clippy::unwrap_used)]
//! Triggers and actions on a fake clock, with a fake desktop: what they read, what they ask the app for, and
//! what they send on. Commands, web requests, downloads and file copies run for real on worker threads.

use std::collections::{BTreeSet, HashMap};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jiff::civil::DateTime;
use lum_automation::{Answer, Clock, Config, Engine, FakeClock, Host, Level, Out, Reply};
use serde_json::{Value, json};

#[derive(Default)]
struct Desk {
    outs: Mutex<Vec<Out>>,
    programs: Mutex<BTreeSet<String>>,
    idle: Mutex<f64>,
    window: Mutex<(String, String)>,
    online: Mutex<Option<bool>>,
    levels: Mutex<HashMap<String, Level>>,
    clipboard: Mutex<String>,
    closed: Mutex<Vec<(String, bool)>>,
    terminal: Mutex<Option<(PathBuf, Vec<String>)>>,
    reads: Mutex<usize>,
}

impl Host for Desk {
    fn send(&self, out: Out) {
        self.outs.lock().unwrap().push(out);
    }

    fn clipboard_text(&self) -> String {
        self.clipboard.lock().unwrap().clone()
    }

    fn run_in_terminal(
        &self,
        _cwd: &Path,
        lines: &[String],
        _shell: &str,
        _stop_on_error: bool,
        marker: &Path,
    ) -> Result<PathBuf, String> {
        *self.terminal.lock().unwrap() = Some((marker.to_owned(), lines.to_vec()));
        let script = marker.with_extension("sh");
        std::fs::write(&script, lines.join("\n")).unwrap();
        Ok(script)
    }

    fn close_program(&self, name: &str, force: bool) -> bool {
        self.closed.lock().unwrap().push((name.to_owned(), force));
        name == "firefox"
    }

    fn process_names(&self) -> BTreeSet<String> {
        *self.reads.lock().unwrap() += 1;
        self.programs.lock().unwrap().clone()
    }

    fn idle_seconds(&self) -> f64 {
        *self.idle.lock().unwrap()
    }

    fn focused_window(&self) -> (String, String) {
        self.window.lock().unwrap().clone()
    }

    fn online(&self) -> Option<bool> {
        *self.online.lock().unwrap()
    }

    fn level(&self, metric: &str) -> Level {
        self.levels.lock().unwrap().get(metric).copied().unwrap_or(Level::NotYet)
    }

    fn keep_awake_on(&self) -> bool {
        true
    }

    fn volume(&self) -> Option<(f64, bool)> {
        Some((42.4, false))
    }
}

impl Desk {
    fn take(&self) -> Vec<Out> {
        std::mem::take(&mut *self.outs.lock().unwrap())
    }
}

struct Rig {
    engine: Engine,
    clock: Arc<FakeClock>,
    desk: Arc<Desk>,
}

fn node(id: &str, ty: &str, fields: Value) -> Value {
    json!({"id": id, "type": ty, "fields": fields})
}

fn wire(src: &str, out: &str, dst: &str, inp: &str) -> Value {
    json!({"src": [src, out], "dst": [dst, inp]})
}

/// A workspace with the node under test and a Show on each of its outputs (`show:<port>`).
fn rig(nodes: Vec<Value>, wires: Vec<Value>) -> Rig {
    rig_at(DateTime::constant(2026, 11, 23, 14, 30, 0, 0), nodes, wires)
}

fn rig_at(at: DateTime, nodes: Vec<Value>, wires: Vec<Value>) -> Rig {
    let clock = Arc::new(FakeClock::new(at));
    let desk = Arc::new(Desk::default());
    let saved = json!({"version": 1, "vars": {}, "workspaces": [
        {"id": "w", "name": "Test", "nodes": nodes, "wires": wires},
        {"id": "other", "name": "Other", "enabled": false, "nodes": [], "wires": []}
    ]});
    let mut engine = Engine::new(Some(saved), clock.clone(), desk.clone(), Config::default());
    engine.start();
    let mut r = Rig { engine, clock, desk };
    r.settle();
    r
}

/// A node whose every output goes to a Show named after the port.
fn shown(id: &str, ty: &str, fields: Value, outs: &[&str]) -> (Vec<Value>, Vec<Value>) {
    let mut nodes = vec![node(id, ty, fields)];
    let mut wires = Vec::new();
    for port in outs {
        nodes.push(node(&format!("show:{port}"), "show", json!({})));
        wires.push(wire(id, &format!("out:{port}"), &format!("show:{port}"), "in:in"));
    }
    (nodes, wires)
}

impl Rig {
    /// Every pass that is due now, waiting for worker threads to report back.
    fn settle(&mut self) {
        let end = Instant::now() + Duration::from_secs(20);
        loop {
            while self.engine.step() {}
            if self.engine.jobs_running() == 0 || Instant::now() > end {
                break;
            }
            self.engine.wait_for_job(Duration::from_secs(10));
        }
    }

    fn run_for(&mut self, secs: f64) {
        let end = self.clock.mono() + secs;
        loop {
            self.settle();
            match self.engine.next_deadline() {
                Some(at) if at <= end + 1e-9 => {
                    let gap = at - self.clock.mono();
                    if gap > 0.0 {
                        self.clock.advance(gap);
                    }
                }
                _ => break,
            }
        }
        let rest = end - self.clock.mono();
        if rest > 0.0 {
            self.clock.advance(rest);
        }
    }

    fn status(&self, nid: &str) -> String {
        self.engine.info("w", nid).map(|s| s.text).unwrap_or_default()
    }

    fn shown(&self, port: &str) -> String {
        self.status(&format!("show:{port}"))
    }

    fn pulse(&mut self, nid: &str, input: &str) {
        self.engine.run_node("w", nid, input);
        self.settle();
    }

    fn answer(&mut self, reply: &Reply, answer: Answer) {
        self.engine.answer(reply, answer);
        self.settle();
    }

    /// Waits (for real) until something from another thread is waiting for the engine.
    fn wait_for_news(&mut self) -> bool {
        let end = Instant::now() + Duration::from_secs(10);
        while Instant::now() < end {
            if self.engine.next_deadline().is_some_and(|at| at <= self.clock.mono()) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }
}

fn reply_of(out: &Out) -> Reply {
    match out {
        Out::Message { reply, .. }
        | Out::Ask { reply, .. }
        | Out::Confirm { reply, .. }
        | Out::StartTimer { reply, .. }
        | Out::OpenNote { reply, .. }
        | Out::Open { reply, .. }
        | Out::Speak { reply, .. }
        | Out::KeepAwake { reply, .. }
        | Out::Volume { reply, .. }
        | Out::Power { reply, .. } => reply.clone(),
        Out::Alert { reply: Some(reply), .. } => reply.clone(),
        other => panic!("{other:?} has no reply"),
    }
}

// Triggers

#[test]
fn at_a_time_of_day_sleeps_until_its_minute_then_fires() {
    let (nodes, wires) = shown("at", "at_time", json!({"time": "14:32", "days": "weekdays"}), &["fire"]);
    let mut r = rig(nodes, wires);
    assert_eq!(r.status("at"), "next: today 14:32 (in 2m)");
    // it never sleeps more than a minute, and never ticks every second
    let wait = r.engine.next_deadline().unwrap() - r.clock.mono();
    assert!(wait > 59.0 && wait <= 60.0, "{wait}");
    r.run_for(119.0);
    assert_eq!(r.shown("fire"), "");
    r.run_for(1.0);
    assert_eq!(r.shown("fire"), "14:32");
    assert_eq!(r.status("at"), "now");
    r.run_for(60.0);
    assert_eq!(r.status("at"), "next: tomorrow 14:32 (in 23h 59m)");

    let (nodes, wires) = shown("at", "at_time", json!({"time": "9.05", "days": "sat-sun"}), &["fire"]);
    let r = rig(nodes, wires);
    assert_eq!(r.status("at"), "next: Sat 09:05 (in 4d 18h)");
    let (nodes, wires) = shown("at", "at_time", json!({"time": "25:00"}), &["fire"]);
    let r = rig(nodes, wires);
    assert_eq!(r.status("at"), "⚠ write the time like 16:00");
}

#[test]
fn started_inside_its_minute_it_fires_straight_away_once() {
    let at = DateTime::constant(2026, 11, 23, 16, 0, 30, 0);
    let (nodes, wires) = shown("at", "at_time", json!({"time": "16:00"}), &["fire"]);
    let mut r = rig_at(at, nodes, wires);
    assert_eq!(r.shown("fire"), "16:00");
    r.engine.set_status("w", "show:fire", "");
    r.run_for(20.0);
    assert_eq!(r.shown("fire"), "");
}

#[test]
fn on_a_date_fires_once_and_remembers() {
    let (nodes, wires) = shown("d", "on_date", json!({"date": "2026-11-23", "time": "14:31"}), &["fire"]);
    let mut r = rig(nodes, wires);
    assert_eq!(r.status("d"), "in 1m");
    r.run_for(60.0);
    assert_eq!(r.shown("fire"), "2026-11-23 14:31");
    assert_eq!(r.engine.workspace("w").unwrap()["nodes"][0]["state"]["fired"], "2026-11-23T14:31:00");
    assert_eq!(r.status("d"), "done (fired once)");
    // nothing left to wait for once it has fired
    assert_eq!(r.engine.next_deadline(), None);
    r.engine.replaced("w");
    r.settle();
    assert_eq!(r.shown("fire"), "");

    let late = json!({"date": "2026-11-20", "time": "09:00", "catch_up": true});
    let (nodes, wires) = shown("d", "on_date", late, &["fire"]);
    let r = rig(nodes, wires);
    assert_eq!(r.shown("fire"), "2026-11-20 09:00");
    let missed = json!({"date": "2026-11-20", "time": "09:00", "catch_up": false});
    let (nodes, wires) = shown("d", "on_date", missed, &["fire"]);
    let r = rig(nodes, wires);
    assert_eq!(r.shown("fire"), "");
    assert_eq!(r.engine.next_deadline(), None);
    let (nodes, wires) = shown("d", "on_date", json!({"date": "someday"}), &["fire"]);
    let r = rig(nodes, wires);
    assert_eq!(r.status("d"), "⚠ pick a date");
}

#[test]
fn hotkey_triggers_are_listed_for_the_app_and_fire_when_pressed() {
    let (nodes, wires) = shown("key", "hotkey", json!({"combo": "ctrl+alt+k"}), &["pressed"]);
    let mut r = rig(nodes, wires);
    assert!(r.desk.take().contains(&Out::HotkeysChanged));
    let keys = r.engine.hotkey_nodes();
    assert_eq!(keys.len(), 1);
    assert_eq!((keys[0].combo.as_str(), keys[0].label.as_str()), ("ctrl+alt+k", "Hotkey"));
    r.engine.hotkey_pressed("w", "key");
    r.settle();
    assert_eq!(r.shown("pressed"), "ctrl+alt+k");
    r.engine.set_enabled("w", false);
    assert!(r.desk.take().contains(&Out::HotkeysChanged));
    assert!(r.engine.hotkey_nodes().is_empty());
}

#[test]
fn event_triggers_hear_the_bus_with_a_filter() {
    let (nodes, wires) = shown("ev", "event", json!({"event": "timer.finished", "filter": "TEA"}), &["fire"]);
    let mut r = rig(nodes, wires);
    r.engine.event("timer.finished", "Coffee");
    r.settle();
    assert_eq!(r.shown("fire"), "");
    r.engine.event("theme.changed", "Green tea");
    r.settle();
    assert_eq!(r.shown("fire"), "");
    r.engine.event("timer.finished", "Green tea");
    r.settle();
    assert_eq!(r.shown("fire"), "Green tea");
}

#[test]
fn the_shared_polls_run_every_two_seconds_only_while_wanted() {
    let (nodes, wires) = shown("p", "process", json!({"name": "Steam.exe"}), &["started", "stopped"]);
    let mut r = rig(nodes, wires);
    assert_eq!(r.status("p"), "not running");
    r.desk.programs.lock().unwrap().insert("steam".into());
    r.run_for(1.9);
    assert_eq!(r.shown("started"), "");
    r.run_for(0.1);
    assert_eq!(r.shown("started"), "steam.exe");
    assert_eq!(r.status("p"), "running");
    r.desk.programs.lock().unwrap().clear();
    r.run_for(2.0);
    assert_eq!(r.shown("stopped"), "steam.exe");
    assert_eq!(*r.desk.reads.lock().unwrap(), 3);
    // with the workspace off nothing is read and nothing wakes the engine
    r.engine.set_enabled("w", false);
    r.settle();
    assert_eq!(r.engine.next_deadline(), None);
    r.run_for(10.0);
    assert_eq!(*r.desk.reads.lock().unwrap(), 3);
}

#[test]
fn idle_window_and_internet_follow_their_readings() {
    let mut nodes = vec![
        node("idle", "idle", json!({"minutes": 1})),
        node("win", "window", json!({"match": "code"})),
        node("net", "network", json!({})),
        node("show:idle", "show", json!({})),
        node("show:back", "show", json!({})),
        node("show:focused", "show", json!({})),
        node("show:left", "show", json!({})),
        node("show:offline", "show", json!({})),
    ];
    nodes.push(node("show:online", "show", json!({})));
    let wires = vec![
        wire("idle", "out:idle", "show:idle", "in:in"),
        wire("idle", "out:back", "show:back", "in:in"),
        wire("win", "out:focused", "show:focused", "in:in"),
        wire("win", "out:left", "show:left", "in:in"),
        wire("net", "out:offline", "show:offline", "in:in"),
        wire("net", "out:online", "show:online", "in:in"),
    ];
    let mut r = rig(nodes, wires);
    *r.desk.online.lock().unwrap() = Some(true);
    *r.desk.window.lock().unwrap() = ("firefox".into(), "News".into());
    r.run_for(2.0);
    assert_eq!((r.status("win"), r.status("net")), ("elsewhere".to_owned(), "online".to_owned()));
    *r.desk.idle.lock().unwrap() = 61.0;
    *r.desk.window.lock().unwrap() = ("Code".into(), "main.rs - Visual Studio Code".into());
    *r.desk.online.lock().unwrap() = Some(false);
    r.run_for(2.0);
    assert_eq!(r.shown("idle"), "1");
    assert_eq!(r.status("idle"), "away");
    assert_eq!(r.shown("focused"), "main.rs - Visual Studio Code");
    assert_eq!(r.shown("offline"), "(empty)");
    *r.desk.idle.lock().unwrap() = 1.0;
    *r.desk.window.lock().unwrap() = ("kitty".into(), "fish".into());
    r.run_for(2.0);
    assert_eq!(r.shown("back"), "(empty)");
    assert_eq!(r.shown("left"), "fish");
}

#[test]
fn thresholds_wait_for_their_line_to_hold() {
    let fields = json!({"metric": "cpu_temp", "op": "above", "value": 80, "for_s": 4});
    let (nodes, wires) = shown("t", "threshold", fields, &["crossed", "back"]);
    let mut r = rig(nodes, wires);
    assert_eq!(r.status("t"), "not available yet");
    r.desk.levels.lock().unwrap().insert("cpu_temp".into(), Level::Value(85.4));
    r.run_for(2.0);
    assert_eq!(r.status("t"), "85 °C");
    r.run_for(2.0);
    assert_eq!(r.shown("crossed"), "");
    r.run_for(2.0);
    assert_eq!(r.shown("crossed"), "85");
    r.desk.levels.lock().unwrap().insert("cpu_temp".into(), Level::Value(60.0));
    r.run_for(2.0);
    assert_eq!(r.shown("back"), "60");
    r.desk.levels.lock().unwrap().insert("cpu_temp".into(), Level::Missing);
    r.run_for(2.0);
    assert_eq!(r.status("t"), "⚠ not available here");
}

#[test]
fn the_clipboard_trigger_fires_on_a_change_and_tells_the_bus() {
    let (nodes, wires) = shown("c", "clipboard", json!({}), &["changed"]);
    let mut r = rig(nodes, wires);
    // the round when it starts only learns what is there (nothing)
    assert_eq!(r.shown("changed"), "");
    *r.desk.clipboard.lock().unwrap() = "copied text".into();
    r.run_for(2.0);
    assert_eq!(r.shown("changed"), "copied text");
    assert!(
        r.desk.take().contains(&Out::Emit { name: "clipboard.changed".into(), text: "copied text".into() })
    );
    r.engine.set_status("w", "show:changed", "");
    r.run_for(2.0);
    assert_eq!(r.shown("changed"), "", "the same text again is no change");
}

#[test]
fn a_watched_file_fires_once_its_changes_settle() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    std::fs::write(&file, "one").unwrap();
    let (nodes, wires) = shown("f", "file_watch", json!({"path": file.to_string_lossy()}), &["changed"]);
    let mut r = rig(nodes, wires);
    assert_eq!(r.status("f"), "watching");
    std::fs::write(dir.path().join("other.txt"), "not this one").unwrap();
    std::fs::write(&file, "two").unwrap();
    assert!(r.wait_for_news());
    r.run_for(0.3);
    assert_eq!(r.shown("changed"), "");
    r.run_for(0.2);
    assert_eq!(r.shown("changed"), file.to_string_lossy());

    let (nodes, wires) = shown("f", "file_watch", json!({"path": "/no/such/place"}), &["changed"]);
    let r = rig(nodes, wires);
    assert_eq!(r.status("f"), "⚠ that path does not exist");
}

// Actions that need the app

#[test]
fn simple_actions_ask_the_app_and_pass_the_pulse_on() {
    let nodes = vec![
        node("tool", "lum_action", json!({"action": "colorpicker:pick"})),
        node("ov", "overlay", json!({"mode": "toggle", "page": ""})),
        node("item", "dash_item", json!({"item": "i1"})),
        node("board", "switch_board", json!({"board": "b2"})),
        node("theme", "theme", json!({"theme": "previous"})),
        node("note", "notify", json!({"title": "Hi {input}", "style": "card"})),
        node("desk", "notify", json!({"style": "desktop", "message": "m"})),
        node("mid", "notify", json!({"style": "center", "message": "m"})),
        node("snd", "sound", json!({"sound": "alarm"})),
        node("keys", "keys", json!({"combo": "ctrl+s"})),
        node("copy", "set_clipboard", json!({"text": "<{input}>"})),
    ];
    let mut r = rig(nodes, vec![]);
    r.desk.take();
    for id in ["tool", "ov", "item", "board", "theme", "note", "desk", "mid", "snd", "keys", "copy"] {
        r.pulse(id, "x");
    }
    let outs = r.desk.take();
    assert_eq!(outs[0], Out::RunAction("colorpicker:pick".into()));
    assert_eq!(outs[1], Out::Overlay { mode: "toggle".into(), page: "dashboard".into() });
    assert_eq!(outs[2], Out::RunItem("i1".into()));
    assert_eq!(outs[3], Out::SwitchBoard("b2".into()));
    assert_eq!(outs[4], Out::Theme("previous".into()));
    assert_eq!(outs[5], Out::Card { title: "Hi x".into(), text: "x".into(), ms: 6000 });
    assert_eq!(outs[6], Out::Toast { title: "Luminescent".into(), text: "m".into(), ms: 8000 });
    assert!(matches!(&outs[7], Out::Alert { level, reply: None, .. } if level == "info"));
    assert!(matches!(&outs[8], Out::Sound { kind, .. } if kind == "alarm"));
    assert_eq!(outs[9], Out::PressKeys("ctrl+s".into()));
    assert_eq!(outs[10], Out::SetClipboard("<x>".into()));
}

#[test]
fn dialogs_carry_on_when_answered() {
    let mut nodes = vec![
        node("msg", "message", json!({"title": "T", "text": "Hello {input}"})),
        node("ask", "ask", json!({"question": "Name?", "default": "{input}", "var": "who"})),
        node("warn", "alert", json!({"title": "Hot", "seconds": 5, "sound": "chime"})),
    ];
    let mut wires = Vec::new();
    for (id, port) in [("msg", "ok"), ("ask", "answer"), ("ask", "cancelled"), ("warn", "ok")] {
        let show = format!("show:{id}:{port}");
        nodes.push(node(&show, "show", json!({})));
        wires.push(wire(id, &format!("out:{port}"), &show, "in:in"));
    }
    let mut r = rig(nodes, wires);
    r.desk.take();
    r.pulse("msg", "you");
    r.pulse("ask", "Sam");
    r.pulse("warn", "90 °C");
    let outs = r.desk.take();
    assert!(matches!(&outs[0], Out::Message { title, text, .. } if title == "T" && text == "Hello you"));
    assert!(matches!(&outs[1], Out::Ask { title, question, default, .. }
        if title == "Ask me" && question == "Name?" && default == "Sam"));
    assert!(matches!(&outs[2], Out::Alert { title, text, level, seconds, sound, .. }
        if title == "Hot" && text == "90 °C" && level == "warning" && *seconds == 5.0 && sound == "chime"));
    assert_eq!(r.status("show:msg:ok"), "");
    r.answer(&reply_of(&outs[0]), Answer::Done(String::new()));
    assert_eq!(r.status("show:msg:ok"), "you");
    r.answer(&reply_of(&outs[1]), Answer::Done("Alex".into()));
    assert_eq!(r.status("show:ask:answer"), "Alex");
    assert_eq!(r.engine.get_var("who"), "Alex");
    r.answer(&reply_of(&outs[2]), Answer::Done(String::new()));
    assert_eq!(r.status("show:warn:ok"), "90 °C");
    // an answer only counts once, and never for a node that started again since
    r.pulse("ask", "again");
    let again = reply_of(&r.desk.take()[0]);
    r.answer(&again, Answer::Cancelled);
    assert_eq!(r.status("show:ask:cancelled"), "again");
    r.engine.replaced("w");
    r.settle();
    r.answer(&again, Answer::Done("late".into()));
    assert_eq!(r.status("show:ask:answer"), "");
}

#[test]
fn timers_notes_and_programs_answer_back() {
    let mut nodes = vec![
        node("timer", "timer", json!({"time": "{input}"})),
        node("note", "open_note", json!({"note": "n1"})),
        node("open", "open", json!({"target": "~/notes.txt"})),
    ];
    let mut wires = Vec::new();
    for (id, port) in [("timer", "started"), ("note", "done"), ("open", "done")] {
        let show = format!("show:{id}");
        nodes.push(node(&show, "show", json!({})));
        wires.push(wire(id, &format!("out:{port}"), &show, "in:in"));
    }
    let mut r = rig(nodes, wires);
    r.desk.take();
    r.pulse("timer", "soon");
    assert_eq!(r.status("timer"), "⚠ could not read that time");
    r.pulse("timer", "25m focus");
    let out = r.desk.take().remove(0);
    assert!(matches!(&out, Out::StartTimer { ms, label, .. } if *ms == 1_500_000.0 && label == "focus"));
    r.answer(&reply_of(&out), Answer::Failed("the Timer module is off".into()));
    assert_eq!(r.status("timer"), "⚠ the Timer module is off");
    r.pulse("timer", "90s");
    let out = r.desk.take().remove(0);
    r.answer(&reply_of(&out), Answer::Done(String::new()));
    assert_eq!(r.status("show:timer"), "90s");

    r.pulse("note", "in");
    let out = r.desk.take().remove(0);
    assert!(matches!(&out, Out::OpenNote { id, .. } if id == "n1"));
    r.answer(&reply_of(&out), Answer::Done(String::new()));
    assert_eq!(r.status("show:note"), "in");

    r.pulse("open", "");
    let out = r.desk.take().remove(0);
    assert!(
        matches!(&out, Out::Open { target, name, .. } if target == "~/notes.txt" && name == "Open a program, file or URL")
    );
    r.answer(&reply_of(&out), Answer::Failed("No such file".into()));
    assert_eq!(r.status("open"), "⚠ No such file");
    r.answer(&reply_of(&out), Answer::Done("~/notes.txt".into()));
    assert_eq!(r.status("show:open"), "~/notes.txt");
}

#[test]
fn speech_volume_keep_awake_and_typing() {
    let mut nodes = vec![
        node("say", "speak", json!({"text": "  {input}  "})),
        node("vol", "volume", json!({"mode": "up", "amount": "{input}"})),
        node("awake", "keep_awake", json!({"mode": "toggle"})),
        node("type", "type_text", json!({"text": "hello", "delay_ms": 500})),
    ];
    let mut wires = Vec::new();
    for id in ["say", "vol", "awake", "type"] {
        let show = format!("show:{id}");
        nodes.push(node(&show, "show", json!({})));
        wires.push(wire(id, "out:done", &show, "in:in"));
    }
    let mut r = rig(nodes, wires);
    r.desk.take();
    r.pulse("say", "");
    assert_eq!(r.status("show:say"), "(empty)");
    assert!(r.desk.take().is_empty());
    r.pulse("say", "hi");
    let out = r.desk.take().remove(0);
    assert!(matches!(&out, Out::Speak { text, .. } if text == "hi"));
    r.answer(&reply_of(&out), Answer::Failed("install speech-dispatcher or espeak-ng".into()));
    assert_eq!(r.status("say"), "⚠ install speech-dispatcher or espeak-ng");

    r.pulse("vol", "10");
    let out = r.desk.take().remove(0);
    assert!(matches!(&out, Out::Volume { mode, amount, .. } if mode == "up" && *amount == 10.0));
    r.answer(&reply_of(&out), Answer::Done("52 %".into()));
    assert_eq!((r.status("vol"), r.status("show:vol")), ("52 %".to_owned(), "10".to_owned()));

    r.pulse("awake", "a");
    let out = r.desk.take().remove(0);
    assert!(matches!(&out, Out::KeepAwake { on: None, why, .. } if why == "Automation: Keep awake"));
    r.answer(&reply_of(&out), Answer::Done("keeping awake".into()));
    assert_eq!(r.status("awake"), "keeping awake");

    r.pulse("type", "t");
    r.run_for(0.4);
    assert!(r.desk.take().is_empty());
    r.run_for(0.1);
    assert_eq!(r.desk.take(), vec![Out::TypeText("hello".into())]);
    assert_eq!(r.status("show:type"), "t");
}

#[test]
fn power_asks_first_then_waits_a_moment() {
    let mut nodes = vec![node("off", "power", json!({"action": "shutdown", "ask": true}))];
    nodes.push(node("show", "show", json!({})));
    let mut r = rig(nodes, vec![wire("off", "out:done", "show", "in:in")]);
    r.desk.take();
    r.pulse("off", "now");
    let out = r.desk.take().remove(0);
    assert!(matches!(&out, Out::Confirm { text, yes, .. }
        if text == "Lock / sleep / shut down: shut down the pc now?" && yes == "Shut down the PC"));
    r.answer(&reply_of(&out), Answer::Cancelled);
    r.run_for(1.0);
    assert!(r.desk.take().is_empty());
    r.pulse("off", "now");
    let out = r.desk.take().remove(0);
    r.answer(&reply_of(&out), Answer::Done(String::new()));
    assert_eq!(r.status("show"), "now");
    r.run_for(0.2);
    assert!(r.desk.take().is_empty());
    r.run_for(0.1);
    let out = r.desk.take().remove(0);
    assert!(matches!(&out, Out::Power { action, .. } if action == "shutdown"));
    r.answer(&reply_of(&out), Answer::Failed("systemctl poweroff failed".into()));
    assert_eq!(r.status("off"), "⚠ systemctl poweroff failed");
}

#[test]
fn a_workspace_node_switches_workspaces() {
    let nodes = vec![
        node("other", "workspace", json!({"workspace": "other", "mode": "toggle"})),
        node("me", "workspace", json!({"mode": "off"})),
    ];
    let mut r = rig(nodes, vec![]);
    r.pulse("other", "");
    assert!(r.engine.is_running("other"));
    r.pulse("me", "");
    assert!(!r.engine.is_running("w"));
}

#[test]
fn values_read_the_desktop() {
    let mut r = rig(
        vec![
            node("vol", "volume", json!({})),
            node("awake", "keep_awake", json!({})),
            node("join", "join", json!({"sep": "/"})),
            node("set", "var_set", json!({"name": "v"})),
        ],
        vec![
            wire("vol", "val:volume", "join", "field:a"),
            wire("awake", "val:on", "join", "field:b"),
            wire("join", "val:text", "set", "field:value"),
        ],
    );
    r.pulse("set", "");
    assert_eq!(r.engine.get_var("v"), "42/yes");
}

// Actions that run on worker threads

#[cfg(unix)]
#[test]
fn commands_run_one_after_another_and_report_each_line() {
    let fields =
        json!({"commands": "echo one\nprintf 'two\\nthree\\n'\necho {input} >&2", "stop_on_error": true});
    let (mut nodes, mut wires) = shown("cmd", "command", fields, &["done", "failed"]);
    nodes.push(node("lines", "var_set", json!({"name": "lines", "mode": "append", "value": "{input};"})));
    wires.push(wire("cmd", "out:line", "lines", "in:run"));
    let mut r = rig(nodes, wires);
    r.pulse("cmd", "err");
    assert_eq!(r.status("cmd"), "done");
    assert_eq!(r.shown("done"), "one ⏎ two ⏎ three ⏎ err");
    assert_eq!(r.engine.get_var("lines"), "one;two;three;err;");

    let fields = json!({"commands": "echo a\nexit 3\necho never", "stop_on_error": true});
    let (nodes, wires) = shown("cmd", "command", fields, &["done", "failed"]);
    let mut r = rig(nodes, wires);
    r.pulse("cmd", "");
    assert_eq!(r.status("cmd"), "failed (exit code 3)");
    assert_eq!(r.shown("failed"), "a");

    let fields = json!({"commands": "false\necho still", "stop_on_error": false});
    let (nodes, wires) = shown("cmd", "command", fields, &["done", "failed"]);
    let mut r = rig(nodes, wires);
    r.pulse("cmd", "");
    assert_eq!(r.shown("failed"), "still");

    let (nodes, wires) = shown("cmd", "command", json!({"commands": "  \n"}), &["done"]);
    let mut r = rig(nodes, wires);
    r.pulse("cmd", "");
    assert_eq!(r.status("cmd"), "⚠ no commands");
}

#[cfg(unix)]
#[test]
fn a_busy_command_queues_the_next_run_and_stop_ends_it() {
    let (mut nodes, wires) = shown("cmd", "command", json!({"commands": "echo {input}"}), &["done"]);
    nodes.push(node("count", "counter", json!({})));
    let mut wires = wires;
    wires.push(wire("cmd", "out:done", "count", "in:in"));
    let mut r = rig(nodes, wires);
    r.engine.run_node("w", "cmd", "first");
    r.engine.run_node("w", "cmd", "second");
    r.settle();
    assert_eq!(r.status("count"), "2");
    assert_eq!(r.shown("done"), "second");

    let (mut nodes, mut wires) =
        shown("cmd", "command", json!({"commands": "sleep 30\necho never"}), &["done"]);
    nodes.push(node("stopper", "manual", json!({})));
    wires.push(wire("stopper", "out:fire", "cmd", "in:stop"));
    let mut r = rig(nodes, wires);
    let started = Instant::now();
    r.engine.run_node("w", "cmd", "");
    while r.engine.step() {}
    r.engine.run_node("w", "stopper", "");
    r.settle();
    assert!(started.elapsed() < Duration::from_secs(10));
    // a stopped command counts as done, as it did in Python
    assert_eq!(r.status("cmd"), "done");
}

#[test]
fn a_command_in_a_terminal_waits_for_its_marker() {
    let (nodes, wires) = shown(
        "cmd",
        "command",
        json!({"commands": "make\nmake install", "terminal": true}),
        &["done", "failed"],
    );
    let mut r = rig(nodes, wires);
    r.pulse("cmd", "");
    assert_eq!(r.status("cmd"), "running in a terminal");
    let (marker, lines) = r.desk.terminal.lock().unwrap().clone().unwrap();
    assert_eq!(lines, ["make", "make install"]);
    r.run_for(2.0);
    assert_eq!(r.status("cmd"), "running in a terminal");
    std::fs::write(&marker, "2\n").unwrap();
    r.run_for(0.5);
    assert_eq!(r.status("cmd"), "failed (exit code 2)");
    assert_eq!(r.shown("failed"), "2");
    assert!(!marker.exists());
    assert!(!marker.with_extension("sh").exists());
}

#[test]
fn closing_a_program_runs_off_the_engine() {
    let (nodes, wires) =
        shown("close", "close_program", json!({"name": "{input}", "force": true}), &["done", "not_running"]);
    let mut r = rig(nodes, wires);
    r.pulse("close", "firefox");
    r.pulse("close", "notepad");
    assert_eq!((r.shown("done"), r.shown("not_running")), ("firefox".to_owned(), "notepad".to_owned()));
    assert_eq!(r.desk.closed.lock().unwrap()[0], ("firefox".to_owned(), true));
}

/// Answers each connection with the next canned response, and keeps what each request said.
fn serve(responses: Vec<&'static str>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let kept = seen.clone();
    std::thread::spawn(move || {
        for response in responses {
            let Ok((mut stream, _)) = listener.accept() else { return };
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut got = Vec::new();
            let mut buf = [0u8; 4096];
            while let Ok(n) = stream.read(&mut buf) {
                got.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&got);
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().to_owned())
                        })
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(0);
                    if got.len() >= end + 4 + length {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            kept.lock().unwrap().push(String::from_utf8_lossy(&got).into_owned());
            let _ = stream.write_all(response.as_bytes());
        }
    });
    (url, seen)
}

#[test]
fn web_requests_send_and_read_like_urllib() {
    let ok = "HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello";
    let missing = "HTTP/1.1 404 Not Found\r\nContent-Length: 4\r\nConnection: close\r\n\r\nnope";
    let (url, seen) = serve(vec![ok, missing]);
    let fields = json!({"method": "POST", "url": format!("{url}/hook"), "body": "{\"content\": \"{input}\"}",
                        "headers": "X-Token: {input}\nbroken line"});
    let (mut nodes, mut wires) = shown("http", "http", fields, &["done", "failed"]);
    nodes.push(node("status", "var_set", json!({"name": "status"})));
    wires.push(wire("http", "val:status", "status", "field:value"));
    let mut r = rig(nodes, wires);
    r.pulse("http", "hi");
    assert_eq!(r.shown("done"), "hello");
    assert_eq!(r.status("http"), "200");
    let request = seen.lock().unwrap()[0].clone();
    assert!(request.starts_with("POST /hook HTTP/1.1"), "{request}");
    let lower = request.to_lowercase();
    assert!(lower.contains("user-agent: luminescent"), "{request}");
    assert!(lower.contains("x-token: hi"), "{request}");
    assert!(lower.contains("content-type: application/json"), "{request}");
    assert!(request.ends_with("{\"content\": \"hi\"}"), "{request}");
    r.pulse("http", "again");
    assert_eq!(r.shown("failed"), "nope");
    assert_eq!(r.status("http"), "⚠ failed 404");
    r.pulse("status", "");
    assert_eq!(r.engine.get_var("status"), "404");
}

#[test]
fn downloads_save_into_the_folder() {
    let ok = "HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\ndata";
    let (url, _) = serve(vec![ok]);
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("new");
    let fields = json!({"url": format!("{url}/files/report.pdf?x=1"), "folder": folder.to_string_lossy()});
    let (nodes, wires) = shown("dl", "download", fields, &["done", "failed"]);
    let mut r = rig(nodes, wires);
    r.pulse("dl", "");
    let saved = folder.join("report.pdf");
    assert_eq!(r.shown("done"), saved.to_string_lossy());
    assert_eq!(std::fs::read_to_string(saved).unwrap(), "data");
    assert_eq!(r.status("dl"), "done");
}

#[test]
fn files_are_copied_moved_deleted_and_written() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    std::fs::create_dir(d.join("src")).unwrap();
    std::fs::write(d.join("src").join("a.txt"), "a").unwrap();
    let path = |name: &str| d.join(name).to_string_lossy().into_owned();
    let nodes = vec![
        node("mkdir", "file_op", json!({"op": "mkdir", "src": path("out/deep")})),
        node("copy", "file_op", json!({"op": "copy", "src": path("src"), "dst": path("out")})),
        node("move", "file_op", json!({"op": "move", "src": path("out/src/a.txt"), "dst": path("b.txt")})),
        node("delete", "file_op", json!({"op": "delete", "src": path("out")})),
        node("fail", "file_op", json!({"op": "copy", "src": path("missing"), "dst": path("x")})),
        node("log", "write_file", json!({"path": path("logs/today.log"), "text": "{input}"})),
        node(
            "over",
            "write_file",
            json!({"path": path("logs/today.log"), "mode": "overwrite", "newline": false}),
        ),
    ];
    let mut r = rig(nodes, vec![]);
    r.pulse("mkdir", "");
    r.pulse("copy", "");
    assert_eq!(std::fs::read_to_string(d.join("out/src/a.txt")).unwrap(), "a");
    r.pulse("copy", "");
    assert!(r.status("copy").starts_with('⚠'), "copying onto a folder that is there fails, as copytree does");
    r.pulse("move", "");
    assert!(d.join("b.txt").exists() && !d.join("out/src/a.txt").exists());
    r.pulse("delete", "");
    assert!(!d.join("out").exists());
    r.pulse("fail", "");
    assert!(r.status("fail").starts_with('⚠'));
    r.pulse("log", "one");
    r.pulse("log", "two");
    assert_eq!(std::fs::read_to_string(d.join("logs/today.log")).unwrap(), "one\ntwo\n");
    r.pulse("over", "three");
    assert_eq!(std::fs::read_to_string(d.join("logs/today.log")).unwrap(), "three");
}
