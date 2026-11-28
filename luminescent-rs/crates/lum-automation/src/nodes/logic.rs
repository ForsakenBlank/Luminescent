//! Logic: nodes that decide where a pulse goes, and when.

use serde_json::json;

use crate::engine::{Cx, NodeRuntime, Status};
use crate::text;

struct Waiting {
    token: u64,
    t0: f64,
    secs: f64,
    input: String,
}

/// Wait: passes the pulse on after a while. Every pulse waits on its own.
#[derive(Default)]
pub struct Delay {
    waiting: Vec<Waiting>,
    next: u64,
}

impl NodeRuntime for Delay {
    fn receive(&mut self, port: &str, input: &str, cx: &mut Cx<'_>) {
        if port == "cancel" {
            self.stop(cx);
            return;
        }
        let secs = cx.num("seconds", input, 5.0).max(0.0);
        self.next += 1;
        cx.after(secs, self.next);
        self.waiting.push(Waiting { token: self.next, t0: cx.mono(), secs, input: input.to_owned() });
    }

    fn timer(&mut self, token: u64, cx: &mut Cx<'_>) {
        if let Some(i) = self.waiting.iter().position(|w| w.token == token) {
            let done = self.waiting.remove(i);
            cx.fire("out", &done.input);
        }
    }

    fn stop(&mut self, cx: &mut Cx<'_>) {
        cx.cancel_all();
        self.waiting.clear();
    }

    fn info(&self, _status: &Status, now: f64) -> Status {
        let Some(first) = self.waiting.first() else { return Status::default() };
        let left = (first.secs - (now - first.t0)).max(0.0);
        let more = match self.waiting.len() {
            1 => String::new(),
            n => format!(" (+{} waiting)", n - 1),
        };
        Status {
            text: format!("{left:.1} s{more}"),
            progress: Some((1.0 - left / first.secs.max(0.01)) as f32),
        }
    }
}

/// If: sends the pulse out of yes or no.
pub struct If;

impl NodeRuntime for If {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let a = cx.f("a", input);
        let b = cx.f("b", input);
        let ok = text::compare(&a, &cx.raw_str("op"), &b);
        cx.set_status(if ok { "yes" } else { "no" }, None);
        cx.fire(if ok { "yes" } else { "no" }, input);
    }
}

/// Gate: lets pulses through only while it is open.
#[derive(Default)]
pub struct Gate {
    open: bool,
}

impl Gate {
    fn show(&self, cx: &mut Cx<'_>) {
        cx.set_status(if self.open { "OPEN" } else { "CLOSED" }, None);
    }
}

impl NodeRuntime for Gate {
    fn start(&mut self, cx: &mut Cx<'_>) {
        self.open = cx.flag("start_open");
        self.show(cx);
    }

    fn receive(&mut self, port: &str, input: &str, cx: &mut Cx<'_>) {
        match port {
            "in" => {
                if self.open {
                    cx.fire("out", input);
                }
                return;
            }
            "open" => self.open = true,
            "close" => self.open = false,
            _ => self.open = !self.open,
        }
        self.show(cx);
    }

    fn value(&self, _port: &str, _input: &str, _cx: &mut Cx<'_>) -> String {
        yes_no(self.open)
    }
}

pub fn yes_no(b: bool) -> String {
    if b { "yes".into() } else { "no".into() }
}

/// Count: out passes every pulse on with the count; every fires each Nth time. The count is saved.
#[derive(Default)]
pub struct Counter {
    count: i64,
}

impl NodeRuntime for Counter {
    fn start(&mut self, cx: &mut Cx<'_>) {
        self.count = match cx.state().get("count") {
            Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(0.0) as i64,
            Some(serde_json::Value::String(s)) => s.trim().parse().unwrap_or(0),
            _ => 0,
        };
        cx.set_status(self.count.to_string(), None);
    }

