//! `data/automation.json`, kept as the JSON the Python app wrote:
//!
//! ```text
//! {"version": 1, "active": workspace id, "vars": {name: text},
//!  "workspaces": [{"id", "name", "enabled", "view": {"x", "y", "zoom"},
//!                  "nodes": [{"id", "type", "x", "y", "title", "off", "fields": {key: value}, "state": {}}],
//!                  "wires": [{"id", "src": [node id, port], "dst": [node id, port]}]}]}
//! ```
//!
//! Like the themes and the dashboard (D32, D39) it stays a `serde_json` document with helpers, so keys this
//! version does not know and the order of keys survive a save; the sample file saves back byte for byte.

use serde_json::{Map, Value, json};

use crate::spec;

/// A new id like Python's `uuid4().hex[:10]`.
pub fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..10].to_owned()
}

/// Reads a saved file (`None` when there is none) and repairs it the way `_load` does: the first start gets
/// the switched-off example, node types this version does not know and wires to missing nodes are dropped,
/// and every field missing from a node gets its default.
pub fn load(saved: Option<Value>) -> Value {
    let mut d = match saved {
        Some(Value::Object(d)) if d.get("workspaces").is_some_and(Value::is_array) => d,
        _ => {
            let mut d = Map::new();
            d.insert("version".into(), json!(1));
            d.insert("vars".into(), json!({}));
            d.insert("workspaces".into(), json!([example()]));
            d
        }
    };
    let mut workspaces: Vec<Value> =
        d["workspaces"].as_array().into_iter().flatten().filter(|w| w.is_object()).cloned().collect();
    if workspaces.is_empty() {
        workspaces.push(blank("Workspace"));
    }
    for ws in &mut workspaces {
        repair_workspace(ws.as_object_mut().expect("only objects were kept"));
    }
    d.insert("workspaces".into(), Value::Array(workspaces));
    if !d.get("vars").is_some_and(Value::is_object) {
        d.insert("vars".into(), json!({}));
    }
    let first = d["workspaces"][0]["id"].clone();
    let active_ok =
        d["workspaces"].as_array().into_iter().flatten().any(|w| Some(&w["id"]) == d.get("active"));
    if !active_ok {
        d.insert("active".into(), first);
    }
    Value::Object(d)
}

fn set_default(map: &mut Map<String, Value>, key: &str, value: impl FnOnce() -> Value) {
    if !map.contains_key(key) {
        map.insert(key.to_owned(), value());
    }
}

fn repair_workspace(ws: &mut Map<String, Value>) {
    set_default(ws, "id", || json!(new_id()));
    set_default(ws, "name", || json!("Workspace"));
    set_default(ws, "enabled", || json!(true));
    set_default(ws, "view", || json!({"x": 0, "y": 0, "zoom": 1.0}));
    let nodes: Vec<Value> = ws
        .get("nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|n| n.get("type").and_then(Value::as_str).and_then(spec::spec).is_some())
        .cloned()
        .collect();
    ws.insert("nodes".into(), Value::Array(nodes));
    let mut ids = Vec::new();
    for n in ws["nodes"].as_array_mut().expect("just set") {
        let n = n.as_object_mut().expect("only objects have a type");
        set_default(n, "id", || json!(new_id()));
        ids.push(n["id"].clone());
    }
    for n in ws["nodes"].as_array_mut().expect("just set") {
        repair_node(n.as_object_mut().expect("only objects have a type"));
    }
    let wire_ok = |w: &Value| {
        let end = |key: &str| w.get(key).and_then(|e| e.get(0)).is_some_and(|id| ids.contains(id));
        w.is_object() && end("src") && end("dst")
    };
    let wires: Vec<Value> = ws
        .get("wires")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|w| wire_ok(w))
        .cloned()
        .collect();
    ws.insert("wires".into(), Value::Array(wires));
    for w in ws["wires"].as_array_mut().expect("just set") {
        set_default(w.as_object_mut().expect("only objects were kept"), "id", || json!(new_id()));
    }
}

fn repair_node(n: &mut Map<String, Value>) {
    set_default(n, "x", || json!(0));
    set_default(n, "y", || json!(0));
    set_default(n, "title", || json!(""));
    set_default(n, "off", || json!(false));
    let spec =
        n.get("type").and_then(Value::as_str).and_then(spec::spec).expect("unknown types were dropped");
    let mut fields = spec.default_fields();
    if let Some(Value::Object(saved)) = n.get("fields") {
        for (k, v) in saved {
            fields.insert(k.clone(), v.clone());
        }
    }
    n.insert("fields".into(), Value::Object(fields));
    if !n.get("state").is_some_and(Value::is_object) {
        n.insert("state".into(), json!({}));
    }
}

pub fn blank(name: &str) -> Value {
    json!({"id": new_id(), "name": name, "enabled": true, "view": {"x": 0, "y": 0, "zoom": 1.0}, "nodes": [], "wires": []})
}

fn new_node(type_id: &str, x: i64, y: i64, fields: &[(&str, Value)]) -> Value {
    let mut all = spec::spec(type_id).map(|s| s.default_fields()).unwrap_or_default();
    for (k, v) in fields {
        all.insert((*k).to_owned(), v.clone());
    }
    json!({"id": new_id(), "type": type_id, "x": x, "y": y, "title": "", "off": false, "state": {}, "fields": all})
}

