//! Global hotkeys and hold keys (`core/hotkeys.py`, `core/hotkeys_win.py`).
//!
//! Modules register hotkeys under an owner, so they can be torn down and registered again when settings
//! change, and a combo another owner already uses is refused, so two modules (or Luminescent and your own
//! Hyprland config) never fight over keys. When a hotkey fires, the module gets a [`HotkeyEvent`] with the id
//! it registered; nothing from another thread ever calls into module code.
//!
//! Hold keys (Doodle's "hold Alt") report going down and coming back up. A missed key release can not leave
//! one stuck: while a hold key is down it is checked every 100 ms with the system, and released if it is not
//! really held any more.
//!
//! The work happens in a backend on its own thread: Hyprland binds through the Lua table and the FIFO, or one
//! low-level keyboard hook on Windows. The manager itself only keeps the books, on the UI thread.

pub mod keys;
mod matcher;
pub mod vk;

#[cfg(target_os = "linux")]
mod hyprland;
#[cfg(windows)]
mod windows;

pub use keys::{Os, normalize, pretty, pretty_parts, validate};

use crate::desktop::{Desktop, DesktopEvent};
use crossbeam_channel::Receiver;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

/// What a module hears about its hotkeys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeyEvent {
    /// A hotkey was pressed: the id and label it was registered with.
    Fired {
        owner: String,
        id: String,
        label: String,
        combo: String,
    },
    HoldDown {
        owner: String,
        key: String,
    },
    HoldUp {
        owner: String,
        key: String,
    },
    /// Another key (or one of our hotkeys, on Hyprland) went down while this owner's hold key was held, so the
    /// owner can tell "holding Alt" from "using an Alt shortcut". `key` is the other key's name.
    Interrupted {
        owner: String,
        key: String,
    },
    /// The system would not take a hotkey registered a moment ago (the keys are in your own Hyprland config,
    /// or Hyprland said no). It is no longer registered.
    Refused {
        owner: String,
        combo: String,
        reason: String,
    },
}

/// How a hotkey behaves.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    /// The key press also reaches the focused app (like Esc for Doodle).
    pub non_consuming: bool,
    /// It also fires while other keys are held (W to walk, Ctrl to sprint in a game); the combo's own
    /// modifiers still have to be down.
    pub always: bool,
}

/// What a backend tells the manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Signal {
    Pressed {
        combo: String,
        at: Instant,
    },
    HoldDown {
        key: String,
    },
    /// `missed`: the release was never seen; the check found the key up.
    HoldUp {
        key: String,
        missed: bool,
    },
    /// `by` went down while `hold` was held. `by_combo` is set when `by` is one of our hotkeys.
    Interrupt {
        hold: String,
        by: String,
        by_combo: Option<String>,
    },
    // the Windows hook takes every combo it can name
    #[cfg_attr(windows, allow(dead_code))]
    Refused {
        combo: String,
        reason: String,
    },
}

/// One way of seeing keys. Each runs on its own thread and reports through the signal channel.
pub(crate) trait Backend: Send {
    /// Starts listening for `combo` (normalised and valid). `check_clash`: refuse it (later, by a signal) when
    /// the user's own binds already use the keys.
    fn add(&mut self, combo: &str, opts: Options, check_clash: bool);
    fn remove(&mut self, combo: &str);
    fn add_hold(&mut self, key: &str);
    fn remove_hold(&mut self, key: &str);
    /// While suspended nothing fires and, on Hyprland, the binds are lifted so the keys reach the app.
    fn suspend(&mut self, on: bool);
    /// Forget every key thought to be held.
    fn reset(&mut self);
    fn send(&self, combo: &str);
    fn release_modifiers(&self);
    fn desktop_event(&mut self, _event: &DesktopEvent) {}
    fn shutdown(&mut self);
}

/// Registers nothing anywhere: tests, `--smoke`, a test copy next to the real one, or no way to see keys.
struct Dry;