    fn receive(&mut self, port: &str, input: &str, cx: &mut Cx<'_>) {
        if port == "reset" {
            self.count = 0;
        } else {
            self.count += 1;
            let every = (cx.num("every", input, 3.0) as i64).max(1);
            cx.fire("out", &self.count.to_string());
            if self.count % every == 0 {
                cx.fire("every", &self.count.to_string());
            }
        }
        cx.save_state("count", json!(self.count));
        cx.set_status(self.count.to_string(), None);
    }

    fn value(&self, _port: &str, _input: &str, _cx: &mut Cx<'_>) -> String {
        self.count.to_string()
    }
}

/// Only once: lets the first pulse through, then nothing until reset.
#[derive(Default)]
pub struct Once {
    used: bool,
}

impl NodeRuntime for Once {
    fn receive(&mut self, port: &str, input: &str, cx: &mut Cx<'_>) {
        if port == "reset" {
            self.used = false;
            cx.set_status("", None);
        } else if !self.used {
            self.used = true;
            cx.set_status("used", None);
            cx.fire("out", input);
        }
    }
}

/// Cooldown: lets a pulse through, then ignores the next ones for a while.
pub struct Cooldown {
    last: f64,
}

impl Default for Cooldown {
    fn default() -> Cooldown {
        Cooldown { last: f64::NEG_INFINITY }
    }
}

impl NodeRuntime for Cooldown {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let now = cx.mono();
        if now - self.last >= cx.num("seconds", "", 10.0) {
            self.last = now;
            cx.fire("out", input);
        }
    }
}

/// Sequence: fires its outputs one after another, first to fourth.
pub struct Sequence;

impl NodeRuntime for Sequence {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        for port in &cx.spec.outs {
            cx.fire(port, input);
        }
    }
}

/// Take turns: each pulse goes out of the next output.
#[derive(Default)]
pub struct Turns {
    i: usize,
}

impl NodeRuntime for Turns {
    fn receive(&mut self, port: &str, input: &str, cx: &mut Cx<'_>) {
        if port == "reset" {
            self.i = 0;
            return;
        }
        let n = (cx.num("count", "", 2.0) as i64).clamp(1, 4) as usize;
        let out = &cx.spec.outs[self.i % n];
        cx.fire(out, input);
        self.i += 1;
    }
}

/// Random chance: yes or no, by the odds you set.
pub struct Chance;

impl NodeRuntime for Chance {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let percent = cx.num("percent", input, 50.0);
        let yes = cx.rng().f64() * 100.0 < percent;
        cx.fire(if yes { "yes" } else { "no" }, input);
    }
}

/// Repeat: fires each the number of times (its input counts 1, 2, 3…), then done.
#[derive(Default)]
pub struct Repeat {
    timer: Option<u64>,
    gap: f64,
    left: i64,
    n: i64,
    input: String,
}

impl Repeat {
    fn step(&mut self, cx: &mut Cx<'_>) {
        self.timer = None;
        if self.left <= 0 {
            cx.fire("done", &self.input);
            return;
        }
        self.left -= 1;
        self.n += 1;
        cx.fire("each", &self.n.to_string());
        if self.left <= 0 {
            cx.fire("done", &self.input);
        } else {
            self.timer = Some(cx.after(self.gap, 0));
        }
    }
}

impl NodeRuntime for Repeat {
    fn receive(&mut self, port: &str, input: &str, cx: &mut Cx<'_>) {
        if let Some(t) = self.timer.take() {
            cx.cancel(t);
        }
        if port == "stop" {
            self.left = 0;
            return;
        }
        self.n = 0;
        self.left = (cx.num("times", input, 3.0) as i64).max(0);
        self.input = input.to_owned();
        self.gap = cx.num("gap", input, 1.0).max(0.0);
        self.step(cx);
    }

    fn timer(&mut self, _token: u64, cx: &mut Cx<'_>) {
        self.step(cx);
    }

    fn info(&self, _status: &Status, _now: f64) -> Status {
        match self.timer {
            Some(_) => Status { text: format!("{} to go", self.left), progress: None },
            None => Status::default(),
        }
    }
}
