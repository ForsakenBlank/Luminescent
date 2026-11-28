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
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use jiff::civil::DateTime;
use serde_json::{Map, Value, json};

use crate::doc::{self, str_of};
use crate::spec::{self, NodeSpec};
use crate::text::{self, MAX_TEXT, clip_chars};
use crate::work::{
    self, Job, JobHandle, JobMsg, JobNote, JobWhat, Level, NodeAddr, Readings, Wake, Want, Watch,
};

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

/// Where an answer from the app goes back to: the node that asked (as it was then) and which question it was.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Reply {
    pub workspace: String,
    pub node: String,
    born: u64,
    token: u64,
}

/// What the app says back to a node that asked it something.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// It happened (the box was closed, the timer started...), with the text that goes on.
    Done(String),
    /// You said no, or closed the box without an answer.
    Cancelled,
    /// It could not be done: why.
    Failed(String),
}

/// What the engine asks of the rest of Luminescent: cards, actions, the overlay... The app answers on its own
/// thread; nothing here waits for it. Requests with a `reply` are answered through [`Engine::answer`].
#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    /// A card or a desktop notification, as Settings → General says (none when notifications are off).
    Notify {
        title: String,
        text: String,
        ms: u32,
    },
    /// A Luminescent card, whatever the notification settings say.
    Card {
        title: String,
        text: String,
        ms: u32,
    },
    /// A desktop notification that stays in its history, and a card as well when that is the style.
    Toast {
        title: String,
        text: String,
        ms: u32,
    },
    /// The big warning in the middle of the screen; `reply` hears when it closes. `sound` plays first.
    Alert {
        title: String,
        text: String,
        level: String,
        seconds: f64,
        sound: String,
        reply: Option<Reply>,
    },
    /// A message box on top of everything; `reply` hears when it is closed.
    Message {
        title: String,
        text: String,
        reply: Reply,
    },
    /// A box to type in: `Done` with what was typed, or `Cancelled`.
    Ask {
        title: String,
        question: String,
        default: String,
        reply: Reply,
    },
    /// A yes or no question: `Done` for yes.
    Confirm {
        title: String,
        text: String,
        yes: String,
        reply: Reply,
    },
    /// A tool action, `<module>:<id>`.
    RunAction(String),
    /// `open` (on `page`), `close` or `toggle` the overlay.
    Overlay {
        mode: String,
        page: String,
    },
    /// `next`, `previous` or a theme's id.
    Theme(String),
    /// A dashboard item, as if it was double-clicked.
    RunItem(String),
    SwitchBoard(String),
    /// A Timer panel, already running: `Done` with what it is called, `Failed` when the Timer module is off.
    StartTimer {
        ms: f64,
        label: String,
        reply: Reply,
    },
    /// The Notes panel on that note: `Failed` when the Notes module is off.
    OpenNote {
        id: String,
        reply: Reply,
    },
    /// Starts a program, opens a file or a web page: `Done`, or `Failed` with why.
    Open {
        target: String,
        args: String,
        name: String,
        reply: Reply,
    },
    /// An event for the bus (the clipboard changed).
    Emit {
        name: String,
        text: String,
    },
    Sound {
        kind: String,
        file: String,
    },
    /// Read out loud: `Done` once it was said, `Failed` when nothing can speak.
    Speak {
        text: String,
        reply: Reply,
    },
    /// Type into the window that has the keyboard.
    TypeText(String),
    PressKeys(String),
    SetClipboard(String),
    /// Keep awake on, off, or the other way (`None`): `Done` with the status line.
    KeepAwake {
        on: Option<bool>,
        why: String,
        reply: Reply,
    },
    /// The speakers' volume (`set`, `up`, `down`, `mute`, `unmute`, `toggle_mute`): `Done` with the status line.
    Volume {
        mode: String,
        amount: f64,
        reply: Reply,
    },
    /// Lock, turn the screens off, sleep, sign out, restart or shut down: `Failed` with why.
    Power {
        action: String,
        reply: Reply,
    },
    /// The Hotkey triggers that run changed: register [`Engine::hotkey_nodes`] again.
    HotkeysChanged,
    /// Workspaces came, went, were renamed or switched on or off (the tray lists them).
    WorkspacesChanged,
}

/// The engine's way out to the app and the desktop. `send` is called from the engine's thread with the engine
/// locked, so it must only pass the request on; the readings are called from worker threads and may wait.
pub trait Host: Send + Sync {
    fn send(&self, out: Out);

    fn clipboard_text(&self) -> String {
        String::new()
    }

