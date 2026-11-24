//! Every option, described once: the Python `SCHEMA` from `core/settings.py`, dumped by the fixture generator
//! and embedded here for the system the app is built for (help texts and a few labels differ per system).

use std::sync::OnceLock;

use serde::Deserialize;
use serde_json::{Map, Value};

#[cfg(windows)]
const SCHEMA_JSON: &str = include_str!("../../../../fixtures/settings_schema.windows.json");
#[cfg(not(windows))]
const SCHEMA_JSON: &str = include_str!("../../../../fixtures/settings_schema.linux.json");

/// What the Settings page builds for a field. `Widget` fields are never stored; `Hidden` ones are stored but
/// not shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    Hotkey,
    Text,
    Dir,
    File,
    Int,
    Float,
    Opacity,
    Bool,
    Color,
    Choice,
    Custom,
    Widget,
    Hidden,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Field {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub kind: FieldType,
    pub default: Value,
    pub help: String,
    /// `(value, label)` pairs for a choice field.
    pub choices: Vec<(Value, String)>,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub file_filter: String,
    /// Needs a restart to apply fully.
    pub restart: bool,
}

impl Field {
    pub fn is_stored(&self) -> bool {
        self.kind != FieldType::Widget
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Section {
    pub key: String,
    pub title: String,
    pub icon: String,
    pub description: String,
    pub fields: Vec<Field>,
}

impl Section {
    pub fn field(&self, key: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.key == key)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Schema {
    /// Sections that are the same in every settings profile.
    pub shared_sections: Vec<String>,
    pub default_profile: String,
    pub sections: Vec<Section>,
}

impl Schema {
    /// The schema for this system, parsed once.
    pub fn get() -> &'static Schema {
        static SCHEMA: OnceLock<Schema> = OnceLock::new();
        SCHEMA.get_or_init(|| Schema::parse(SCHEMA_JSON).expect("the embedded settings schema is valid"))
    }

    pub fn parse(json: &str) -> serde_json::Result<Schema> {
        serde_json::from_str(json)
    }

    pub fn section(&self, key: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.key == key)
    }

    pub fn field(&self, dotted: &str) -> Option<&Field> {
        let (section, key) = dotted.split_once('.')?;
        self.section(section)?.field(key)
    }

    pub fn is_shared(&self, section: &str) -> bool {
        self.shared_sections.iter().any(|s| s == section)
    }

    /// What settings.json holds on a first start: every stored field's default, section by section.
    pub fn defaults(&self, places: &DefaultPlaces) -> Map<String, Value> {
        self.sections
            .iter()
            .map(|s| {
                let fields = s
                    .fields
                    .iter()
                    .filter(|f| f.is_stored())
                    .map(|f| (f.key.clone(), places.expand(&f.default)))
                    .collect();
                (s.key.clone(), Value::Object(fields))
            })
            .collect()
    }
}

/// The folders some defaults point into (the snippets folder, the vault...). The schema dump writes them as
/// `<HOME>/Pictures/Clipboard`; this turns that into the real path, with this system's separators, the way
/// Python's `os.path.join(os.path.expanduser("~"), ...)` builds it.
#[derive(Debug, Clone)]
pub struct DefaultPlaces {
    pub home: String,
    /// `$XDG_DATA_HOME` or `~/.local/share` (Linux).
    pub data_home: String,
    /// `%LOCALAPPDATA%` (Windows).
    pub local_app_data: String,
    pub separator: char,
}

impl DefaultPlaces {
    pub fn for_this_system() -> DefaultPlaces {
        let home = crate::paths::user_home();
        let data_home = crate::paths::user_data_dir();
        DefaultPlaces {
            home: crate::paths::display(&home),
            data_home: crate::paths::display(&data_home),
            local_app_data: crate::paths::display(&data_home),
            separator: std::path::MAIN_SEPARATOR,
        }
    }

    /// Leaves the placeholders as the fixtures write them, so tests compare text directly.
    pub fn placeholders() -> DefaultPlaces {
        DefaultPlaces {
            home: "<HOME>".into(),
            data_home: "<HOME>/.local/share".into(),
            local_app_data: "<HOME>/AppData/Local".into(),
            separator: '/',
        }
    }

    fn expand(&self, default: &Value) -> Value {
        match default.as_str() {
            Some(text) if text.starts_with("<HOME>") => Value::String(self.expand_path(text)),
            _ => default.clone(),
        }
    }

    fn expand_path(&self, template: &str) -> String {
        let prefixes = [
            ("<HOME>/AppData/Local", &self.local_app_data),
            ("<HOME>/.local/share", &self.data_home),
            ("<HOME>", &self.home),
        ];
        for (prefix, base) in prefixes {
            if let Some(rest) = template.strip_prefix(prefix) {
                let rest = rest.replace('/', &self.separator.to_string());
                return format!("{base}{rest}");
            }
        }
        template.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_schema_parses() {
        let schema = Schema::get();
        assert_eq!(schema.default_profile, "default");
        assert!(schema.is_shared("general"));
        assert!(!schema.is_shared("notes"));
        assert_eq!(schema.field("ui.overlay_hotkey").map(|f| f.kind), Some(FieldType::Hotkey));
        assert_eq!(schema.field("general.toast_test").map(|f| f.is_stored()), Some(false));
    }

    #[test]
    fn default_paths_use_this_systems_separator() {
        let places = DefaultPlaces {
            home: r"C:\Users\me".into(),
            data_home: String::new(),
            local_app_data: r"C:\Users\me\AppData\Local".into(),
            separator: '\\',
        };
        assert_eq!(places.expand_path("<HOME>/Pictures/Clipboard"), r"C:\Users\me\Pictures\Clipboard");
        assert_eq!(
            places.expand_path("<HOME>/AppData/Local/Microsoft/Windows/Caches/.idx"),
            r"C:\Users\me\AppData\Local\Microsoft\Windows\Caches\.idx"
        );
    }
}
