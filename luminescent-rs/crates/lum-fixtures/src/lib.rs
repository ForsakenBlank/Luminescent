//! Reads the golden fixtures in `luminescent-rs/fixtures`, made by `tools/make_fixtures.py` from the Python app.
//!
//! The generator writes a few things in its own way (see `fixtures/README.md`): numbers that must survive
//! exactly as `{"int": "..."}` or `{"float": "repr"}`, byte strings as lower-case hex, and a case that raised in
//! Python as `{"error": type, "message": text}` instead of `{"ok": value}`. Scratch paths are written as
//! `<SCRATCH>`, `<HOME>` or `<LUMINESCENT_HOME>`. This crate turns all of that back into Rust values so every
//! crate's tests read fixtures the same way.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// The `luminescent-rs/fixtures` folder.
pub fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

/// A file or folder inside `fixtures/`, given with forward slashes.
pub fn path(rel: &str) -> PathBuf {
    rel.split('/').fold(dir(), |p, part| p.join(part))
}

pub fn read_text(rel: &str) -> String {
    let p = path(rel);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("fixture {} could not be read: {e}", p.display()))
}

pub fn read_bytes(rel: &str) -> Vec<u8> {
    let p = path(rel);
    fs::read(&p).unwrap_or_else(|e| panic!("fixture {} could not be read: {e}", p.display()))
}

pub fn read_json(rel: &str) -> Value {
    serde_json::from_str(&read_text(rel)).unwrap_or_else(|e| panic!("fixture {rel} is not JSON: {e}"))
}

/// Any JSON file, inside `fixtures/` or not (a test's temp folder).
pub fn read_json_file(path: &Path) -> Value {
    let text =
        fs::read_to_string(path).unwrap_or_else(|e| panic!("{} could not be read: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{} is not JSON: {e}", path.display()))
}

/// A number written as `{"int": "..."}` or `{"float": "repr"}`.
#[derive(Debug, Clone, PartialEq)]
pub enum Exact {
    Int(i128),
    Float(f64),
}

impl Exact {
    /// Reads an exact number, or a plain JSON number. `None` for anything else.
    pub fn from_value(v: &Value) -> Option<Exact> {
        if let Some(obj) = v.as_object()
            && obj.len() == 1
        {
            if let Some(text) = obj.get("int").and_then(Value::as_str) {
                return text.parse().ok().map(Exact::Int);
            }
            if let Some(text) = obj.get("float").and_then(Value::as_str) {
                return parse_python_float(text).map(Exact::Float);
            }
            return None;
        }
        if let Some(i) = v.as_i64() {
            return Some(Exact::Int(i.into()));
        }
        v.as_f64().map(Exact::Float)
    }

    pub fn as_f64(&self) -> f64 {
        match *self {
            Exact::Int(i) => i as f64,
            Exact::Float(f) => f,
        }
    }
}

/// Python's float repr, including `inf`, `-inf` and `nan`.
pub fn parse_python_float(text: &str) -> Option<f64> {
    match text {
        "inf" => Some(f64::INFINITY),
        "-inf" => Some(f64::NEG_INFINITY),
        "nan" => Some(f64::NAN),
        _ => text.parse().ok(),
    }
}

/// Lower-case hex as the generator writes byte strings.
pub fn hex_bytes(text: &str) -> Vec<u8> {
    assert!(text.len().is_multiple_of(2), "odd-length hex string in a fixture");
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("fixture hex is valid"))
        .collect()
}

/// What a fixture case says the Python code did.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Ok(Value),
    Error { kind: String, message: String },
}

impl Outcome {
    /// Reads `{"ok": ...}` or `{"error": ..., "message": ...}` from a case object.
    pub fn of(case: &Value) -> Outcome {
        if let Some(kind) = case.get("error").and_then(Value::as_str) {
            let message = case.get("message").and_then(Value::as_str).unwrap_or_default();
            return Outcome::Error { kind: kind.to_owned(), message: message.to_owned() };
        }
        match case.get("ok") {
            Some(v) => Outcome::Ok(v.clone()),
            None => panic!("fixture case has neither ok nor error: {case}"),
        }
    }
}

/// Swaps placeholders such as `<SCRATCH>` for real paths everywhere in a value, keys included.
pub fn replace_placeholders(value: &Value, swaps: &[(&str, &str)]) -> Value {
    let swap = |s: &str| swaps.iter().fold(s.to_owned(), |acc, (from, to)| acc.replace(from, to));
    match value {
        Value::String(s) => Value::String(swap(s)),
        Value::Array(items) => Value::Array(items.iter().map(|v| replace_placeholders(v, swaps)).collect()),
        Value::Object(map) => {
            Value::Object(map.iter().map(|(k, v)| (swap(k), replace_placeholders(v, swaps))).collect())
        }
        other => other.clone(),
    }
}

/// JSON text with every space, tab and newline outside strings removed, for comparing files that only differ
/// in indentation.
pub fn without_whitespace(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    let mut in_string = false;
    let mut escaped = false;
    for c in json.chars() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
            out.push(c);
        } else if !c.is_whitespace() {
            out.push(c);
        }
    }
    out
}

/// Copies a fixture folder into `dest` (made if needed), for tests that change files.
pub fn copy_dir(rel: &str, dest: &Path) {
    copy_tree(&path(rel), dest);
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("test folder can be made");
    for entry in fs::read_dir(from).expect("fixture folder can be listed") {
        let entry = entry.expect("fixture entry can be read");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("fixture file can be copied");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn exact_numbers() {
        assert_eq!(
            Exact::from_value(&json!({"int": "123456789012345678901"})),
            Some(Exact::Int(123456789012345678901))
        );
        assert_eq!(Exact::from_value(&json!({"float": "0.1"})), Some(Exact::Float(0.1)));
        assert_eq!(Exact::from_value(&json!({"float": "-inf"})), Some(Exact::Float(f64::NEG_INFINITY)));
        assert_eq!(Exact::from_value(&json!(3)), Some(Exact::Int(3)));
        assert_eq!(Exact::from_value(&json!("x")), None);
    }

    #[test]
    fn hex() {
        assert_eq!(hex_bytes("00ff10"), vec![0, 255, 16]);
    }

    #[test]
    fn outcomes() {
        assert_eq!(Outcome::of(&json!({"ok": 1})), Outcome::Ok(json!(1)));
        assert_eq!(
            Outcome::of(&json!({"error": "ValueError", "message": "no"})),
            Outcome::Error { kind: "ValueError".into(), message: "no".into() }
        );
    }

    #[test]
    fn whitespace_outside_strings_only() {
        assert_eq!(without_whitespace("{\n  \"a b\": [1, 2]\n}"), "{\"a b\":[1,2]}");
        assert_eq!(without_whitespace(r#"{"q": "x \" y"}"#), r#"{"q":"x \" y"}"#);
    }

    #[test]
    fn fixtures_are_there() {
        assert!(path("settings_defaults.json").is_file());
        assert!(path("sample_home/settings.json").is_file(), "sample_home lost its settings.json");
    }
}
