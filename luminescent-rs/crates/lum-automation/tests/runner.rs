#![allow(clippy::unwrap_used)]
//! The engine on its own thread, with a real clock: it runs, saves, and comes back where it left off.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use lum_automation::{Automation, Config, Engine, Host, Out, SystemClock};
use serde_json::json;

struct Silent;

impl Host for Silent {
    fn send(&self, _out: Out) {}
}

fn engine(saved: serde_json::Value) -> Engine {
    Engine::new(Some(saved), Arc::new(SystemClock::default()), Arc::new(Silent), Config::default())
}

fn graph() -> serde_json::Value {
    json!({"version": 1, "vars": {}, "workspaces": [{"id": "w", "name": "Clock", "nodes": [
        {"id": "tick", "type": "interval", "fields": {"seconds": 0.1}},
        {"id": "count", "type": "counter", "fields": {}}
    ], "wires": [{"id": "a", "src": ["tick", "out:tick"], "dst": ["count", "in:in"]}]}]})
}

#[test]
fn runs_in_the_background_and_saves_on_the_way_out() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("automation.json");
    let wakes = Arc::new(AtomicUsize::new(0));
    let seen = wakes.clone();
    let mut auto = Automation::start(engine(graph()), path.clone(), move || {
        seen.fetch_add(1, Ordering::Relaxed);
    });
    auto.with(|e| e.watch_pulses(true));

    // The full test suite runs slowly under Xvfb, so give it plenty of time.
    let start = Instant::now();
    while auto.read(|e| e.info("w", "count").map(|s| s.text)) < Some("3".into()) {
        assert!(start.elapsed() < Duration::from_secs(20), "the interval never got to 3");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(wakes.load(Ordering::Relaxed) > 0, "the watching editor was woken");
    auto.shutdown();

    let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let count = saved["workspaces"][0]["nodes"][1]["state"]["count"].as_i64().unwrap();
    assert!(count >= 3);
    let again = Automation::start(engine(saved), path, || {});
    assert_eq!(again.read(|e| e.info("w", "count").unwrap().text), count.to_string());
}

#[test]
fn edits_through_the_handle_reach_the_running_nodes() {
    let dir = tempfile::tempdir().unwrap();
    let mut doc = graph();
    doc["workspaces"][0]["nodes"][0]["fields"]["seconds"] = json!(3600);
    let auto = Automation::start(engine(doc), dir.path().join("automation.json"), || {});
    auto.with(|e| e.run_node("w", "tick", ""));
    let start = Instant::now();
    while auto.read(|e| e.info("w", "count").unwrap().text) != "1" {
        assert!(start.elapsed() < Duration::from_secs(20), "the pulse from ▶ never arrived");
        std::thread::sleep(Duration::from_millis(10));
    }
}
