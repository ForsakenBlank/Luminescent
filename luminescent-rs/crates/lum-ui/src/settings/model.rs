//! What the Settings page holds between Save presses, with no drawing in it: the draft values, the rules that
//! make a stored value into what the widget would show, the search, and the hotkey clash checks. Python keeps
//! all of this in widgets (`settings_ui.py`); here it is plain data so it can be tested.

use std::collections::HashMap;

use lum_core::settings::{Field, FieldType, Schema, Section};
use lum_platform::hotkeys::{Os, keys};
use serde_json::{Map, Number, Value};

use crate::overlay::pages::{REQUIRED_PAGES, all_pages};

/// Panels that can open by themselves when Luminescent starts (Settings → General → Show on start).
pub const START_PANELS: &[(&str, &str)] = &[
    ("notes", "Notes"),
    ("hearing", "Hearing Guard"),
    ("sysmon", "System Monitor"),
    ("performance", "Performance"),
    ("media", "Media controls"),
];

/// What the tray menu can list (Settings → General → Tray menu).
pub const TRAY_ITEMS: &[(&str, &str)] = &[
    ("open", "Open Luminescent"),
    ("palette", "Command palette"),
    ("dashboard", "Dashboard items"),
    ("automation", "Automation (pause, buttons, workspaces)"),
    ("modules", "Modules submenu"),
    ("actions", "Module actions (Snippets, Notes, Calculator…)"),
    ("colours", "Recent colours"),
    ("keepawake", "Keep awake"),
    ("reset", "Reset stuck keys"),
    ("settings", "Settings"),
    ("log", "Open log file"),
    ("restart", "Restart"),
    ("quit", "Quit"),
];

/// The fields that have a widget on the page and are stored. Hidden ones are kept as they are.
pub fn is_edited(section: &Section, field: &Field) -> bool {
    match field.kind {
        FieldType::Widget | FieldType::Hidden => false,
        FieldType::Custom => has_editor(&section.key, &field.key),
        _ => true,
    }
}

pub fn has_editor(section: &str, field: &str) -> bool {
    matches!(
        (section, field),
        ("ui", "nav_order") | ("general", "start_panels" | "tray_items") | ("reminders", "items")
    )
}

/// What a field is drawn as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Hotkey,
    Text,
    Path,
    Int,
    Float,
    Opacity,
    Switch,
    Colour,
    Choice,
    NavOrder,
    CheckList,
    Backup,
    Quit,
    Updates,
    NotificationTest,
    /// A module's own editor that has not been built yet (the reminders list, phase 8).
    Slot,
}

pub fn control_of(section: &str, field: &Field) -> Option<Control> {
    Some(match field.kind {
        FieldType::Hotkey => Control::Hotkey,
        FieldType::Text => Control::Text,
        FieldType::Dir | FieldType::File => Control::Path,
        FieldType::Int => Control::Int,
        FieldType::Float => Control::Float,
        FieldType::Opacity => Control::Opacity,
        FieldType::Bool => Control::Switch,
        FieldType::Color => Control::Colour,
        FieldType::Choice => Control::Choice,
        FieldType::Hidden => return None,
        FieldType::Custom | FieldType::Widget => match (section, field.key.as_str()) {
            ("ui", "nav_order") => Control::NavOrder,
            ("general", "start_panels" | "tray_items") => Control::CheckList,
            ("general", "backup") => Control::Backup,
            ("general", "quit") => Control::Quit,
            ("general", "updates") => Control::Updates,
            ("general", "toast_test") => Control::NotificationTest,
            _ => Control::Slot,
        },
    })
}

/// A field that is listed on the page (everything but the hidden ones).
pub fn is_shown(field: &Field) -> bool {
    field.kind != FieldType::Hidden
}

/// The values on the page. `data` has the same shape as settings.json; only sections that were built (shown
/// once) have been through [`normalise`], as in Python where a section's widgets exist from then on.
#[derive(Debug, Clone)]
pub struct Draft {
    schema: &'static Schema,
    data: Map<String, Value>,
    loaded: Map<String, Value>,
    built: Vec<bool>,
}

impl Draft {
    pub fn new(schema: &'static Schema, saved: &Map<String, Value>) -> Draft {
        Draft {
            schema,
            data: saved.clone(),
            loaded: saved.clone(),
            built: vec![false; schema.sections.len()],
        }
    }

    /// Start again from the saved values (Undo changes, a profile switch, a module storing something).
    pub fn reload(&mut self, saved: &Map<String, Value>) {
        self.data = saved.clone();
        self.loaded = saved.clone();
        for i in 0..self.built.len() {
            if self.built[i] {
                self.normalise_section(i);
            }
        }
    }

