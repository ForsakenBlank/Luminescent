//! Work that waits: commands, web requests, downloads, file copies and the shared watchers run on threads of
//! their own and report back into the engine, which hands what they say to the node on its next pass.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crossbeam_channel::Sender;

use crate::engine::Host;

pub(crate) type Wake = Arc<dyn Fn() + Send + Sync>;

/// What a job tells its node.
#[derive(Debug, Clone, PartialEq)]
pub enum JobMsg {
    /// A line of output on the way (a command printing).
    Line(String),
    /// The node's status line, while it works.
    Status(String),
    /// It finished: whether it went well, the text that goes on, and a short code (an exit or HTTP status).
    Done { ok: bool, text: String, code: String },
}

/// The shared polls a node can ask for. They run every two seconds while any node wants them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Watch {
    Programs,
    Idle,
    Levels,
    Window,
    Internet,
    Clipboard,
}

/// A level the threshold trigger watches.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Level {
    Value(f64),
    /// This computer has none (no battery, no sensor).
    Missing,
    /// Luminescent can not read it yet (the sampler comes in a later version).
    NotYet,
}

/// One round of the shared polls. Only what some node wanted is filled in.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Readings {
    pub programs: Option<BTreeSet<String>>,
    pub idle: Option<f64>,
    /// Each metric a threshold trigger watches, with its level.
    pub levels: Vec<(String, Level)>,
    /// The focused window: its app (class) and title.
    pub window: Option<(String, String)>,
    /// Whether the internet can be reached (`Some(None)`: not known yet).
    pub online: Option<Option<bool>>,
    /// The clipboard's text, only when it changed since the last round.
    pub clipboard: Option<String>,
}

impl Readings {
    pub fn level(&self, metric: &str) -> Option<Level> {
        self.levels.iter().find(|(m, _)| m == metric).map(|(_, l)| *l)
    }
}

/// What the readings job should read.
#[derive(Debug, Default)]
pub(crate) struct Want {
    pub programs: bool,
    pub idle: bool,
    pub metrics: Vec<String>,
    pub window: bool,
    pub online: bool,
    pub clipboard: bool,
}

impl Want {
    pub fn read(&self, host: &dyn Host) -> Readings {
        Readings {
            programs: self.programs.then(|| host.process_names()),
            idle: self.idle.then(|| host.idle_seconds()),
            levels: self.metrics.iter().map(|m| (m.clone(), host.level(m))).collect(),
            window: self.window.then(|| host.focused_window()),
            online: self.online.then(|| host.online()),
            clipboard: self.clipboard.then(|| host.clipboard_text()),
        }
    }
}

/// Where a job's news goes: a node (as it was when the job started) and the token it asked with.
#[derive(Debug, Clone)]
pub(crate) struct NodeAddr {
    pub ws: String,
    pub node: String,
    pub born: u64,
    pub token: u64,
}

pub(crate) enum JobWhat {
    Msg(JobMsg),
    Readings(Readings),
}

pub(crate) struct JobNote {
    pub to: Option<NodeAddr>,
    pub what: JobWhat,
}

/// A job's way back to its node, usable from any thread.
#[derive(Clone)]
pub struct Job {
    pub(crate) to: Option<NodeAddr>,
    pub(crate) tx: Sender<JobNote>,
    pub(crate) wake: Wake,
    pub(crate) host: Arc<dyn Host>,
    pub(crate) stop: Arc<AtomicBool>,
}

impl Job {
    pub fn send(&self, msg: JobMsg) {
        self.note(JobWhat::Msg(msg));
    }

    pub(crate) fn note(&self, what: JobWhat) {
        if self.tx.send(JobNote { to: self.to.clone(), what }).is_ok() {
            (self.wake)();
        }
    }

    pub fn host(&self) -> &dyn Host {
        &*self.host
    }

    /// The node went away or asked the job to stop.
    pub fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }
}

/// Stops a job the node started (the Run command's stop input).
#[derive(Debug, Clone)]
pub struct JobHandle(pub(crate) Arc<AtomicBool>);

impl JobHandle {
    pub fn stop(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// Counts a job thread for as long as it lives.
pub(crate) struct Running(Arc<AtomicUsize>);

impl Running {
    pub fn new(count: &Arc<AtomicUsize>) -> Running {
        count.fetch_add(1, Ordering::SeqCst);
        Running(count.clone())
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Runs `work` on a thread of its own, counted until it ends.
pub(crate) fn spawn(count: &Arc<AtomicUsize>, name: &str, work: impl FnOnce() + Send + 'static) {
    let running = Running::new(count);
    let spawned = std::thread::Builder::new().name(name.to_owned()).spawn(move || {
        let _running = running;
        work();
    });
    if let Err(e) = spawned {
        tracing::warn!("automation: could not start a thread: {e}");
    }
}
