//! `data/themes.json` and everything done to the themes in it, from `core/themes.py`.
//!
//! ```text
//! {"active": id, "folders": [{"id", "name", "colour", "collapsed"}], "themes": [...], "pack": 1,
//!  "previous", "group_order", "collapsed_groups", "favourite_colours", "recent_colours": ...}
//! ```
//!
//! The built-in Dark and Light themes can not be deleted, so there is always at least one. The store only
//! changes data and says what happened ([`ThemeEvent`]); the app restyles everything, switches the settings
//! profile and moves the wallpaper when it hears `Applied`.

use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use lum_core::Paths;
use lum_core::pyjson::{self, Style};
use lum_core::store::{Writer, load_json};
use serde_json::{Map, Value, json};

use crate::colour::Rgb;
use crate::history::{RECENT_KEPT, RECENT_SAVED, Recent};
use crate::palette::{self, Palette, Role, Section, colours_from_roles, derive_primary_text, preset_colours};
use crate::theme::{Theme, truthy};

pub const PACK_VERSION: i64 = 1;
/// A theme of the "More themes" folder every copy gets once: a handful of colours and a corner radius.
struct PackTheme {
    name: &'static str,
    primary: &'static str,
    bg: &'static str,
    card: &'static str,
    text: &'static str,
    muted: &'static str,
    border: &'static str,
    radius: i64,
}

const fn pack(name: &'static str, colours: [&'static str; 6], radius: i64) -> PackTheme {
    let [primary, bg, card, text, muted, border] = colours;
    PackTheme { name, primary, bg, card, text, muted, border, radius }
}

const PACK: [PackTheme; 10] = [
    pack("Caelestia", ["#CBA6F7", "#141218", "#211F26", "#E6E0E9", "#CAC4D0", "#36343B"], 14),
    pack("Catppuccin Mocha", ["#CBA6F7", "#1E1E2E", "#313244", "#CDD6F4", "#A6ADC8", "#45475A"], 12),
    pack("Catppuccin Latte", ["#8839EF", "#EFF1F5", "#E6E9EF", "#4C4F69", "#6C6F85", "#CCD0DA"], 12),
    pack("Tokyo Night", ["#7AA2F7", "#1A1B26", "#24283B", "#C0CAF5", "#A9B1D6", "#3B4261"], 10),
    pack("Gruvbox", ["#FE8019", "#282828", "#3C3836", "#EBDBB2", "#BDAE93", "#504945"], 6),
    pack("Nord", ["#88C0D0", "#2E3440", "#3B4252", "#ECEFF4", "#D8DEE9", "#4C566A"], 8),
    pack("Rosé Pine", ["#EBBCBA", "#191724", "#26233A", "#E0DEF4", "#908CAA", "#403D52"], 14),
    pack("Dracula", ["#BD93F9", "#282A36", "#343746", "#F8F8F2", "#BFBFBF", "#44475A"], 10),
    pack("Everforest", ["#A7C080", "#2D353B", "#343F44", "#D3C6AA", "#9DA9A0", "#475258"], 10),
    pack("Material You", ["#D0BCFF", "#1C1B1F", "#2B2930", "#E6E1E5", "#CAC4D0", "#49454F"], 18),
];

/// A `.lumtheme` file larger than this has its pictures left out.
const MAX_PICTURE: u64 = 20_000_000;

#[derive(Debug, thiserror::Error)]
pub enum ThemeError {
    #[error("There is no such theme.")]
    NoSuchTheme,
    #[error("This file is not a Luminescent theme (.lumtheme).")]
    NotATheme,
    #[error("{0}")]
    Io(String),
}

/// What happened to the themes, for the app to act on (`themes.applied` and `themes.changed`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeEvent {
    /// The active theme changed or was edited: restyle everything.
    Applied(String),
    /// The list, the folders or a name changed.
    Changed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub colour: String,
    pub collapsed: bool,
}

/// One block on the Themes page: BUILT-IN, MY THEMES or a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub key: String,
    pub name: String,
    pub colour: String,
    pub folder: Option<Folder>,
    pub themes: Vec<String>,
}

