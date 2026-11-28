//! The engine: nodes wired together, running in the background.
//!
//! A pulse leaves a flow output (`out:done`) and arrives at the flow inputs wired to it (`in:run`), carrying a
//! bit of text along: its input. A value output (`val:text`) is read when a node needs it: a field with a ◆
//! next to it (`field:commands`) takes its value from the wire plugged into it instead of its box.
//!
//! Pulses are delivered on the next pass, never inside the `fire` that sent them, so a node wired back into
//! itself can not recurse; a workspace that fires more than `automation.max_rate` times a second is switched
//! off. The engine itself owns no thread and reads time only from its [`Clock`]: [`Engine::step`] does what is
//! due and [`Engine::next_deadline`] says when to call it again. [`crate::Automation`] runs it on a thread of
//! its own; tests drive it with a [`FakeClock`].

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use jiff::civil::DateTime;
use serde_json::{Map, Value, json};

use crate::doc::{self, str_of};
use crate::spec::{self, NodeSpec};
use crate::text::{self, MAX_TEXT, clip_chars};

/// Where the engine gets the time from.
pub trait Clock: Send + Sync {
    /// Local time, for `{date}`, the Date / time node and the time-of-day triggers.
    fn local(&self) -> DateTime;
    /// Seconds from some fixed start, for timers and the safety stop.
    fn mono(&self) -> f64;
}

#[derive(Debug)]
pub struct SystemClock {
    start: Instant,
}

impl Default for SystemClock {
    fn default() -> SystemClock {
        SystemClock { start: Instant::now() }
    }
}

impl Clock for SystemClock {
    fn local(&self) -> DateTime {
        jiff::ZonedDateTime::now().datetime()
    }

    fn mono(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }
}

/// A clock that only moves when told to; both its readings move together.
#[derive(Debug)]
pub struct FakeClock(Mutex<(DateTime, f64)>);

impl FakeClock {
    pub fn new(local: DateTime) -> FakeClock {
        FakeClock(Mutex::new((local, 0.0)))
    }

    pub fn advance(&self, secs: f64) {
        let mut t = self.0.lock().unwrap_or_else(|e| e.into_inner());
        t.1 += secs;
        let micros = (secs * 1e6).round() as i64;
        t.0 = t.0.checked_add(jiff::SignedDuration::from_micros(micros)).unwrap_or(t.0);
    }
}

impl Clock for FakeClock {
    fn local(&self) -> DateTime {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).0
    }

    fn mono(&self) -> f64 {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).1
    }
}

/// What the engine asks of the rest of Luminescent: cards, actions, the overlay... The app answers on its own
/// thread; nothing here waits for an answer.
#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    Notify { title: String, text: String, ms: u32 },
}

/// The engine's way out to the app and the desktop. Calls come from the engine's thread.
pub trait Host: Send + Sync {
    fn clipboard_text(&self) -> String {
        String::new()
    }

    fn send(&self, out: Out);
}

/// The Automation section of Settings, as the engine needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// "Run automations": off pauses every workspace.
    pub enabled: bool,
    pub max_rate: u32,
    pub shell: String,
    pub command_timeout: u64,
}

impl Default for Config {
    fn default() -> Config {
        Config { enabled: true, max_rate: 200, shell: "bash".into(), command_timeout: 0 }
    }
}

/// A node's status line and progress bar, as the editor shows them under it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Status {
    pub text: String,
    pub progress: Option<f32>,
}

