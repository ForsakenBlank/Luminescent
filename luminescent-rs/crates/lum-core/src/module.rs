//! Modules: every tool is one, as in `core/module.py`, and [`ModuleHost`] starts, stops and isolates them the
//! way the Python launcher does.
//!
//! This crate has no UI, so the trait here is the part every module has. Drawing (the `show` step in
//! ARCHITECTURE.md) is added by lum-ui on top of it in phase 2.

use crate::actions::{Action, ActionRegistry};
use crate::events::{Event, EventBus};
use crate::guard::{Faults, safely};
use crate::settings::Settings;

/// Messages for the person at the screen. Phase 3 shows them as notification cards; until then they are
/// logged.
pub trait Notifier {
    /// A small notification (respects Settings → General → Show notifications).
    fn notify(&self, title: &str, message: &str, ms: u32);
    /// One that always shows (reminders, warnings).
    fn toast(&self, title: &str, message: &str, ms: u32);
}

/// Writes notifications to the log; for headless runs and tests.
#[derive(Debug, Default)]
pub struct LogNotifier;

impl Notifier for LogNotifier {
    fn notify(&self, title: &str, message: &str, _ms: u32) {
        tracing::info!("notify: {title}: {message}");
    }

    fn toast(&self, title: &str, message: &str, _ms: u32) {
        tracing::info!("toast: {title}: {message}");
    }
}

/// What the app hands every module while it starts, stops or handles something.
pub struct ModuleCx<'a> {
    pub settings: &'a mut Settings,
    pub events: &'a EventBus,
    pub notifier: &'a dyn Notifier,
    pub safe_mode: bool,
}

impl ModuleCx<'_> {
    /// Settings → General → Show on start has this module's panel ticked.
    pub fn shows_on_start(&self, key: &str) -> bool {
        self.settings.get_str_list("general.start_panels").iter().any(|k| k == key)
    }
}

pub type StartError = Box<dyn std::error::Error + Send + Sync>;

pub trait Module {
    /// The settings section and hotkey owner name.
    fn key(&self) -> &'static str;
    fn title(&self) -> &'static str;

    /// Make windows, register hotkeys.
    fn start(&mut self, cx: &mut ModuleCx<'_>) -> Result<(), StartError>;

    /// Hide or drop windows. The host unregisters the module's actions itself.
    fn stop(&mut self, cx: &mut ModuleCx<'_>);

    /// Settings were saved. By default a full restart of the module.
    fn apply_settings(&mut self, cx: &mut ModuleCx<'_>) -> Result<(), StartError> {
        self.stop(cx);
        self.start(cx)
    }

    /// For the command palette, the tray, the dashboard's Tools tab and automations, while it runs.
    fn actions(&self) -> Vec<Action> {
        Vec::new()
    }

    fn on_event(&mut self, _cx: &mut ModuleCx<'_>, _event: &Event) {}
}

struct Slot<M: ?Sized> {
    module: Box<M>,
    running: bool,
    /// Whether settings wanted it on at the last apply, so only a change starts or stops it.
    wanted: bool,
    error: Option<String>,
}

/// Runs a list of modules. `M` is the kind of module it holds: plain [`Module`]s here, modules that also draw
/// (lum-ui's `UiModule`) in the app, so the host can hand them back for drawing.
pub struct ModuleHost<M: ?Sized + Module = dyn Module> {
    slots: Vec<Slot<M>>,
    faults: Faults,
}

impl<M: ?Sized + Module> Default for ModuleHost<M> {
    fn default() -> Self {
        ModuleHost { slots: Vec::new(), faults: Faults::default() }
    }
}

impl<M: ?Sized + Module> ModuleHost<M> {
    pub fn new(modules: Vec<Box<M>>) -> ModuleHost<M> {
        let slots = modules
            .into_iter()
            .map(|module| Slot { module, running: false, wanted: false, error: None })
            .collect();
        ModuleHost { slots, faults: Faults::default() }
    }

    pub fn keys(&self) -> Vec<&'static str> {
        self.slots.iter().map(|s| s.module.key()).collect()
    }

    pub fn running(&self) -> Vec<&'static str> {
        self.slots.iter().filter(|s| s.running).map(|s| s.module.key()).collect()
    }

    pub fn title(&self, key: &str) -> Option<&'static str> {
        self.slot(key).map(|s| s.module.title())
    }

