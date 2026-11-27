//! The Settings page's side of the app: what happens after Save & Apply, the module buttons, backups and the
//! notification test (`SettingsPage` and `Launcher.apply_settings` in the Python app).

use std::path::PathBuf;

use lum_core::module::Notifier;
use lum_core::{APP_NAME, backup};
use lum_ui::settings::{FileAsk, FileJob, Out};
use serde_json::Value;

use super::{App, AppMsg};

/// How a backup job ended, back on the main thread.
#[derive(Debug)]
pub enum BackupDone {
    Written(Result<usize, String>, PathBuf),
    Checked(Result<usize, String>, PathBuf),
    Restored(Result<(), String>),
}

impl App {
    /// Opens the overlay on Settings, on a section when one is named (a section's key, or its title).
    pub fn open_settings(&mut self, section: &str) {
        let section = self.settings.schema().sections.iter().find(|s| s.key == section || s.title == section);
        if let Some(section) = section {
            let key = section.key.clone();
            self.settings_ui.open_section(&key);
        }
        self.open_overlay("settings");
    }

    /// What the page decided on the last frame.
    pub(super) fn settings_out(&mut self, out: Out) {
        for ask in out.files {
            self.settings_file_dialog(ask);
        }
        for (title, text) in out.notify {
            self.notifier.notify(&title, &text, 4000);
        }
        if let Some(suspended) = out.suspend_keys {
            self.hotkeys.set_suspended(suspended);
        }
        if let Some(saved) = out.saved {
            self.settings_saved(&saved.changed);
        }
        for id in out.run {
            let answer = self.run_action(&id);
            if let Some(error) = answer.strip_prefix("error: ") {
                self.notifier.notify(APP_NAME, error, 4000);
            }
        }
        for key in out.turn_on {
            self.turn_module_on(&key);
        }
        if let Some((title, text, values)) = out.toast_test {
            self.try_notification(&title, &text, &values);
        }
        if let Some(path) = out.export_to {
            self.backup_job(move |paths, _| BackupDone::Written(export(paths, &path), path));
        }
        if let Some(path) = out.check_import {
            self.backup_job(move |_, _| BackupDone::Checked(check(&path), path));
        }
        if let Some(path) = out.restore {
            self.backup_job(move |paths, now| {
                BackupDone::Restored(
                    backup::restore(paths, &path, now).map(|_| ()).map_err(|e| e.to_string()),
                )
            });
        }
        if out.quit {
            self.confirm_quit();
        }
        if out.restart {
            self.restart_app(false);
        }
    }

    fn settings_file_dialog(&mut self, ask: FileAsk) {
        let mailbox = self.mailbox.clone();
        let job = ask.job;
        let done = move |path| mailbox.send(AppMsg::SettingsFile(job, path));
        if ask.folder {
            lum_platform::dialogs::pick_folder(ask.request, done);
        } else if ask.save {
            lum_platform::dialogs::save_file(ask.request, done);
        } else {
            lum_platform::dialogs::open_file(ask.request, done);
        }
    }

    pub(super) fn settings_file_chosen(&mut self, job: &FileJob, path: Option<PathBuf>) {
        let out = self.settings_ui.file_chosen(job, path);
        self.settings_out(out);
        self.waker.wake();
    }

    /// Settings were saved from the page (or a profile was switched): what is made from them is made again,
    /// and only the modules whose section changed restart.
    pub(crate) fn settings_saved(&mut self, changed: &[String]) {
        self.settings_changed();
        self.hotkeys.unregister_all("launcher");
        self.register_hotkeys();
        self.register_dashboard_hotkeys();
        self.with_modules(|host, cx, actions| host.apply_changed(cx, actions, changed));
        self.refresh_tray();
        self.sync_autostart();
        self.restyle();
        tracing::info!(
            "settings applied ({})",
            if changed.is_empty() { "nothing changed".to_owned() } else { changed.join(", ") }
        );
        self.waker.wake();
    }

    /// Run at login follows the setting. A test copy with its own data folder never touches the real entry.
    fn sync_autostart(&self) {
        if self.paths.custom_home {
            return;
        }
        let want = self.settings.get_bool("general.run_at_startup", false);
        if let Err(e) = lum_platform::autostart::sync(want) {
            tracing::warn!("could not update the startup entry: {e}");
        }
    }

    /// "Turn it on" on a module's page: it starts now and stays on.
    fn turn_module_on(&mut self, key: &str) {
        self.settings.set(&format!("{key}.enabled"), Value::Bool(true));
        if let Err(e) = self.settings.save_quiet() {
            tracing::warn!("could not store that {key} is on: {e}");
        }
        self.with_modules(|host, cx, actions| host.apply_changed(cx, actions, &[]));
        self.refresh_tray();
        self.waker.wake();
    }

    /// Shows a card with the values on the page, saved or not. The look goes back to the saved one once the
    /// card has gone.
    fn try_notification(&mut self, title: &str, text: &str, values: &serde_json::Map<String, Value>) {
        self.toast_look = lum_ui::toast::Look::from_values(values);
        self.toast_preview = true;
        self.toasts.push(title, text, 6000, &self.toast_look);
        self.waker.wake();
    }

    pub(super) fn restore_toast_look(&mut self) {
        if self.toast_preview && self.toasts.is_empty() {
            self.toast_preview = false;
            self.toast_look = lum_ui::toast::Look::from_settings(&self.settings);
        }
    }

    /// Zips and unzips take a while, so they run off the UI thread and report back through the mailbox.
    fn backup_job(
        &mut self,
        job: impl FnOnce(&lum_core::Paths, jiff::civil::DateTime) -> BackupDone + Send + 'static,
    ) {
        let paths = self.paths.clone();
        let now = self.clock.now();
        let mailbox = self.mailbox.clone();
        let run = move || mailbox.send(AppMsg::Backup(job(&paths, now)));
        match &self.runtime {
            Some(runtime) => {
                runtime.spawn_blocking(run);
            }
            None => run(),
        }
    }

    pub(super) fn backup_done(&mut self, done: BackupDone) {
        match done {
            BackupDone::Written(result, path) => self.settings_ui.backup_written(result, &path),
            BackupDone::Checked(result, path) => self.settings_ui.backup_checked(result, path),
            BackupDone::Restored(Ok(())) => {
                tracing::info!("backup imported");
                self.restart_app(false);
            }
            BackupDone::Restored(Err(e)) => self.settings_ui.backup_failed(&e),
        }
        self.waker.wake();
    }
}

fn export(paths: &lum_core::Paths, path: &std::path::Path) -> Result<usize, String> {
    backup::export(paths, path).map_err(|e| e.to_string())
}

fn check(path: &std::path::Path) -> Result<usize, String> {
    backup::check(path).map(|names| names.len()).map_err(|e| e.to_string())
}
