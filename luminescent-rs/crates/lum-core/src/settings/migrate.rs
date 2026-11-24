//! Settings that moved between versions: `_migrate` from `core/settings.py`, rule for rule.
//!
//! `data` is the merged result and `saved` the file as it was read. The rules look at `saved` to tell an old
//! file from a new one, then change `data`.
//!
//! In Python a section that is not an object (`"ui": "not a dict"`) raises part way through, the load swallows
//! the error and keeps whatever had already changed. `Abort` reproduces that, so a damaged file loads the same
//! in both apps.

use serde_json::{Map, Value};

pub(crate) struct Abort;

type Obj = Map<String, Value>;

pub(crate) fn migrate(data: &mut Obj, saved: &Obj) {
    let _ = run_rules(data, saved);
}

fn run_rules(data: &mut Obj, saved: &Obj) -> Result<(), Abort> {
    let ui = saved_section(saved, "ui")?;

    // The Automation page joins an existing navigation bar once (its jump hotkey came with it).
    if ui.get("nav_order").is_some_and(Value::is_array)
        && !ui.contains_key("hotkey_automation")
        && !contains_str(list_at(data, "ui", "nav_order"), "automation")
    {
        let mut order = list_at(data, "ui", "nav_order").cloned().unwrap_or_default();
        let at = usize::from(order.first().and_then(Value::as_str) == Some("dashboard"));
        order.insert(at, "automation".into());
        set(data, "ui", "nav_order", Value::Array(order));
        let general = saved_section(saved, "general")?;
        if let Some(tray) = general.get("tray_items").and_then(Value::as_array)
            && !contains_str(Some(tray), "automation")
        {
            let mut tray = tray.clone();
            tray.push("automation".into());
            set(data, "general", "tray_items", Value::Array(tray));
        }
    }

    if !ui.get("panel_place_v").is_some_and(truthy) {
        // panels used to reopen where they were last, which was often half off another monitor: they now open
        // in the middle of the screen with the mouse (pick "Where I left them" to get the old way back)
        let place = ui.get("panel_place").cloned().unwrap_or_else(|| "kept".into());
        if place == "kept" {
            set(data, "ui", "panel_place", "centre".into());
        }
        set(data, "ui", "panel_place_v", 1.into());
    }

    if !saved.contains_key("animations") {
        if let Some(animate) = animate_of(saved.get("automation"))? {
            set(data, "animations", "pulses", truthy(&animate).into());
        }
        if ui.get("overlay_screen").and_then(Value::as_str) == Some("primary") {
            set(data, "ui", "overlay_screen", "cursor".into());
        }
    }

    let general = saved_section(saved, "general")?;
    if general.get("tray_items").is_some_and(Value::is_array)
        && !general.contains_key("keep_awake_hotkey")
        && !contains_str(list_at(data, "general", "tray_items"), "keepawake")
    {
        let mut tray = list_at(data, "general", "tray_items").cloned().unwrap_or_default();
        tray.push("keepawake".into());
        set(data, "general", "tray_items", Value::Array(tray));
    }

    // the per-module "show on start" boxes became General → Show on start
    if !general.contains_key("start_panels") {
        let mut panels = Vec::new();
        let notes = saved_section(saved, "notes")?;
        if notes.get("show_on_start").is_none_or(truthy) {
            panels.push(Value::from("notes"));
        }
        let hearing = saved_section(saved, "hearing")?;
        if hearing.get("show_on_start").is_some_and(truthy) {
            panels.push(Value::from("hearing"));
        }
        set(data, "general", "start_panels", Value::Array(panels));
    }
    Ok(())
}

/// `saved.get(key) or {}`: an object, or empty for anything Python counts as false. Anything else would raise.
fn saved_section<'a>(saved: &'a Obj, key: &str) -> Result<SectionRef<'a>, Abort> {
    match saved.get(key) {
        Some(Value::Object(map)) => Ok(SectionRef(Some(map))),
        Some(v) if truthy(v) => Err(Abort),
        _ => Ok(SectionRef(None)),
    }
}

struct SectionRef<'a>(Option<&'a Obj>);

impl SectionRef<'_> {
    fn get(&self, key: &str) -> Option<&Value> {
        self.0.and_then(|m| m.get(key))
    }

    fn contains_key(&self, key: &str) -> bool {
        self.0.is_some_and(|m| m.contains_key(key))
    }
}

/// `if "animate" in auto: ... auto["animate"]` where `auto = saved.get("automation") or {}`. On a string or a
/// list Python's `in` still works, but indexing it with a string then raises.
fn animate_of(automation: Option<&Value>) -> Result<Option<Value>, Abort> {
    match automation {
        Some(Value::Object(map)) => Ok(map.get("animate").cloned()),
        Some(Value::String(s)) if s.contains("animate") => Err(Abort),
        Some(Value::Array(items)) if contains_str(Some(items), "animate") => Err(Abort),
        Some(Value::Number(_) | Value::Bool(true)) => Err(Abort),
        _ => Ok(None),
    }
}

/// Python's truth value of a JSON value.
pub(crate) fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

fn list_at<'a>(data: &'a Obj, section: &str, key: &str) -> Option<&'a Vec<Value>> {
    data.get(section)?.get(key)?.as_array()
}

fn contains_str(list: Option<&Vec<Value>>, item: &str) -> bool {
    list.is_some_and(|l| l.iter().any(|v| v.as_str() == Some(item)))
}

fn set(data: &mut Obj, section: &str, key: &str, value: Value) {
    if let Some(Value::Object(map)) = data.get_mut(section) {
        map.insert(key.to_owned(), value);
    }
}