impl Backend for Dry {
    fn add(&mut self, _combo: &str, _opts: Options, _check_clash: bool) {}
    fn remove(&mut self, _combo: &str) {}
    fn add_hold(&mut self, _key: &str) {}
    fn remove_hold(&mut self, _key: &str) {}
    fn suspend(&mut self, _on: bool) {}
    fn reset(&mut self) {}
    fn send(&self, _combo: &str) {}
    fn release_modifiers(&self) {}
    fn shutdown(&mut self) {}
}

struct Hotkey {
    owner: String,
    id: String,
    label: String,
}

struct Hold {
    owner: String,
    interruptible: bool,
    down: bool,
    since: Instant,
}

/// One row of [`HotkeyManager::listing`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub combo: String,
    pub owner: String,
    pub label: String,
}

/// Wakes the UI thread when a backend has something to report.
pub type Wake = Arc<dyn Fn() + Send + Sync>;

pub struct HotkeyManager {
    backend: Box<dyn Backend>,
    signals: Receiver<Signal>,
    hotkeys: BTreeMap<String, Hotkey>,
    holds: BTreeMap<String, Hold>,
    suspended: bool,
    dry: bool,
    os: Os,
    latency: Latency,
    out: Vec<HotkeyEvent>,
}

impl HotkeyManager {
    /// The backend this system has: Hyprland's binds, the Windows hook, or none. A test copy
    /// (`LUMINESCENT_NO_HOOK`) never hooks, so it can not take keys from the real one.
    pub fn start(desktop: &Desktop, wake: Wake) -> HotkeyManager {
        let (tx, rx) = crossbeam_channel::unbounded();
        if std::env::var_os("LUMINESCENT_NO_HOOK").is_some() {
            return HotkeyManager::with(Box::new(Dry), rx, true);
        }
        #[cfg(target_os = "linux")]
        if let Some(hypr) = desktop.hypr() {
            return HotkeyManager::with(
                Box::new(hyprland::HyprKeys::start(hypr.clone(), tx, wake)),
                rx,
                false,
            );
        }
        #[cfg(windows)]
        {
            let _ = desktop;
            match windows::HookKeys::start(tx, wake) {
                Ok(backend) => return HotkeyManager::with(Box::new(backend), rx, false),
                Err(e) => tracing::error!("could not hook the keyboard: {e}"),
            }
        }
        #[cfg(not(windows))]
        {
            let _ = (tx, wake);
            if desktop.describe() != "Hyprland" {
                tracing::warn!("Hyprland is not running: global hotkeys are off");
            }
        }
        HotkeyManager::with(Box::new(Dry), rx, true)
    }

    /// No hotkeys reach the system; registering still keeps the books (tests, `--smoke`).
    pub fn dry() -> HotkeyManager {
        let (_tx, rx) = crossbeam_channel::unbounded();
        HotkeyManager::with(Box::new(Dry), rx, true)
    }

    fn with(backend: Box<dyn Backend>, signals: Receiver<Signal>, dry: bool) -> HotkeyManager {
        HotkeyManager {
            backend,
            signals,
            hotkeys: BTreeMap::new(),
            holds: BTreeMap::new(),
            suspended: false,
            dry,
            os: Os::current(),
            latency: Latency::default(),
            out: Vec::new(),
        }
    }

    #[cfg(test)]
    fn testing(backend: Box<dyn Backend>, signals: Receiver<Signal>, os: Os) -> HotkeyManager {
        HotkeyManager { os, ..HotkeyManager::with(backend, signals, false) }
    }

