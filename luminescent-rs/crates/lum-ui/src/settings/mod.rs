//! The Settings page (`settings_ui.py`): a profile bar on top, a sidebar with search and the sections, rows of
//! "label | field" with a hairline between them, and a bar at the bottom (Reset page, the unsaved count, Undo
//! changes, Save & Apply). Every row comes from the settings schema, so a field added there appears here.
//!
//! The page edits a copy of the settings and writes nothing until Save & Apply. It draws into whatever
//! rectangle it is given (the overlay's panel, or a box on the dashboard): below about 640 px the sidebar
//! turns into a search box and a list of sections above the page.

mod bars;
mod custom;
mod fields;
pub mod model;

use std::collections::HashMap;
use std::path::PathBuf;

use egui::{Id, Rect, Ui, vec2};
use lum_core::settings::{Field, Schema, Section, Settings};
use lum_platform::dialogs::Request;
use lum_platform::hotkeys::{Os, keys};
use serde_json::{Map, Value};

use crate::dialog::{AskDialog, Outcome};
use crate::picker::{ColourDialog, PickerKind};
use model::{Draft, NavState};

pub const LABEL_WIDTH: f32 = 220.0;

/// Below this width the sidebar is a list above the page.
const COMPACT_WIDTH: f32 = 640.0;

/// A module's section has its own buttons and a switch to turn it on.
#[derive(Debug, Clone, Default)]
pub struct ModuleInfo {
    pub title: String,
    pub running: bool,
    /// The actions it offers while it runs: (full id, label).
    pub actions: Vec<(String, String)>,
    /// Why it could not start, if that is what happened.
    pub error: Option<String>,
}

/// What the page needs to know about the rest of the app.
pub struct Env<'a> {
    /// What else uses a hotkey that is not on these pages (a dashboard item, a theme's own key).
    pub key_user: &'a dyn Fn(&str) -> Option<String>,
    /// The dashboard's item keys as (combo, what), for the check on Save.
    pub dashboard_keys: &'a dyn Fn() -> Vec<(String, String)>,
    /// The module behind a section, if it has one.
    pub module: &'a dyn Fn(&str) -> Option<ModuleInfo>,
    pub version: &'a str,
    /// Today as `2026-11-27`, for the name of an exported backup.
    pub today: &'a str,
}

/// What a file dialog is for; the answer comes back with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileJob {
    /// A path box (`section.key`).
    Browse(String),
    BackupExport,
    BackupImport,
}

#[derive(Debug, Clone)]
pub struct FileAsk {
    pub job: FileJob,
    pub save: bool,
    pub folder: bool,
    pub request: Request,
}

/// Save & Apply happened (or a profile was switched): the sections whose values changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub changed: Vec<String>,
}

/// What the app has to do after a frame of the page.
#[derive(Debug, Default)]
pub struct Out {
    pub files: Vec<FileAsk>,
    /// Desktop notifications: (title, text).
    pub notify: Vec<(String, String)>,
    /// Hotkeys must not fire while a key is being recorded (`Some(true)`), and may again afterwards.
    pub suspend_keys: Option<bool>,
    pub saved: Option<Saved>,
    /// Actions to run (full ids), from a module's buttons.
    pub run: Vec<String>,
    /// Modules to switch on.
    pub turn_on: Vec<String>,
    pub quit: bool,
    pub restart: bool,
    /// A notification to show with the values on the page: (title, text, the page's values).
    pub toast_test: Option<(String, String, Map<String, Value>)>,
    /// Write a backup zip here.
    pub export_to: Option<PathBuf>,
    /// Look inside this zip before asking whether to import it.
    pub check_import: Option<PathBuf>,
    /// Replace the setup with this zip and restart.
    pub restore: Option<PathBuf>,
}

#[derive(Debug, Clone)]
enum Ask {
    Message,
    Discard(Next),
    NewProfile,
    Rename(String),
    Delete(String),
    Hotkeys,
    Restart,
    Import(PathBuf),
}

/// What waits behind the "discard your changes?" question.
#[derive(Debug, Clone)]
enum Next {
    Switch(String),
    NewProfile,
}