    /// The command that runs one line of the Run command node in `shell` (bash, fish, cmd, powershell...).
    fn shell_command(&self, shell: &str, line: &str) -> std::process::Command {
        let mut cmd = std::process::Command::new(if shell.is_empty() { "sh" } else { shell });
        cmd.arg("-c").arg(line);
        cmd
    }

    /// Runs the lines as a script in a terminal window; when it ends, its exit code is written to `marker`.
    /// Gives back the script's path (removed once the marker is there).
    fn run_in_terminal(
        &self,
        _cwd: &Path,
        _lines: &[String],
        _shell: &str,
        _stop_on_error: bool,
        _marker: &Path,
    ) -> Result<PathBuf, String> {
        Err("No terminal program found.".into())
    }

    /// Closes every running copy of a program. `true` when one was running.
    fn close_program(&self, _name: &str, _force: bool) -> bool {
        false
    }

    /// The lowercase names of the running programs.
    fn process_names(&self) -> BTreeSet<String> {
        BTreeSet::new()
    }

    /// Seconds since the PC was last used.
    fn idle_seconds(&self) -> f64 {
        0.0
    }

    /// The focused window's app (class) and title.
    fn focused_window(&self) -> (String, String) {
        (String::new(), String::new())
    }

    /// Whether the internet can be reached, `None` while that is not known.
    fn online(&self) -> Option<bool> {
        None
    }

    fn level(&self, _metric: &str) -> Level {
        Level::NotYet
    }

    fn keep_awake_on(&self) -> bool {
        false
    }

    /// The speakers' volume in percent, and whether they are muted.
    fn volume(&self) -> Option<(f64, bool)> {
        None
    }
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