pub const BUILTIN_GROUP: &str = "builtin";
pub const MINE_GROUP: &str = "mine";
pub const MINE_COLOUR: &str = "#3DBE96";

pub struct ThemeStore {
    path: PathBuf,
    images_dir: PathBuf,
    /// The whole file except the themes, which sit in `themes`; `"themes"` stays as a placeholder so the key
    /// is written where it was.
    doc: Map<String, Value>,
    themes: Vec<Theme>,
    writer: Writer,
    next_id: Box<dyn FnMut() -> String + Send>,
    events: Vec<ThemeEvent>,
}

impl std::fmt::Debug for ThemeStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThemeStore").field("path", &self.path).field("themes", &self.themes.len()).finish()
    }
}

fn random_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..10].to_owned()
}

impl ThemeStore {
    /// Reads `themes.json` (making it on a first start from the old Settings → Appearance in `raw_settings`),
    /// fills what is missing and adds the theme pack once.
    pub fn load(paths: &Paths, raw_settings: &Map<String, Value>) -> ThemeStore {
        ThemeStore::open(
            paths.data_file("themes.json"),
            paths.data_file("theme-images"),
            raw_settings,
            Box::new(random_id),
        )
    }

    pub fn open(
        path: PathBuf,
        images_dir: PathBuf,
        raw_settings: &Map<String, Value>,
        next_id: Box<dyn FnMut() -> String + Send>,
    ) -> ThemeStore {
        let mut store = ThemeStore {
            path,
            images_dir,
            doc: Map::new(),
            themes: Vec::new(),
            writer: Writer::new("themes-saver"),
            next_id,
            events: Vec::new(),
        };
        store.read(raw_settings);
        store
    }