pub struct SettingsUi {
    schema: &'static Schema,
    draft: Option<Draft>,
    query: String,
    current: usize,
    nav: NavState,
    /// `section.key` of the hotkey field that is waiting for keys.
    recording: Option<String>,
    warnings: HashMap<String, String>,
    /// Changes to the draft so far, so the clash warnings are only worked out again when something moved.
    revision: u64,
    warned_at: Option<u64>,
    dialog: Option<(Ask, AskDialog)>,
    picker: ColourDialog,
    near: Rect,
    /// What the settings were changed to elsewhere (a profile a theme picked) while the page was open.
    stale: bool,
    note: String,
    out: Out,
    /// The fields drawn since the page last started, with what each was drawn as (for the tests).
    drawn: Vec<(String, model::Control)>,
}

fn dialog_id() -> Id {
    Id::unique("settings-page-dialog")
}

fn base_id() -> Id {
    Id::unique("lum-settings")
}

impl SettingsUi {
    /// `picker_file` is `picker.json`, `eyedropper_file` the screen colour picker's `colours.json`.
    pub fn new(picker_file: PathBuf, eyedropper_file: PathBuf) -> SettingsUi {
        SettingsUi::with_schema(Schema::get(), picker_file, eyedropper_file)
    }

    /// For a schema other than this system's (the tests check both systems' dumps).
    pub fn with_schema(
        schema: &'static Schema,
        picker_file: PathBuf,
        eyedropper_file: PathBuf,
    ) -> SettingsUi {
        SettingsUi {
            schema,
            draft: None,
            query: String::new(),
            current: 0,
            nav: NavState::from_value(&[]),
            recording: None,
            warnings: HashMap::new(),
            revision: 0,
            warned_at: None,
            dialog: None,
            picker: ColourDialog::new(picker_file, eyedropper_file),
            near: Rect::NOTHING,
            stale: false,
            note: String::new(),
            out: Out::default(),
            drawn: Vec::new(),
        }
    }

    /// A dialog of the page or a key recording has the keyboard (Esc must not close the overlay).
    pub fn busy(&self) -> bool {
        self.dialog.is_some() || self.recording.is_some() || self.picker.is_open()
    }

    /// The settings changed behind the page's back (a profile a theme picked): it takes them over as soon as
    /// nothing on it is being edited.
    pub fn mark_stale(&mut self) {
        self.stale = true;
    }

    /// A page that failed while drawing starts again.
    pub fn reset_page(&mut self) {
        self.draft = None;
        self.recording = None;
        self.dialog = None;
        self.warned_at = None;
    }

    /// The overlay went away. Gives back whether keys were suspended for a recording.
    pub fn on_hide(&mut self) -> bool {
        self.dialog = None;
        self.recording.take().is_some()
    }

    pub fn open_section(&mut self, key: &str) {
        if let Some(i) = self.schema.sections.iter().position(|s| s.key == key) {
            self.query.clear();
            self.select(i);
        }
    }

    pub fn current_section(&self) -> &str {
        &self.schema.sections[self.current].key
    }

    /// Every field drawn so far and what it was drawn as.
    pub fn drawn(&self) -> &[(String, model::Control)] {
        &self.drawn
    }

    pub fn draft(&self) -> Option<&Draft> {
        self.draft.as_ref()
    }

    fn select(&mut self, index: usize) {
        self.current = index.min(self.schema.sections.len() - 1);
        self.warned_at = None;
        if let Some(draft) = &mut self.draft {
            draft.build(self.current);
        }
    }

    fn only_matching(settings: &Settings) -> bool {
        settings.get_bool("ui.settings_search_only", false)
    }

    pub fn message(&mut self, title: &str, text: &str) {
        self.dialog = Some((Ask::Message, AskDialog::error(dialog_id(), title, text)));
    }

    fn ask(&mut self, ask: Ask, dialog: AskDialog) {
        self.dialog = Some((ask, dialog));
    }

    /// A backup was written (or not): tell the person.
    pub fn backup_written(&mut self, result: Result<usize, String>, path: &std::path::Path) {
        match result {
            Ok(n) => {
                let text = format!("Saved {n} files to\n{}", path.display());
                self.dialog = Some((
                    Ask::Message,
                    AskDialog::message(dialog_id(), "Backup", &text, crate::dialog::Icon::None),
                ));
            }
            Err(e) => self.message("Backup", &format!("Could not write the backup:\n{e}")),
        }
    }