/// A node for the editor to add, at that spot, with its fields at their defaults.
pub fn node(type_id: &str, x: f64, y: f64) -> Value {
    let mut n = new_node(type_id, 0, 0, &[]);
    n["x"] = json!(x.round() as i64);
    n["y"] = json!(y.round() as i64);
    n
}

/// The first start: a small example, switched off, to show how wiring works.
pub fn example() -> Value {
    let mut ws = blank("Example");
    ws["enabled"] = json!(false);
    let a = new_node("interval", -420, -120, &[("seconds", json!(60))]);
    let b = new_node("now", -420, 120, &[("format", json!("%H:%M"))]);
    let c = new_node(
        "notify",
        -40,
        -120,
        &[("title", json!("Automation")), ("message", json!("Another minute went by."))],
    );
    let mut note = new_node("comment", 340, -120, &[]);
    note["fields"] = json!({"text": "This workspace is off (the switch at the bottom). Turn it on and a card pops up every minute.\n\nDrag from a port to another to wire them. Right-click or double-click the space to add nodes."});
    ws["wires"] = json!([
        {"id": new_id(), "src": [a["id"], "out:tick"], "dst": [c["id"], "in:run"]},
        {"id": new_id(), "src": [b["id"], "val:text"], "dst": [c["id"], "field:title"]},
    ]);
    ws["nodes"] = json!([a, b, c, note]);
    ws
}

/// A copy of a workspace with new ids everywhere, its saved node state dropped and switched off, as Python's
/// `add_ws(copy_from=...)` makes it (duplicating, importing a `.lumflow`).
pub fn copy_workspace(from: &Value, name: &str) -> Value {
    let mut ws = from.clone();
    ws["id"] = json!(new_id());
    ws["name"] = json!(name);
    let mut remap = Map::new();
    for n in ws["nodes"].as_array_mut().into_iter().flatten() {
        let fresh = new_id();
        if let Some(old) = n["id"].as_str() {
            remap.insert(old.to_owned(), json!(fresh));
        }
        n["id"] = json!(fresh);
        n["state"] = json!({});
    }
    for w in ws["wires"].as_array_mut().into_iter().flatten() {
        w["id"] = json!(new_id());
        for end in ["src", "dst"] {
            let old = w[end][0].as_str().unwrap_or_default().to_owned();
            if let Some(new) = remap.get(&old) {
                w[end][0] = new.clone();
            }
        }
    }
    ws["enabled"] = json!(false);
    ws
}

/// A workspace read from a `.lumflow` file, made whole first (`{**self._blank(""), **ws}`), so a hand-written
/// file with only nodes still opens.
pub fn imported_workspace(file: &Value) -> Option<Value> {
    let Value::Object(given) = file else { return None };
    let mut ws = blank("").as_object().cloned().expect("blank is an object");
    for (k, v) in given {
        ws.insert(k.clone(), v.clone());
    }
    let name =
        given.get("name").and_then(Value::as_str).filter(|n| !n.is_empty()).unwrap_or("Imported").to_owned();
    let mut ws = Value::Object(ws);
    if !ws["nodes"].is_array() || !ws["wires"].is_array() {
        return None;
    }
    let mut repaired = copy_workspace(&ws, &name);
    repair_workspace(repaired.as_object_mut().expect("still an object"));
    ws = repaired;
    Some(ws)
}

pub fn workspaces(doc: &Value) -> &[Value] {
    doc["workspaces"].as_array().map(Vec::as_slice).unwrap_or(&[])
}

pub fn workspace<'a>(doc: &'a Value, wid: &str) -> Option<&'a Value> {
    workspaces(doc).iter().find(|w| w["id"] == wid)
}

pub fn workspace_mut<'a>(doc: &'a mut Value, wid: &str) -> Option<&'a mut Value> {
    doc["workspaces"].as_array_mut()?.iter_mut().find(|w| w["id"] == wid)
}

pub fn node_in<'a>(ws: &'a Value, nid: &str) -> Option<&'a Value> {
    ws["nodes"].as_array()?.iter().find(|n| n["id"] == nid)
}

pub fn node_in_mut<'a>(ws: &'a mut Value, nid: &str) -> Option<&'a mut Value> {
    ws["nodes"].as_array_mut()?.iter_mut().find(|n| n["id"] == nid)
}

pub fn str_of<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_get_new_ids_and_keep_their_wiring() {
        let ex = example();
        let copy = copy_workspace(&ex, "Copy");
        assert_ne!(copy["id"], ex["id"]);
        assert_eq!(copy["enabled"], json!(false));
        let ids: Vec<&Value> = copy["nodes"].as_array().unwrap().iter().map(|n| &n["id"]).collect();
        for w in copy["wires"].as_array().unwrap() {
            assert!(ids.contains(&&w["src"][0]) && ids.contains(&&w["dst"][0]));
        }
        assert!(!ids.contains(&&ex["nodes"][0]["id"]));
    }

    #[test]
    fn an_import_with_only_nodes_still_opens() {
        let file = json!({"nodes": [{"id": "a", "type": "show"}, {"id": "b", "type": "gone"}], "wires": []});
        let ws = imported_workspace(&file).unwrap();
        assert_eq!(ws["name"], "Imported");
        assert_eq!(ws["nodes"].as_array().unwrap().len(), 1);
        assert_eq!(ws["nodes"][0]["title"], "");
        assert!(imported_workspace(&json!({"nodes": "no"})).is_none());
    }
}
