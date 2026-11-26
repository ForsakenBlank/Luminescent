//! Hotkeys as Hyprland binds. Wayland gives no program a global keyboard hook, so every hotkey is a bind added
//! at runtime through our Lua table; when it fires, Hyprland writes the bind's id into our FIFO. Binds are put
//! back after a config reload, which throws away everything added at runtime.
//!
//! Hold keys are a press bind plus a release bind, both non-consuming so the key still reaches the focused
//! app. While one is down, the worker asks Hyprland every 100 ms whether it really still is.

use super::{Backend, Options, Signal, Wake, keys};
use crate::desktop::DesktopEvent;
use crossbeam_channel::{Receiver, Sender};
use lum_hypr::Hypr;
use lum_hypr::lua::BindOptions;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
enum Target {
    Combo { combo: String, check_mods: Vec<String> },
    HoldPress(String),
    HoldRelease(String),
}

#[derive(Debug, Clone)]
struct Bind {
    keys: String,
    opts: BindOptions,
    target: Target,
}

enum Cmd {
    /// Binds these (all for one combo or hold key); `clash` asks to check the user's own binds first.
    Bind {
        binds: Vec<(String, Bind)>,
        clash: Option<(String, Vec<String>)>,
    },
    Unbind(Vec<String>),
    UnbindAll,
    Reinstall(Vec<(String, Bind)>),
    /// The combo's modifiers must really be down (an `always` hotkey: `ignore_mods` drops them).
    FireIfHeld {
        combo: String,
        mods: Vec<String>,
        at: Instant,
    },
    Watch(String),
    Unwatch(String),
    ResetWatches,
    WatchRelease(Option<String>),
    Send(String),
    Stop,
}

pub struct HyprKeys {
    tx: Sender<Cmd>,
    signals: Sender<Signal>,
    wake: Wake,
    binds: BTreeMap<String, Bind>,
    seq: u64,
    suspended: bool,
    held: Vec<String>,
}

impl HyprKeys {
    pub fn start(hypr: Hypr, signals: Sender<Signal>, wake: Wake) -> HyprKeys {
        let (tx, rx) = crossbeam_channel::unbounded();
        let worker = Worker {
            hypr,
            signals: signals.clone(),
            wake: wake.clone(),
            trusted: false,
            foreign: None,
            watches: HashMap::new(),
            release: None,
        };
        std::thread::Builder::new()
            .name("hypr-keys".into())
            .spawn(move || worker.run(rx))
            .expect("a thread can be started at launch");
        HyprKeys { tx, signals, wake, binds: BTreeMap::new(), seq: 0, suspended: false, held: Vec::new() }
    }

    fn signal(&self, s: Signal) {
        let _ = self.signals.send(s);
        (self.wake)();
    }

    fn new_bind(&mut self, keys: String, opts: BindOptions, target: Target) -> (String, Bind) {
        self.seq += 1;
        let id = format!("k{}", self.seq);
        let bind = Bind { keys, opts, target };
        self.binds.insert(id.clone(), bind.clone());
        (id, bind)
    }

    fn drop_where(&mut self, pred: impl Fn(&Target) -> bool) {
        let ids: Vec<String> =
            self.binds.iter().filter(|(_, b)| pred(&b.target)).map(|(id, _)| id.clone()).collect();
        for id in &ids {
            self.binds.remove(id);
        }
        if !ids.is_empty() && !self.suspended {
            let _ = self.tx.send(Cmd::Unbind(ids));
        }
    }

    fn bind(&self, binds: Vec<(String, Bind)>, clash: Option<(String, Vec<String>)>) {
        if !self.suspended {
            let _ = self.tx.send(Cmd::Bind { binds, clash });
        }
    }
}

const fn opts(release: bool, non_consuming: bool, ignore_mods: bool) -> BindOptions {
    BindOptions { release, non_consuming, ignore_mods }
}