    /// The zip was looked at: ask before it replaces the setup, or say why it can not.
    pub fn backup_checked(&mut self, result: Result<usize, String>, path: PathBuf) {
        match result {
            Err(e) => self.message("Backup", &e),
            Ok(n) => {
                let text = format!(
                    "Replace your current setup with the {n} files in this backup?\n\nYour current files are zipped to data/backups first. Luminescent restarts afterwards."
                );
                let dialog = AskDialog::confirm(
                    dialog_id(),
                    "Import backup",
                    &text,
                    "Import and restart",
                    "Cancel",
                    true,
                );
                self.ask(Ask::Import(path), dialog);
            }
        }
    }

    pub fn backup_failed(&mut self, error: &str) {
        self.message("Backup", &format!("Import failed:\n{error}"));
    }

    /// A file dialog the page asked for came back.
    pub fn file_chosen(&mut self, job: &FileJob, path: Option<PathBuf>) -> Out {
        let mut out = Out::default();
        let Some(path) = path else { return out };
        match job {
            FileJob::Browse(full) => {
                if let Some((section, key)) = full.split_once('.') {
                    let text = path.display().to_string();
                    self.set_value(section, key, Value::String(text));
                }
            }
            FileJob::BackupExport => out.export_to = Some(path),
            FileJob::BackupImport => out.check_import = Some(path),
        }
        out
    }

    fn set_value(&mut self, section: &str, key: &str, value: Value) {
        if let Some(draft) = &mut self.draft {
            draft.set(section, key, value);
            self.revision += 1;
        }
    }

    fn value(&self, section: &str, key: &str) -> Option<&Value> {
        self.draft.as_ref()?.value(section, key)
    }

    /// Draws the page into all of `ui`. `near` is the window it is in (desktop coordinates), so the colour
    /// picker opens in front of it. Settings are only read and, for pins and the profile
    /// bar, written through the store; Save & Apply hands back what changed in the result.
    pub fn page_ui(&mut self, ui: &mut Ui, near: Rect, settings: &mut Settings, env: &Env<'_>) -> Out {
        let rect = ui.max_rect();
        self.near = near;
        self.out = Out::default();
        self.drawn.clear();
        self.prepare(ui.ctx(), settings);
        self.windows(ui.ctx(), settings);

        let tokens = crate::theme::Tokens::of(ui.ctx());
        let compact = rect.width() < COMPACT_WIDTH;
        let short = rect.height() < 300.0;
        let top_height = if short { 38.0 } else { 46.0 };
        let foot_height = if short { 46.0 } else { 56.0 };
        let top = Rect::from_min_max(rect.min, rect.min + vec2(rect.width(), top_height));
        let foot = Rect::from_min_max(rect.max - vec2(rect.width(), foot_height), rect.max);
        let middle = Rect::from_min_max(top.left_bottom(), foot.right_top());

        self.profile_bar(ui, top, settings, &tokens);
        if compact {
            let strip_height = (middle.height() * 0.34).clamp(78.0, 118.0).min(middle.height() * 0.6);
            let strip = Rect::from_min_size(middle.min, vec2(middle.width(), strip_height));
            let body = Rect::from_min_max(strip.left_bottom(), middle.max);
            self.sidebar_strip(ui, strip, settings, &tokens);
            self.content(ui, body, settings, env, true);
        } else {
            let side = Rect::from_min_size(middle.min, vec2(232.0, middle.height()));
            let body = Rect::from_min_max(side.right_top(), middle.max);
            self.sidebar(ui, side, settings, &tokens);
            self.content(ui, body, settings, env, false);
        }
        self.footer(ui, foot, settings, env, &tokens, compact);
        if let (Some(test), Some(draft)) = (&mut self.out.toast_test, &self.draft) {
            test.2 = draft.collect(settings.all());
        }
        std::mem::take(&mut self.out)
    }

