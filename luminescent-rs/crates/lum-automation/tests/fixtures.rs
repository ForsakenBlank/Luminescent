#![allow(clippy::unwrap_used)]
//! The Python app's own answers: compare(), the time and day parsers, numbers, token expansion and how a saved
//! file is repaired.

use std::collections::HashMap;

use jiff::civil::DateTime;
use lum_automation::{doc, text};
use serde_json::{Map, Value};

#[test]
fn compare_matches_python() {
    let cases = lum_fixtures::read_json("automation_compare_cases.json");
    for case in cases["compare"].as_array().unwrap() {
        let (a, op, b) =
            (case["a"].as_str().unwrap(), case["op"].as_str().unwrap(), case["b"].as_str().unwrap());
        assert_eq!(text::compare(a, op, b), case["result"].as_bool().unwrap(), "{a:?} {op} {b:?}");
    }
}

#[test]
fn days_and_times_match_python() {
    let cases = lum_fixtures::read_json("automation_compare_cases.json");
    for case in cases["days"].as_array().unwrap() {
        let want: Vec<u8> =
            case["days"].as_array().unwrap().iter().map(|d| d.as_u64().unwrap() as u8).collect();
        assert_eq!(text::days(case["text"].as_str().unwrap()), want, "{}", case["text"]);
    }
    for case in cases["hm"].as_array().unwrap() {
        let want =
            case["result"].as_array().map(|r| (r[0].as_u64().unwrap() as u8, r[1].as_u64().unwrap() as u8));
        assert_eq!(text::hm(case["text"].as_str().unwrap()), want, "{}", case["text"]);
    }
}

#[test]
fn numbers_are_written_like_python() {
    let cases = lum_fixtures::read_json("automation_compare_cases.json");
    for case in cases["number_fmt"].as_array().unwrap() {
        let x = case["value"].as_f64().unwrap();
        assert_eq!(text::fmt_number(x), case["text"].as_str().unwrap(), "{x}");
    }
}

#[test]
fn tokens_expand_like_python() {
    let fixture = lum_fixtures::read_json("automation_expand_cases.json");
    let now: DateTime = fixture["now"].as_str().unwrap().parse().unwrap();
    let vars: Map<String, Value> = fixture["vars"].as_object().unwrap().clone();
    let clip = fixture["clipboard"].as_str().unwrap().to_owned();
    for case in fixture["cases"].as_array().unwrap() {
        let (t, input) = (case["text"].as_str().unwrap(), case["input"].as_str().unwrap());
        let mut clipboard = || clip.clone();
        assert_eq!(
            text::expand(t, input, now, &vars, &mut clipboard),
            case["result"].as_str().unwrap(),
            "{t:?}"
        );
    }
}

#[test]
fn saved_files_are_repaired_like_python() {
    for case in lum_fixtures::read_json("automation_load_cases.json").as_array().unwrap() {
        let saved = (!case["saved"].is_null()).then(|| case["saved"].clone());
        let ours = doc::load(saved.clone());
        let theirs = case["result"].clone();
        let mut ids = HashMap::new();
        let known = saved.map(|s| s.to_string()).unwrap_or_default();
        same(&ours, &theirs, &known, &mut ids, case["name"].as_str().unwrap());
    }
}

/// Compares two documents, key order included. Ids the Python app made up (not in the saved file) only have to
/// line up with the ones made here: the same new id in the same places.
fn same(ours: &Value, theirs: &Value, known: &str, ids: &mut HashMap<String, String>, at: &str) {
    match (ours, theirs) {
        (Value::Object(a), Value::Object(b)) => {
            let (ka, kb): (Vec<_>, Vec<_>) = (a.keys().collect(), b.keys().collect());
            assert_eq!(ka, kb, "keys at {at}");
            for (k, v) in b {
                same(&a[k], v, known, ids, &format!("{at}.{k}"));
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "length at {at}");
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                same(x, y, known, ids, &format!("{at}[{i}]"));
            }
        }
        (Value::String(a), Value::String(b)) if a != b => {
            let made_up =
                b.len() == 10 && b.bytes().all(|c| c.is_ascii_hexdigit()) && !known.contains(b.as_str());
            assert!(made_up, "{a:?} != {b:?} at {at}");
            let mapped = ids.entry(b.clone()).or_insert_with(|| a.clone());
            assert_eq!(mapped, a, "id {b} lines up with two ids at {at}");
        }
        _ => assert_eq!(ours, theirs, "at {at}"),
    }
}

#[test]
fn the_sample_file_saves_back_byte_for_byte() {
    let original = lum_fixtures::read_text("sample_home/data/automation.json");
    let engine = lum_automation::Engine::new(
        Some(serde_json::from_str(&original).unwrap()),
        std::sync::Arc::new(lum_automation::SystemClock::default()),
        std::sync::Arc::new(Silent),
        lum_automation::Config::default(),
    );
    assert_eq!(engine.save_text(), original);
}

struct Silent;

impl lum_automation::Host for Silent {
    fn send(&self, _out: lum_automation::Out) {}
}