    pub fn is_built(&self, section: usize) -> bool {
        self.built.get(section).copied().unwrap_or(true)
    }

    pub fn next_unbuilt(&self) -> Option<usize> {
        self.built.iter().position(|b| !b)
    }

    /// Takes the section's values the way its widgets would show them. What the person has not touched yet
    /// counts as unchanged, so this also moves what the page started from.
    pub fn build(&mut self, section: usize) {
        if self.built.get(section) == Some(&false) {
            self.built[section] = true;
            self.normalise_section(section);
        }
    }

    fn normalise_section(&mut self, index: usize) {
        let section = &self.schema.sections[index];
        for field in section.fields.iter().filter(|f| is_edited(section, f)) {
            let now = self.data.get(&section.key).and_then(|s| s.get(&field.key));
            let value = normalise(section, field, now.unwrap_or(&field.default));
            put(&mut self.data, &section.key, &field.key, value.clone());
            put(&mut self.loaded, &section.key, &field.key, value);
        }
    }

    pub fn data(&self) -> &Map<String, Value> {
        &self.data
    }

    pub fn value(&self, section: &str, key: &str) -> Option<&Value> {
        self.data.get(section)?.get(key)
    }

    pub fn set(&mut self, section: &str, key: &str, value: Value) {
        put(&mut self.data, section, key, value);
    }

    /// A value that was stored by something else just now (a module switched on from its page): the page
    /// shows it and does not count it as a change.
    pub fn set_stored(&mut self, section: &str, key: &str, value: Value) {
        put(&mut self.loaded, section, key, value.clone());
        put(&mut self.data, section, key, value);
    }

    fn differs(&self, section: &Section, field: &Field) -> bool {
        let at = |map: &Map<String, Value>| map.get(&section.key).and_then(|s| s.get(&field.key)).cloned();
        at(&self.data) != at(&self.loaded)
    }

    /// How many settings on the page differ from what it started from.
    pub fn changed_count(&self) -> usize {
        let sections = self.schema.sections.iter().enumerate().filter(|(i, _)| self.built[*i]);
        let fields = sections.flat_map(|(_, s)| s.fields.iter().map(move |f| (s, f)));
        fields.filter(|(s, f)| is_edited(s, f) && self.differs(s, f)).count()
    }

    pub fn is_dirty(&self) -> bool {
        self.changed_count() > 0
    }

    /// What was just saved is what the page starts from now.
    pub fn mark_saved(&mut self) {
        self.loaded = self.data.clone();
    }

    /// Puts a section's fields back to their defaults (Reset page). Lists with their own editor stay as they
    /// are, as in Python.
    pub fn reset_section(&mut self, index: usize, defaults: &Map<String, Value>) {
        let section = &self.schema.sections[index];
        for field in section.fields.iter().filter(|f| is_edited(section, f) && f.kind != FieldType::Custom) {
            let Some(default) = defaults.get(&section.key).and_then(|s| s.get(&field.key)) else { continue };
            let value = normalise(section, field, default);
            put(&mut self.data, &section.key, &field.key, value);
        }
    }

    /// The sections where the person changed something (compared with how the page showed them at first, so a
    /// hotkey written another way is not a change).
    pub fn edited_sections(&self) -> Vec<String> {
        let built = self.schema.sections.iter().zip(&self.built).filter(|(_, b)| **b).map(|(s, _)| s);
        let edited = built.filter(|s| s.fields.iter().any(|f| is_edited(s, f) && self.differs(s, f)));
        edited.map(|s| s.key.clone()).collect()
    }

    /// Whether a field marked "needs a restart" got another value.
    pub fn restart_needed(&self) -> bool {
        let built = self.schema.sections.iter().zip(&self.built).filter(|(_, b)| **b).map(|(s, _)| s);
        let mut fields = built.flat_map(|s| s.fields.iter().map(move |f| (s, f)));
        fields.any(|(s, f)| f.restart && is_edited(s, f) && self.differs(s, f))
    }

    /// What would be stored: the saved settings with this page's values over them. Settings the page does not
    /// show (the pinned sections, the layout of the overlay's bar) keep what the app stored meanwhile.
    pub fn collect(&self, saved: &Map<String, Value>) -> Map<String, Value> {
        let mut out = saved.clone();
        for section in self.schema.sections.iter().zip(&self.built).filter(|(_, b)| **b).map(|(s, _)| s) {
            for field in section.fields.iter().filter(|f| is_edited(section, f)) {
                if let Some(value) = self.value(&section.key, &field.key) {
                    put(&mut out, &section.key, &field.key, value.clone());
                }
            }
        }
        out
    }
}

