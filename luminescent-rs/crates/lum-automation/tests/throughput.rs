#![allow(clippy::unwrap_used)]
//! The budget: 1 000 pulses a second must cost the engine little enough that nothing else notices.

use std::sync::Arc;
use std::time::{Duration, Instant};

use jiff::civil::DateTime;
use lum_automation::{Clock, Config, Engine, FakeClock, Host, Out};
use serde_json::json;

struct Silent;

impl Host for Silent {
    fn send(&self, _out: Out) {}
}

#[test]
fn a_thousand_pulses_a_second_cost_little() {
    // Repeat every millisecond, through If and Join into Show and Count: three wires and a value pull per pulse.
    let saved = json!({"version": 1, "vars": {}, "workspaces": [{"id": "w", "name": "Busy", "nodes": [
        {"id": "rep", "type": "repeat", "fields": {"times": 1000, "gap": 0.001}},
        {"id": "if", "type": "if", "fields": {"a": "{input}", "op": "gt", "b": "0"}},
        {"id": "join", "type": "join", "fields": {"a": "pulse", "b": "{input}"}},
        {"id": "show", "type": "show", "fields": {}},
        {"id": "count", "type": "counter", "fields": {}},
        {"id": "set", "type": "var_set", "fields": {"name": "last"}}
    ], "wires": [
        {"id": "1", "src": ["rep", "out:each"], "dst": ["if", "in:in"]},
        {"id": "2", "src": ["if", "out:yes"], "dst": ["show", "in:in"]},
        {"id": "3", "src": ["show", "out:out"], "dst": ["count", "in:in"]},
        {"id": "4", "src": ["count", "out:out"], "dst": ["set", "in:run"]},
        {"id": "5", "src": ["join", "val:text"], "dst": ["set", "field:value"]}
    ]}]});
    let clock = Arc::new(FakeClock::new(DateTime::constant(2026, 11, 23, 14, 30, 0, 0)));
    let config = Config { max_rate: 5000, ..Config::default() };
    let mut engine = Engine::new(Some(saved), clock.clone(), Arc::new(Silent), config);
    engine.start();
    engine.watch_pulses(true);
    engine.run_node("w", "rep", "go");

    let started = Instant::now();
    let mut longest = Duration::ZERO;
    let mut passes = 0;
    loop {
        let pass = Instant::now();
        while engine.step() {}
        longest = longest.max(pass.elapsed());
        passes += 1;
        engine.take_updates();
        match engine.next_deadline() {
            Some(at) => clock.advance((at - clock.mono()).max(0.0)),
            None => break,
        }
    }
    let total = started.elapsed();
    assert_eq!(engine.get_var("last"), "pulse 1000");
    assert!(engine.is_running("w"));
    println!("1 000 pulses (4 000 deliveries) in {total:?}, {passes} passes, the longest {longest:?}");
    // A debug build on a busy machine: a second of pulses has to take well under a second of work, and no pass may
    // hold the engine for a frame.
    assert!(total < Duration::from_millis(500), "{total:?}");
    assert!(longest < Duration::from_millis(16), "{longest:?}");
}
