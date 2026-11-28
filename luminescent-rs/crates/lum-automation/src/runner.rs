//! Runs the engine in the background, on a thread of its own.
//!
//! The thread sleeps until the engine's next timer, until something pokes it (an edit, ▶, a hotkey, a job with
//! news) or until Luminescent says something happened, so with no workspace running it never wakes up. The UI
//! reaches the engine through [`Automation::with`]; the engine is only locked for one short pass at a time, never
//! while it waits.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, select};
use lum_core::events::Event;
use lum_core::store::Writer;

use crate::engine::{Answer, Engine, Reply, Updates};

/// How long after a change the file is saved. Python restarted its 600 ms timer on every change; here the first
/// change starts it, so a workspace that never stops changing things still gets saved.
const SAVE_AFTER: f64 = 0.6;

enum Msg {
    Poke,
    Quit,
}

type Wake = Arc<dyn Fn() + Send + Sync>;

pub struct Automation {
    handle: Handle,
    thread: Option<JoinHandle<()>>,
    path: PathBuf,
    writer: Arc<Writer>,
}

/// The engine from any thread (a worker answering a node): the same `with` as [`Automation`], cheap to clone.
#[derive(Clone)]
pub struct Handle {
    engine: Arc<Mutex<Engine>>,
    tx: Sender<Msg>,
}

impl std::fmt::Debug for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("automation::Handle")
    }
}

impl Handle {
    /// Does something with the engine, then lets its thread look at what changed (new timers, pulses to send).
    pub fn with<R>(&self, f: impl FnOnce(&mut Engine) -> R) -> R {
        let result = f(&mut lock(&self.engine));
        let _ = self.tx.send(Msg::Poke);
        result
    }

    /// Reads the engine without poking its thread (the editor drawing).
    pub fn read<R>(&self, f: impl FnOnce(&Engine) -> R) -> R {
        f(&lock(&self.engine))
    }

    /// The app's answer to a node's request.
    pub fn answer(&self, reply: &Reply, answer: Answer) {
        self.with(|e| e.answer(reply, answer));
    }

    /// What changed since the editor last looked. Takes nothing the engine's thread needs, so it is not poked.
    pub fn take_updates(&self) -> Updates {
        lock(&self.engine).take_updates()
    }
}

impl std::fmt::Debug for Automation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Automation").field("path", &self.path).finish_non_exhaustive()
    }
}

impl Automation {
    /// Starts the engine and its thread. `path` is `automation.json`; `ui_wake` is called when the editor is
    /// watching and has something new to draw.
    pub fn start(engine: Engine, path: PathBuf, ui_wake: impl Fn() + Send + Sync + 'static) -> Automation {
        Automation::start_with_events(engine, path, ui_wake, crossbeam_channel::never())
    }

    /// The same, and the event triggers hear what Luminescent's event bus says.
    pub fn start_with_events(
        mut engine: Engine,
        path: PathBuf,
        ui_wake: impl Fn() + Send + Sync + 'static,
        events: Receiver<Event>,
    ) -> Automation {
        let (tx, rx) = crossbeam_channel::unbounded();
        let poke = tx.clone();
        engine.set_waker(move || {
            let _ = poke.send(Msg::Poke);
        });
        engine.start();
        let engine = Arc::new(Mutex::new(engine));
        let writer = Arc::new(Writer::new("automation-save"));
        let thread = {
            let (engine, writer, path) = (engine.clone(), writer.clone(), path.clone());
            let ui_wake: Wake = Arc::new(ui_wake);
            std::thread::Builder::new()
                .name("automation".into())
                .spawn(move || run(&engine, &rx, &events, &writer, &path, &ui_wake))
                .ok()
        };
        Automation { handle: Handle { engine, tx }, thread, path, writer }
    }

    pub fn handle(&self) -> Handle {
        self.handle.clone()
    }

    /// Does something with the engine, then lets its thread look at what changed (new timers, pulses to send).
    pub fn with<R>(&self, f: impl FnOnce(&mut Engine) -> R) -> R {
        self.handle.with(f)
    }

    /// Reads the engine without poking its thread (the editor drawing).
    pub fn read<R>(&self, f: impl FnOnce(&Engine) -> R) -> R {
        self.handle.read(f)
    }

    /// Stops every node and writes the file, waiting until it is on disk (at quit).
    pub fn shutdown(&mut self) {
        let Some(thread) = self.thread.take() else { return };
        let _ = self.handle.tx.send(Msg::Quit);
        let _ = thread.join();
        let text = {
            let mut engine = lock(&self.handle.engine);
            engine.shutdown();
            engine.save_text()
        };
        self.writer.save(&self.path, text);
        self.writer.flush();
    }
}

impl Drop for Automation {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn lock(engine: &Mutex<Engine>) -> MutexGuard<'_, Engine> {
    engine.lock().unwrap_or_else(|e| e.into_inner())
}

fn run(
    engine: &Mutex<Engine>,
    rx: &Receiver<Msg>,
    events: &Receiver<Event>,
    writer: &Writer,
    path: &std::path::Path,
    ui_wake: &Wake,
) {
    let clock = lock(engine).clock();
    let mut save_at: Option<f64> = None;
    let mut events = events.clone();
    loop {
        let (more, deadline, tell_ui) = {
            let mut e = lock(engine);
            let more = e.step();
            if e.take_dirty() && save_at.is_none() {
                save_at = Some(clock.mono() + SAVE_AFTER);
            }
            if save_at.is_some_and(|at| at <= clock.mono()) {
                writer.save(path, e.save_text());
                save_at = None;
            }
            (more, e.next_deadline(), e.watching() && e.has_updates())
        };
        if tell_ui {
            ui_wake();
        }
        if more {
            continue;
        }
        let wake_at = match (deadline, save_at) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let timeout = match wake_at {
            Some(at) => {
                crossbeam_channel::after(Duration::from_secs_f64((at - clock.mono()).clamp(0.0, 86_400.0)))
            }
            None => crossbeam_channel::never(),
        };
        let msg = select! {
            recv(rx) -> msg => Some(msg.unwrap_or(Msg::Quit)),
            recv(events) -> event => {
                match event {
                    Ok(event) => lock(engine).event(&event.name, &event.text),
                    // the bus went away (the app is closing): nothing more to hear
                    Err(_) => events = crossbeam_channel::never(),
                }
                None
            }
            recv(timeout) -> _ => None,
        };
        if matches!(msg, Some(Msg::Quit)) {
            break;
        }
        // A burst of pokes needs only one pass.
        while let Ok(more) = rx.try_recv() {
            if matches!(more, Msg::Quit) {
                return;
            }
        }
    }
}