    /// The modules that run now, to read from.
    pub fn running_refs(&self) -> impl Iterator<Item = &M> {
        self.slots.iter().filter(|s| s.running).map(|s| &*s.module)
    }

    /// The module behind `key`, while it runs (for drawing its page).
    pub fn running_module(&mut self, key: &str) -> Option<&mut M> {
        self.slots.iter_mut().find(|s| s.running && s.module.key() == key).map(|s| &mut *s.module)
    }

    pub fn is_running(&self, key: &str) -> bool {
        self.slot(key).is_some_and(|s| s.running)
    }

    /// Modules that failed to start or were switched off after repeated errors, with why.
    pub fn errors(&self) -> Vec<(&'static str, String)> {
        self.slots.iter().filter_map(|s| Some((s.module.key(), s.error.clone()?))).collect()
    }

    fn slot(&self, key: &str) -> Option<&Slot<M>> {
        self.slots.iter().find(|s| s.module.key() == key)
    }

    fn index(&self, key: &str) -> Option<usize> {
        self.slots.iter().position(|s| s.module.key() == key)
    }

    fn enabled(settings: &Settings, key: &str) -> bool {
        settings.get_bool(&format!("{key}.enabled"), true)
    }

    /// Starts every module whose `enabled` setting is on, unless this is safe mode.
    pub fn start_enabled(&mut self, cx: &mut ModuleCx<'_>, actions: &mut ActionRegistry) {
        for i in 0..self.slots.len() {
            let key = self.slots[i].module.key();
            let wanted = Self::enabled(cx.settings, key);
            self.slots[i].wanted = wanted;
            if wanted && !cx.safe_mode {
                self.start_at(i, cx, actions);
            }
        }
    }

    pub fn start(&mut self, key: &str, cx: &mut ModuleCx<'_>, actions: &mut ActionRegistry) -> bool {
        self.index(key).is_some_and(|i| self.start_at(i, cx, actions))
    }

    fn start_at(&mut self, i: usize, cx: &mut ModuleCx<'_>, actions: &mut ActionRegistry) -> bool {
        let slot = &mut self.slots[i];
        if slot.running {
            return true;
        }
        let key = slot.module.key();
        let outcome = safely(&format!("starting {key}"), || slot.module.start(cx));
        match outcome {
            Some(Ok(())) => {
                slot.running = true;
                slot.error = None;
                actions.set_owner(key, slot.module.actions());
                tracing::info!("started {key}");
                true
            }
            Some(Err(e)) => {
                tracing::error!("module {key} failed to start: {e}");
                cx.notifier.notify(slot.module.title(), &format!("Failed to start: {e}"), 4000);
                slot.error = Some(e.to_string());
                false
            }
            None => {
                cx.notifier.notify(slot.module.title(), "Failed to start (see the log).", 4000);
                slot.error = Some("it crashed while starting".to_owned());
                false
            }
        }
    }

    pub fn stop(&mut self, key: &str, cx: &mut ModuleCx<'_>, actions: &mut ActionRegistry) {
        if let Some(i) = self.index(key) {
            self.stop_at(i, cx, actions);
        }
    }

    fn stop_at(&mut self, i: usize, cx: &mut ModuleCx<'_>, actions: &mut ActionRegistry) {
        let slot = &mut self.slots[i];
        if !slot.running {
            return;
        }
        slot.running = false;
        let key = slot.module.key();
        actions.remove_owner(key);
        safely(&format!("stopping {key}"), || slot.module.stop(cx));
        tracing::info!("stopped {key}");
    }

    pub fn stop_all(&mut self, cx: &mut ModuleCx<'_>, actions: &mut ActionRegistry) {
        for i in 0..self.slots.len() {
            self.stop_at(i, cx, actions);
        }
    }

    /// Settings were saved: start what was switched on, stop what was switched off, re-apply the rest.
    pub fn apply_settings(&mut self, cx: &mut ModuleCx<'_>, actions: &mut ActionRegistry) {
        self.apply_where(cx, actions, |_| true);
    }

    /// Like [`ModuleHost::apply_settings`], but a running module is only re-applied (by default restarted) when
    /// its own section is among the `changed` ones. Switching on and off follows `enabled` as always.
    pub fn apply_changed(&mut self, cx: &mut ModuleCx<'_>, actions: &mut ActionRegistry, changed: &[String]) {
        self.apply_where(cx, actions, |key| changed.iter().any(|c| c == key));
    }