impl Backend for HyprKeys {
    fn add(&mut self, combo: &str, o: Options, check_clash: bool) {
        let mut keys_list = keys::to_hyprland(combo);
        let (mods, key) = keys::split(combo);
        let mut check_mods = Vec::new();
        if o.always && !mods.is_empty() {
            // Hyprland's ignore_mods drops the combo's own modifiers too: check those by hand
            let ks = keys::keysym(&key, keys::Os::Linux);
            if !ks.is_empty() {
                keys_list = vec![ks];
            }
            check_mods = mods;
        }
        // a bare modifier (the cheat sheet on "alt") fires on release, or it would eat every Alt shortcut
        let bare_mod = keys::is_modifier(combo);
        // hotkeys also reach the focused app, as the Windows hook does
        let options = opts(bare_mod, true, o.always && !bare_mod);
        let binds: Vec<(String, Bind)> = keys_list
            .iter()
            .map(|k| {
                let target = Target::Combo { combo: combo.to_owned(), check_mods: check_mods.clone() };
                self.new_bind(k.clone(), options, target)
            })
            .collect();
        let clash = check_clash.then(|| (combo.to_owned(), keys_list));
        self.bind(binds, clash);
    }

    fn remove(&mut self, combo: &str) {
        self.drop_where(|t| matches!(t, Target::Combo { combo: c, .. } if c == combo));
    }

    fn add_hold(&mut self, key: &str) {
        let mut binds = Vec::new();
        let press = || Target::HoldPress(key.to_owned());
        let release = || Target::HoldRelease(key.to_owned());
        for ks in keys::keysyms_of(key, keys::Os::Linux) {
            match keys::hypr_modifier(key) {
                // depending on timing Hyprland sees the modifier itself as held or not: bind both
                Some(m) => {
                    binds.push(self.new_bind(ks.clone(), opts(false, true, false), press()));
                    binds.push(self.new_bind(format!("{m} + {ks}"), opts(false, true, false), press()));
                    binds.push(self.new_bind(format!("{m} + {ks}"), opts(true, true, false), release()));
                    binds.push(self.new_bind(ks, opts(true, true, false), release()));
                }
                None => {
                    binds.push(self.new_bind(ks.clone(), opts(false, true, false), press()));
                    binds.push(self.new_bind(ks, opts(true, true, false), release()));
                }
            }
        }
        self.bind(binds, None);
    }

    fn remove_hold(&mut self, key: &str) {
        self.drop_where(|t| matches!(t, Target::HoldPress(k) | Target::HoldRelease(k) if k == key));
        self.held.retain(|k| k != key);
        let _ = self.tx.send(Cmd::Unwatch(key.to_owned()));
    }

    fn suspend(&mut self, on: bool) {
        self.suspended = on;
        let _ = self.tx.send(if on {
            Cmd::UnbindAll
        } else {
            Cmd::Reinstall(self.binds.iter().map(|(id, b)| (id.clone(), b.clone())).collect())
        });
    }

    fn reset(&mut self) {
        self.held.clear();
        let _ = self.tx.send(Cmd::ResetWatches);
    }

    fn send(&self, combo: &str) {
        let _ = self.tx.send(Cmd::Send(combo.to_owned()));
    }

    fn release_modifiers(&self) {}

    fn watch_release(&mut self, combo: Option<&str>) {
        let _ = self.tx.send(Cmd::WatchRelease(combo.map(str::to_owned)));
    }

    fn desktop_event(&mut self, event: &DesktopEvent) {
        match event {
            DesktopEvent::Bind { id, at } => {
                let Some(bind) = self.binds.get(id) else { return };
                match bind.target.clone() {
                    Target::Combo { combo, check_mods } => {
                        let by = combo.rsplit('+').next().unwrap_or(&combo).to_owned();
                        for hold in &self.held {
                            self.signal(Signal::Interrupt {
                                hold: hold.clone(),
                                by: by.clone(),
                                by_combo: Some(combo.clone()),
                            });
                        }
                        if check_mods.is_empty() {
                            self.signal(Signal::Pressed { combo, at: *at });
                        } else {
                            let _ = self.tx.send(Cmd::FireIfHeld { combo, mods: check_mods, at: *at });
                        }
                    }
                    Target::HoldPress(key) => {
                        if !self.held.contains(&key) {
                            self.held.push(key.clone());
                        }
                        let _ = self.tx.send(Cmd::Watch(key.clone()));
                        self.signal(Signal::HoldDown { key });
                    }
                    Target::HoldRelease(key) => {
                        self.held.retain(|k| *k != key);
                        let _ = self.tx.send(Cmd::Unwatch(key.clone()));
                        self.signal(Signal::HoldUp { key, missed: false });
                    }
                }
            }
            DesktopEvent::Reloaded if !self.suspended => {
                let all = self.binds.iter().map(|(id, b)| (id.clone(), b.clone())).collect();
                let _ = self.tx.send(Cmd::Reinstall(all));
            }
            _ => {}
        }
    }

