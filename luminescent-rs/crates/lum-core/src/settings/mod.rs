//! The settings store: `core/settings.py`'s `Settings`, with one change on purpose.
//!
//! settings.json always holds the active profile. The other profiles live in `data/profiles/<id>.json` and are
//! swapped in by [`Settings::switch_profile`]; the sections in `shared_sections` are the same in every profile.
//!
//! The change: Python drops sections and keys it does not know when it loads, this store keeps them. That way a
//! setting only a newer version knows survives a run of an older one, and settings Rust adds survive in Rust.
//! (Python still drops them when it saves; see DATA_AND_MIGRATION.md, rule 7.)

mod migrate;
pub mod schema;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crossbeam_channel::{Receiver, Sender};
use serde_json::{Map, Value};

use crate::paths::Paths;
use crate::pyjson::Style;
use crate::store::{self, StoreError};
pub use schema::{DefaultPlaces, Field, FieldType, Schema, Section};

pub(crate) use migrate::truthy;

const FILE_STYLE: Style = Style::indented(2);

/// What listeners hear about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsChange {
    /// Saved from the Settings page (or a toggle): everything re-applies.
    Saved,
    /// A module stored something itself (a reminder...): no re-apply, but the Settings page reloads it so its
    /// next save does not put the old value back.
    QuietlySaved,
    ProfileChanged(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub id: String,
    pub name: String,
}

/// Where the store reads and writes.
#[derive(Debug, Clone)]
pub struct SettingsFiles {
    pub settings: PathBuf,
    pub profiles: PathBuf,
    pub profiles_dir: PathBuf,
}

impl SettingsFiles {
    pub fn of(paths: &Paths) -> SettingsFiles {
        SettingsFiles {
            settings: paths.settings_file(),
            profiles: paths.profiles_file(),
            profiles_dir: paths.profiles_dir(),
        }
    }
}

pub struct Settings {
    files: SettingsFiles,
    schema: &'static Schema,
    places: DefaultPlaces,
    data: Map<String, Value>,
    raw_saved: Map<String, Value>,
    /// settings.json was there but could not be read. It is left alone until the first save, which keeps it
    /// as settings.json.bad instead of writing over it.
    damaged: bool,
    listeners: Vec<Sender<SettingsChange>>,
    new_id: Box<dyn FnMut() -> String + Send>,
}

impl Settings {
    /// Loads settings.json for this system, writing the defaults straight away on a first start.
    pub fn load(paths: &Paths) -> Settings {
        Settings::open(SettingsFiles::of(paths), Schema::get(), DefaultPlaces::for_this_system())
    }

    pub fn open(files: SettingsFiles, schema: &'static Schema, places: DefaultPlaces) -> Settings {
        let mut settings = Settings {
            files,
            schema,
            places,
            data: Map::new(),
            raw_saved: Map::new(),
            damaged: false,
            listeners: Vec::new(),
            new_id: Box::new(|| uuid::Uuid::new_v4().simple().to_string()[..10].to_owned()),
        };
        settings.reload();
        settings
    }

    /// Where new profile ids come from (tests pin them).
    pub fn set_id_source(&mut self, source: impl FnMut() -> String + Send + 'static) {
        self.new_id = Box::new(source);
    }