    fn apply_where(
        &mut self,
        cx: &mut ModuleCx<'_>,
        actions: &mut ActionRegistry,
        reapply: impl Fn(&str) -> bool,
    ) {
        for i in 0..self.slots.len() {
            let key = self.slots[i].module.key();
            let want = Self::enabled(cx.settings, key);
            let had = self.slots[i].wanted;
            if want && !had {
                self.start_at(i, cx, actions);
            } else if had && !want {
                self.stop_at(i, cx, actions);
            } else if want && self.slots[i].running && reapply(key) {
                self.reapply_at(i, cx, actions);
            }
            self.slots[i].wanted = want;
        }
    }

    fn reapply_at(&mut self, i: usize, cx: &mut ModuleCx<'_>, actions: &mut ActionRegistry) {
        let slot = &mut self.slots[i];
        let key = slot.module.key();
        match safely(&format!("apply_settings for {key}"), || slot.module.apply_settings(cx)) {
            Some(Ok(())) => actions.set_owner(key, slot.module.actions()),
            Some(Err(e)) => {
                tracing::error!("apply_settings failed for {key}: {e}");
                cx.notifier.notify(slot.module.title(), &format!("Could not apply settings: {e}"), 4000);
            }
            None => {
                cx.notifier.notify(slot.module.title(), "Could not apply settings (see the log).", 4000);
            }
        }
    }

