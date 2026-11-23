//! The settings store against the Python app's recorded behaviour (fixtures/settings_*.json).

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use lum_core::settings::{DefaultPlaces, Schema, Settings, SettingsFiles};
use serde_json::{Map, Value, json};

fn schema_for(os: &str) -> &'static Schema {
    static LINUX: OnceLock<Schema> = OnceLock::new();
    static WINDOWS: OnceLock<Schema> = OnceLock::new();
    let cell = if os == "windows" { &WINDOWS } else { &LINUX };
    cell.get_or_init(|| {
        Schema::parse(&lum_fixtures::read_text(&format!("settings_schema.{os}.json"))).expect("schema parses")
    })
}

fn files_in(home: &Path) -> SettingsFiles {
    SettingsFiles {
        settings: home.join("settings.json"),
        profiles: home.join("data").join("profiles.json"),
        profiles_dir: home.join("data").join("profiles"),
    }
}

/// The Python fixtures were made with the Linux schema (the generator reloads it last).
fn open_linux(home: &Path) -> Settings {
    Settings::open(files_in(home), schema_for("linux"), DefaultPlaces::placeholders())
}

fn text(v: &Value) -> String {
    serde_json::to_string(v).expect("values serialise")
}

#[test]
fn defaults_match_python_on_each_system() {
    let expected = lum_fixtures::read_json("settings_defaults.json");
    for os in ["linux", "windows"] {
        let defaults = Value::Object(schema_for(os).defaults(&DefaultPlaces::placeholders()));
        assert_eq!(text(&defaults), text(&expected[os]), "defaults on {os}");
    }
}

#[test]
fn embedded_schema_is_this_systems_dump() {
    let os = if cfg!(windows) { "windows" } else { "linux" };
    let embedded = Value::Object(Schema::get().defaults(&DefaultPlaces::placeholders()));
    assert_eq!(text(&embedded), text(&lum_fixtures::read_json("settings_defaults.json")[os]));
    assert_eq!(Schema::get().sections.len(), schema_for(os).sections.len());
}

#[test]
fn every_migration_case_loads_like_python() {
    let cases = lum_fixtures::read_json("settings_migration_cases.json");
    for case in cases.as_array().expect("a list of cases") {
        let name = case["name"].as_str().expect("named");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let written_text = match &case["saved"] {
            Value::Null => None,
            Value::String(raw) => Some(raw.clone()),
            saved => Some(saved.to_string()),
        };
        if let Some(t) = &written_text {
            fs::write(&path, t).unwrap();
        }

        let settings = open_linux(dir.path());
        let result = case["result"].as_object().expect("result is an object");
        for (section, values) in result {
            for (key, want) in values.as_object().expect("sections are objects") {
                let got = settings.get(&format!("{section}.{key}"));
                assert_eq!(got, Some(want), "{name}: {section}.{key}");
            }
        }
        // known sections come first, in the schema's order
        let known: Vec<_> = settings.all().keys().take(result.len()).collect();
        assert_eq!(known, result.keys().collect::<Vec<_>>(), "{name}: section order");

        if let Some(saved) = case["saved"].as_object() {
            check_unknown_kept(name, &settings, saved, result);
        }
        match &case["written"] {
            Value::Null => {}
            written => {
                assert_eq!(text(&lum_fixtures::read_json_file(&path)), text(written), "{name}: written")
            }
        }
        if case["saved"].is_string() {
            assert_eq!(
                fs::read_to_string(&path).ok(),
                written_text,
                "{name}: an unreadable file is left alone"
            );
        }
    }
}

/// Python drops what it does not know; the Rust store keeps it.
fn check_unknown_kept(
    name: &str,
    settings: &Settings,
    saved: &Map<String, Value>,
    known: &Map<String, Value>,
) {
    for (section, values) in saved {
        match known.get(section) {
            None => {
                assert_eq!(settings.all().get(section), Some(values), "{name}: unknown section {section}")
            }
            Some(Value::Object(fields)) => {
                for (key, value) in values.as_object().into_iter().flatten() {
                    if !fields.contains_key(key) {
                        let got = settings.get(&format!("{section}.{key}"));
                        assert_eq!(got, Some(value), "{name}: unknown key {section}.{key}");
                    }
                }
            }
            Some(_) => unreachable!("known sections are objects"),
        }
    }
}