    fn read(&mut self, raw_settings: &Map<String, Value>) {
        let mut save = false;
        let loaded = load_json(&self.path).and_then(|v| match v {
            Value::Object(m) if m.get("themes").is_some_and(Value::is_array) => Some(m),
            _ => None,
        });
        let mut doc = match loaded {
            Some(m) => m,
            None => {
                save = true;
                self.first_run(raw_settings)
            }
        };
        doc.entry("folders").or_insert_with(|| json!([]));
        let mut themes: Vec<Theme> = doc
            .get("themes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|t| t.as_object())
            .filter(|t| t.get("id").and_then(Value::as_str).is_some_and(|id| !id.is_empty()))
            .map(|t| Theme(t.clone()))
            .collect();
        for (id, name, at) in [("dark", "Dark", 0), ("light", "Light", 1)] {
            if !themes.iter().any(|t| t.id() == id) {
                themes.insert(at.min(themes.len()), Theme::builtin(id, name));
            }
        }
        let mut migrated = false;
        for t in &mut themes {
            migrated |= normalise(t);
        }
        doc.insert("themes".into(), Value::Null);
        self.doc = doc;
        self.themes = themes;
        if migrated {
            save = true;
        }
        if self.add_pack() {
            save = true;
        }
        let active = self.active_id().to_owned();
        if !self.themes.iter().any(|t| t.id() == active) {
            self.doc.insert("active".into(), json!("dark"));
        }
        if save {
            self.save();
            self.writer.flush();
        }
    }

    /// Turns the old Settings → Appearance (mode and custom colours) into theme profiles.
    fn first_run(&mut self, raw_settings: &Map<String, Value>) -> Map<String, Value> {
        let mut themes = vec![Theme::builtin("dark", "Dark"), Theme::builtin("light", "Light")];
        let mut active = "dark".to_owned();
        let old = raw_settings.get("appearance").and_then(Value::as_object).cloned().unwrap_or_default();
        let get = |k: &str| old.get(k).filter(|v| truthy(Some(v)));
        let mode = match get("mode").and_then(Value::as_str) {
            Some(m) => m.to_owned(),
            None if get("primary").is_some() => "custom".to_owned(),
            None => "dark".to_owned(),
        };
        let look: Vec<(&str, Value)> = ["font_family", "radius"]
            .into_iter()
            .filter_map(|k| old.get(k).map(|v| (k, v.clone())))
            .collect();
        if mode == "custom" {
            let id = (self.next_id)();
            let mut custom = Theme::new(id.clone(), "My old colours", None);
            for role in Role::ALL {
                if let Some(v) = old.get(role.key()) {
                    custom.colours_mut().insert(role.key().to_owned(), v.clone());
                }
            }
            if !old.contains_key("on_primary")
                && let Some(bg) = old.get("bg_base")
            {
                custom.colours_mut().insert("on_primary".into(), bg.clone());
            }
            for (k, v) in &look {
                custom.set(k, v.clone());
            }
            themes.push(custom);
            // the old default "custom" colours were never chosen on purpose: keep them, start on Dark
            if old.get("mode").and_then(Value::as_str) == Some("custom") {
                active = id;
            }
        } else if mode == "dark" || mode == "light" {
            active = mode.clone();
            for t in &mut themes {
                if t.id() == mode {
                    for (k, v) in &look {
                        t.set(k, v.clone());
                    }
                }
            }
        }
        let mut doc = Map::new();
        doc.insert("active".into(), json!(active));
        doc.insert("folders".into(), json!([]));
        doc.insert("themes".into(), Value::Array(themes.into_iter().map(|t| Value::Object(t.0)).collect()));
        doc
    }

    /// Adds the theme pack once per `PACK_VERSION`; deleted ones stay deleted. Returns whether it changed.
    fn add_pack(&mut self) -> bool {
        let seen = self
            .doc
            .get("pack")
            .and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))
            .unwrap_or(0);
        if seen >= PACK_VERSION {
            return false;
        }
        let folder_id = match self.folders().into_iter().find(|f| f.name == "More themes") {
            Some(f) => f.id,
            None => {
                let id = (self.next_id)();
                self.folder_list_mut()
                    .push(json!({"id": id, "name": "More themes", "colour": "#CBA6F7", "collapsed": false}));
                id
            }
        };
        let names: Vec<String> = self.themes.iter().map(|t| t.name().to_owned()).collect();
        for entry in PACK {
            if names.iter().any(|n| n == entry.name) {
                continue;
            }
            let id = (self.next_id)();
            let mut t = Theme::new(id, entry.name, None);
            let roles: Map<String, Value> = [
                ("primary", entry.primary),
                ("bg", entry.bg),
                ("card", entry.card),
                ("text", entry.text),
                ("muted", entry.muted),
                ("border", entry.border),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), json!(v)))
            .collect();
            let colours = colours_from_roles(&roles);
            t.set("colours", Value::Object(colours.into_iter().map(|(k, v)| (k, json!(v))).collect()));
            t.set("folder", json!(folder_id));
            t.set("radius", json!(entry.radius));
            self.themes.push(t);
        }
        self.doc.insert("pack".into(), json!(PACK_VERSION));
        true
    }

    fn folder_list_mut(&mut self) -> &mut Vec<Value> {
        let slot = self.doc.entry("folders").or_insert_with(|| json!([]));
        if !slot.is_array() {
            *slot = json!([]);
        }
        match slot {
            Value::Array(list) => list,
            _ => unreachable!("it was just made an array"),
        }
    }

    // ── persistence ────────────────────────────────────────────────────────

    fn document(&self) -> Value {
        let mut out = Map::new();
        for (k, v) in &self.doc {
            if k == "themes" {
                out.insert(
                    k.clone(),
                    Value::Array(self.themes.iter().map(|t| Value::Object(t.0.clone())).collect()),
                );
            } else {
                out.insert(k.clone(), v.clone());
            }
        }
        Value::Object(out)
    }

    /// Queues the file for writing (the disk is not touched on this thread).
    pub fn save(&self) {
        self.writer.save(&self.path, pyjson::to_string(&self.document(), Style::indented(2)));
    }

    /// Waits until everything saved is on disk.
    pub fn flush(&self) {
        self.writer.flush();
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// What happened since the last call.
    pub fn take_events(&mut self) -> Vec<ThemeEvent> {
        std::mem::take(&mut self.events)
    }

    fn changed(&mut self) {
        self.save();
        self.events.push(ThemeEvent::Changed);
    }

    // ── access ─────────────────────────────────────────────────────────────

    pub fn themes(&self) -> &[Theme] {
        &self.themes
    }

    pub fn get(&self, id: &str) -> Option<&Theme> {
        self.themes.iter().find(|t| t.id() == id)
    }

    pub fn active_id(&self) -> &str {
        self.doc.get("active").and_then(Value::as_str).unwrap_or("dark")
    }

    pub fn active(&self) -> &Theme {
        self.get(self.active_id()).unwrap_or(&self.themes[0])
    }

    /// The theme that was on before the active one ("Back to the theme before").
    pub fn previous(&self) -> Option<&Theme> {
        self.get(self.doc.get("previous").and_then(Value::as_str).unwrap_or(""))
    }

    pub fn folders(&self) -> Vec<Folder> {
        self.doc
            .get("folders")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|f| {
                let f = f.as_object()?;
                Some(Folder {
                    id: f.get("id")?.as_str()?.to_owned(),
                    name: f.get("name").and_then(Value::as_str).unwrap_or("").to_owned(),
                    colour: f.get("colour").and_then(Value::as_str).unwrap_or("").to_owned(),
                    collapsed: truthy(f.get("collapsed")),
                })
            })
            .collect()
    }

    /// The active theme's palette and the looks of its modules, for the app to install.
    pub fn active_look(&self) -> (Palette, Vec<(String, Section)>) {
        let t = self.active();
        (t.palette(), t.sections())
    }

    /// Anything else stored in the file (favourites, recent changes, group order).
    pub fn extra(&self, key: &str) -> Option<&Value> {
        self.doc.get(key)
    }

    pub fn set_extra(&mut self, key: &str, value: Value) {
        self.doc.insert(key.to_owned(), value);
        self.save();
    }

    // ── changes ────────────────────────────────────────────────────────────

    /// Makes `id` the active theme. Returns false for a theme that does not exist.
    pub fn apply(&mut self, id: &str) -> bool {
        if self.get(id).is_none() {
            return false;
        }
        if self.active_id() != id {
            let was = self.active_id().to_owned();
            self.doc.insert("previous".into(), json!(was));
        }
        self.doc.insert("active".into(), json!(id));
        self.save();
        self.events.push(ThemeEvent::Applied(id.to_owned()));
        true
    }

    /// Stores an edited theme; re-applies it if it is the active one.
    pub fn update_theme(&mut self, t: Theme) {
        let id = t.id().to_owned();
        if let Some(slot) = self.themes.iter_mut().find(|o| o.id() == id) {
            *slot = t;
        }
        self.changed();
        if id == self.active_id() {
            self.apply(&id);
        }
    }

    pub fn add(&mut self, t: Theme) {
        self.themes.push(t);
        self.changed();
    }

    /// A new theme made as a copy of `base` (the active theme when `None`), added but not applied.
    pub fn new_theme(&mut self, name: &str, base: Option<&Theme>, folder: &str) -> Theme {
        let id = (self.next_id)();
        let mut t = Theme::new(id, name, Some(base.unwrap_or_else(|| self.active())));
        t.set("profile", json!(""));
        t.set("folder", json!(folder));
        self.add(t.clone());
        t
    }

    pub fn duplicate(&mut self, id: &str) -> Option<Theme> {
        let src = self.get(id)?.clone();
        let copy = Theme::new((self.next_id)(), &format!("{} copy", src.name()), Some(&src));
        self.add(copy.clone());
        Some(copy)
    }

    pub fn rename(&mut self, id: &str, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        if let Some(t) = self.themes.iter_mut().find(|t| t.id() == id) {
            t.set("name", json!(name));
            self.changed();
        }
    }

    pub fn can_delete(&self, id: &str) -> bool {
        self.get(id).is_some_and(|t| !t.builtin_flag()) && self.themes.len() > 1
    }

    pub fn delete(&mut self, id: &str) {
        if !self.can_delete(id) {
            return;
        }
        let was_active = self.active_id() == id;
        self.themes.retain(|t| t.id() != id);
        self.changed();
        if was_active {
            let first = self.themes[0].id().to_owned();
            self.apply(&first);
        }
    }

    pub fn move_to_folder(&mut self, id: &str, folder: &str) {
        if let Some(t) = self.themes.iter_mut().find(|t| t.id() == id) {
            t.set("folder", json!(folder));
            self.changed();
        }
    }

    pub fn add_folder(&mut self, name: &str, colour: &str) -> Folder {
        let name = match name.trim() {
            "" => "Folder",
            n => n,
        };
        let id = (self.next_id)();
        self.folder_list_mut().push(json!({"id": id, "name": name, "colour": colour, "collapsed": false}));
        self.changed();
        Folder { id, name: name.to_owned(), colour: colour.to_owned(), collapsed: false }
    }

    /// Changes one folder: `name`, `colour` or `collapsed`.
    pub fn update_folder(&mut self, id: &str, key: &str, value: Value) {
        for f in self.folder_list_mut() {
            if f.get("id").and_then(Value::as_str) == Some(id)
                && let Some(map) = f.as_object_mut()
            {
                map.insert(key.to_owned(), value.clone());
            }
        }
        self.changed();
    }

    /// Swaps two themes' places (the order they show in on the Themes page and in Next theme).
    pub fn swap_themes(&mut self, a: &str, b: &str) {
        let find = |id: &str| self.themes.iter().position(|t| t.id() == id);
        if let (Some(i), Some(j)) = (find(a), find(b)) {
            self.themes.swap(i, j);
            self.changed();
        }
    }

    /// The order of the groups on the Themes page (`builtin`, `mine` and folder ids, top to bottom). The tray
    /// and the palette follow it too.
    pub fn set_group_order(&mut self, keys: &[String]) {
        self.doc.insert("group_order".into(), json!(keys));
        let mut folders = std::mem::take(self.folder_list_mut());
        let mut ordered = Vec::new();
        for key in keys {
            if let Some(i) = folders.iter().position(|f| f.get("id").and_then(Value::as_str) == Some(key)) {
                ordered.push(folders.remove(i));
            }
        }
        ordered.extend(folders);
        *self.folder_list_mut() = ordered;
        self.changed();
    }

    pub fn delete_folder(&mut self, id: &str) {
        self.folder_list_mut().retain(|f| f.get("id").and_then(Value::as_str) != Some(id));
        for t in &mut self.themes {
            if t.folder() == id {
                t.set("folder", json!(""));
            }
        }
        self.changed();
    }

    /// The theme after the active one (`step` -1: before it) within the active theme's folder (or the built-in
    /// and loose themes it sits with), wrapping round.
    pub fn next_theme(&self, step: i64) -> Option<&Theme> {
        let act = self.active();
        let folders: Vec<String> = self.folders().into_iter().map(|f| f.id).collect();
        let folder = act.folder();
        let mut ring: Vec<&Theme> = if folders.iter().any(|f| f == folder) {
            self.themes.iter().filter(|t| t.folder() == folder).collect()
        } else {
            self.themes
                .iter()
                .filter(|t| t.folder().is_empty() || !folders.iter().any(|f| f == t.folder()))
                .collect()
        };
        if ring.len() < 2 {
            ring = self.themes.iter().collect();
        }
        if ring.is_empty() {
            return None;
        }
        let at = ring.iter().position(|t| t.id() == act.id()).map_or(-1, |i| i as i64);
        let n = ring.len() as i64;
        Some(ring[(at + step).rem_euclid(n) as usize])
    }

    /// The groups of the Themes page in the order they were dragged into: BUILT-IN, MY THEMES, then the folders.
    pub fn groups(&self) -> Vec<Group> {
        let folders = self.folders();
        let in_a_folder = |t: &Theme| folders.iter().any(|f| f.id == t.folder());
        let loose: Vec<&Theme> = self.themes.iter().filter(|t| !in_a_folder(t)).collect();
        let mut out = vec![Group {
            key: BUILTIN_GROUP.into(),
            name: "Built-in".into(),
            colour: preset_colours(false)["primary"].clone(),
            folder: None,
            themes: loose.iter().filter(|t| t.builtin_flag()).map(|t| t.id().to_owned()).collect(),
        }];
        let mine: Vec<String> =
            loose.iter().filter(|t| !t.builtin_flag()).map(|t| t.id().to_owned()).collect();
        if !mine.is_empty() || folders.is_empty() {
            out.push(Group {
                key: MINE_GROUP.into(),
                name: "My themes".into(),
                colour: MINE_COLOUR.into(),
                folder: None,
                themes: mine,
            });
        }
        for f in &folders {
            out.push(Group {
                key: f.id.clone(),
                name: f.name.clone(),
                colour: f.colour.clone(),
                folder: Some(f.clone()),
                themes: self
                    .themes
                    .iter()
                    .filter(|t| t.folder() == f.id)
                    .map(|t| t.id().to_owned())
                    .collect(),
            });
        }
        let order: Vec<&str> = self
            .doc
            .get("group_order")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let default_place: Vec<String> = out.iter().map(|g| g.key.clone()).collect();
        out.sort_by_key(|g| {
            order
                .iter()
                .position(|k| *k == g.key)
                .unwrap_or(order.len() + default_place.iter().position(|k| *k == g.key).unwrap_or(0))
        });
        out
    }

    /// Whether a built-in or "my themes" group is folded (real folders keep it in the folder).
    pub fn group_collapsed(&self, group: &Group) -> bool {
        match &group.folder {
            Some(f) => f.collapsed,
            None => self
                .doc
                .get("collapsed_groups")
                .and_then(Value::as_array)
                .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(group.key.as_str()))),
        }
    }

    pub fn toggle_group(&mut self, group: &Group) {
        if let Some(f) = &group.folder {
            self.update_folder(&f.id, "collapsed", json!(!f.collapsed));
            return;
        }
        let mut closed: Vec<String> = self
            .doc
            .get("collapsed_groups")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect();
        if let Some(i) = closed.iter().position(|k| *k == group.key) {
            closed.remove(i);
        } else {
            closed.push(group.key.clone());
        }
        closed.sort();
        self.doc.insert("collapsed_groups".into(), json!(closed));
        self.save();
        self.events.push(ThemeEvent::Changed);
    }

    // ── made from one colour ───────────────────────────────────────────────

    /// A new theme whose colours all go with `accent` (added, not applied).
    pub fn from_accent(&mut self, accent: Rgb, light: bool, name: &str) -> Theme {
        let name = if name.is_empty() {
            format!("{} {}", accent.hex(), if light { "light" } else { "dark" })
        } else {
            name.to_owned()
        };
        let mut t = Theme::new((self.next_id)(), &name, None);
        let mut colours = preset_colours(light);
        for (k, v) in palette::generate(accent, light) {
            colours.insert(k, v);
        }
        t.set("colours", Value::Object(colours.into_iter().map(|(k, v)| (k, json!(v))).collect()));
        self.add(t.clone());
        t
    }

    /// A theme from another app's colours (`primary`, `bg`, `card`, `text`, `muted`, `border`). A theme with the
    /// same name is updated instead of adding another. Returns its id (`None` without an accent).
    pub fn from_colours(&mut self, name: &str, roles: &Map<String, Value>) -> Option<String> {
        let cols = colours_from_roles(roles);
        if cols.is_empty() {
            return None;
        }
        let value = Value::Object(cols.into_iter().map(|(k, v)| (k, json!(v))).collect());
        if let Some(old) = self.themes.iter().find(|t| t.name() == name && !t.builtin_flag()) {
            let mut t = old.clone();
            t.set("colours", value);
            let id = t.id().to_owned();
            self.update_theme(t);
            return Some(id);
        }
        let mut t = Theme::new((self.next_id)(), name, None);
        t.set("colours", value);
        let id = t.id().to_owned();
        self.add(t);
        Some(id)
    }

    /// Gives a theme a new accent colour; its accent shades follow.
    pub fn set_accent(&mut self, id: &str, accent: Rgb) {
        let Some(mut t) = self.get(id).cloned() else { return };
        let bg = t
            .colours()
            .get("bg_base")
            .and_then(Value::as_str)
            .and_then(Rgb::parse)
            .unwrap_or(Rgb::from_u32(0x131317));
        let roles = palette::accent_roles(accent, bg);
        for (role, colour) in roles {
            t.colours_mut().insert(role.key().to_owned(), json!(colour.hex()));
        }
        if truthy(t.colours().get("folder_border")) {
            let text = t.colours().get("primary_text").cloned().unwrap_or(Value::Null);
            t.colours_mut().insert("folder_border".into(), text);
        }
        self.update_theme(t);
    }

    /// The built-in theme back to its original colours.
    pub fn reset_builtin(&mut self, id: &str) {
        let Some(t) = self.get(id) else { return };
        if !t.builtin_flag() || !matches!(id, "dark" | "light") {
            return;
        }
        let mut fresh = Theme::builtin(id, t.name());
        fresh.set("folder", json!(t.folder()));
        self.update_theme(fresh);
    }

    // ── sharing ────────────────────────────────────────────────────────────

    /// Writes one theme to a `.lumtheme` file (JSON). Its background image and card icon go inside it.
    pub fn export(&self, id: &str, path: &Path) -> Result<(), ThemeError> {
        let t = self.get(id).ok_or(ThemeError::NoSuchTheme)?;
        let mut out: Map<String, Value> =
            t.0.iter()
                .filter(|(k, _)| !matches!(k.as_str(), "id" | "builtin" | "folder" | "profile"))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
        let mut files = Map::new();
        for key in ["background_image", "icon"] {
            let picture = Path::new(t.str(key));
            if !t.str(key).is_empty()
                && let Ok(meta) = fs::metadata(picture)
                && meta.is_file()
                && meta.len() < MAX_PICTURE
                && let Ok(bytes) = fs::read(picture)
            {
                let name = picture.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                files.insert(key.into(), json!({"name": name, "data": BASE64.encode(bytes)}));
            }
            out.insert(key.into(), json!(""));
        }
        // a path on this PC means nothing on another one
        out.insert("wallpaper".into(), json!(""));
        let doc = json!({"luminescent_theme": 1, "theme": out, "files": files});
        lum_core::store::save_json(path, &doc, Style::indented(1)).map_err(|e| ThemeError::Io(e.to_string()))
    }

    /// Adds the theme in a `.lumtheme` file.
    pub fn import_file(&mut self, path: &Path) -> Result<Theme, ThemeError> {
        let d = fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .ok_or(ThemeError::NotATheme)?;
        let src = d.get("theme").and_then(Value::as_object).filter(|_| d.get("luminescent_theme").is_some());
        let src = src.ok_or(ThemeError::NotATheme)?;
        let name = match src.get("name").and_then(Value::as_str) {
            Some(n) if !n.is_empty() => n.to_owned(),
            _ => "Imported theme".to_owned(),
        };
        let mut t = Theme::new((self.next_id)(), &name, None);
        for key in [
            "colours",
            "gradients",
            "font_family",
            "radius",
            "glow",
            "image_opacity",
            "modules",
            "toasts",
            "caelestia",
        ] {
            if let Some(v) = src.get(key) {
                t.set(key, v.clone());
            }
        }
        let own: Map<String, Value> = preset_colours(false).into_iter().map(|(k, v)| (k, json!(v))).collect();
        let mut colours = own;
        for (k, v) in t.colours() {
            if v.as_str().is_some_and(|s| s.starts_with('#')) {
                colours.insert(k, v);
            }
        }
        t.set("colours", Value::Object(colours));
        if let Some(files) = d.get("files").and_then(Value::as_object) {
            for (key, f) in files {
                if !matches!(key.as_str(), "background_image" | "icon") {
                    continue;
                }
                let Some(f) = f.as_object() else { continue };
                match self.unpack_picture(t.id(), key, f) {
                    Ok(dst) => t.set(key, json!(dst.to_string_lossy())),
                    Err(e) => tracing::warn!("could not unpack {key} from {}: {e}", path.display()),
                }
            }
        }
        t.0.shift_remove("hotkey");
        self.add(t.clone());
        Ok(t)
    }

    fn unpack_picture(
        &self,
        theme_id: &str,
        key: &str,
        file: &Map<String, Value>,
    ) -> std::io::Result<PathBuf> {
        fs::create_dir_all(&self.images_dir)?;
        let given = file.get("name").and_then(Value::as_str).unwrap_or(key);
        let name =
            Path::new(given).file_name().map_or_else(|| key.to_owned(), |n| n.to_string_lossy().into_owned());
        let data = BASE64
            .decode(file.get("data").and_then(Value::as_str).unwrap_or(""))
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let dst = self.images_dir.join(format!("{theme_id}-{name}"));
        fs::write(&dst, data)?;
        Ok(dst)
    }

    // ── what stays with a theme ────────────────────────────────────────────

    /// "Recently changed" for one theme, newest first.
    pub fn recent_changes(&self, id: &str) -> Vec<Recent> {
        self.doc
            .get("recent_colours")
            .and_then(|r| r.get(id))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Recent::from_value)
            .take(RECENT_KEPT)
            .collect()
    }

    pub fn set_recent_changes(&mut self, id: &str, list: &[Recent]) {
        let keep: Vec<Value> = list.iter().take(RECENT_SAVED).map(Recent::to_value).collect();
        let slot = self.doc.entry("recent_colours").or_insert_with(|| json!({}));
        if !slot.is_object() {
            *slot = json!({});
        }
        if let Some(map) = slot.as_object_mut() {
            map.insert(id.to_owned(), Value::Array(keep));
        }
        self.save();
    }

    /// The colours starred in the theme editor (`lum:primary`, `cael:surface`...).
    pub fn favourites(&self) -> Vec<String> {
        self.doc
            .get("favourite_colours")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect()
    }

    pub fn set_favourite(&mut self, id: &str, on: bool) {
        let mut favs: Vec<String> = self.favourites().into_iter().filter(|f| f != id).collect();
        if on {
            favs.push(id.to_owned());
        }
        self.doc.insert("favourite_colours".into(), json!(favs));
        self.save();
    }

    // ── keys ───────────────────────────────────────────────────────────────

    /// The keys themes have of their own: (theme id, theme name, combo).
    pub fn hotkeys(&self) -> Vec<(String, String, String)> {
        self.themes
            .iter()
            .filter(|t| !t.hotkey().is_empty())
            .map(|t| (t.id().to_owned(), t.name().to_owned(), t.hotkey().to_owned()))
            .collect()
    }
}

