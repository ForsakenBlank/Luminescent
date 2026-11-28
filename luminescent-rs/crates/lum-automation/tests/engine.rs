#![allow(clippy::unwrap_used)]
//! Graphs run on a fake clock: what arrives where, in which order, and when.

use std::sync::{Arc, Mutex};

use jiff::civil::DateTime;
use lum_automation::{Clock, Config, Engine, FakeClock, Host, Out};
use serde_json::{Value, json};

#[derive(Default)]
struct Recorder {
    outs: Mutex<Vec<Out>>,
}

impl Host for Recorder {
    fn clipboard_text(&self) -> String {
        "from the clipboard".into()
    }

    fn send(&self, out: Out) {
        self.outs.lock().unwrap().push(out);
    }
}

impl Recorder {
    fn notes(&self) -> Vec<String> {
        let outs = self.outs.lock().unwrap();
        outs.iter()
            .filter_map(|o| match o {
                Out::Notify { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }
}

struct Rig {
    engine: Engine,
    clock: Arc<FakeClock>,
    host: Arc<Recorder>,
}

fn node(id: &str, ty: &str, fields: Value) -> Value {
    json!({"id": id, "type": ty, "fields": fields})
}

fn wire(src: &str, out: &str, dst: &str, inp: &str) -> Value {
    json!({"src": [src, out], "dst": [dst, inp]})
}

fn rig(nodes: Vec<Value>, wires: Vec<Value>) -> Rig {
    rig_with(nodes, wires, Config::default())
}

fn rig_with(nodes: Vec<Value>, wires: Vec<Value>, config: Config) -> Rig {
    let saved = json!({"version": 1, "vars": {}, "workspaces": [{"id": "w", "name": "Test", "nodes": nodes, "wires": wires}]});
    rig_from(saved, config)
}

fn rig_from(saved: Value, config: Config) -> Rig {
    let clock = Arc::new(FakeClock::new(DateTime::constant(2026, 11, 23, 14, 30, 0, 0)));
    let host = Arc::new(Recorder::default());
    let mut engine = Engine::new(Some(saved), clock.clone(), host.clone(), config);
    engine.seed(7);
    engine.start();
    engine.step();
    Rig { engine, clock, host }
}

impl Rig {
    /// Lets `secs` pass, stopping at every moment something is due, as the engine's thread would.
    fn run_for(&mut self, secs: f64) {
        let end = self.clock.mono() + secs;
        loop {
            while self.engine.step() {}
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

    fn var(&self, name: &str) -> String {
        self.engine.get_var(name)
    }

    fn run(&mut self, nid: &str, input: &str) {
        self.engine.run_node("w", nid, input);
        while self.engine.step() {}
    }
}

#[test]
fn an_interval_ticks_on_time_and_counts() {
    let mut r = rig(
        vec![
            node("tick", "interval", json!({"seconds": 2})),
            node("count", "counter", json!({"every": 2})),
            node("show", "show", json!({})),
        ],
        vec![wire("tick", "out:tick", "count", "in:in"), wire("count", "out:every", "show", "in:in")],
    );
    r.run_for(1.9);
    assert_eq!(r.status("count"), "0");
    assert_eq!(r.status("tick"), "now");
    r.run_for(0.1);
    assert_eq!(r.status("count"), "1");
    assert_eq!(r.status("show"), "");
    r.run_for(2.0);
    assert_eq!(r.status("count"), "2");
    assert_eq!(r.status("show"), "2");
    r.run_for(1.0);
    let info = r.engine.info("w", "tick").unwrap();
    assert_eq!(info.text, "next in 1 s");
    assert_eq!(info.progress, Some(0.5));
}

#[test]
fn pulses_arrive_on_the_next_pass_in_order() {
    let mut r = rig(
        vec![
            node("seq", "sequence", json!({})),
            node("a", "var_set", json!({"name": "order", "value": "1", "mode": "append"})),
            node("b", "var_set", json!({"name": "order", "value": "2", "mode": "append"})),
            node("c", "var_set", json!({"name": "order", "value": "3", "mode": "append"})),
            node("d", "var_set", json!({"name": "order", "value": "4", "mode": "append"})),
        ],
        vec![
            wire("seq", "out:fourth", "d", "in:run"),
            wire("seq", "out:first", "a", "in:run"),
            wire("seq", "out:second", "b", "in:run"),
            wire("seq", "out:third", "c", "in:run"),
        ],
    );
    r.engine.run_node("w", "seq", "go");
    assert_eq!(r.var("order"), "", "nothing arrives inside the fire that sent it");
    while r.engine.step() {}
    assert_eq!(r.var("order"), "1234");
}

#[test]
fn values_are_pulled_through_wires() {
    let mut r = rig(
        vec![
            node("btn", "manual", json!({})),
            node("name", "text", json!({"text": "Hello {input} and {var:who}"})),
            node("sum", "math", json!({"a": "{input}", "op": "×", "b": "2,5"})),
            node("join", "join", json!({"sep": " / "})),
            node("set", "var_set", json!({"name": "out"})),
        ],
        vec![
            wire("btn", "out:fire", "set", "in:run"),
            wire("name", "val:text", "join", "field:a"),
            wire("sum", "val:result", "join", "field:c"),
            wire("join", "val:text", "set", "field:value"),
        ],
    );
    r.engine.set_var("who", "Ada");
    r.run("btn", "4");
    assert_eq!(r.var("out"), "Hello 4 and Ada / 10");
    assert_eq!(r.status("set"), "out = Hello 4 and Ada / 10");
}

#[test]
fn values_wired_into_each_other_do_not_hang() {
    let mut r = rig(
        vec![
            node("btn", "manual", json!({})),
            node("x", "join", json!({"b": "x"})),
            node("y", "join", json!({"b": "y"})),
            node("set", "var_set", json!({"name": "loop"})),
        ],
        vec![
            wire("btn", "out:fire", "set", "in:run"),
            wire("x", "val:text", "y", "field:a"),
            wire("y", "val:text", "x", "field:a"),
            wire("x", "val:text", "set", "field:value"),
        ],
    );
    r.run("btn", "");
    // Python's depth limit: 41 nested reads, the innermost finding nothing wired that deep.
    let chain: Vec<&str> = (0..41).map(|i| if i % 2 == 0 { "x" } else { "y" }).collect();
    assert_eq!(r.var("loop"), chain.join(" "));
}

#[test]
fn a_wire_loop_trips_the_safety_stop() {
    let mut r = rig_with(
        vec![node("show", "show", json!({})), node("count", "counter", json!({}))],
        vec![wire("show", "out:out", "show", "in:in"), wire("show", "out:out", "count", "in:in")],
        Config { max_rate: 50, ..Config::default() },
    );
    r.run("show", "round and round");
    assert!(!r.engine.is_running("w"));
    assert_eq!(r.engine.workspace("w").unwrap()["enabled"], json!(false));
    let notes = r.host.notes();
    assert_eq!(notes.len(), 1);
    assert_eq!(
        notes[0],
        "“Test” fired more than 50 times in a second, so it was switched off. Look for a wire loop."
    );
    assert_eq!(r.engine.next_deadline(), None);
    assert_eq!(r.engine.to_json()["workspaces"][0]["nodes"][1]["state"]["count"], json!(25));
}

#[test]
fn wait_holds_each_pulse_on_its_own() {
    let mut r = rig(
        vec![node("wait", "delay", json!({"seconds": 5})), node("show", "show", json!({}))],
        vec![wire("wait", "out:out", "show", "in:in")],
    );
    r.run("wait", "first");
    r.run_for(1.0);
    r.run("wait", "second");
    assert_eq!(r.status("wait"), "4.0 s (+1 waiting)");
    r.run_for(4.0);
    assert_eq!(r.status("show"), "first");
    assert_eq!(r.status("wait"), "1.0 s");
    r.run_for(1.0);
    assert_eq!(r.status("show"), "second");
    assert_eq!(r.status("wait"), "");
    r.run("wait", "third");
    deliver(&mut r, "wait", "cancel", "");
    r.run_for(10.0);
    assert_eq!(r.status("show"), "second");
}

#[test]
fn repeat_counts_with_gaps_then_says_done() {
    let mut r = rig(
        vec![
            node("rep", "repeat", json!({"times": 3, "gap": 2})),
            node("each", "var_set", json!({"name": "each", "mode": "append"})),
            node("done", "var_set", json!({"name": "done"})),
        ],
        vec![wire("rep", "out:each", "each", "in:run"), wire("rep", "out:done", "done", "in:run")],
    );
    r.run("rep", "go");
    assert_eq!((r.var("each"), r.status("rep")), ("1".into(), "2 to go".into()));
    r.run_for(2.0);
    assert_eq!(r.var("each"), "12");
    r.run_for(2.0);
    assert_eq!((r.var("each"), r.var("done")), ("123".into(), "go".into()));
    assert_eq!(r.engine.next_deadline(), None);
}

#[test]
fn a_count_survives_a_restart() {
    let mut r = rig(vec![node("count", "counter", json!({}))], vec![]);
    r.run("count", "");
    r.run("count", "");
    assert!(r.engine.take_dirty());
    let saved = r.engine.to_json();
    assert_eq!(saved["workspaces"][0]["nodes"][0]["state"], json!({"count": 2}));
    let r = rig_from(saved, Config::default());
    assert_eq!(r.status("count"), "2");
}

#[test]
fn paused_and_switched_off_workspaces_say_so() {
    let mut r = rig_with(
        vec![node("btn", "manual", json!({}))],
        vec![],
        Config { enabled: false, ..Config::default() },
    );
    r.run("btn", "");
    assert_eq!(r.host.notes(), ["Automations are paused. Turn it on at the bottom of the Automation page."]);
    r.engine.set_config(Config::default());
    r.engine.set_enabled("w", false);
    r.run("btn", "");
    assert_eq!(r.host.notes()[1], "“Test” is switched off. Turn it on at the bottom of the Automation page.");
}

#[test]
fn edits_keep_running_nodes_running() {
    let mut r = rig(vec![node("tick", "interval", json!({"seconds": 1}))], vec![]);
    r.run_for(3.0);
    assert_eq!(r.engine.info("w", "tick").unwrap().text, "next in 1 s");
    let added = lum_automation::doc::node("show", 0.0, 0.0);
    let show_id = added["id"].as_str().unwrap().to_owned();
    let ws = lum_automation::doc::workspace_mut(r.engine.doc_mut(), "w").unwrap();
    ws["nodes"].as_array_mut().unwrap().push(added);
    ws["wires"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": "w1", "src": ["tick", "out:tick"], "dst": [show_id, "in:in"]}));
    r.engine.edited("w", true, "");
    r.run_for(1.0);
    assert_eq!(r.engine.info("w", &show_id).unwrap().text, "4", "the interval kept its count");

    let ws = lum_automation::doc::workspace_mut(r.engine.doc_mut(), "w").unwrap();
    ws["nodes"][0]["fields"]["seconds"] = json!(10);
    r.engine.edited("w", false, "tick");
    r.run_for(5.0);
    assert_eq!(r.engine.info("w", &show_id).unwrap().text, "4");
    r.run_for(5.0);
    assert_eq!(r.engine.info("w", &show_id).unwrap().text, "1", "a trigger with a new time starts again");
}

#[test]
fn the_editor_sees_pulses_only_while_it_watches() {
    let mut r = rig(
        vec![node("btn", "manual", json!({})), node("show", "show", json!({}))],
        vec![json!({"id": "the-wire", "src": ["btn", "out:fire"], "dst": ["show", "in:in"]})],
    );
    r.run("btn", "unseen");
    assert!(r.engine.take_updates().pulses.is_empty());
    r.engine.watch_pulses(true);
    r.run("btn", &"x".repeat(400));
    let updates = r.engine.take_updates();
    assert_eq!(updates.pulses.len(), 1);
    assert_eq!(updates.pulses[0].wire, "the-wire");
    assert_eq!(updates.pulses[0].text.len(), 300);
    assert!(updates.nodes.contains(&("w".to_owned(), "show".to_owned())));
}

#[test]
fn logic_nodes_route_pulses() {
    let mut r = rig(
        vec![
            node("if", "if", json!({"a": "{input}", "op": "gt", "b": "10"})),
            node("gate", "gate", json!({"start_open": false})),
            node("once", "once", json!({})),
            node("turns", "turns", json!({"count": 3})),
            node("cool", "cooldown", json!({"seconds": 10})),
            node("yes", "var_set", json!({"name": "yes", "mode": "append"})),
            node("no", "var_set", json!({"name": "no", "mode": "append"})),
            node("t1", "var_set", json!({"name": "turns", "value": "1", "mode": "append"})),
            node("t2", "var_set", json!({"name": "turns", "value": "2", "mode": "append"})),
            node("t3", "var_set", json!({"name": "turns", "value": "3", "mode": "append"})),
            node("gated", "var_set", json!({"name": "gated", "mode": "append"})),
            node("onced", "var_set", json!({"name": "once", "mode": "append"})),
            node("cooled", "var_set", json!({"name": "cool", "mode": "append"})),
        ],
        vec![
            wire("if", "out:yes", "yes", "in:run"),
            wire("if", "out:no", "no", "in:run"),
            wire("turns", "out:first", "t1", "in:run"),
            wire("turns", "out:second", "t2", "in:run"),
            wire("turns", "out:third", "t3", "in:run"),
            wire("gate", "out:out", "gated", "in:run"),
            wire("once", "out:out", "onced", "in:run"),
            wire("cool", "out:out", "cooled", "in:run"),
        ],
    );
    for n in ["5", "12", "10,5"] {
        r.run("if", n);
    }
    assert_eq!((r.var("yes"), r.var("no")), ("1210,5".into(), "5".into()));

    for _ in 0..5 {
        r.run("turns", "");
    }
    assert_eq!(r.var("turns"), "12312");

    r.run("gate", "shut");
    assert_eq!(r.status("gate"), "CLOSED");
    r.engine.doc_mut();
    for (port, text) in [("open", ""), ("in", "a"), ("toggle", ""), ("in", "b")] {
        deliver(&mut r, "gate", port, text);
    }
    assert_eq!(r.var("gated"), "a");

    r.run("once", "1");
    r.run("once", "2");
    deliver(&mut r, "once", "reset", "");
    r.run("once", "3");
    assert_eq!(r.var("once"), "13");

    r.run("cool", "a");
    r.run_for(9.0);
    r.run("cool", "b");
    r.run_for(1.0);
    r.run("cool", "c");
    assert_eq!(r.var("cool"), "ac");
}

/// Sends a pulse into any input, through a Button wired there for the occasion.
fn deliver(r: &mut Rig, nid: &str, port: &str, text: &str) {
    let id = format!("btn-{nid}-{port}");
    let doc = r.engine.doc_mut();
    let ws = lum_automation::doc::workspace_mut(doc, "w").unwrap();
    if lum_automation::doc::node_in(ws, &id).is_none() {
        let mut btn = lum_automation::doc::node("manual", 0.0, 0.0);
        btn["id"] = json!(id);
        ws["nodes"].as_array_mut().unwrap().push(btn);
        ws["wires"].as_array_mut().unwrap().push(
            json!({"id": format!("{id}-w"), "src": [id, "out:fire"], "dst": [nid, format!("in:{port}")]}),
        );
        r.engine.edited("w", true, "");
    }
    r.run(&id, text);
}

#[test]
fn value_nodes_read_like_python() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("note.txt");
    std::fs::write(&file, "  first line\nsecond\nlast  \n").unwrap();
    let mut r = rig(
        vec![
            node("btn", "manual", json!({})),
            node("file", "read_file", json!({"path": file.to_str().unwrap()})),
            node("last", "change", json!({"op": "last"})),
            node("upper", "change", json!({"op": "upper"})),
            node("swap", "replace", json!({"find": "(\\w+) (\\w+)", "with": "\\2 \\1", "regex": true})),
            node("when", "now", json!({"format": "%A %d %B, %H:%M"})),
            node("clip", "clip_get", json!({})),
            node("num", "number", json!({"number": "2,50"})),
            node("dice", "random_num", json!({"min": 6, "max": 1})),
            node("set_last", "var_set", json!({"name": "last"})),
            node("set_upper", "var_set", json!({"name": "upper"})),
            node("set_when", "var_set", json!({"name": "when"})),
            node("set_clip", "var_set", json!({"name": "clip"})),
            node("set_num", "var_set", json!({"name": "num"})),
            node("set_dice", "var_set", json!({"name": "dice"})),
            node("get", "var_get", json!({"name": "num"})),
        ],
        vec![
            wire("file", "val:text", "last", "field:text"),
            wire("last", "val:text", "swap", "field:text"),
            wire("swap", "val:text", "set_last", "field:value"),
            wire("clip", "val:text", "upper", "field:text"),
            wire("upper", "val:text", "set_upper", "field:value"),
            wire("when", "val:text", "set_when", "field:value"),
            wire("clip", "val:text", "set_clip", "field:value"),
            wire("num", "val:number", "set_num", "field:value"),
            wire("dice", "val:number", "set_dice", "field:value"),
            wire("btn", "out:fire", "set_last", "in:run"),
            wire("btn", "out:fire", "set_upper", "in:run"),
            wire("btn", "out:fire", "set_when", "in:run"),
            wire("btn", "out:fire", "set_clip", "in:run"),
            wire("btn", "out:fire", "set_num", "in:run"),
            wire("btn", "out:fire", "set_dice", "in:run"),
        ],
    );
    assert_eq!(r.status("get"), "(empty)");
    r.run("btn", "");
    assert_eq!(r.var("last"), "last");
    assert_eq!(r.var("upper"), "FROM THE CLIPBOARD");
    assert_eq!(r.var("when"), "Monday 23 November, 14:30");
    assert_eq!(r.var("clip"), "from the clipboard");
    assert_eq!(r.var("num"), "2.5");
    assert_eq!(r.status("get"), "= 2.5");
    let dice: i64 = r.var("dice").parse().unwrap();
    assert!((1..=6).contains(&dice));
}

#[test]
fn nodes_from_later_phases_say_they_are_not_here_yet() {
    let r = rig(vec![node("paper", "wallpaper", json!({}))], vec![]);
    assert_eq!(r.status("paper"), "not available yet");
}