    /// Registers a press hotkey; `id` comes back in [`HotkeyEvent::Fired`]. Errors are the Python wording.
    pub fn register(
        &mut self,
        owner: &str,
        combo: &str,
        id: &str,
        label: &str,
        opts: Options,
    ) -> Result<(), String> {
        let combo = normalize(combo);
        if combo.is_empty() {
            return Ok(());
        }
        let err = validate(&combo, self.os);
        if !err.is_empty() {
            tracing::warn!("{owner}: invalid hotkey {combo:?} ({err})");
            return Err(err);
        }
        if let Some(other) = self.hotkeys.get(&combo).filter(|h| h.owner != owner) {
            let msg = format!("{} is already used by {}", pretty(&combo, self.os), other.owner);
            tracing::warn!("{owner}: {msg}");
            return Err(msg);
        }
        if self.hotkeys.contains_key(&combo) {
            self.backend.remove(&combo);
        }
        self.hotkeys.insert(
            combo.clone(),
            Hotkey { owner: owner.to_owned(), id: id.to_owned(), label: label.to_owned() },
        );
        if !self.dry {
            self.backend.add(&combo, opts, !(opts.non_consuming || opts.always));
        }
        tracing::info!(
            "{owner}: {} -> {}",
            pretty(&combo, self.os),
            if label.is_empty() { id } else { label }
        );
        Ok(())
    }

    /// Reports `key` going down and coming back up. `interruptible`: also report other keys pressed while it
    /// is held.
    pub fn register_hold(&mut self, owner: &str, key: &str, interruptible: bool) -> Result<(), String> {
        let key = normalize(key);
        if key.is_empty() {
            return Ok(());
        }
        if key.contains('+') && key != "+" {
            return Err("Hold keys must be a single key (e.g. alt, or f8)".to_owned());
        }
        let err = validate(&key, self.os);
        if !err.is_empty() {
            return Err(err);
        }
        if let Some(other) = self.holds.get(&key).filter(|h| h.owner != owner) {
            return Err(format!("{} is already a hold key for {}", pretty(&key, self.os), other.owner));
        }
        if self.holds.contains_key(&key) {
            self.backend.remove_hold(&key);
        }
        self.holds.insert(
            key.clone(),
            Hold { owner: owner.to_owned(), interruptible, down: false, since: Instant::now() },
        );
        if !self.dry {
            self.backend.add_hold(&key);
        }
        tracing::info!("{owner}: hold {}", pretty(&key, self.os));
        Ok(())
    }

    pub fn unregister_all(&mut self, owner: &str) {
        let combos: Vec<String> =
            self.hotkeys.iter().filter(|(_, h)| h.owner == owner).map(|(c, _)| c.clone()).collect();
        for combo in combos {
            self.hotkeys.remove(&combo);
            self.backend.remove(&combo);
        }
        let keys: Vec<String> =
            self.holds.iter().filter(|(_, h)| h.owner == owner).map(|(k, _)| k.clone()).collect();
        for key in keys {
            self.holds.remove(&key);
            self.backend.remove_hold(&key);
        }
    }

    pub fn owner_of(&self, combo: &str) -> Option<&str> {
        self.hotkeys.get(&normalize(combo)).map(|h| h.owner.as_str())
    }

    /// Every hotkey and hold key, for the Keybinds page.
    pub fn listing(&self) -> Vec<Listed> {
        let presses = self.hotkeys.iter().map(|(combo, h)| Listed {
            combo: combo.clone(),
            owner: h.owner.clone(),
            label: if h.label.is_empty() { h.id.clone() } else { h.label.clone() },
        });
        let holds = self.holds.iter().map(|(key, h)| Listed {
            combo: key.clone(),
            owner: h.owner.clone(),
            label: "hold to use".to_owned(),
        });
        presses.chain(holds).collect()
    }

    /// `{combo: owner}` for every press hotkey.
    pub fn combos(&self) -> BTreeMap<String, String> {
        self.hotkeys.iter().map(|(c, h)| (c.clone(), h.owner.clone())).collect()
    }

    /// Ignores every hotkey (while a new one is being recorded in Settings). On Hyprland the binds are lifted
    /// so the keys reach the recorder instead of being swallowed.
    pub fn set_suspended(&mut self, on: bool) {
        if on == self.suspended {
            return;
        }
        self.suspended = on;
        self.backend.suspend(on);
        if !on {
            self.release_holds();
        }
    }

