//! What each node type looks like: its ports, fields and help text.
//!
//! The list is the Python app's own (`@node(...)` in `core/automation.py`), dumped to
//! `fixtures/automation_nodes.json`, so titles, labels and help texts can not drift from it. The dump was made on
//! Linux; the four help texts that read differently on Windows are swapped in here.

use std::sync::OnceLock;

use serde_json::Value;

const NODES_JSON: &str = include_str!("../../../fixtures/automation_nodes.json");

/// The colour of each category's header, and its name in the node picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
    pub id: String,
    pub label: String,
    pub colour: String,
}

/// What a choice field offers: a fixed list, or one the app fills in (themes, dashboard items...).
#[derive(Debug, Clone, PartialEq)]
pub enum Choices {
    Fixed(Vec<(String, String)>),
    /// The Python function's name (`_lum_actions`, `_themes`...).
    Dynamic(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldSpec {
    pub key: String,
    pub label: String,
    /// text, multiline, number, bool, choice, hotkey, time, date, file or folder.
    pub kind: String,
    pub default: Value,
    pub choices: Choices,
    /// It can take its value from a wire (a ◆ on its left).
    pub pin: bool,
    pub hint: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeSpec {
    pub id: String,
    pub title: String,
    pub category: String,
    pub ins: Vec<String>,
    pub outs: Vec<String>,
    pub values: Vec<String>,
    pub fields: Vec<FieldSpec>,
    pub help: String,
    pub width: u32,
    /// Its status line moves by itself (a countdown), so an open editor redraws it.
    pub live: bool,
}

impl NodeSpec {
    pub fn field(&self, key: &str) -> Option<&FieldSpec> {
        self.fields.iter().find(|f| f.key == key)
    }

    pub fn is_trigger(&self) -> bool {
        self.category == "trigger"
    }

    /// The fields with their defaults, in order (`default_fields`).
    pub fn default_fields(&self) -> serde_json::Map<String, Value> {
        self.fields.iter().map(|f| (f.key.clone(), f.default.clone())).collect()
    }
}

#[derive(Debug)]
pub struct Specs {
    pub categories: Vec<Category>,
    pub compare_ops: Vec<(String, String)>,
    pub nodes: Vec<NodeSpec>,
}

pub fn specs() -> &'static Specs {
    static SPECS: OnceLock<Specs> = OnceLock::new();
    SPECS.get_or_init(|| parse(NODES_JSON))
}

pub fn spec(type_id: &str) -> Option<&'static NodeSpec> {
    specs().nodes.iter().find(|n| n.id == type_id)
}

pub fn category(id: &str) -> Option<&'static Category> {
    specs().categories.iter().find(|c| c.id == id)
}

fn parse(text: &str) -> Specs {
    let root: Value = serde_json::from_str(text).expect("the node list is part of the build and valid JSON");
    let categories = list(&root["categories"])
        .iter()
        .map(|c| Category { id: s(&c["id"]), label: s(&c["label"]), colour: s(&c["colour"]) })
        .collect();
    let compare_ops = pairs(&root["compare_ops"]);
    let nodes = list(&root["nodes"]).iter().map(node).collect();
    Specs { categories, compare_ops, nodes }
}

fn node(n: &Value) -> NodeSpec {
    let id = s(&n["id"]);
    let help = platform_help(&id, s(&n["help"]));
    NodeSpec {
        title: s(&n["title"]),
        category: s(&n["category"]),
        ins: strings(&n["ins"]),
        outs: strings(&n["outs"]),
        values: strings(&n["values"]),
        fields: list(&n["fields"]).iter().map(field).collect(),
        help,
        width: n["width"].as_u64().unwrap_or(260) as u32,
        live: n["live"].as_bool().unwrap_or(false),
        id,
    }
}

fn field(f: &Value) -> FieldSpec {
    let choices = match &f["choices"] {
        Value::Object(o) => Choices::Dynamic(o.get("dynamic").map(s).unwrap_or_default()),
        other => Choices::Fixed(pairs(other)),
    };
    FieldSpec {
        key: s(&f["key"]),
        label: s(&f["label"]),
        kind: s(&f["kind"]),
        default: f["default"].clone(),
        choices,
        pin: f["pin"].as_bool().unwrap_or(false),
        hint: s(&f["hint"]),
    }
}

#[cfg(windows)]
fn platform_help(id: &str, linux: String) -> String {
    match id {
        "window" => linux.trim_end_matches(" Needs Hyprland.").to_owned(),
        "speak" => {
            "Windows reads it with its own voice. Linux needs spd-say (speech-dispatcher) or espeak-ng."
                .into()
        }
        "type_text" => "Types into the window that has the keyboard.".into(),
        "wallpaper" => linux
            .replace("GIFs play with swww, videos with mpvpaper.", "Windows shows the first frame of a GIF."),
        _ => linux,
    }
}

#[cfg(not(windows))]
fn platform_help(_id: &str, linux: String) -> String {
    linux
}

fn list(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_owned()
}

fn strings(v: &Value) -> Vec<String> {
    list(v).iter().map(s).collect()
}

fn pairs(v: &Value) -> Vec<(String, String)> {
    list(v).iter().map(|p| (s(&p[0]), s(&p[1]))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_node_type_is_there() {
        let specs = specs();
        assert_eq!(specs.nodes.len(), 65);
        let count = |cat: &str| specs.nodes.iter().filter(|n| n.category == cat).count();
        assert_eq!(
            (count("trigger"), count("action"), count("logic"), count("value"), count("other")),
            (13, 30, 10, 11, 1)
        );
        assert_eq!(specs.categories[1].label, "Actions");
        assert_eq!(specs.compare_ops[10], ("ge".to_owned(), "≥".to_owned()));
    }

    #[test]
    fn fields_keep_their_defaults_and_choices() {
        let interval = spec("interval").unwrap();
        assert_eq!(interval.title, "Every…");
        assert!(interval.live);
        assert_eq!(interval.field("seconds").unwrap().default, serde_json::json!(5));
        assert!(interval.field("seconds").unwrap().pin);
        let action = spec("lum_action").unwrap().field("action").unwrap();
        assert_eq!(action.choices, Choices::Dynamic("_lum_actions".into()));
        assert_eq!(spec("hotkey").unwrap().width, 330);
        let fields = spec("notify").unwrap().default_fields();
        assert_eq!(fields.keys().collect::<Vec<_>>(), ["title", "message", "style"]);
    }
}