    pub fn schema(&self) -> &'static Schema {
        self.schema
    }

    pub fn defaults(&self) -> Map<String, Value> {
        self.schema.defaults(&self.places)
    }

    /// Reads settings.json again.
    pub fn reload(&mut self) {
        let defaults = self.defaults();
        self.damaged = false;
        self.raw_saved = Map::new();
        match fs::read_to_string(&self.files.settings) {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(Value::Object(saved)) => {
                    let mut data = defaults;
                    merge(&mut data, &saved, |_| true);
                    migrate::migrate(&mut data, &saved);
                    self.data = data;
                    self.raw_saved = saved;
                }
                Ok(_) | Err(_) => {
                    tracing::error!(
                        "{} could not be read; using the defaults",
                        self.files.settings.display()
                    );
                    self.data = defaults;
                    self.damaged = true;
                }
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                self.data = defaults;
                if let Err(e) = self.write() {
                    tracing::error!("{e}");
                }
            }
            Err(e) => {
                tracing::error!("could not read {}: {e}", self.files.settings.display());
                self.data = defaults;
                self.damaged = true;
            }
        }
    }

    /// The file as it was read at start-up, including sections nothing uses any more.
    pub fn raw_saved(&self) -> &Map<String, Value> {
        &self.raw_saved
    }

    pub fn subscribe(&mut self) -> Receiver<SettingsChange> {
        let (tx, rx) = crossbeam_channel::unbounded();
        self.listeners.push(tx);
        rx
    }

    fn notify(&mut self, change: SettingsChange) {
        self.listeners.retain(|tx| tx.send(change.clone()).is_ok());
    }

    /// Saves and tells listeners to re-apply (unless `emit` is false).
    pub fn save(&mut self, emit: bool) -> Result<(), StoreError> {
        self.write()?;
        if emit {
            self.notify(SettingsChange::Saved);
        }
        Ok(())
    }

    /// Stores a value a module changed itself, without a re-apply round.
    pub fn save_quiet(&mut self) -> Result<(), StoreError> {
        self.write()?;
        self.notify(SettingsChange::QuietlySaved);
        Ok(())
    }

    fn write(&mut self) -> Result<(), StoreError> {
        if self.damaged {
            store::keep_as_bad(&self.files.settings);
            self.damaged = false;
        }
        store::save_json(&self.files.settings, &Value::Object(self.data.clone()), FILE_STYLE)
    }

    pub fn get(&self, dotted: &str) -> Option<&Value> {
        let (section, key) = dotted.split_once('.')?;
        self.data.get(section)?.as_object()?.get(key)
    }

    /// The value's truth, Python style (`bool(settings.get(...))`).
    pub fn get_bool(&self, dotted: &str, default: bool) -> bool {
        self.get(dotted).map_or(default, truthy)
    }

    pub fn get_str(&self, dotted: &str) -> Option<&str> {
        self.get(dotted).and_then(Value::as_str)
    }

    pub fn get_f64(&self, dotted: &str) -> Option<f64> {
        self.get(dotted).and_then(Value::as_f64)
    }

    pub fn get_i64(&self, dotted: &str) -> Option<i64> {
        self.get(dotted).and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))
    }

    /// The strings in a list setting (`general.start_panels`...), skipping anything else.
    pub fn get_str_list(&self, dotted: &str) -> Vec<String> {
        let items = self.get(dotted).and_then(Value::as_array);
        items.map(|l| l.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect()).unwrap_or_default()
    }

    pub fn set(&mut self, dotted: &str, value: Value) {
        let (section, key) = dotted.split_once('.').unwrap_or((dotted, ""));
        let entry = self.data.entry(section.to_owned()).or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        if let Value::Object(map) = entry {
            map.insert(key.to_owned(), value);
        }
    }

    pub fn section(&self, key: &str) -> Option<&Map<String, Value>> {
        self.data.get(key)?.as_object()
    }

    pub fn all(&self) -> &Map<String, Value> {
        &self.data
    }

    /// Replaces everything (the Settings page on save) and tells listeners.
    pub fn replace(&mut self, data: Map<String, Value>) -> Result<(), StoreError> {
        self.data = data;
        self.save(true)
    }

    /// Like [`Settings::replace`] for a caller that applies the change itself and knows which sections it
    /// touched, so listeners are not told.
    pub fn replace_silently(&mut self, data: Map<String, Value>) -> Result<(), StoreError> {
        self.data = data;
        self.save(false)
    }

    fn profiles_meta(&self) -> Map<String, Value> {
        let default_id = self.schema.default_profile.as_str();
        let default_entry = || Value::Object(profile_entry(default_id, "Default"));
        let loaded = store::load_json(&self.files.profiles);
        let mut meta = match loaded {
            Some(Value::Object(meta)) if meta.get("profiles").is_some_and(Value::is_array) => meta,
            _ => {
                let mut meta = Map::new();
                meta.insert("active".into(), default_id.into());
                meta.insert("profiles".into(), Value::Array(vec![default_entry()]));
                meta
            }
        };
        if !has_profile(&meta, default_id) {
            profile_list(&mut meta).insert(0, default_entry());
        }
        let active = meta.get("active").and_then(Value::as_str);
        if !active.is_some_and(|a| has_profile(&meta, a)) {
            meta.insert("active".into(), default_id.into());
        }
        meta
    }

    pub fn profiles(&self) -> Vec<Profile> {
        let mut meta = self.profiles_meta();
        profile_list(&mut meta)
            .iter()
            .filter_map(|p| {
                let id = id_of(p)?.to_owned();
                let name = p.get("name").and_then(Value::as_str).unwrap_or_default().to_owned();
                Some(Profile { id, name })
            })
            .collect()
    }

    pub fn active_profile(&self) -> String {
        let meta = self.profiles_meta();
        meta.get("active").and_then(Value::as_str).unwrap_or(&self.schema.default_profile).to_owned()
    }

    pub fn profile_name(&self, id: &str) -> String {
        match self.profiles().into_iter().find(|p| p.id == id) {
            Some(p) if !p.name.is_empty() => p.name,
            Some(p) => p.id,
            None => "Default".to_owned(),
        }
    }

    fn profile_path(&self, id: &str) -> PathBuf {
        self.files.profiles_dir.join(format!("{id}.json"))
    }

    fn own_sections(&self) -> Map<String, Value> {
        let own = self.data.iter().filter(|(k, _)| !self.schema.is_shared(k));
        own.map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    fn save_meta(&self, meta: Map<String, Value>) -> Result<(), StoreError> {
        store::save_json(&self.files.profiles, &Value::Object(meta), FILE_STYLE)
    }

    fn make_profiles_dir(&self) -> Result<(), StoreError> {
        let dir = &self.files.profiles_dir;
        fs::create_dir_all(dir).map_err(|source| StoreError::Write { path: dir.clone(), source })
    }

    /// A new profile starting as a copy of the current settings. Returns its id.
    pub fn create_profile(&mut self, name: &str) -> Result<String, StoreError> {
        let mut meta = self.profiles_meta();
        let id = (self.new_id)();
        self.make_profiles_dir()?;
        store::save_json(&self.profile_path(&id), &Value::Object(self.own_sections()), FILE_STYLE)?;
        let name = match name.trim() {
            "" => "Profile",
            trimmed => trimmed,
        };
        profile_list(&mut meta).push(Value::Object(profile_entry(&id, name)));
        self.save_meta(meta)?;
        Ok(id)
    }

    pub fn rename_profile(&mut self, id: &str, name: &str) -> Result<(), StoreError> {
        let mut meta = self.profiles_meta();
        let name = name.trim();
        for p in profile_list(&mut meta) {
            if id_of(p) == Some(id)
                && !name.is_empty()
                && let Value::Object(entry) = p
            {
                entry.insert("name".into(), name.into());
            }
        }
        self.save_meta(meta)
    }

    pub fn delete_profile(&mut self, id: &str) -> Result<(), StoreError> {
        let default_id = self.schema.default_profile.clone();
        if id == default_id {
            return Ok(());
        }
        if self.active_profile() == id {
            self.switch_profile(&default_id, true)?;
        }
        let mut meta = self.profiles_meta();
        profile_list(&mut meta).retain(|p| id_of(p) != Some(id));
        self.save_meta(meta)?;
        let _ = fs::remove_file(self.profile_path(id));
        Ok(())
    }

    /// Stores the current settings in the active profile and loads `id`. Returns true if anything changed.
    pub fn switch_profile(&mut self, id: &str, emit: bool) -> Result<bool, StoreError> {
        let mut meta = self.profiles_meta();
        let id = if id.is_empty() { self.schema.default_profile.clone() } else { id.to_owned() };
        let active = meta.get("active").and_then(Value::as_str).unwrap_or_default().to_owned();
        if id == active || !has_profile(&meta, &id) {
            return Ok(false);
        }
        self.make_profiles_dir()?;
        store::save_json(&self.profile_path(&active), &Value::Object(self.own_sections()), FILE_STYLE)?;
        if let Some(Value::Object(stored)) = store::load_json(&self.profile_path(&id)) {
            let schema = self.schema;
            let mut data = self.defaults();
            merge(&mut data, &self.data, |section| schema.is_shared(section));
            merge(&mut data, &stored, |section| !schema.is_shared(section));
            self.data = data;
        }
        meta.insert("active".into(), id.clone().into());
        self.save_meta(meta)?;
        self.save(emit)?;
        self.notify(SettingsChange::ProfileChanged(id));
        Ok(true)
    }

    pub fn settings_path(&self) -> &Path {
        &self.files.settings
    }
}