fn put(data: &mut Map<String, Value>, section: &str, key: &str, value: Value) {
    let entry = data.entry(section.to_owned()).or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    if let Value::Object(map) = entry {
        map.insert(key.to_owned(), value);
    }
}

/// Python's `bool(value)`.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

fn text_of(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn int_of(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f.trunc() as i64)),
        Value::Bool(b) => Some(i64::from(*b)),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn float_of(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn number(value: f64) -> Value {
    Number::from_f64(value).map_or(Value::Null, Value::Number)
}

/// What a field's widget would give back after being filled with `value`: a spin box clamps, a text box
/// trims, a hotkey is written the usual way, a choice that is not listed becomes the first one.
pub fn normalise(section: &Section, field: &Field, value: &Value) -> Value {
    let fallback = || normalise_default(section, field);
    match field.kind {
        FieldType::Hotkey => Value::String(keys::normalize(&text_of(value))),
        FieldType::Text | FieldType::Dir | FieldType::File => Value::String(text_of(value).trim().to_owned()),
        FieldType::Int => match int_of(value) {
            Some(n) => Value::from((n as f64).min(field.max).max(field.min) as i64),
            None => fallback(),
        },
        FieldType::Float => match float_of(value) {
            Some(f) => number((f.min(field.max).max(field.min) * 100.0).round() / 100.0),
            None => fallback(),
        },
        FieldType::Opacity => match float_of(value) {
            Some(f) => number(((f * 100.0).round_ties_even().clamp(0.0, 100.0)) / 100.0),
            None => fallback(),
        },
        FieldType::Bool => Value::Bool(truthy(value)),
        FieldType::Color => match value.as_str() {
            Some(s) if !s.is_empty() => value.clone(),
            _ => Value::String("#000000".to_owned()),
        },
        FieldType::Choice => {
            let listed = field.choices.iter().any(|(v, _)| v == value);
            match field.choices.first() {
                Some((first, _)) if !listed => first.clone(),
                _ => value.clone(),
            }
        }
        FieldType::Custom => normalise_list(&section.key, &field.key, value),
        FieldType::Widget | FieldType::Hidden => value.clone(),
    }
}

/// A value the widget can not show falls back to the field's default (Python's "could not load" branch).
fn normalise_default(section: &Section, field: &Field) -> Value {
    match field.kind {
        FieldType::Int | FieldType::Float | FieldType::Opacity => normalise(section, field, &field.default),
        _ => field.default.clone(),
    }
}

fn strings(value: &Value) -> Vec<String> {
    let list = value.as_array();
    list.map(|l| l.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect()).unwrap_or_default()
}

fn normalise_list(section: &str, field: &str, value: &Value) -> Value {
    let list = match (section, field) {
        ("ui", "nav_order") => NavState::from_value(&strings(value)).value(),
        ("general", "start_panels") => ticked(START_PANELS, &strings(value)),
        ("general", "tray_items") => ticked(TRAY_ITEMS, &strings(value)),
        _ => return value.clone(),
    };
    Value::Array(list.into_iter().map(Value::String).collect())
}

/// The choices that are ticked, in the order the list shows them.
pub fn ticked(choices: &[(&str, &str)], chosen: &[String]) -> Vec<String> {
    let on = choices.iter().filter(|(key, _)| chosen.iter().any(|c| c == key));
    on.map(|(key, _)| (*key).to_owned()).collect()
}

/// The page switcher's editor: every page in an order, each on or off. Dashboard and Settings are always on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavState {
    pub keys: Vec<String>,
    pub on: Vec<String>,
}

impl NavState {
    pub fn from_value(order: &[String]) -> NavState {
        let mut keys: Vec<String> = Vec::new();
        for key in order {
            if all_pages().any(|(k, _)| k == key) && !keys.contains(key) {
                keys.push(key.clone());
            }
        }
        let mut on = keys.clone();
        on.extend(REQUIRED_PAGES.iter().map(|k| (*k).to_owned()));
        for (key, _) in all_pages() {
            if !keys.iter().any(|k| k == key) {
                keys.push((*key).to_owned());
            }
        }
        NavState { keys, on }
    }

    pub fn is_on(&self, key: &str) -> bool {
        self.on.iter().any(|k| k == key)
    }

    pub fn toggle(&mut self, key: &str, on: bool) {
        self.on.retain(|k| k != key);
        if on {
            self.on.push(key.to_owned());
        }
    }