    /// Lets go of every hold key and forgets every key thought to be held (tray → Reset stuck keys).
    pub fn reset_keys(&mut self) {
        self.backend.reset();
        self.release_holds();
        tracing::info!("hold keys reset");
    }

    fn release_holds(&mut self) {
        for (key, h) in &mut self.holds {
            if std::mem::take(&mut h.down) {
                self.out.push(HotkeyEvent::HoldUp { owner: h.owner.clone(), key: key.clone() });
            }
        }
    }

    /// Sends a key combination (`ctrl+v`, `enter`) to the focused window.
    pub fn send(&self, combo: &str) {
        self.backend.send(&normalize(combo));
    }

    /// Lets go of any modifier the user is still physically holding (Ctrl after a Ctrl+click), so a sent key
    /// is not read as a shortcut. Nothing to do on Wayland: sending names its modifiers exactly.
    pub fn release_modifiers(&self) {
        self.backend.release_modifiers();
    }

    /// Hands desktop events to the backend (Hyprland's binds report through the FIFO and need putting back
    /// after a config reload).
    pub fn desktop_event(&mut self, event: &DesktopEvent) {
        self.backend.desktop_event(event);
    }

    /// Everything that happened since the last call. Call it when woken.
    pub fn poll(&mut self) -> Vec<HotkeyEvent> {
        while let Ok(signal) = self.signals.try_recv() {
            self.handle(signal);
        }
        std::mem::take(&mut self.out)
    }

    fn handle(&mut self, signal: Signal) {
        match signal {
            Signal::Pressed { combo, at } => {
                let Some(h) = self.hotkeys.get(&combo) else { return };
                if self.suspended {
                    return;
                }
                self.out.push(HotkeyEvent::Fired {
                    owner: h.owner.clone(),
                    id: h.id.clone(),
                    label: h.label.clone(),
                    combo: combo.clone(),
                });
                self.latency.record(at.elapsed());
            }
            Signal::HoldDown { key } => {
                let Some(h) = self.holds.get_mut(&key) else { return };
                if h.down || self.suspended {
                    return;
                }
                tracing::info!("{}: {} down", h.owner, pretty(&key, self.os));
                h.down = true;
                h.since = Instant::now();
                self.out.push(HotkeyEvent::HoldDown { owner: h.owner.clone(), key });
            }
            Signal::HoldUp { key, missed } => {
                let Some(h) = self.holds.get_mut(&key) else { return };
                if !h.down {
                    return;
                }
                if missed {
                    tracing::info!("{}: release of {} missed; releasing", h.owner, pretty(&key, self.os));
                } else {
                    let held = h.since.elapsed().as_secs_f64();
                    tracing::info!("{}: {} up after {held:.2} s", h.owner, pretty(&key, self.os));
                }
                h.down = false;
                self.out.push(HotkeyEvent::HoldUp { owner: h.owner.clone(), key });
            }
            Signal::Interrupt { hold, by, by_combo } => {
                let Some(h) = self.holds.get(&hold) else { return };
                let same_owner =
                    by_combo.and_then(|c| self.hotkeys.get(&c)).is_some_and(|c| c.owner == h.owner);
                if h.down && h.interruptible && !same_owner && !self.suspended {
                    self.out.push(HotkeyEvent::Interrupted { owner: h.owner.clone(), key: by });
                }
            }
            Signal::Refused { combo, reason } => {
                tracing::warn!("{combo}: {reason}");
                if let Some(h) = self.hotkeys.remove(&combo) {
                    self.backend.remove(&combo);
                    self.out.push(HotkeyEvent::Refused { owner: h.owner, combo, reason });
                }
            }
        }
    }

    /// The slowest 1 % of hotkeys, from the system's report to the module's event, in milliseconds.
    pub fn latency_p99_ms(&self) -> Option<f64> {
        self.latency.p99_ms()
    }