/// One running node. Every method gets a [`Cx`] to read its fields, fire, set its status and ask for timers.
pub trait NodeRuntime: Send {
    fn start(&mut self, _cx: &mut Cx<'_>) {}

    /// The node is going away (removed, switched off, its workspace stopped). Its timers go with it.
    fn stop(&mut self, _cx: &mut Cx<'_>) {}

    /// A pulse arrived at the flow input `port`.
    fn receive(&mut self, _port: &str, _input: &str, _cx: &mut Cx<'_>) {}

    /// Someone reads the value output `port`.
    fn value(&self, _port: &str, _input: &str, _cx: &mut Cx<'_>) -> String {
        String::new()
    }

    /// A timer it asked for with [`Cx::after`] is due.
    fn timer(&mut self, _token: u64, _cx: &mut Cx<'_>) {}

    /// ▶ on the node, the palette or the tray: a trigger fires, anything else runs as if a pulse came in.
    fn run_now(&mut self, input: &str, cx: &mut Cx<'_>) {
        let spec = cx.spec;
        if spec.is_trigger() {
            if let Some(port) = spec.outs.first() {
                cx.fire(port, input);
            }
        } else if let Some(port) = spec.ins.first() {
            self.receive(port, input, cx);
        }
    }

    /// A variable changed somewhere (the Variable node shows its value).
    fn vars_changed(&mut self, _cx: &mut Cx<'_>) {}

    /// What the editor shows under the node. Live nodes work it out from the time.
    fn info(&self, status: &Status, _now: f64) -> Status {
        status.clone()
    }
}

/// A pulse on its way: for the editor's animation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pulse {
    pub workspace: String,
    pub wire: String,
    pub text: String,
}

/// What changed since the editor last looked.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Updates {
    pub pulses: Vec<Pulse>,
    /// Nodes whose status line changed: (workspace, node).
    pub nodes: Vec<(String, String)>,
    /// Workspaces added, removed, renamed, switched on or off.
    pub workspaces: bool,
    pub vars: bool,
}

impl Updates {
    pub fn is_empty(&self) -> bool {
        self.pulses.is_empty() && self.nodes.is_empty() && !self.workspaces && !self.vars
    }
}

/// Pulses kept for the editor between two looks; more than this in one frame could not be seen anyway.
const MAX_PULSES: usize = 2000;
/// How much one call to `step` does before it lets go, so a node that keeps itself busy can not hold the lock.
const STEP_BUDGET: usize = 4096;
/// How deep value pulls may nest (a value wired into itself).
const MAX_DEPTH: u32 = 40;

struct LiveNode {
    spec: &'static NodeSpec,
    rt: RefCell<Box<dyn NodeRuntime>>,
    status: RefCell<Status>,
}

struct Wire {
    id: String,
    dst: String,
    port: String,
}

#[derive(Default)]
struct WsLive {
    nodes: HashMap<String, LiveNode>,
    /// (node, `out:port`) to the wires leaving it.
    out: HashMap<(String, String), Vec<Wire>>,
    /// (node, `field:key`) to the (node, `val:port`) wired into it.
    inn: HashMap<(String, String), (String, String)>,
}

/// What waits for the next pass: a pulse, or a workspace to switch on or off. Both go through one queue, as
/// Python's `QTimer.singleShot(0, ...)` did, so pulses sent before a switch still arrive.
enum Queued {
    Pulse { ws: String, node: String, port: String, text: String },
    Switch { ws: String, on: bool },
}

struct TimerTarget {
    ws: String,
    node: String,
    token: u64,
}

enum Effect {
    Fire { ws: String, node: String, port: String, text: String },
    SaveState { ws: String, node: String, key: String, value: Value },
    SetEnabled { ws: String, on: bool },
    VarsChanged,
}

/// Everything a node may change while it runs. Kept apart from the document and the running nodes, which are
/// only borrowed while a node runs.
struct Core {
    clock: Arc<dyn Clock>,
    host: Arc<dyn Host>,
    config: Config,
    vars: Map<String, Value>,
    timers: BTreeMap<(i64, u64), TimerTarget>,
    timer_keys: HashMap<u64, (i64, u64)>,
    next_timer: u64,
    effects: Vec<Effect>,
    rate: HashMap<String, (f64, u32)>,
    depth: u32,
    rng: fastrand::Rng,
    updates: Updates,
    watch_pulses: bool,
    dirty: bool,
    last: HashMap<(String, String), String>,
}

pub struct Engine {
    doc: Value,
    live: HashMap<String, WsLive>,
    queue: VecDeque<Queued>,
    started: bool,
    core: Core,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("workspaces", &doc::workspaces(&self.doc).len())
            .finish_non_exhaustive()
    }
}

static NULL: Value = Value::Null;

impl Engine {
    /// `saved` is what `automation.json` held (`None` for the first start); it is repaired as Python's `_load`
    /// repairs it. Nothing runs until [`Engine::start`].
    pub fn new(saved: Option<Value>, clock: Arc<dyn Clock>, host: Arc<dyn Host>, config: Config) -> Engine {
        let mut doc = doc::load(saved);
        let vars = match doc.get_mut("vars").map(Value::take) {
            Some(Value::Object(vars)) => vars,
            _ => Map::new(),
        };
        Engine {
            doc,
            live: HashMap::new(),
            queue: VecDeque::new(),
            started: false,
            core: Core {
                clock,
                host,
                config,
                vars,
                timers: BTreeMap::new(),
                timer_keys: HashMap::new(),
                next_timer: 1,
                effects: Vec::new(),
                rate: HashMap::new(),
                depth: 0,
                rng: fastrand::Rng::new(),
                updates: Updates::default(),
                watch_pulses: false,
                dirty: false,
                last: HashMap::new(),
            },
        }
    }

    /// Makes Random chance and Random number repeatable (tests).
    pub fn seed(&mut self, seed: u64) {
        self.core.rng = fastrand::Rng::with_seed(seed);
    }

    // The document

    /// The whole file as it would be saved.
    pub fn to_json(&self) -> Value {
        let mut out = self.doc.clone();
        out["vars"] = Value::Object(self.core.vars.clone());
        out
    }

    pub fn save_text(&self) -> String {
        lum_core::pyjson::to_string(&self.to_json(), lum_core::pyjson::Style::indented(2))
    }

    /// The document without its variables (they live in [`Engine::vars`] while running).
    pub fn doc(&self) -> &Value {
        &self.doc
    }

    /// For the editor. Call [`Engine::edited`] after a change so the running nodes follow it.
    pub fn doc_mut(&mut self) -> &mut Value {
        self.core.dirty = true;
        &mut self.doc
    }

    /// Whether something changed that should be saved since the last call.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.core.dirty)
    }

    pub fn workspace(&self, wid: &str) -> Option<&Value> {
        doc::workspace(&self.doc, wid)
    }

    pub fn active(&self) -> String {
        let active = str_of(&self.doc, "active");
        match self.workspace(active) {
            Some(_) => active.to_owned(),
            None => {
                doc::workspaces(&self.doc).first().map(|w| str_of(w, "id").to_owned()).unwrap_or_default()
            }
        }
    }

    pub fn set_active(&mut self, wid: &str) {
        if self.workspace(wid).is_some() {
            self.doc["active"] = json!(wid);
            self.core.dirty = true;
        }
    }

    /// A new workspace (a copy of `copy_from` when given), made the active one.
    pub fn add_workspace(&mut self, name: &str, copy_from: Option<&Value>) -> String {
        let ws = match copy_from {
            Some(from) => doc::copy_workspace(from, name),
            None => doc::blank(name),
        };
        self.push_workspace(ws)
    }

    /// A workspace read from a `.lumflow` file: new ids, switched off until you look at it.
    pub fn import_workspace(&mut self, file: &Value) -> Option<String> {
        let ws = doc::imported_workspace(file)?;
        Some(self.push_workspace(ws))
    }

    fn push_workspace(&mut self, ws: Value) -> String {
        let wid = str_of(&ws, "id").to_owned();
        if let Some(list) = self.doc["workspaces"].as_array_mut() {
            list.push(ws);
        }
        self.doc["active"] = json!(wid);
        self.build(&wid, false);
        self.core.dirty = true;
        self.core.updates.workspaces = true;
        wid
    }

    pub fn rename_workspace(&mut self, wid: &str, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        if let Some(ws) = doc::workspace_mut(&mut self.doc, wid) {
            ws["name"] = json!(name);
            self.core.dirty = true;
            self.core.updates.workspaces = true;
        }
    }

    /// The last workspace can not go.
    pub fn delete_workspace(&mut self, wid: &str) {
        if doc::workspaces(&self.doc).len() < 2 {
            return;
        }
        self.stop_workspace(wid);
        if let Some(list) = self.doc["workspaces"].as_array_mut() {
            list.retain(|w| w["id"] != wid);
        }
        if self.doc["active"] == wid {
            self.doc["active"] = doc::workspaces(&self.doc)[0]["id"].clone();
        }
        self.core.dirty = true;
        self.core.updates.workspaces = true;
    }

    pub fn set_enabled(&mut self, wid: &str, on: bool) {
        let Some(ws) = doc::workspace_mut(&mut self.doc, wid) else { return };
        if text::truthy(&ws["enabled"]) == on {
            return;
        }
        ws["enabled"] = json!(on);
        self.build(wid, false);
        self.core.dirty = true;
        self.core.updates.workspaces = true;
    }

    // Variables

    pub fn vars(&self) -> &Map<String, Value> {
        &self.core.vars
    }

    pub fn get_var(&self, name: &str) -> String {
        self.core.vars.get(name.trim()).map(text::py_str).unwrap_or_default()
    }

    pub fn set_var(&mut self, name: &str, value: &str) {
        self.core.set_var(name, value);
        self.apply_effects();
    }

    pub fn delete_var(&mut self, name: &str) {
        if self.core.vars.shift_remove(name).is_some() {
            self.core.dirty = true;
            self.core.updates.vars = true;
            self.core.effects.push(Effect::VarsChanged);
            self.apply_effects();
        }
    }

    // Running

    pub fn config(&self) -> &Config {
        &self.core.config
    }

    pub fn paused(&self) -> bool {
        !self.core.config.enabled
    }

    pub fn started(&self) -> bool {
        self.started
    }

    pub fn start(&mut self) {
        self.started = true;
        for wid in self.workspace_ids() {
            self.build(&wid, false);
        }
        tracing::info!(
            "automation: {} workspace(s), {} running",
            doc::workspaces(&self.doc).len(),
            doc::workspaces(&self.doc).iter().filter(|w| self.running(w)).count()
        );
    }

    pub fn shutdown(&mut self) {
        for wid in self.live.keys().cloned().collect::<Vec<_>>() {
            self.stop_workspace(&wid);
        }
        self.started = false;
    }

    /// Settings changed (pause, the safety stop, the shell...).
    pub fn set_config(&mut self, config: Config) {
        self.core.config = config;
        for wid in self.workspace_ids() {
            self.build(&wid, false);
        }
        self.core.updates.workspaces = true;
    }

    pub fn running(&self, ws: &Value) -> bool {
        self.started && text::truthy(&ws["enabled"]) && !self.paused()
    }

    pub fn is_running(&self, wid: &str) -> bool {
        self.workspace(wid).is_some_and(|ws| self.running(ws))
    }

    /// The editor changed a workspace. A field change restarts only that node, and only if it is a trigger (an
    /// interval with a new time); other nodes read their fields when they run.
    pub fn edited(&mut self, wid: &str, structural: bool, nid: &str) {
        self.core.dirty = true;
        if structural {
            self.build(wid, false);
            return;
        }
        let is_trigger =
            self.live.get(wid).and_then(|l| l.nodes.get(nid)).is_some_and(|n| n.spec.is_trigger());
        if is_trigger {
            self.stop_node(wid, nid);
            self.build(wid, false);
        }
    }

    /// The editor put back a whole earlier version (undo, redo): every node starts again.
    pub fn replaced(&mut self, wid: &str) {
        self.core.dirty = true;
        self.build(wid, true);
    }

    /// Whether the editor wants pulses for its animation.
    pub fn watch_pulses(&mut self, on: bool) {
        self.core.watch_pulses = on;
        if !on {
            self.core.updates.pulses.clear();
        }
    }

    /// Whether the editor wants to hear about pulses and status lines right now.
    pub fn watching(&self) -> bool {
        self.core.watch_pulses
    }

    pub fn has_updates(&self) -> bool {
        !self.core.updates.is_empty()
    }

    pub fn clock(&self) -> Arc<dyn Clock> {
        self.core.clock.clone()
    }

    pub fn take_updates(&mut self) -> Updates {
        std::mem::take(&mut self.core.updates)
    }

    /// The node's status line and progress, as the editor shows them now.
    pub fn info(&self, wid: &str, nid: &str) -> Option<Status> {
        let node = self.live.get(wid)?.nodes.get(nid)?;
        let status = node.status.borrow().clone();
        let rt = node.rt.try_borrow().ok()?;
        Some(rt.info(&status, self.core.clock.mono()))
    }

    /// Whether the node is running (it is in a running workspace and not switched off).
    pub fn has_node(&self, wid: &str, nid: &str) -> bool {
        self.live.get(wid).is_some_and(|l| l.nodes.contains_key(nid))
    }

    pub fn is_live(&self, wid: &str, nid: &str) -> bool {
        self.live.get(wid).and_then(|l| l.nodes.get(nid)).is_some_and(|n| n.spec.live)
    }

    /// The last text a node sent on.
    pub fn last_text(&self, wid: &str, nid: &str) -> Option<&str> {
        self.core.last.get(&(wid.to_owned(), nid.to_owned())).map(String::as_str)
    }

    /// ▶ on a node (or the palette, the tray).
    pub fn run_node(&mut self, wid: &str, nid: &str, input: &str) {
        let Some(ws) = self.workspace(wid) else { return };
        if !self.running(ws) {
            let why = if self.paused() {
                "Automations are paused".to_owned()
            } else {
                format!("“{}” is switched off", str_of(ws, "name"))
            };
            self.core.host.send(Out::Notify {
                title: "Automation".into(),
                text: format!("{why}. Turn it on at the bottom of the Automation page."),
                ms: 6000,
            });
            return;
        }
        self.with_node(wid, nid, |rt, cx| rt.run_now(input, cx));
        self.apply_effects();
    }

    /// When [`Engine::step`] has something to do next, on the clock's `mono` scale. `None`: nothing until
    /// something from outside happens.
    pub fn next_deadline(&self) -> Option<f64> {
        if !self.queue.is_empty() {
            return Some(self.core.clock.mono());
        }
        self.core.timers.keys().next().map(|(micros, _)| *micros as f64 / 1e6)
    }

    /// Delivers waiting pulses and runs due timers until nothing is left that is due now. Returns `true` when it
    /// stopped early to let go of the engine, with more still to do.
    pub fn step(&mut self) -> bool {
        for _ in 0..STEP_BUDGET {
            let now = to_micros(self.core.clock.mono());
            let due = self.core.timers.first_key_value().is_some_and(|((at, _), _)| *at <= now);
            if due {
                let (key, target) = self.core.timers.pop_first().expect("checked above");
                self.core.timer_keys.remove(&key.1);
                self.with_node(&target.ws, &target.node, |rt, cx| rt.timer(target.token, cx));
            } else if let Some(next) = self.queue.pop_front() {
                match next {
                    Queued::Pulse { ws, node, port, text } => {
                        self.with_node(&ws, &node, |rt, cx| rt.receive(&port, &text, cx));
                    }
                    Queued::Switch { ws, on } => self.set_enabled(&ws, on),
                }
            } else {
                return false;
            }
            self.apply_effects();
        }
        true
    }

    fn workspace_ids(&self) -> Vec<String> {
        doc::workspaces(&self.doc).iter().map(|w| str_of(w, "id").to_owned()).collect()
    }

    /// Brings the running nodes in line with the workspace: starts new ones, stops removed ones, indexes the
    /// wires again. Nodes that stay keep running (an interval is not restarted by adding a wire).
    fn build(&mut self, wid: &str, restart_all: bool) {
        let Some(ws) = self.workspace(wid) else {
            self.stop_workspace(wid);
            return;
        };
        if !self.running(ws) {
            self.stop_workspace(wid);
            return;
        }
        let want: Vec<(String, String)> = ws["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|n| !text::truthy(&n["off"]))
            .map(|n| (str_of(n, "id").to_owned(), str_of(n, "type").to_owned()))
            .collect();
        let mut out: HashMap<(String, String), Vec<Wire>> = HashMap::new();
        let mut inn = HashMap::new();
        for w in ws["wires"].as_array().into_iter().flatten() {
            let end = |key: &str, i: usize| w[key][i].as_str().unwrap_or_default().to_owned();
            let (src, src_port, dst, dst_port) = (end("src", 0), end("src", 1), end("dst", 0), end("dst", 1));
            if dst_port.starts_with("field:") {
                inn.insert((dst.clone(), dst_port.clone()), (src.clone(), src_port.clone()));
            }
            let port = dst_port.split_once(':').map(|(_, p)| p.to_owned()).unwrap_or_default();
            out.entry((src, src_port)).or_default().push(Wire { id: str_of(w, "id").to_owned(), dst, port });
        }

        let stale: Vec<String> = match self.live.get(wid) {
            Some(live) => live
                .nodes
                .iter()
                .filter(|(nid, n)| restart_all || !want.iter().any(|(id, ty)| id == *nid && *ty == n.spec.id))
                .map(|(nid, _)| nid.clone())
                .collect(),
            None => Vec::new(),
        };
        for nid in stale {
            self.stop_node(wid, &nid);
        }
        let live = self.live.entry(wid.to_owned()).or_default();
        live.out = out;
        live.inn = inn;
        let mut fresh = Vec::new();
        for (nid, ty) in want {
            if live.nodes.contains_key(&nid) {
                continue;
            }
            let Some(spec) = spec::spec(&ty) else { continue };
            let rt = crate::nodes::make(spec);
            live.nodes
                .insert(nid.clone(), LiveNode { spec, rt: RefCell::new(rt), status: RefCell::default() });
            fresh.push(nid);
        }
        for nid in fresh {
            self.with_node(wid, &nid, |rt, cx| rt.start(cx));
            self.apply_effects();
        }
    }

    fn stop_workspace(&mut self, wid: &str) {
        let Some(live) = self.live.get(wid) else { return };
        let ids: Vec<String> = live.nodes.keys().cloned().collect();
        for nid in ids {
            self.stop_node(wid, &nid);
        }
        self.live.remove(wid);
        self.queue.retain(|q| !matches!(q, Queued::Pulse { ws, .. } if ws == wid));
    }

    fn stop_node(&mut self, wid: &str, nid: &str) {
        self.with_node(wid, nid, |rt, cx| rt.stop(cx));
        self.core.cancel_node_timers(wid, nid);
        if let Some(live) = self.live.get_mut(wid) {
            live.nodes.remove(nid);
        }
        // Whatever the node was still going to send is dropped, as Python's `alive = False` does.
        self.core.effects.retain(|e| !matches!(e, Effect::Fire { ws, node, .. } if ws == wid && node == nid));
    }

    /// Runs `f` on a running node with its context. A panic inside a node is caught and shown on the node, as
    /// Python's `_deliver` catches exceptions.
    fn with_node(&mut self, wid: &str, nid: &str, f: impl FnOnce(&mut dyn NodeRuntime, &mut Cx<'_>)) {
        let Some(live) = self.live.get(wid) else { return };
        let Some(node) = live.nodes.get(nid) else { return };
        let Ok(mut rt) = node.rt.try_borrow_mut() else { return };
        let ws_doc = doc::workspace(&self.doc, wid).unwrap_or(&NULL);
        let node_doc = doc::node_in(ws_doc, nid).unwrap_or(&NULL);
        let mut cx = Cx {
            core: &mut self.core,
            ws: live,
            ws_doc,
            node_doc,
            wid,
            nid,
            spec: node.spec,
            status: &node.status,
        };
        if catch_unwind(AssertUnwindSafe(|| f(&mut **rt, &mut cx))).is_err() {
            tracing::error!("automation node {} ({nid}) failed", node.spec.id);
            cx.set_status("⚠ it went wrong (see the log)", None);
        }
    }

    fn apply_effects(&mut self) {
        while !self.core.effects.is_empty() {
            let effects = std::mem::take(&mut self.core.effects);
            let mut vars_changed = false;
            for effect in effects {
                match effect {
                    Effect::Fire { ws, node, port, text } => self.deliver_fire(&ws, &node, &port, text),
                    Effect::SaveState { ws, node, key, value } => {
                        let state = doc::workspace_mut(&mut self.doc, &ws)
                            .and_then(|w| doc::node_in_mut(w, &node))
                            .and_then(|n| n.get_mut("state"))
                            .and_then(Value::as_object_mut);
                        if let Some(state) = state {
                            state.insert(key, value);
                            self.core.dirty = true;
                        }
                    }
                    Effect::SetEnabled { ws, on } => self.queue.push_back(Queued::Switch { ws, on }),
                    Effect::VarsChanged => vars_changed = true,
                }
            }
            if vars_changed {
                let targets: Vec<(String, String)> = self
                    .live
                    .iter()
                    .flat_map(|(wid, l)| l.nodes.keys().map(move |nid| (wid.clone(), nid.clone())))
                    .collect();
                for (wid, nid) in targets {
                    self.with_node(&wid, &nid, |rt, cx| rt.vars_changed(cx));
                }
            }
        }
    }

    fn deliver_fire(&mut self, wid: &str, nid: &str, port: &str, text: String) {
        let Some(live) = self.live.get(wid) else { return };
        if !live.nodes.contains_key(nid) {
            return;
        }
        let text = clip_chars(&text, MAX_TEXT).to_owned();
        self.core.last.insert((wid.to_owned(), nid.to_owned()), text.clone());
        let key = (nid.to_owned(), format!("out:{port}"));
        let Some(wires) = live.out.get(&key) else { return };
        for w in wires {
            if !self.core.rate_ok(wid, &self.doc) {
                return;
            }
            if self.core.watch_pulses && self.core.updates.pulses.len() < MAX_PULSES {
                self.core.updates.pulses.push(Pulse {
                    workspace: wid.to_owned(),
                    wire: w.id.clone(),
                    text: clip_chars(&text, 300).to_owned(),
                });
            }
            if live.nodes.contains_key(&w.dst) {
                self.queue.push_back(Queued::Pulse {
                    ws: wid.to_owned(),
                    node: w.dst.clone(),
                    port: w.port.clone(),
                    text: text.clone(),
                });
            }
        }
    }
}

impl Core {
    fn rate_ok(&mut self, wid: &str, doc: &Value) -> bool {
        let now = self.clock.mono();
        let r = self.rate.entry(wid.to_owned()).or_insert((now, 0));
        if now - r.0 > 1.0 {
            *r = (now, 0);
        }
        r.1 += 1;
        let limit = self.config.max_rate.max(1);
        if r.1 <= limit {
            return true;
        }
        // Only the first pulse over the limit says so; the workspace is switched off right after this pass.
        if r.1 == limit + 1 {
            let name = doc::workspace(doc, wid).map(|w| str_of(w, "name")).unwrap_or_default().to_owned();
            tracing::warn!("workspace {name} fired more than {limit} times a second: switched off");
            self.effects.push(Effect::SetEnabled { ws: wid.to_owned(), on: false });
            self.host.send(Out::Notify {
                title: "Automation".into(),
                text: format!(
                    "“{name}” fired more than {limit} times in a second, so it was switched off. Look for a wire loop."
                ),
                ms: 8000,
            });
        }
        false
    }

    fn set_var(&mut self, name: &str, value: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        self.vars.insert(name.to_owned(), json!(clip_chars(value, MAX_TEXT)));
        self.dirty = true;
        self.updates.vars = true;
        self.effects.push(Effect::VarsChanged);
    }

    fn add_timer(&mut self, wid: &str, nid: &str, secs: f64, token: u64) -> u64 {
        let id = self.next_timer;
        self.next_timer += 1;
        let at = to_micros(self.clock.mono() + secs.max(0.0));
        self.timers.insert((at, id), TimerTarget { ws: wid.to_owned(), node: nid.to_owned(), token });
        self.timer_keys.insert(id, (at, id));
        id
    }

    fn cancel_timer(&mut self, id: u64) {
        if let Some(key) = self.timer_keys.remove(&id) {
            self.timers.remove(&key);
        }
    }

    fn cancel_node_timers(&mut self, wid: &str, nid: &str) {
        let gone: Vec<(i64, u64)> =
            self.timers.iter().filter(|(_, t)| t.ws == wid && t.node == nid).map(|(k, _)| *k).collect();
        for key in gone {
            self.timers.remove(&key);
            self.timer_keys.remove(&key.1);
        }
    }
}

fn to_micros(secs: f64) -> i64 {
    (secs * 1e6).round() as i64
}

/// What a node can see and do while it runs.
pub struct Cx<'a> {
    core: &'a mut Core,
    ws: &'a WsLive,
    ws_doc: &'a Value,
    node_doc: &'a Value,
    wid: &'a str,
    nid: &'a str,
    pub spec: &'static NodeSpec,
    status: &'a RefCell<Status>,
}

impl Cx<'_> {
    pub fn workspace_id(&self) -> &str {
        self.wid
    }

    pub fn node_id(&self) -> &str {
        self.nid
    }

    pub fn workspace_name(&self) -> &str {
        str_of(self.ws_doc, "name")
    }

    /// The node's own title, or its type's.
    pub fn label(&self) -> String {
        match str_of(self.node_doc, "title") {
            "" => self.spec.title.clone(),
            title => title.to_owned(),
        }
    }

    /// A field as saved, or its default.
    pub fn raw(&self, key: &str) -> &Value {
        match self.node_doc["fields"].get(key) {
            Some(v) => v,
            None => self.spec.field(key).map_or(&NULL, |f| &f.default),
        }
    }

    /// `str(self.raw(key) or "")`: a field as text, without filling in tokens.
    pub fn raw_str(&self, key: &str) -> String {
        let v = self.raw(key);
        if text::truthy(v) { text::py_str(v) } else { String::new() }
    }

    pub fn flag(&self, key: &str) -> bool {
        text::truthy(self.raw(key))
    }

    /// A field's value: what is wired into its ◆ when something is, otherwise its box with the tokens filled in.
    pub fn f(&mut self, key: &str, input: &str) -> String {
        if let Some(pulled) = self.pull(key, input) {
            return pulled;
        }
        match self.raw(key) {
            Value::String(s) => {
                let s = s.clone();
                self.expand(&s, input)
            }
            other => text::py_str(other),
        }
    }

    /// A field read as a number, or `default` when it is not one.
    pub fn num(&mut self, key: &str, input: &str, default: f64) -> f64 {
        text::parse_number(&self.f(key, input)).unwrap_or(default)
    }

    pub fn expand(&mut self, text: &str, input: &str) -> String {
        let now = self.core.clock.local();
        let host = self.core.host.clone();
        text::expand(text, input, now, &self.core.vars, &mut || host.clipboard_text())
    }

    /// The value wired into field `key`, or `None` when nothing is wired there.
    pub fn pull(&mut self, key: &str, input: &str) -> Option<String> {
        let (src, port) = self.ws.inn.get(&(self.nid.to_owned(), format!("field:{key}")))?;
        let Some(node) = self.ws.nodes.get(src) else { return Some(String::new()) };
        if self.core.depth > MAX_DEPTH {
            return Some(String::new());
        }
        // A node busy running right now (it pulls its own value) has nothing to give.
        let Ok(rt) = node.rt.try_borrow() else { return Some(String::new()) };
        let node_doc = doc::node_in(self.ws_doc, src).unwrap_or(&NULL);
        let port = port.split_once(':').map_or(port.as_str(), |(_, p)| p);
        self.core.depth += 1;
        let mut cx = Cx {
            core: &mut *self.core,
            ws: self.ws,
            ws_doc: self.ws_doc,
            node_doc,
            wid: self.wid,
            nid: src,
            spec: node.spec,
            status: &node.status,
        };
        let got = catch_unwind(AssertUnwindSafe(|| rt.value(port, input, &mut cx)));
        self.core.depth -= 1;
        Some(got.unwrap_or_else(|_| {
            tracing::error!("reading automation node {} failed", node.spec.id);
            String::new()
        }))
    }

    /// Sends a pulse out of `port`. It arrives after this node is done.
    pub fn fire(&mut self, port: &str, text: &str) {
        self.core.effects.push(Effect::Fire {
            ws: self.wid.to_owned(),
            node: self.nid.to_owned(),
            port: port.to_owned(),
            text: text.to_owned(),
        });
    }

    pub fn set_status(&mut self, text: impl Into<String>, progress: Option<f32>) {
        *self.status.borrow_mut() = Status { text: text.into(), progress };
        let id = (self.wid.to_owned(), self.nid.to_owned());
        if !self.core.updates.nodes.contains(&id) {
            self.core.updates.nodes.push(id);
        }
    }

    pub fn status(&self) -> String {
        self.status.borrow().text.clone()
    }

    /// Calls the node's `timer(token)` in `secs` seconds. The id cancels it.
    pub fn after(&mut self, secs: f64, token: u64) -> u64 {
        self.core.add_timer(self.wid, self.nid, secs, token)
    }

    pub fn cancel(&mut self, timer: u64) {
        self.core.cancel_timer(timer);
    }

    pub fn cancel_all(&mut self) {
        self.core.cancel_node_timers(self.wid, self.nid);
    }

    pub fn state(&self) -> &Value {
        &self.node_doc["state"]
    }

    /// Keeps a value in the node's saved state (a count that survives a restart).
    pub fn save_state(&mut self, key: &str, value: Value) {
        self.core.effects.push(Effect::SaveState {
            ws: self.wid.to_owned(),
            node: self.nid.to_owned(),
            key: key.to_owned(),
            value,
        });
    }

    pub fn get_var(&self, name: &str) -> String {
        self.core.vars.get(name.trim()).map(text::py_str).unwrap_or_default()
    }

    pub fn set_var(&mut self, name: &str, value: &str) {
        self.core.set_var(name, value);
    }

    /// Switches a workspace on or off once this pass is over.
    pub fn set_enabled(&mut self, wid: &str, on: bool) {
        self.core.effects.push(Effect::SetEnabled { ws: wid.to_owned(), on });
    }

    pub fn mono(&self) -> f64 {
        self.core.clock.mono()
    }

    pub fn local(&self) -> DateTime {
        self.core.clock.local()
    }

    pub fn rng(&mut self) -> &mut fastrand::Rng {
        &mut self.core.rng
    }

    pub fn config(&self) -> &Config {
        &self.core.config
    }

    pub fn host(&self) -> &Arc<dyn Host> {
        &self.core.host
    }

    pub fn clipboard_text(&self) -> String {
        self.core.host.clipboard_text()
    }

    pub fn send(&self, out: Out) {
        self.core.host.send(out);
    }
}
