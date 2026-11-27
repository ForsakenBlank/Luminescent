//! What the app does for the everyday tools: hotkeys they own, the requests they make, their overlay pages,
//! their rows in the palette and their lines in the tray's tooltip.

use std::time::{Duration, Instant};

use lum_core::dashboard::MODULE_ICONS;
use lum_core::module::Notifier;
use lum_platform::hotkeys;
use lum_platform::tray::MenuItem;
use lum_ui::Request;

use super::App;

/// How often the tray's tooltip is made again while a timer runs.
const TOOLTIP_EVERY: Duration = Duration::from_secs(15);

impl App {
    /// Registers the hotkeys of every module that runs, as its `binds` say.
    pub(super) fn register_module_hotkeys(&mut self) {
        for key in self.host.keys() {
            self.hotkeys.unregister_all(key);
        }
        let mut wanted = Vec::new();
        for module in self.host.running_refs() {
            for bind in module.binds() {
                let combo =
                    self.settings.get_str(&format!("{}.{}", module.key(), bind.setting)).unwrap_or_default();
                wanted.push((module.key(), module.title(), bind, combo.to_owned()));
            }
        }
        for (owner, title, bind, combo) in wanted {
            let result =
                self.hotkeys.register(owner, &combo, bind.id, bind.label, hotkeys::Options::default());
            if let Err(e) = result
                && !combo.is_empty()
            {
                self.notifier.notify(title, &format!("{} hotkey not registered: {e}", bind.label), 4000);
            }
        }
    }

    /// A key a module owns was pressed.
    pub(super) fn module_hotkey(&mut self, owner: &str, id: &str) {
        let failed = self.host.each_running("handling a hotkey", |m| {
            if m.key() == owner {
                m.bind_pressed(id);
            }
        });
        for key in failed {
            self.module_failed(key);
        }
    }

    pub(super) fn module_failed(&mut self, key: &'static str) {
        if let Some(text) = self.with_modules(|host, cx, actions| host.report_fault(key, cx, actions)) {
            self.notifier.toast("Luminescent error", &text, 6000);
        }
    }

    /// Answers what the modules asked for since the last pass.
    pub(super) fn handle_requests(&mut self) {
        for request in self.services.take() {
            match request {
                Request::HideOverlay => self.hide_overlay(),
                Request::OpenSettings(section) => self.open_settings(&section),
                Request::Notify { title, text, ms } => self.notifier.notify(&title, &text, ms),
                Request::Toast { title, text, ms } => self.notifier.toast(&title, &text, ms),
                Request::Emit { name, text } => self.emit(&name, &text),
                Request::Usage { kind, key, label } => self.usage.record(&kind, &key, &label),
                Request::Activity { kind, label, secs } => self.usage.activity(&kind, &label, secs, None),
                Request::StatusChanged => self.refresh_tray(),
            }
        }
    }

    /// Lines for the tray's tooltip and the overlay's chips: running timers.
    pub(crate) fn status_lines(&self) -> Vec<String> {
        if !self.settings.get_bool("ui.tray_timers", true) {
            return Vec::new();
        }
        self.host.running_refs().flat_map(|m| m.status_lines()).collect()
    }

    /// Makes the tooltip again every so often while there is something counting in it.
    pub(super) fn tooltip_deadline(&mut self) -> Option<Instant> {
        if self.status_lines().is_empty() {
            self.tooltip_at = None;
            return None;
        }
        let due = *self.tooltip_at.get_or_insert_with(|| Instant::now() + TOOLTIP_EVERY);
        if Instant::now() >= due {
            self.tooltip_at = Some(Instant::now() + TOOLTIP_EVERY);
            self.refresh_tray();
        }
        self.tooltip_at
    }

    /// The modules' own entries in the tray menu (`Settings → General → Tray menu → actions`).
    pub(super) fn module_tray_items(&self) -> Vec<MenuItem> {
        self.host
            .running_refs()
            .flat_map(|m| {
                let icon = MODULE_ICONS.iter().find(|(k, _)| *k == m.key()).map_or("", |(_, i)| *i);
                m.tray_actions().into_iter().map(move |(id, label)| {
                    MenuItem::action(&format!("act:{}:{id}", m.key()), format!("{icon}  {label}"))
                })
            })
            .collect()
    }

    /// The rows the modules add to the palette whatever is typed.
    pub(super) fn module_palette_entries(&self) -> Vec<lum_ui::palette::Entry> {
        self.host.running_refs().flat_map(|m| m.palette_entries()).collect()
    }

    /// A row a module made: the module that owns it runs it.
    pub(super) fn run_module_row(&mut self, id: &str) {
        let services = self.services.clone();
        let failed = self.host.each_running("running a palette row", |m| {
            m.run_palette(id, &services);
        });
        for key in failed {
            self.module_failed(key);
        }
    }

    /// A page that failed while drawing: the module's own state for it is dropped.
    pub(super) fn reset_module_page(&mut self, page: &str) {
        self.host.each_running("resetting a page", |m| {
            if m.page_key() == Some(page) {
                m.reset_page();
            }
        });
    }
}