    pub fn shutdown(&mut self) {
        let owners: Vec<String> = self.hotkeys.values().map(|h| h.owner.clone()).collect();
        for owner in owners {
            self.unregister_all(&owner);
        }
        self.backend.shutdown();
    }
}

/// The last 256 hotkey delays, logged now and then at debug level.
#[derive(Default)]
struct Latency {
    samples: Vec<f64>,
    next: usize,
    count: u64,
}

impl Latency {
    fn record(&mut self, took: std::time::Duration) {
        let ms = took.as_secs_f64() * 1000.0;
        if self.samples.len() < 256 {
            self.samples.push(ms);
        } else {
            self.samples[self.next] = ms;
        }
        self.next = (self.next + 1) % 256;
        self.count += 1;
        if self.count.is_multiple_of(50) {
            tracing::debug!(
                "hotkey latency: p99 {:.2} ms over the last {}",
                self.p99_ms().unwrap_or(0.0),
                self.samples.len()
            );
        }
    }

    fn p99_ms(&self) -> Option<f64> {
        if self.samples.is_empty() {
            return None;
        }
        let mut sorted = self.samples.clone();
        sorted.sort_by(f64::total_cmp);
        let i = ((sorted.len() as f64) * 0.99).ceil() as usize;
        Some(sorted[i.clamp(1, sorted.len()) - 1])
    }
}

/// Feeds the manager by hand, for tests of the bookkeeping.
#[cfg(test)]
struct Scripted {
    log: Arc<std::sync::Mutex<Vec<String>>>,
}