    pub fn shift(&mut self, at: usize, by: isize) {
        let Some(to) = at.checked_add_signed(by).filter(|to| *to < self.keys.len()) else { return };
        self.keys.swap(at, to);
    }

    pub fn value(&self) -> Vec<String> {
        self.keys.iter().filter(|k| self.is_on(k)).cloned().collect()
    }
}

/// Searching: a setting matches when what you typed is in its name or its help text (`_field_hit`). `query`
/// is already trimmed and lower case.
pub fn field_hit(field: &Field, query: &str) -> bool {
    format!("{} {}", field.label, field.help).to_lowercase().contains(query)
}

pub fn section_hits(section: &Section, query: &str) -> usize {
    section.fields.iter().filter(|f| is_shown(f) && field_hit(f, query)).count()
}

/// The sidebar hides a section the search does not touch. Without "Only matching settings" a match in the
/// section's own name or description keeps it.
pub fn section_hidden(section: &Section, query: &str, only_matching: bool) -> bool {
    if query.is_empty() || section_hits(section, query) > 0 {
        return false;
    }
    if only_matching {
        return true;
    }
    !format!("{} {}", section.title, section.description).to_lowercase().contains(query)
}

/// How many settings the sidebar counts for a section: the ones with something to show.
pub fn field_count(section: &Section) -> usize {
    section.fields.iter().filter(|f| !matches!(f.kind, FieldType::Widget | FieldType::Hidden)).count()
}

fn enabled(data: &Map<String, Value>, section: &str) -> bool {
    data.get(section).and_then(|s| s.get("enabled")).is_none_or(truthy)
}

fn combo_in(data: &Map<String, Value>, section: &str, key: &str) -> String {
    let value = data.get(section).and_then(|s| s.get(key));
    keys::normalize(&value.map(text_of).unwrap_or_default())
}

/// What else uses `combo` right now: another hotkey field on these pages (a module that is switched off does
/// not count), or what `elsewhere` knows of (dashboard items, a theme's own key). Empty when nothing.
pub fn live_clash(
    schema: &Schema,
    data: &Map<String, Value>,
    own: (&str, &str),
    combo: &str,
    elsewhere: &dyn Fn(&str) -> Option<String>,
) -> String {
    if combo.is_empty() {
        return String::new();
    }
    for section in &schema.sections {
        if !enabled(data, &section.key) {
            continue;
        }
        for field in section.fields.iter().filter(|f| f.kind == FieldType::Hotkey) {
            if (section.key.as_str(), field.key.as_str()) == own {
                continue;
            }
            // Escape cancels a doodle, so its other keys may share it
            if section.key == own.0 && own.0 == "doodle" && combo.contains("esc") {
                continue;
            }
            if combo_in(data, &section.key, &field.key) == combo {
                return format!("Also used by {} → {}", section.title, field.label);
            }
        }
    }
    elsewhere(combo).map(|other| format!("Also used by {other}")).unwrap_or_default()
}