    /// Loads the values the first time and after a change from outside, builds one more section each frame
    /// until all are built (they are built in the background, as in Python), and keeps the warnings current.
    fn prepare(&mut self, ctx: &egui::Context, settings: &Settings) {
        let clean = self.draft.as_ref().is_none_or(|d| !d.is_dirty());
        if self.draft.is_none() {
            self.draft = Some(Draft::new(self.schema, settings.all()));
            self.select(self.current);
            self.load_nav();
        } else if self.stale && clean {
            self.reload(settings);
        }
        if let Some(draft) = &mut self.draft
            && let Some(next) = draft.next_unbuilt()
        {
            draft.build(next);
            self.load_nav();
            // the rest are built a frame at a time, which stops by itself
            ctx.request_repaint();
        }
    }

    fn reload(&mut self, settings: &Settings) {
        if let Some(draft) = &mut self.draft {
            draft.reload(settings.all());
        }
        self.stale = false;
        self.revision += 1;
        self.load_nav();
    }

    /// The page switcher's editor keeps pages that are switched off in their place, which the stored list can
    /// not say, so it has a state of its own that starts from the stored list.
    fn load_nav(&mut self) {
        let order =
            self.draft.as_ref().and_then(|d| d.value("ui", "nav_order")).map(strings).unwrap_or_default();
        if self.nav.value() != order {
            self.nav = NavState::from_value(&order);
        }
    }

    fn windows(&mut self, ctx: &egui::Context, settings: &mut Settings) {
        for event in self.picker.show(ctx) {
            self.picked(&event.target, &event.kind);
        }
        let Some((ask, mut dialog)) = self.dialog.take() else { return };
        match dialog.show(ctx) {
            Outcome::Open => self.dialog = Some((ask, dialog)),
            Outcome::Cancelled => {}
            Outcome::Accepted(_) => self.answered(ask, dialog.text().trim().to_owned(), settings),
        }
    }

    fn picked(&mut self, target: &str, kind: &PickerKind) {
        let Some((section, key)) = target.strip_prefix("settings:").and_then(|full| full.split_once('.'))
        else {
            return;
        };
        if let PickerKind::Done(picked) = kind {
            self.set_value(section, key, Value::String(picked.first().hex()));
        }
    }

    fn answered(&mut self, ask: Ask, text: String, settings: &mut Settings) {
        match ask {
            Ask::Message | Ask::Hotkeys => {}
            Ask::Discard(Next::Switch(id)) => self.switch_profile(settings, &id),
            Ask::Discard(Next::NewProfile) => self.new_profile_dialog(),
            Ask::NewProfile if !text.is_empty() => self.create_profile(settings, &text),
            Ask::NewProfile => {}
            Ask::Rename(id) if !text.is_empty() => {
                if let Err(e) = settings.rename_profile(&id, &text) {
                    self.message("Profile", &format!("Could not rename the profile:\n{e}"));
                }
            }
            Ask::Rename(_) => {}
            Ask::Delete(id) => self.delete_profile(settings, &id),
            Ask::Restart => self.out.restart = true,
            Ask::Import(path) => self.out.restore = Some(path),
        }
    }

    /// What `section.key` shows in the field right now, as the text a hotkey field needs.
    fn hotkey_text(&self, section: &str, key: &str) -> String {
        self.value(section, key).and_then(Value::as_str).unwrap_or_default().to_owned()
    }

    /// The warning under each hotkey field: why the combo does not work, or what else uses it. Worked out
    /// again only after something on the page changed.
    fn refresh_warnings(&mut self, env: &Env<'_>) {
        if self.warned_at == Some(self.revision) {
            return;
        }
        self.warned_at = Some(self.revision);
        self.warnings.clear();
        let Some(draft) = &self.draft else { return };
        let os = Os::current();
        // only the section on show needs its warnings; choosing another one works them out again
        let section = &self.schema.sections[self.current];
        for field in section.fields.iter().filter(|f| f.kind == lum_core::settings::FieldType::Hotkey) {
            let combo = keys::normalize(&self.hotkey_text(&section.key, &field.key));
            if combo.is_empty() {
                continue;
            }
            let invalid = keys::validate(&combo, os);
            let own = (section.key.as_str(), field.key.as_str());
            let warning = if invalid.is_empty() {
                model::live_clash(self.schema, draft.data(), own, &combo, env.key_user)
            } else {
                invalid
            };
            if !warning.is_empty() {
                self.warnings.insert(format!("{}.{}", section.key, field.key), warning);
            }
        }
    }