#[test]
fn the_unknown_keys_case_is_really_checked() {
    let dir = tempfile::tempdir().unwrap();
    let saved = json!({"general": {"tray_messages": false, "made_up": 1}, "nonsense": {"a": 1}});
    fs::write(dir.path().join("settings.json"), saved.to_string()).unwrap();
    let mut settings = open_linux(dir.path());
    assert_eq!(settings.get("general.made_up"), Some(&json!(1)));
    settings.save(false).unwrap();
    let back = lum_fixtures::read_json_file(&dir.path().join("settings.json"));
    assert_eq!(back["nonsense"], json!({"a": 1}));
    assert_eq!(back["general"]["made_up"], json!(1));
    assert_eq!(back["general"]["tray_messages"], json!(false));
}

#[test]
fn a_damaged_file_is_kept_as_bad_on_the_first_save() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, "{this is not json").unwrap();
    let mut settings = open_linux(dir.path());
    assert_eq!(fs::read_to_string(&path).unwrap(), "{this is not json");
    settings.save(false).unwrap();
    assert_eq!(fs::read_to_string(dir.path().join("settings.json.bad")).unwrap(), "{this is not json");
    assert!(lum_fixtures::read_json_file(&path).is_object());
}

#[test]
fn profile_steps_leave_the_same_files() {
    let fixture = lum_fixtures::read_json("settings_profile_cases.json");
    let shared: Vec<&str> =
        fixture["shared_sections"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    let mut schema_shared = schema_for("linux").shared_sections.clone();
    schema_shared.sort();
    assert_eq!(schema_shared, shared);

    let steps = fixture["steps"].as_array().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    fs::create_dir_all(home.join("data")).unwrap();
    let profiles_file = home.join("data").join("profiles.json");

    let mut s = open_linux(home);
    s.set_id_source(|| "52274616cc".to_owned());
    check_step(&steps[0], &s, home);

    let id = s.create_profile("  Work  ").unwrap();
    check_step(&steps[1], &s, home);

    s.set("doodle.colour", json!("#00FF00"));
    s.set("general.tray_messages", json!(false));
    s.save(false).unwrap();
    check_step(&steps[2], &s, home);

    assert!(s.switch_profile(&id, false).unwrap());
    check_step(&steps[3], &s, home);

    s.set("doodle.colour", json!("#123456"));
    s.save(false).unwrap();
    s.switch_profile("default", false).unwrap();
    check_step(&steps[4], &s, home);

    s.rename_profile(&id, "Office").unwrap();
    assert!(!s.switch_profile("nope", false).unwrap());
    check_step(&steps[5], &s, home);

    s.switch_profile(&id, false).unwrap();
    s.delete_profile(&id).unwrap();
    check_step(&steps[6], &s, home);

    fs::write(&profiles_file, r#"{"active": "gone", "profiles": [{"id": "x", "name": "X"}]}"#).unwrap();
    check_step(&steps[7], &s, home);
    assert_eq!(steps.len(), 8, "a new step was added to the fixture");
}

fn check_step(step: &Value, s: &Settings, home: &Path) {
    let what = step["step"].as_str().unwrap();
    assert_eq!(s.active_profile(), step["active"].as_str().unwrap(), "{what}: active");
    let profiles: Vec<Value> =
        s.profiles().into_iter().map(|p| json!({"id": p.id, "name": p.name})).collect();
    assert_eq!(Value::Array(profiles), step["profiles"], "{what}: profiles");

    let mut on_disk = BTreeMap::new();
    collect_files(home, home, &mut on_disk);
    let expected = step["files"].as_object().unwrap();
    let mut expected_names: Vec<_> = expected.keys().collect();
    expected_names.sort();
    assert_eq!(on_disk.keys().collect::<Vec<_>>(), expected_names, "{what}: file list");
    for (rel, want) in expected {
        assert_eq!(text(&on_disk[rel]), text(want), "{what}: {rel}");
    }
}

fn collect_files(root: &Path, dir: &Path, out: &mut BTreeMap<String, Value>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_files(root, &path, out);
        } else {
            let rel = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            out.insert(rel, lum_fixtures::read_json_file(&path));
        }
    }
}