/// `ThemeStore._load` for one theme: missing keys filled from Dark's, an untouched pre-redesign built-in moved
/// to the new colours and type, the accent text made up when it is missing. Returns whether it migrated.
fn normalise(t: &mut Theme) -> bool {
    let base = Theme::builtin("dark", t.name());
    for (k, v) in base.0 {
        t.0.entry(k).or_insert(v);
    }
    let mut cols = t.colours();
    let mut migrated = false;
    if t.builtin_flag()
        && let Some(old) = palette::old_preset(t.id())
        && old.iter().all(|(role, colour)| {
            cols.get(role.key()).map_or(String::new(), value_text).to_uppercase()
                == colour.hex().to_uppercase()
        })
    {
        // an untouched built-in from before the redesign: move it to the new colours and type
        cols = preset_colours(t.id() == "light").into_iter().map(|(k, v)| (k, json!(v))).collect();
        if matches!(t.str("font_family"), "" | "Segoe UI") {
            t.set("font_family", json!(palette::DEFAULT_FONT));
        }
        migrated = true;
    }
    if !truthy(cols.get("primary_text")) {
        let get = |k: &str, default: u32| {
            cols.get(k).and_then(Value::as_str).and_then(Rgb::parse).unwrap_or(Rgb::from_u32(default))
        };
        let text = derive_primary_text(get("primary", 0x7B6CF6), get("bg_base", 0x131317));
        cols.insert("primary_text".into(), json!(text.hex()));
    }
    let mut merged: Map<String, Value> =
        preset_colours(false).into_iter().map(|(k, v)| (k, json!(v))).collect();
    for (k, v) in cols {
        merged.insert(k, v);
    }
    t.set("colours", Value::Object(merged));
    migrated
}

fn value_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