    /// A job it started with [`Cx::spawn`] (or a [`Job`] it handed out) has news.
    fn job(&mut self, _token: u64, _msg: JobMsg, _cx: &mut Cx<'_>) {}

    /// The app answered a request it sent with [`Cx::reply`].
    fn answer(&mut self, _token: u64, _answer: Answer, _cx: &mut Cx<'_>) {}

    /// A round of the shared polls it asked for with [`Cx::watch`].
    fn reading(&mut self, _readings: &Readings, _cx: &mut Cx<'_>) {}

    /// Something happened in Luminescent (`timer.finished`, `theme.changed`...).
    fn event(&mut self, _name: &str, _text: &str, _cx: &mut Cx<'_>) {}

    /// What the editor shows under the node. Live nodes work it out from the time.
    fn info(&self, status: &Status, _look: &Look<'_>) -> Status {
        status.clone()
    }
}

/// What a node can read while the editor asks for its status line.
pub struct Look<'a> {
    pub mono: f64,
    pub local: DateTime,
    node: &'a Value,
    spec: &'static NodeSpec,
}

impl Look<'_> {
    pub fn raw(&self, key: &str) -> &Value {
        field_of(self.node, self.spec, key)
    }

    pub fn raw_str(&self, key: &str) -> String {
        raw_text(self.raw(key))
    }

    pub fn state(&self) -> &Value {
        &self.node["state"]
    }
}

/// A Hotkey trigger that runs, for the app to register.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyNode {
    pub workspace: String,
    pub node: String,
    pub combo: String,
    pub label: String,
}

/// A Button trigger, for the palette and the tray.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Button {
    pub workspace: String,
    pub workspace_name: String,
    pub node: String,
    pub label: String,
    pub running: bool,
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

/// How often the shared polls run while a node wants them.
const WATCH_EVERY: f64 = 2.0;

struct LiveNode {
    spec: &'static NodeSpec,
    /// Unique to this start of the node: news for an earlier start of it is dropped.
    born: u64,
    rt: RefCell<Box<dyn NodeRuntime>>,
    status: RefCell<Status>,
    /// The stop switches of the jobs it started.
    jobs: RefCell<Vec<Arc<AtomicBool>>>,
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
    Pulse {
        ws: String,
        node: String,
        port: String,
        text: String,
    },
    /// `None`: the other way.
    Switch {
        ws: String,
        on: Option<bool>,
    },
}

struct TimerTarget {
    ws: String,
    node: String,
    token: u64,
}

enum Effect {
    Fire { ws: String, node: String, port: String, text: String },
    SaveState { ws: String, node: String, key: String, value: Value },
    SetEnabled { ws: String, on: Option<bool> },
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
    next_born: u64,
    jobs_tx: Sender<JobNote>,
    jobs_rx: Receiver<JobNote>,
    /// News a test waited for, before the channel's.
    jobs_waiting: VecDeque<JobNote>,
    jobs_running: Arc<AtomicUsize>,
    waker: Wake,
    watchers: BTreeMap<Watch, BTreeSet<(String, String)>>,
    watch_timer: Option<u64>,
    /// A round of readings is on its way.
    reading: bool,
    clip_last: Option<String>,
    hotkeys: Vec<HotkeyNode>,
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
        let (jobs_tx, jobs_rx) = crossbeam_channel::unbounded();
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
                next_born: 1,
                jobs_tx,
                jobs_rx,
                jobs_waiting: VecDeque::new(),
                jobs_running: Arc::default(),
                waker: Arc::new(|| {}),
                watchers: BTreeMap::new(),
                watch_timer: None,
                reading: false,
                clip_last: None,
                hotkeys: Vec::new(),
            },
        }
    }

    /// Called from worker threads when a job has news, so whoever runs the engine comes back to it.
    pub fn set_waker(&mut self, wake: impl Fn() + Send + Sync + 'static) {
        self.core.waker = Arc::new(wake);
    }

    /// Jobs still working, and news from them not taken yet.
    pub fn jobs_running(&self) -> usize {
        self.core.jobs_running.load(Ordering::SeqCst) + self.core.jobs_waiting.len() + self.core.jobs_rx.len()
    }

    /// Waits up to `timeout` for news from a job (tests). `true` when some came.
    pub fn wait_for_job(&mut self, timeout: Duration) -> bool {
        if !self.core.jobs_waiting.is_empty() || !self.core.jobs_rx.is_empty() {
            return true;
        }
        let end = Instant::now() + timeout;
        while Instant::now() < end {
            if let Ok(note) = self.core.jobs_rx.recv_timeout(Duration::from_millis(10)) {
                self.core.jobs_waiting.push_back(note);
                return true;
            }
            if self.core.jobs_running.load(Ordering::SeqCst) == 0 && self.core.jobs_rx.is_empty() {
                return false;
            }
        }
        false
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
        self.workspaces_changed();
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
            self.workspaces_changed();
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
        self.workspaces_changed();
    }

    pub fn set_enabled(&mut self, wid: &str, on: bool) {
        let Some(ws) = doc::workspace_mut(&mut self.doc, wid) else { return };
        if text::truthy(&ws["enabled"]) == on {
            return;
        }
        ws["enabled"] = json!(on);
        self.build(wid, false);
        self.core.dirty = true;
        self.workspaces_changed();
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

    /// The Variables panel's Save: these and no others.
    pub fn replace_vars(&mut self, vars: Map<String, Value>) {
        self.core.vars = vars;
        self.core.dirty = true;
        self.core.updates.vars = true;
        self.core.effects.push(Effect::VarsChanged);
        self.apply_effects();
    }

    fn workspaces_changed(&mut self) {
        self.core.updates.workspaces = true;
        self.core.host.send(Out::WorkspacesChanged);
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
        self.workspaces_changed();
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
        let node_doc = self.workspace(wid).and_then(|ws| doc::node_in(ws, nid)).unwrap_or(&NULL);
        let look = Look {
            mono: self.core.clock.mono(),
            local: self.core.clock.local(),
            node: node_doc,
            spec: node.spec,
        };
        Some(rt.info(&status, &look))
    }

    /// Sets a node's status line from outside (the app could not register its hotkey).
    pub fn set_status(&mut self, wid: &str, nid: &str, text: &str) {
        self.with_node(wid, nid, |_, cx| cx.set_status(text, None));
    }

    /// The app answers a node's request.
    pub fn answer(&mut self, reply: &Reply, answer: Answer) {
        if self.born_of(&reply.workspace, &reply.node) != Some(reply.born) {
            return;
        }
        let token = reply.token;
        self.with_node(&reply.workspace, &reply.node, |rt, cx| rt.answer(token, answer, cx));
        self.apply_effects();
    }

    /// Something happened in Luminescent: the event triggers hear it.
    pub fn event(&mut self, name: &str, text: &str) {
        for (wid, nid) in self.live_of_type("event") {
            self.with_node(&wid, &nid, |rt, cx| rt.event(name, text, cx));
            self.apply_effects();
        }
    }

    /// The Hotkey triggers that run and have a key, in the order of the workspaces and their nodes.
    pub fn hotkey_nodes(&self) -> Vec<HotkeyNode> {
        let mut out = Vec::new();
        for ws in doc::workspaces(&self.doc) {
            let wid = str_of(ws, "id");
            let Some(live) = self.live.get(wid) else { continue };
            for n in ws["nodes"].as_array().into_iter().flatten() {
                let nid = str_of(n, "id");
                let Some(node) = live.nodes.get(nid).filter(|l| l.spec.id == "hotkey") else { continue };
                let combo = raw_text(field_of(n, node.spec, "combo"));
                if combo.is_empty() {
                    continue;
                }
                let label = match str_of(n, "title") {
                    "" => node.spec.title.clone(),
                    title => title.to_owned(),
                };
                out.push(HotkeyNode { workspace: wid.to_owned(), node: nid.to_owned(), combo, label });
            }
        }
        out
    }

    /// A Hotkey trigger's key was pressed.
    pub fn hotkey_pressed(&mut self, wid: &str, nid: &str) {
        let combo = self
            .workspace(wid)
            .and_then(|ws| doc::node_in(ws, nid))
            .map(|n| raw_text(&n["fields"]["combo"]))
            .unwrap_or_default();
        self.with_node(wid, nid, |rt, cx| rt.run_now(&combo, cx));
        self.apply_effects();
    }

    /// Every Button trigger that is not switched off, running or not.
    pub fn buttons(&self) -> Vec<Button> {
        let mut out = Vec::new();
        for ws in doc::workspaces(&self.doc) {
            for n in ws["nodes"].as_array().into_iter().flatten() {
                if str_of(n, "type") != "manual" || text::truthy(&n["off"]) {
                    continue;
                }
                let label = match raw_text(&n["fields"]["label"]) {
                    l if l.is_empty() => "Run".to_owned(),
                    l => l,
                };
                out.push(Button {
                    workspace: str_of(ws, "id").to_owned(),
                    workspace_name: str_of(ws, "name").to_owned(),
                    node: str_of(n, "id").to_owned(),
                    label,
                    running: self.running(ws),
                });
            }
        }
        out
    }

    fn born_of(&self, wid: &str, nid: &str) -> Option<u64> {
        self.live.get(wid)?.nodes.get(nid).map(|n| n.born)
    }

    fn live_of_type(&self, type_id: &str) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = self
            .live
            .iter()
            .flat_map(|(wid, l)| {
                l.nodes
                    .iter()
                    .filter(|(_, n)| n.spec.id == type_id)
                    .map(move |(nid, _)| (wid.clone(), nid.clone()))
            })
            .collect();
        out.sort();
        out
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
        if !self.queue.is_empty() || !self.core.jobs_waiting.is_empty() || !self.core.jobs_rx.is_empty() {
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
                if target.node.is_empty() {
                    self.watch_tick();
                } else {
                    self.with_node(&target.ws, &target.node, |rt, cx| rt.timer(target.token, cx));
                }
            } else if let Some(note) = self.next_job_note() {
                self.job_note(note);
            } else if let Some(next) = self.queue.pop_front() {
                match next {
                    Queued::Pulse { ws, node, port, text } => {
                        self.with_node(&ws, &node, |rt, cx| rt.receive(&port, &text, cx));
                    }
                    Queued::Switch { ws, on } => {
                        let Some(was) = self.workspace(&ws).map(|w| text::truthy(&w["enabled"])) else {
                            continue;
                        };
                        self.set_enabled(&ws, on.unwrap_or(!was));
                    }
                }
            } else {
                return false;
            }
            self.apply_effects();
        }
        true
    }

    fn next_job_note(&mut self) -> Option<JobNote> {
        self.core.jobs_waiting.pop_front().or_else(|| self.core.jobs_rx.try_recv().ok())
    }

    fn job_note(&mut self, note: JobNote) {
        match (note.to, note.what) {
            (Some(to), JobWhat::Msg(msg)) => {
                if self.born_of(&to.ws, &to.node) == Some(to.born) {
                    self.with_node(&to.ws, &to.node, |rt, cx| rt.job(to.token, msg, cx));
                }
            }
            (_, JobWhat::Readings(readings)) => self.deliver_readings(readings),
            (None, JobWhat::Msg(_)) => {}
        }
    }

    /// The shared polls' timer: schedules the next round and starts reading unless the last round is still busy.
    fn watch_tick(&mut self) {
        self.core.watch_timer = None;
        if self.core.watchers.values().all(BTreeSet::is_empty) {
            return;
        }
        self.core.watch_timer = Some(self.core.add_timer("", "", WATCH_EVERY, 0));
        if self.core.reading {
            return;
        }
        let has = |kind: Watch| self.core.watchers.get(&kind).is_some_and(|s| !s.is_empty());
        let mut metrics: Vec<String> = Vec::new();
        for (wid, nid) in self.core.watchers.get(&Watch::Levels).into_iter().flatten() {
            let node = self.workspace(wid).and_then(|ws| doc::node_in(ws, nid)).unwrap_or(&NULL);
            let metric = match raw_text(&node["fields"]["metric"]) {
                m if m.is_empty() => "cpu".to_owned(),
                m => m,
            };
            if !metrics.contains(&metric) {
                metrics.push(metric);
            }
        }
        let want = Want {
            programs: has(Watch::Programs),
            idle: has(Watch::Idle),
            metrics,
            window: has(Watch::Window),
            online: has(Watch::Internet),
            clipboard: has(Watch::Clipboard),
        };
        self.core.reading = true;
        let job = Job {
            to: None,
            tx: self.core.jobs_tx.clone(),
            wake: self.core.waker.clone(),
            host: self.core.host.clone(),
            stop: Arc::default(),
        };
        work::spawn(&self.core.jobs_running, "automation-watch", move || {
            let readings = want.read(job.host());
            job.note(JobWhat::Readings(readings));
        });
    }

    fn deliver_readings(&mut self, mut readings: Readings) {
        self.core.reading = false;
        let clip = readings.clipboard.take();
        let has = [
            (Watch::Programs, readings.programs.is_some()),
            (Watch::Idle, readings.idle.is_some()),
            (Watch::Levels, !readings.levels.is_empty()),
            (Watch::Window, readings.window.is_some()),
            (Watch::Internet, readings.online.is_some()),
        ];
        for (kind, there) in has {
            if !there {
                continue;
            }
            let subs: Vec<(String, String)> =
                self.core.watchers.get(&kind).map(|s| s.iter().cloned().collect()).unwrap_or_default();
            for (wid, nid) in subs {
                self.with_node(&wid, &nid, |rt, cx| rt.reading(&readings, cx));
                self.apply_effects();
            }
        }
        let Some(text) = clip else { return };
        let subs: Vec<(String, String)> = self
            .core
            .watchers
            .get(&Watch::Clipboard)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default();
        if subs.is_empty() || self.core.clip_last.as_deref() == Some(text.as_str()) {
            return;
        }
        // the first look only learns what is there; a change is what fires
        let first = self.core.clip_last.is_none();
        self.core.clip_last = Some(text.clone());
        if first {
            return;
        }
        let changed = Readings { clipboard: Some(text.clone()), ..Readings::default() };
        for (wid, nid) in subs {
            self.with_node(&wid, &nid, |rt, cx| rt.reading(&changed, cx));
            self.apply_effects();
        }
        self.core
            .host
            .send(Out::Emit { name: "clipboard.changed".into(), text: clip_chars(&text, 200).to_owned() });
    }

    /// Tells the app when the set of Hotkey triggers to register changed.
    fn check_hotkeys(&mut self) {
        let now = self.hotkey_nodes();
        if now != self.core.hotkeys {
            self.core.hotkeys = now;
            self.core.host.send(Out::HotkeysChanged);
        }
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
            let born = self.core.next_born;
            self.core.next_born += 1;
            live.nodes.insert(
                nid.clone(),
                LiveNode {
                    spec,
                    born,
                    rt: RefCell::new(rt),
                    status: RefCell::default(),
                    jobs: RefCell::default(),
                },
            );
            fresh.push(nid);
        }
        for nid in fresh {
            self.with_node(wid, &nid, |rt, cx| rt.start(cx));
            self.apply_effects();
        }
        self.check_hotkeys();
    }

    fn stop_workspace(&mut self, wid: &str) {
        let Some(live) = self.live.get(wid) else { return };
        let ids: Vec<String> = live.nodes.keys().cloned().collect();
        for nid in ids {
            self.stop_node(wid, &nid);
        }
        self.live.remove(wid);
        self.queue.retain(|q| !matches!(q, Queued::Pulse { ws, .. } if ws == wid));
        self.check_hotkeys();
    }

    fn stop_node(&mut self, wid: &str, nid: &str) {
        self.with_node(wid, nid, |rt, cx| rt.stop(cx));
        self.core.cancel_node_timers(wid, nid);
        self.core.unwatch_node(wid, nid);
        if let Some(node) = self.live.get_mut(wid).and_then(|live| live.nodes.remove(nid)) {
            for stop in node.jobs.borrow().iter() {
                stop.store(true, Ordering::Relaxed);
            }
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
            doc: &self.doc,
            ws_doc,
            node_doc,
            wid,
            nid,
            born: node.born,
            spec: node.spec,
            status: &node.status,
            jobs: &node.jobs,
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
            self.effects.push(Effect::SetEnabled { ws: wid.to_owned(), on: Some(false) });
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

    fn watch(&mut self, kind: Watch, wid: &str, nid: &str) {
        self.watchers.entry(kind).or_default().insert((wid.to_owned(), nid.to_owned()));
        if self.watch_timer.is_none() {
            // the first round straight away, as Python's `singleShot(0, self._tick)`
            self.watch_timer = Some(self.add_timer("", "", 0.0, 0));
        }
    }

    fn unwatch_node(&mut self, wid: &str, nid: &str) {
        let key = (wid.to_owned(), nid.to_owned());
        for subs in self.watchers.values_mut() {
            subs.remove(&key);
        }
        if self.watchers.values().all(BTreeSet::is_empty)
            && let Some(id) = self.watch_timer.take()
        {
            self.cancel_timer(id);
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

/// A field as saved, or its default.
fn field_of<'v>(node: &'v Value, spec: &'static NodeSpec, key: &str) -> &'v Value {
    match node["fields"].get(key) {
        Some(v) => v,
        None => spec.field(key).map_or(&NULL, |f| &f.default),
    }
}

/// `str(value or "")`.
fn raw_text(v: &Value) -> String {
    if text::truthy(v) { text::py_str(v) } else { String::new() }
}

/// What a node can see and do while it runs.
pub struct Cx<'a> {
    core: &'a mut Core,
    ws: &'a WsLive,
    doc: &'a Value,
    ws_doc: &'a Value,
    node_doc: &'a Value,
    wid: &'a str,
    nid: &'a str,
    born: u64,
    pub spec: &'static NodeSpec,
    status: &'a RefCell<Status>,
    jobs: &'a RefCell<Vec<Arc<AtomicBool>>>,
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
        field_of(self.node_doc, self.spec, key)
    }

    /// `str(self.raw(key) or "")`: a field as text, without filling in tokens.
    pub fn raw_str(&self, key: &str) -> String {
        raw_text(self.raw(key))
    }

    /// Another workspace (or this one), as saved.
    pub fn workspace(&self, wid: &str) -> Option<&Value> {
        doc::workspace(self.doc, wid)
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
            doc: self.doc,
            ws_doc: self.ws_doc,
            node_doc,
            wid: self.wid,
            nid: src,
            born: node.born,
            spec: node.spec,
            status: &node.status,
            jobs: &node.jobs,
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

    /// Switches a workspace on or off (`None`: the other way) once this pass is over.
    pub fn set_enabled(&mut self, wid: &str, on: Option<bool>) {
        self.core.effects.push(Effect::SetEnabled { ws: wid.to_owned(), on });
    }

    /// Where the app's answer to a request goes; `token` tells the node which request it was.
    pub fn reply(&self, token: u64) -> Reply {
        Reply { workspace: self.wid.to_owned(), node: self.nid.to_owned(), born: self.born, token }
    }

    /// A way back into this node for another thread (a file watcher's callback). Its news arrives through
    /// [`NodeRuntime::job`] with `token`; it is told to stop when the node goes.
    pub fn job(&mut self, token: u64) -> Job {
        let stop = Arc::new(AtomicBool::new(false));
        let mut jobs = self.jobs.borrow_mut();
        jobs.retain(|s| Arc::strong_count(s) > 1);
        jobs.push(stop.clone());
        Job {
            to: Some(NodeAddr { ws: self.wid.to_owned(), node: self.nid.to_owned(), born: self.born, token }),
            tx: self.core.jobs_tx.clone(),
            wake: self.core.waker.clone(),
            host: self.core.host.clone(),
            stop,
        }
    }

    /// Runs `work` on a thread of its own. What it sends arrives through [`NodeRuntime::job`] with `token`.
    pub fn spawn(&mut self, token: u64, work: impl FnOnce(&Job) + Send + 'static) -> JobHandle {
        let job = self.job(token);
        let handle = JobHandle(job.stop.clone());
        work::spawn(&self.core.jobs_running, "automation-job", move || work(&job));
        handle
    }

    /// Asks for a shared poll; the readings arrive through [`NodeRuntime::reading`] until the node stops.
    pub fn watch(&mut self, kind: Watch) {
        self.core.watch(kind, self.wid, self.nid);
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
