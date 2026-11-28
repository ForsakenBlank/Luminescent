//! Triggers: nodes that start pulses by themselves.

use crate::engine::{Cx, NodeRuntime, Status};

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

    fn info(&self, _status: &Status, now: f64) -> Status {
        if self.timer.is_none() {
            return Status { text: "stopped".into(), progress: None };
        }
        let left = (self.period - (now - self.t0)).max(0.0);
        let text = if left >= 1.0 { format!("next in {left:.0} s") } else { "now".into() };
        Status { text, progress: Some((1.0 - left / self.period.max(0.1)) as f32) }
    }
}