/// Hotkeys used twice or not valid, as the sentences Save & Apply lists (`_conflicts`). `others` are the
/// dashboard's item keys as (combo, what).
pub fn conflicts(schema: &Schema, data: &Map<String, Value>, others: &[(String, String)]) -> Vec<String> {
    let os = Os::current();
    let mut seen: HashMap<String, String> = HashMap::new();
    let mut problems = Vec::new();
    for section in &schema.sections {
        if data.get(&section.key).and_then(|s| s.get("enabled")).is_some_and(|e| !truthy(e)) {
            continue;
        }
        for field in section.fields.iter().filter(|f| f.kind == FieldType::Hotkey) {
            let combo = combo_in(data, &section.key, &field.key);
            if combo.is_empty() {
                continue;
            }
            let label = format!("{} → {}", section.title, field.label);
            let invalid = keys::validate(&combo, os);
            if !invalid.is_empty() {
                problems.push(format!("{label}: '{combo}' is not a valid hotkey ({invalid})"));
                continue;
            }
            if let Some(first) = seen.get(&combo) {
                problems.push(format!("{} is used by both {first} and {label}", keys::pretty(&combo, os)));
            }
            seen.insert(combo, label);
        }
    }
    for (combo, label) in others {
        if let Some(first) = seen.get(combo) {
            problems.push(format!("{} is used by both {first} and {label}", keys::pretty(combo, os)));
        }
        seen.insert(combo.clone(), label.clone());
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> &'static Schema {
        Schema::get()
    }

    fn field<'a>(dotted: &str) -> (&'a Section, &'a Field) {
        let (section, key) = dotted.split_once('.').expect("a dotted key");
        let section = schema().section(section).expect("a known section");
        (section, section.field(key).expect("a known field"))
    }

    #[test]
    fn values_come_back_as_their_widget_gives_them() {
        let (s, f) = field("ui.overlay_hotkey");
        assert_eq!(normalise(s, f, &json!("Ctrl + Alt+Space")), json!("ctrl+alt+space"));
        let (s, f) = field("dashboard.group_delay_ms");
        assert_eq!(normalise(s, f, &json!(999_999)), json!(f.max as i64));
        assert_eq!(normalise(s, f, &json!(-5)), json!(f.min as i64));
        let (s, f) = field("dashboard.terminal");
        assert_eq!(normalise(s, f, &json!("  kitty ")), json!("kitty"));
        let (s, f) = field("general.tray_messages");
        assert_eq!(normalise(s, f, &json!(1)), json!(true));
    }

    #[test]
    fn an_unknown_choice_becomes_the_first_one() {
        let (s, f) = field("general.notify_style");
        assert_eq!(normalise(s, f, &json!("nonsense")), json!("luminescent"));
        assert_eq!(normalise(s, f, &json!("system")), json!("system"));
    }

    #[test]
    fn opacity_rounds_halves_to_even_like_python() {
        let (s, f) = field("ui.overlay_dim");
        assert_eq!(normalise(s, f, &json!(0.125)), json!(0.12));
        assert_eq!(normalise(s, f, &json!(0.92)), json!(0.92));
        assert_eq!(normalise(s, f, &json!(7)), json!(1.0));
    }

    #[test]
    fn the_switcher_keeps_dashboard_and_settings() {
        let state = NavState::from_value(&["notes".to_owned(), "bogus".to_owned(), "notes".to_owned()]);
        assert_eq!(state.value(), ["notes", "dashboard", "settings"]);
        assert_eq!(state.keys[0], "notes");
        let mut state = state;
        state.toggle("dashboard", false);
        state.toggle("themes", true);
        state.shift(0, 1);
        assert_eq!(state.value(), ["notes", "themes", "settings"]);
    }

    #[test]
    fn lists_keep_the_order_of_the_choices() {
        let (s, f) = field("general.tray_items");
        let value = normalise(s, f, &json!(["quit", "open", "nonsense"]));
        assert_eq!(value, json!(["open", "quit"]));
    }

    #[test]
    fn search_finds_names_and_help_texts() {
        let section = schema().section("general").expect("general");
        assert!(section_hits(section, "tray") > 0);
        let field = section.field("run_at_startup").expect("run_at_startup");
        assert!(field_hit(field, "autostart"));
        assert!(!field_hit(field, "zzzz"));
        assert!(section_hidden(section, "zzzz", false));
        // a match in the section's own name keeps it unless only matching settings are wanted
        assert!(!section_hidden(section, "launcher-wide", false));
        assert!(section_hidden(section, "launcher-wide", true));
    }

    #[test]
    fn draft_counts_changes_in_built_sections_only() {
        let saved = schema().defaults(&lum_core::settings::DefaultPlaces::placeholders());
        let mut draft = Draft::new(schema(), &saved);
        assert_eq!(draft.changed_count(), 0);
        draft.build(0);
        draft.set("general", "tray_messages", json!(false));
        draft.set("notes", "enabled", json!(false));
        assert_eq!(draft.changed_count(), 1, "notes was never shown, so it has no widgets to change");
        assert_eq!(draft.edited_sections(), ["general"]);
        draft.mark_saved();
        assert!(!draft.is_dirty());
    }

    #[test]
    fn a_hotkey_used_twice_is_found_unless_its_module_is_off() {
        let saved = schema().defaults(&lum_core::settings::DefaultPlaces::placeholders());
        let mut data = saved.clone();
        put(&mut data, "general", "settings_hotkey", json!("ctrl+u"));
        let problems = conflicts(schema(), &data, &[]);
        assert!(problems.iter().any(|p| p.contains("Ctrl+U is used by both")), "{problems:?}");

        let clash = live_clash(schema(), &data, ("general", "settings_hotkey"), "ctrl+u", &|_| None);
        assert_eq!(clash, "Also used by General → Quit everything hotkey");

        let elsewhere = |c: &str| (c == "f9").then(|| "Dashboard → Tools → Run".to_owned());
        let clash = live_clash(schema(), &data, ("general", "settings_hotkey"), "f9", &elsewhere);
        assert_eq!(clash, "Also used by Dashboard → Tools → Run");
    }
}