/// Copies `saved` over the defaults in `data`. Known keys stay in schema order; unknown keys and sections are
/// kept after them. A known section saved as something other than an object is ignored, as in Python.
fn merge(data: &mut Map<String, Value>, saved: &Map<String, Value>, wanted: impl Fn(&str) -> bool) {
    for (section, values) in saved {
        if !wanted(section) {
            continue;
        }
        match data.get_mut(section) {
            Some(Value::Object(known)) => {
                if let Value::Object(values) = values {
                    for (key, value) in values {
                        known.insert(key.clone(), value.clone());
                    }
                }
            }
            Some(_) => {}
            None => {
                data.insert(section.clone(), values.clone());
            }
        }
    }
}

fn profile_entry(id: &str, name: &str) -> Map<String, Value> {
    let mut entry = Map::new();
    entry.insert("id".into(), id.into());
    entry.insert("name".into(), name.into());
    entry
}

fn profile_list(meta: &mut Map<String, Value>) -> &mut Vec<Value> {
    if !meta.get("profiles").is_some_and(Value::is_array) {
        meta.insert("profiles".into(), Value::Array(Vec::new()));
    }
    meta.get_mut("profiles").and_then(Value::as_array_mut).expect("profiles was just made a list")
}

fn has_profile(meta: &Map<String, Value>, id: &str) -> bool {
    let list = meta.get("profiles").and_then(Value::as_array);
    list.is_some_and(|l| l.iter().any(|p| id_of(p) == Some(id)))
}

fn id_of(profile: &Value) -> Option<&str> {
    profile.get("id")?.as_str()
}