#[cfg(test)]
impl Backend for Scripted {
    fn add(&mut self, combo: &str, opts: Options, check_clash: bool) {
        self.log.lock().expect("lock").push(format!("add {combo} {opts:?} clash={check_clash}"));
    }
    fn remove(&mut self, combo: &str) {
        self.log.lock().expect("lock").push(format!("remove {combo}"));
    }
    fn add_hold(&mut self, key: &str) {
        self.log.lock().expect("lock").push(format!("hold {key}"));
    }
    fn remove_hold(&mut self, key: &str) {
        self.log.lock().expect("lock").push(format!("unhold {key}"));
    }
    fn suspend(&mut self, on: bool) {
        self.log.lock().expect("lock").push(format!("suspend {on}"));
    }
    fn reset(&mut self) {}
    fn send(&self, _combo: &str) {}
    fn release_modifiers(&self) {}
    fn shutdown(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::Sender;

    fn manager() -> (HotkeyManager, Sender<Signal>, Arc<std::sync::Mutex<Vec<String>>>) {
        let (tx, rx) = crossbeam_channel::unbounded();
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let m = HotkeyManager::testing(Box::new(Scripted { log: log.clone() }), rx, Os::Linux);
        (m, tx, log)
    }

    fn pressed(combo: &str) -> Signal {
        Signal::Pressed { combo: combo.to_owned(), at: Instant::now() }
    }

    #[test]
    fn owners_can_not_take_each_others_keys() {
        let (mut m, _tx, log) = manager();
        assert_eq!(
            m.register("overlay", "Ctrl + Shift+Space", "open", "Open the overlay", Options::default()),
            Ok(())
        );
        assert_eq!(
            m.register("notes", "ctrl+shift+space", "open", "", Options::default()),
            Err("Ctrl+Shift+Space is already used by overlay".to_owned())
        );
        assert_eq!(
            m.register("notes", "ctrl+unknownkey", "x", "", Options::default()),
            Err("Unknown key: unknownkey".into())
        );
        assert_eq!(m.register("notes", "", "x", "", Options::default()), Ok(()));
        assert_eq!(m.owner_of("CTRL+SHIFT+SPACE"), Some("overlay"));
        assert_eq!(
            log.lock().expect("lock")[0],
            "add ctrl+shift+space Options { non_consuming: false, always: false } clash=true"
        );
        m.unregister_all("overlay");
        assert_eq!(m.owner_of("ctrl+shift+space"), None);
        assert!(log.lock().expect("lock").contains(&"remove ctrl+shift+space".to_owned()));
    }

    #[test]
    fn fired_hotkeys_carry_their_id() {
        let (mut m, tx, _log) = manager();
        m.register(
            "overlay",
            "ctrl+shift+space",
            "open",
            "",
            Options { non_consuming: true, ..Default::default() },
        )
        .expect("ok");
        tx.send(pressed("ctrl+shift+space")).expect("send");
        tx.send(pressed("ctrl+x")).expect("send");
        assert_eq!(
            m.poll(),
            [HotkeyEvent::Fired {
                owner: "overlay".into(),
                id: "open".into(),
                label: String::new(),
                combo: "ctrl+shift+space".into()
            }]
        );
        assert!(m.latency_p99_ms().is_some());
        m.set_suspended(true);
        tx.send(pressed("ctrl+shift+space")).expect("send");
        assert!(m.poll().is_empty(), "nothing fires while suspended");
    }

    #[test]
    fn holds_go_down_once_and_come_up() {
        let (mut m, tx, _log) = manager();
        assert_eq!(
            m.register_hold("doodle", "alt+x", true),
            Err("Hold keys must be a single key (e.g. alt, or f8)".into())
        );
        m.register_hold("doodle", "alt", true).expect("ok");
        m.register("notes", "alt+n", "open", "", Options::default()).expect("ok");
        assert_eq!(
            m.register_hold("timers", "Alt", false),
            Err("Alt is already a hold key for doodle".into())
        );
        // the two press binds of a modifier both report
        tx.send(Signal::HoldDown { key: "alt".into() }).expect("send");
        tx.send(Signal::HoldDown { key: "alt".into() }).expect("send");
        tx.send(Signal::Interrupt { hold: "alt".into(), by: "n".into(), by_combo: Some("alt+n".into()) })
            .expect("send");
        tx.send(Signal::HoldUp { key: "alt".into(), missed: true }).expect("send");
        tx.send(Signal::HoldUp { key: "alt".into(), missed: false }).expect("send");
        assert_eq!(
            m.poll(),
            [
                HotkeyEvent::HoldDown { owner: "doodle".into(), key: "alt".into() },
                HotkeyEvent::Interrupted { owner: "doodle".into(), key: "n".into() },
                HotkeyEvent::HoldUp { owner: "doodle".into(), key: "alt".into() },
            ]
        );
        assert_eq!(m.listing().len(), 2);
    }

    #[test]
    fn reset_lets_go_of_held_keys() {
        let (mut m, tx, _log) = manager();
        m.register_hold("doodle", "f8", false).expect("ok");
        tx.send(Signal::HoldDown { key: "f8".into() }).expect("send");
        assert_eq!(m.poll().len(), 1);
        m.reset_keys();
        assert_eq!(m.poll(), [HotkeyEvent::HoldUp { owner: "doodle".into(), key: "f8".into() }]);
    }

    #[test]
    fn a_refusal_takes_the_hotkey_back() {
        let (mut m, tx, log) = manager();
        m.register("overlay", "super+space", "open", "", Options::default()).expect("ok");
        tx.send(Signal::Refused { combo: "windows+space".into(), reason: "already bound".into() })
            .expect("send");
        assert_eq!(
            m.poll(),
            [HotkeyEvent::Refused {
                owner: "overlay".into(),
                combo: "windows+space".into(),
                reason: "already bound".into()
            }]
        );
        assert_eq!(m.owner_of("super+space"), None);
        assert_eq!(log.lock().expect("lock").last().map(String::as_str), Some("remove windows+space"));
    }

    #[test]
    fn latency_p99() {
        let mut l = Latency::default();
        for ms in 1..=100 {
            l.record(std::time::Duration::from_millis(ms));
        }
        assert_eq!(l.p99_ms(), Some(99.0));
    }
}
