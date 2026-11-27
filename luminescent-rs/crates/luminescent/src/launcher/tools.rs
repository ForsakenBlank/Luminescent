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
        let mut refused = Vec::new();
        let holds: Vec<_> = self
            .host
            .running_refs()
            .filter_map(|m| Some((m.key(), m.title(), m.hold_setting()?)))
            .map(|(key, title, setting)| {
                let combo = self.settings.get_str(&format!("{key}.{setting}")).unwrap_or_default().to_owned();
                (key, title, combo)
            })
            .collect();
        for (owner, title, combo) in holds {
            if let Err(e) = self.hotkeys.register_hold(owner, &combo, true) {
                self.notifier.notify(
                    title,
                    &format!("Hold key '{combo}' not usable ({e}); using toggle mode instead."),
                    4000,
                );
                refused.push((owner, e));
            }
        }
        for (owner, reason) in refused {
            self.host.each_running("refusing a hold key", |m| {
                if m.key() == owner {
                    m.hold_event(lum_ui::HoldEvent::Refused(reason.clone()));
                }
            });
        }
        let mut wanted = Vec::new();
        let mut fixed = Vec::new();
        for module in self.host.running_refs() {
            fixed.extend(module.fixed_binds().into_iter().map(|b| (module.key(), b)));
            for bind in module.binds() {
                let combo =
                    self.settings.get_str(&format!("{}.{}", module.key(), bind.setting)).unwrap_or_default();
                wanted.push((module.key(), module.title(), bind, combo.to_owned()));
            }
        }
        for (owner, bind) in fixed {
            let options =
                hotkeys::Options { non_consuming: bind.non_consuming, ..hotkeys::Options::default() };
            if let Err(e) = self.hotkeys.register(owner, &bind.combo, bind.id, bind.label, options) {
                tracing::warn!("{owner}: {} not registered: {e}", bind.combo);
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

    /// A module's hold key went down, came up or was interrupted.
    pub(super) fn module_hold(&mut self, owner: &str, event: lum_ui::HoldEvent) {
        let failed = self.host.each_running("handling a hold key", |m| {
            if m.key() == owner {
                m.hold_event(event.clone());
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
                Request::TodoReminder(sync) => self.todo_reminder(&sync),
                Request::ReminderEdit(edit) => self.edit_reminders(&edit),
                Request::ApplyTheme(id) => self.apply_theme(&id),
                Request::SystemNotice { title, text, ms } => {
                    lum_platform::notify::show(&title, &text, ms, None)
                }
                Request::SendKeys { combo, release_modifiers } => {
                    if release_modifiers {
                        self.hotkeys.release_modifiers();
                    }
                    self.hotkeys.send(&combo);
                }
                Request::SetSetting { key, value } => {
                    self.settings.set(&key, value);
                    if let Err(e) = self.settings.save_quiet() {
                        tracing::warn!("could not save {key}: {e}");
                    }
                }
                Request::ThemeAccent(colour) => self.accent_from_colour(&colour),
                Request::ThemeFromColour { colour, light } => self.theme_from_colour(&colour, light),
            }
        }
    }

    /// The colour picker's "Use as the theme's accent".
    fn accent_from_colour(&mut self, colour: &str) {
        let Some(rgb) = lum_theme::Rgb::parse(colour) else { return };
        let id = self.themes.active_id().to_owned();
        self.themes.set_accent(&id, rgb);
        self.theme_events();
        let name = self.themes.active().name().to_owned();
        self.notifier.notify("Colour Picker", &format!("{} is now the accent of {name}.", rgb.hex()), 2500);
    }

    /// The colour picker's "New dark (or light) theme from it": made and switched to.
    fn theme_from_colour(&mut self, colour: &str, light: bool) {
        let Some(rgb) = lum_theme::Rgb::parse(colour) else { return };
        let theme = self.themes.from_accent(rgb, light, &format!("Picked {}", rgb.hex()));
        self.apply_theme(theme.id());
        self.notifier.notify(
            "Colour Picker",
            &format!("New theme “{}” (Themes page to rename or change it).", theme.name()),
            3000,
        );
    }

    /// Another module changed the list of reminders: it is stored where Settings keeps it, and the Settings page
    /// is told so its next save does not put the old list back.
    fn edit_reminders(&mut self, edit: &lum_calc::reminder::Edit) {
        let mut list = self
            .settings
            .get("reminders.items")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        lum_calc::reminder::apply_edit(&mut list, edit, jiff::ZonedDateTime::now().date());
        self.settings.set("reminders.items", serde_json::Value::Array(list));
        if let Err(e) = self.settings.save_quiet() {
            tracing::warn!("could not save the reminders: {e}");
        }
    }

    /// A to-do item's due date changed: Reminders keeps its reminder in step. With Reminders off there is
    /// nobody to remind, which is said once, when a date that would have reminded is set.
    fn todo_reminder(&mut self, sync: &lum_core::notes::TodoSync) {
        let mut handled = false;
        self.host.each_running("setting a to-do reminder", |m| handled |= m.todo_reminder(sync));
        if !handled && matches!(sync, lum_core::notes::TodoSync::Set { .. }) {
            self.notifier.notify(
                "Notes",
                "Reminders is turned off, so this due date will not remind you.",
                4000,
            );
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
                let actions = m.tray_actions().into_iter().map(move |(id, label)| {
                    MenuItem::action(&format!("act:{}:{id}", m.key()), format!("{icon}  {label}"))
                });
                actions.chain(m.tray_menu()).collect::<Vec<_>>()
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