    // Pins and the search option are stored with the app's own settings, not with the page's edits.

    fn pinned(&self, settings: &Settings) -> Vec<String> {
        let known: Vec<&str> = self.schema.sections.iter().map(|s| s.key.as_str()).collect();
        let pins = settings.get_str_list("ui.settings_pinned");
        pins.into_iter().filter(|k| known.contains(&k.as_str())).collect()
    }

    fn pin(&mut self, settings: &mut Settings, key: &str, on: bool) {
        let mut pins: Vec<String> = self.pinned(settings).into_iter().filter(|k| k != key).collect();
        if on {
            pins.push(key.to_owned());
        }
        settings.set("ui.settings_pinned", Value::Array(pins.into_iter().map(Value::String).collect()));
        if let Err(e) = settings.save_quiet() {
            tracing::warn!("could not store the pinned sections: {e}");
        }
    }

    fn set_only_matching(&mut self, settings: &mut Settings, on: bool) {
        settings.set("ui.settings_search_only", Value::Bool(on));
        if let Err(e) = settings.save_quiet() {
            tracing::warn!("could not store the search option: {e}");
        }
    }

    /// The first section the search leaves visible, when the one shown is hidden by it.
    fn follow_search(&mut self, settings: &Settings) {
        let q = self.query.trim().to_lowercase();
        if q.is_empty() {
            return;
        }
        let only = Self::only_matching(settings);
        let hidden = |s: &Section| model::section_hidden(s, &q, only);
        if hidden(&self.schema.sections[self.current])
            && let Some(first) = self.schema.sections.iter().position(|s| !hidden(s))
        {
            self.select(first);
        }
    }

    fn dirty_count(&self) -> usize {
        self.draft.as_ref().map_or(0, Draft::changed_count)
    }

    fn undo_changes(&mut self, settings: &Settings) {
        self.reload(settings);
        self.note = "CHANGES UNDONE".to_owned();
    }

    fn reset_section(&mut self, settings: &Settings) {
        let defaults = settings.defaults();
        let current = self.current;
        if let Some(draft) = &mut self.draft {
            draft.reset_section(current, &defaults);
        }
        self.revision += 1;
        self.load_nav();
    }

    fn save(&mut self, settings: &mut Settings, env: &Env<'_>) {
        let Some(draft) = &mut self.draft else { return };
        let data = draft.collect(settings.all());
        let problems = model::conflicts(self.schema, &data, &(env.dashboard_keys)());
        if !problems.is_empty() {
            let text = format!("Fix these before saving:\n\n• {}", problems.join("\n• "));
            self.ask(Ask::Hotkeys, AskDialog::error(dialog_id(), "Hotkey problems", &text));
            return;
        }
        let changed = draft.edited_sections();
        let restart = draft.restart_needed();
        if let Err(e) = settings.replace_silently(data) {
            self.message("Settings", &format!("Could not save the settings:\n{e}"));
            return;
        }
        draft.mark_saved();
        self.note = "SAVED AND APPLIED".to_owned();
        self.out.saved = Some(Saved { changed });
        if restart {
            let text = "Some changes only fully apply after a restart.\nRestart Luminescent now?";
            self.ask(
                Ask::Restart,
                AskDialog::confirm(dialog_id(), "Restart needed", text, "Restart", "Later", false),
            );
        }
    }

    fn turn_on(&mut self, key: &str) {
        let enabled = format!("{key}.enabled");
        if self.schema.field(&enabled).is_some()
            && let Some(draft) = &mut self.draft
        {
            draft.set_stored(key, "enabled", Value::Bool(true));
            self.revision += 1;
        }
        self.out.turn_on.push(key.to_owned());
    }
}

fn strings(value: &Value) -> Vec<String> {
    let list = value.as_array();
    list.map(|l| l.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect()).unwrap_or_default()
}

/// The key of a section's field, for ids.
fn full_key(section: &Section, field: &Field) -> String {
    format!("{}.{}", section.key, field.key)
}

/// Used by the profile bar: `~/.config/...` for a path inside the home folder.
fn short_path(path: &std::path::Path) -> String {
    let home = lum_core::paths::user_home();
    match path.strip_prefix(&home) {
        Ok(rest) => format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display()),
        Err(_) => path.display().to_string(),
    }
}