    fn shutdown(&mut self) {
        let _ = self.tx.send(Cmd::Stop);
    }
}

struct Worker {
    hypr: Hypr,
    signals: Sender<Signal>,
    wake: Wake,
    /// Hyprland's `is_key_down()` answers "up" for keys it does not track (Alt, at least, on 0.56). Only once
    /// it has said "down" for something is an "up" believed; until then the answer is "don't know".
    trusted: bool,
    foreign: Option<(Instant, Vec<Value>)>,
    watches: HashMap<String, Instant>,
    /// A combo to report once its keys are let go, and whether it has been checked yet.
    release: Option<(String, bool)>,
}

impl Worker {
    fn run(mut self, rx: Receiver<Cmd>) {
        loop {
            let cmd = if self.watches.is_empty() && self.release.is_none() {
                rx.recv().ok()
            } else {
                // the cheat sheet's keys are checked every 50 ms, hold keys every 100 ms; only while watched
                let every = if self.release.is_some() { 50 } else { 100 };
                match rx.recv_timeout(Duration::from_millis(every)) {
                    Ok(cmd) => Some(cmd),
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                        if !self.watches.is_empty() {
                            self.check_holds();
                        }
                        self.check_release();
                        continue;
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => None,
                }
            };
            match cmd {
                None | Some(Cmd::Stop) => return,
                Some(cmd) => self.handle(cmd),
            }
        }
    }

    fn signal(&self, s: Signal) {
        let _ = self.signals.send(s);
        (self.wake)();
    }

    fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Bind { binds, clash } => {
                if let Some((combo, keys_list)) = clash
                    && let Some(reason) = keys_list.iter().find_map(|k| self.conflict(k))
                {
                    self.signal(Signal::Refused { combo, reason });
                    return;
                }
                for (i, (id, b)) in binds.iter().enumerate() {
                    if let Err(e) = self.hypr.bind(id, &b.keys, b.opts) {
                        for (done, _) in &binds[..i] {
                            let _ = self.hypr.unbind(done);
                        }
                        let what = match &b.target {
                            Target::Combo { combo, .. } => combo.clone(),
                            Target::HoldPress(k) | Target::HoldRelease(k) => k.clone(),
                        };
                        tracing::warn!("could not bind {what:?}: {e}");
                        self.signal(Signal::Refused { combo: what, reason: e });
                        return;
                    }
                }
            }
            Cmd::Unbind(ids) => {
                for id in ids {
                    let _ = self.hypr.unbind(&id);
                }
            }
            Cmd::UnbindAll => {
                let _ = self.hypr.unbind_all();
            }
            Cmd::Reinstall(binds) => {
                for (id, b) in binds {
                    let _ = self.hypr.bind(&id, &b.keys, b.opts);
                }
            }
            Cmd::FireIfHeld { combo, mods, at } => {
                if mods.iter().all(|m| self.key_state(m) != Some(false)) {
                    self.signal(Signal::Pressed { combo, at });
                }
            }
            Cmd::Watch(key) => {
                self.watches.entry(key).or_insert_with(Instant::now);
            }
            Cmd::Unwatch(key) => {
                self.watches.remove(&key);
            }
            Cmd::ResetWatches => self.watches.clear(),
            Cmd::WatchRelease(combo) => self.release = combo.map(|c| (c, false)),
            Cmd::Send(combo) => {
                let (mods, key) = keys::split(&combo);
                let hmods: Vec<&str> = mods.iter().filter_map(|m| keys::hypr_modifier(m)).collect();
                let ks = keys::keysym(&key, keys::Os::Linux);
                let _ = self.hypr.send_shortcut(&hmods.join(" "), if ks.is_empty() { &key } else { &ks });
            }
            Cmd::Stop => {}
        }
    }

    /// Reports the watched combo once its keys are up. Where Hyprland can not tell at the first check, it says so
    /// and stops; later "can not tell" answers count as still held, so nothing goes away wrongly.
    fn check_release(&mut self) {
        let Some((combo, checked)) = self.release.clone() else { return };
        match self.key_state(&combo) {
            Some(false) => {
                self.release = None;
                self.signal(Signal::Released { combo, unknown: false });
            }
            None if !checked => {
                self.release = None;
                self.signal(Signal::Released { combo, unknown: true });
            }
            _ => self.release = Some((combo, true)),
        }
    }

    /// True if every key in `combo` is held, False if one is not, None if Hyprland can not tell.
    fn key_state(&mut self, combo: &str) -> Option<bool> {
        let mut result = Some(true);
        for part in keys::normalize(combo).split('+') {
            let syms = keys::keysyms_of(part, keys::Os::Linux);
            let syms: Vec<&str> = syms.iter().map(String::as_str).collect();
            match self.hypr.key_down(&syms) {
                Some(true) => {
                    if !self.trusted {
                        tracing::info!("Hyprland reports held keys: key-state checks are on");
                    }
                    self.trusted = true;
                }
                Some(false) if self.trusted => return Some(false),
                _ => result = None,
            }
        }
        result
    }

    fn check_holds(&mut self) {
        let now = Instant::now();
        let due: Vec<String> = self
            .watches
            .iter()
            .filter(|(_, since)| now.duration_since(**since) > Duration::from_millis(150))
            .map(|(k, _)| k.clone())
            .collect();
        for key in due {
            // never cut a hold short on an untrusted "up"
            if self.key_state(&key) == Some(false) {
                self.watches.remove(&key);
                self.signal(Signal::HoldUp { key, missed: true });
            }
        }
    }

    /// The user's own Hyprland bind on `keys`, described, if there is one. Their binds are read at most every
    /// 5 s.
    fn conflict(&mut self, keys: &str) -> Option<String> {
        let fresh = self.foreign.as_ref().is_some_and(|(at, _)| at.elapsed() < Duration::from_secs(5));
        if !fresh {
            self.foreign = Some((Instant::now(), self.hypr.foreign_binds()));
        }
        let binds = &self.foreign.as_ref()?.1;
        let parts: Vec<&str> = keys.split('+').map(str::trim).collect();
        let (key, hmods) = parts.split_last()?;
        let key = key.to_lowercase();
        let mask = i64::from(keys::mask(hmods));
        binds.iter().find_map(|b| {
            let same_key = b.get("key").and_then(Value::as_str).unwrap_or("").to_lowercase() == key;
            let same_mods = b.get("modmask").and_then(Value::as_i64).unwrap_or(-1) == mask;
            let release = b.get("release").and_then(Value::as_bool).unwrap_or(false);
            (same_key && same_mods && !release).then(|| {
                let what = b.get("dispatcher").and_then(Value::as_str).unwrap_or("");
                let mut text = format!("{keys} is already bound in your Hyprland config");
                if !what.is_empty() {
                    text += &format!(" ({what})");
                }
                text
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{HotkeyEvent, HotkeyManager};
    use super::*;
    use lum_hypr::fake::FakeHyprland;
    use std::sync::Arc;

    fn manager(fake: &FakeHyprland) -> HotkeyManager {
        let (tx, rx) = crossbeam_channel::unbounded();
        let backend = HyprKeys::start(fake.hypr(), tx, Arc::new(|| {}));
        HotkeyManager::testing(Box::new(backend), rx, keys::Os::Linux)
    }

    fn poll_until(m: &mut HotkeyManager, n: usize) -> Vec<HotkeyEvent> {
        let end = Instant::now() + Duration::from_secs(3);
        let mut got = Vec::new();
        while got.len() < n && Instant::now() < end {
            got.extend(m.poll());
            std::thread::sleep(Duration::from_millis(5));
        }
        got
    }

    fn bind(id: &str) -> DesktopEvent {
        DesktopEvent::Bind { id: id.to_owned(), at: Instant::now() }
    }

    #[test]
    fn a_hotkey_becomes_a_bind_and_fires_through_the_fifo() {
        let fake = FakeHyprland::start();
        let mut m = manager(&fake);
        m.register("overlay", "ctrl+shift+space", "open", "Open", Default::default()).expect("ok");
        let sent = fake.wait_for(|c| c.contains("LUMRS.bind")).expect("bound");
        assert_eq!(
            sent,
            "eval LUMRS.bind(\"CTRL + SHIFT + space|0|1\", \"k1\", \"CTRL + SHIFT + space\", { description = \"Luminescent\", non_consuming = true })"
        );
        m.desktop_event(&bind("k1"));
        m.desktop_event(&bind("k99"));
        assert_eq!(
            poll_until(&mut m, 1),
            [HotkeyEvent::Fired {
                owner: "overlay".into(),
                id: "open".into(),
                label: "Open".into(),
                combo: "ctrl+shift+space".into()
            }]
        );
    }

    #[test]
    fn your_own_hyprland_bind_wins() {
        let fake = FakeHyprland::start();
        fake.answer(|c| {
            (c == "j/binds").then(|| {
                r#"[{"key": "space", "modmask": 64, "dispatcher": "exec", "description": ""}]"#.into()
            })
        });
        let mut m = manager(&fake);
        m.register("overlay", "super+space", "open", "", Default::default()).expect("ok for now");
        assert_eq!(
            poll_until(&mut m, 1),
            [HotkeyEvent::Refused {
                owner: "overlay".into(),
                combo: "windows+space".into(),
                reason: "SUPER + space is already bound in your Hyprland config (exec)".into()
            }]
        );
        assert!(!fake.sent().iter().any(|c| c.contains("LUMRS.bind")));
    }

    #[test]
    fn binds_come_back_after_a_reload_and_lift_while_suspended() {
        let fake = FakeHyprland::start();
        let mut m = manager(&fake);
        m.register("overlay", "ctrl+k", "open", "", Default::default()).expect("ok");
        fake.wait_for(|c| c.contains("LUMRS.bind")).expect("bound");
        fake.clear();
        m.desktop_event(&DesktopEvent::Reloaded);
        fake.wait_for(|c| c.contains("LUMRS.bind(\"CTRL + k|0|1\", \"k1\"")).expect("bound again");
        m.set_suspended(true);
        fake.wait_for(|c| c == "eval LUMRS.unbind_all()").expect("lifted");
        m.desktop_event(&bind("k1"));
        assert!(poll_until(&mut m, 1).is_empty());
    }

    #[test]
    fn a_hold_key_is_four_binds_and_a_missed_release_is_caught() {
        let fake = FakeHyprland::start();
        // Hyprland says Alt is down at first (so "up" is trusted), then up
        let down = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let d = down.clone();
        fake.answer(move |c| {
            c.starts_with("repl return").then(|| d.load(std::sync::atomic::Ordering::SeqCst).to_string())
        });
        let mut m = manager(&fake);
        m.register_hold("doodle", "alt", true).expect("ok");
        let end = Instant::now() + Duration::from_secs(3);
        while fake.sent().iter().filter(|c| c.contains("LUMRS.bind")).count() < 8 && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(5));
        }
        let binds: Vec<String> = fake.sent().into_iter().filter(|c| c.contains("LUMRS.bind")).collect();
        assert_eq!(binds.len(), 8, "four for each Alt key");
        assert!(binds[1].contains("\"ALT + Alt_L|0|1\""));
        assert!(binds[2].contains("\"ALT + Alt_L|1|1\""));
        m.desktop_event(&bind("k1"));
        assert_eq!(
            poll_until(&mut m, 1),
            [HotkeyEvent::HoldDown { owner: "doodle".into(), key: "alt".into() }]
        );
        // trusted "down" first, then the key goes up without a release bind firing
        std::thread::sleep(Duration::from_millis(300));
        down.store(false, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            poll_until(&mut m, 1),
            [HotkeyEvent::HoldUp { owner: "doodle".into(), key: "alt".into() }]
        );
    }

    #[test]
    fn send_names_its_modifiers() {
        let fake = FakeHyprland::start();
        let m = manager(&fake);
        m.send("ctrl+v");
        fake.wait_for(|c| c == format!("eval {}", lum_hypr::lua::send_shortcut("CTRL", "v"))).expect("sent");
    }
}