    /// Calls `step` on every running module, each inside its own panic guard. Gives back the keys of the ones
    /// that panicked; the caller counts those with [`ModuleHost::report_fault`].
    pub fn each_running(&mut self, what: &str, mut step: impl FnMut(&mut M)) -> Vec<&'static str> {
        let mut failed = Vec::new();
        for slot in self.slots.iter_mut().filter(|s| s.running) {
            let key = slot.module.key();
            if safely(&format!("{key} {what}"), || step(&mut slot.module)).is_none() {
                failed.push(key);
            }
        }
        failed
    }

    /// Hands an event to every running module; one that panics counts a fault.
    pub fn dispatch(&mut self, event: &Event, cx: &mut ModuleCx<'_>, actions: &mut ActionRegistry) {
        for i in 0..self.slots.len() {
            if !self.slots[i].running {
                continue;
            }
            let slot = &mut self.slots[i];
            let key = slot.module.key();
            if safely(&format!("{key} handling {}", event.name), || slot.module.on_event(cx, event)).is_none()
            {
                self.report_fault(key, cx, actions);
            }
        }
    }

    /// Counts one failure of a module. After too many in a minute it is stopped for this session, and the
    /// text for the error toast says so.
    pub fn report_fault(
        &mut self,
        key: &str,
        cx: &mut ModuleCx<'_>,
        actions: &mut ActionRegistry,
    ) -> Option<String> {
        if !self.faults.add(&format!("module:{key}")) {
            return None;
        }
        let i = self.index(key)?;
        if !self.slots[i].running {
            return None;
        }
        tracing::error!("{key} kept failing: stopping it for this session");
        self.stop_at(i, cx, actions);
        let slot = &mut self.slots[i];
        slot.wanted = false;
        slot.error = Some("stopped after repeated errors".to_owned());
        let title = slot.module.title();
        Some(format!(
            "{title} kept failing, so it was switched off until the next start. Tray → Modules turns it back on."
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{DefaultPlaces, Schema, SettingsFiles};
    use serde_json::json;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct Fake {
        key: &'static str,
        fail_start: bool,
        panic_on_event: bool,
        log: Rc<RefCell<Vec<String>>>,
    }

    impl Module for Fake {
        fn key(&self) -> &'static str {
            self.key
        }
        fn title(&self) -> &'static str {
            "Fake"
        }
        fn start(&mut self, _cx: &mut ModuleCx<'_>) -> Result<(), StartError> {
            self.log.borrow_mut().push(format!("start {}", self.key));
            if self.fail_start { Err("no screen".into()) } else { Ok(()) }
        }
        fn stop(&mut self, _cx: &mut ModuleCx<'_>) {
            self.log.borrow_mut().push(format!("stop {}", self.key));
        }
        fn actions(&self) -> Vec<Action> {
            vec![Action::from_label("Do it", || {})]
        }
        fn on_event(&mut self, _cx: &mut ModuleCx<'_>, _event: &Event) {
            if self.panic_on_event {
                panic!("broken handler");
            }
        }
    }

    fn settings(dir: &std::path::Path) -> Settings {
        let files = SettingsFiles {
            settings: dir.join("settings.json"),
            profiles: dir.join("profiles.json"),
            profiles_dir: dir.join("profiles"),
        };
        Settings::open(files, Schema::get(), DefaultPlaces::placeholders())
    }

    fn fake(key: &'static str, log: &Rc<RefCell<Vec<String>>>) -> Box<dyn Module> {
        Box::new(Fake { key, fail_start: false, panic_on_event: false, log: log.clone() })
    }

    #[test]
    fn starts_enabled_modules_and_follows_settings() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = settings(dir.path());
        settings.set("doodle.enabled", json!(false));
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut host = ModuleHost::new(vec![fake("notes", &log), fake("doodle", &log)]);
        let events = EventBus::new();
        let mut actions = ActionRegistry::new();
        let mut cx =
            ModuleCx { settings: &mut settings, events: &events, notifier: &LogNotifier, safe_mode: false };
        host.start_enabled(&mut cx, &mut actions);
        assert_eq!(host.running(), vec!["notes"]);
        assert!(actions.find("notes:do_it").is_ok());

        cx.settings.set("doodle.enabled", json!(true));
        cx.settings.set("notes.enabled", json!(false));
        host.apply_settings(&mut cx, &mut actions);
        assert_eq!(host.running(), vec!["doodle"]);
        assert!(actions.find("notes:do_it").is_err());
        assert_eq!(*log.borrow(), ["start notes", "stop notes", "start doodle"]);
    }

    #[test]
    fn only_modules_whose_section_changed_are_restarted() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = settings(dir.path());
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut host = ModuleHost::new(vec![fake("notes", &log), fake("doodle", &log)]);
        let events = EventBus::new();
        let mut actions = ActionRegistry::new();
        let mut cx =
            ModuleCx { settings: &mut settings, events: &events, notifier: &LogNotifier, safe_mode: false };
        host.start_enabled(&mut cx, &mut actions);
        log.borrow_mut().clear();

        host.apply_changed(&mut cx, &mut actions, &["doodle".to_owned()]);
        assert_eq!(*log.borrow(), ["stop doodle", "start doodle"]);

        log.borrow_mut().clear();
        host.apply_changed(&mut cx, &mut actions, &["general".to_owned()]);
        assert!(log.borrow().is_empty());
    }

    #[test]
    fn a_failing_start_is_recorded_and_safe_mode_starts_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = settings(dir.path());
        let log = Rc::new(RefCell::new(Vec::new()));
        let broken =
            Box::new(Fake { key: "vault", fail_start: true, panic_on_event: false, log: log.clone() });
        let mut host: ModuleHost = ModuleHost::new(vec![broken, fake("notes", &log)]);
        let events = EventBus::new();
        let mut actions = ActionRegistry::new();
        let mut cx =
            ModuleCx { settings: &mut settings, events: &events, notifier: &LogNotifier, safe_mode: false };
        host.start_enabled(&mut cx, &mut actions);
        assert_eq!(host.running(), vec!["notes"]);
        assert_eq!(host.errors(), vec![("vault", "no screen".to_owned())]);

        let mut safe = ModuleHost::new(vec![fake("notes", &log)]);
        cx.safe_mode = true;
        safe.start_enabled(&mut cx, &mut actions);
        assert!(safe.running().is_empty());
    }

    #[test]
    fn a_module_that_keeps_panicking_is_switched_off() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = settings(dir.path());
        let log = Rc::new(RefCell::new(Vec::new()));
        let flaky =
            Box::new(Fake { key: "media", fail_start: false, panic_on_event: true, log: log.clone() });
        let mut host: ModuleHost = ModuleHost::new(vec![flaky]);
        let events = EventBus::new();
        let mut actions = ActionRegistry::new();
        let mut cx =
            ModuleCx { settings: &mut settings, events: &events, notifier: &LogNotifier, safe_mode: false };
        host.start_enabled(&mut cx, &mut actions);
        let event = Event { name: "theme.changed".into(), text: String::new() };
        for _ in 0..6 {
            host.dispatch(&event, &mut cx, &mut actions);
        }
        assert!(!host.is_running("media"));
        assert_eq!(host.errors(), vec![("media", "stopped after repeated errors".to_owned())]);
        // as in Python, the next settings save (Tray → Modules is one) gives it another go
        host.apply_settings(&mut cx, &mut actions);
        assert!(host.is_running("media"));
    }
}
