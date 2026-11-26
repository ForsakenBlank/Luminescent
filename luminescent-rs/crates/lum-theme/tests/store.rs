//! The theme store against the Python app's files: migrations, the sample data folder, sharing both ways.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::Path;

use lum_core::pyjson::{self, Style};
use lum_fixtures::{copy_dir, path as fixture_path, read_json, read_json_file};
use lum_theme::history::{Recent, note_change};
use lum_theme::store::{BUILTIN_GROUP, MINE_GROUP};
use lum_theme::{Rgb, Theme, ThemeEvent, ThemeStore};
use serde_json::{Map, Value, json};

fn counting_ids() -> Box<dyn FnMut() -> String + Send> {
    let mut n = 0;
    Box::new(move || {
        n += 1;
        format!("{n:010x}")
    })
}

fn open(dir: &Path, raw_settings: &Map<String, Value>) -> ThemeStore {
    ThemeStore::open(dir.join("themes.json"), dir.join("theme-images"), raw_settings, counting_ids())
}

fn none() -> Map<String, Value> {
    Map::new()
}

/// Ids are random in Python; what has to match is where they are and that they are ten characters.
fn without_ids(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, v)| {
                    let v = match (k.as_str(), v) {
                        ("id" | "folder" | "active", Value::String(s))
                            if s.len() == 10 && s.bytes().all(|b| b.is_ascii_hexdigit()) =>
                        {
                            json!("<id>")
                        }
                        _ => without_ids(v),
                    };
                    (k.clone(), v)
                })
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(without_ids).collect()),
        other => other.clone(),
    }
}

#[test]
fn every_migration_case_loads_and_saves_like_python() {
    for case in read_json("themes_migration_cases.json").as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let dir = tempfile::tempdir().unwrap();
        if !case["saved"].is_null() {
            fs::write(dir.path().join("themes.json"), case["saved"].to_string()).unwrap();
        }
        let raw = match case["old_appearance"].as_object() {
            Some(a) => json!({"appearance": a}).as_object().unwrap().clone(),
            None => none(),
        };
        let store = open(dir.path(), &raw);
        store.flush();
        let file = dir.path().join("themes.json");
        let written = if file.exists() { read_json_file(&file) } else { Value::Null };
        assert_eq!(without_ids(&written), without_ids(&case["written"]), "{name}: the file");
        let loaded = read_back(&store);
        assert_eq!(without_ids(&loaded), without_ids(&case["result"]), "{name}: the data in memory");
    }
}

/// The store's data as a document (what a save would write).
fn read_back(store: &ThemeStore) -> Value {
    let mut doc = Map::new();
    let file = store.path().to_owned();
    store.save();
    store.flush();
    if let Ok(text) = fs::read_to_string(&file)
        && let Ok(Value::Object(m)) = serde_json::from_str(&text)
    {
        doc = m;
    }
    Value::Object(doc)
}

#[test]
fn the_theme_pack_is_added_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = open(dir.path(), &none());
    let names: Vec<&str> = store.themes().iter().map(Theme::name).collect();
    assert_eq!(names.len(), 12);
    assert!(names.contains(&"Catppuccin Mocha") && names.contains(&"Material You"));
    let folders = store.folders();
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0].name, "More themes");
    let mocha = store.themes().iter().find(|t| t.name() == "Catppuccin Mocha").unwrap().id().to_owned();
    store.delete(&mocha);
    store.flush();
    let again = open(dir.path(), &none());
    assert_eq!(again.themes().len(), 11, "a deleted pack theme stays deleted");
}

#[test]
fn sample_home_themes_load_and_save_back_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let want = fs::read_to_string(fixture_path("sample_home/data/themes.json")).unwrap();
    fs::write(dir.path().join("themes.json"), &want).unwrap();
    let store = open(dir.path(), &none());
    store.save();
    store.flush();
    let got = fs::read_to_string(dir.path().join("themes.json")).unwrap();
    assert_eq!(got, want);
    let doc: Value = serde_json::from_str(&want).unwrap();
    assert_eq!(pyjson::to_string(&doc, Style::indented(2)), want, "the writer is Python's");
    assert!(!store.themes().is_empty());
}

#[test]
fn applying_remembers_the_theme_before() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = open(dir.path(), &none());
    assert!(store.previous().is_none());
    assert!(store.apply("light"));
    assert_eq!(store.previous().map(Theme::id), Some("dark"));
    assert_eq!(store.take_events(), [ThemeEvent::Applied("light".into())]);
    assert!(!store.apply("nope"));
    assert!(store.apply("light"), "applying the active one again changes no history");
    assert_eq!(store.previous().map(Theme::id), Some("dark"));
}

#[test]
fn next_and_previous_stay_in_their_folder() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = open(dir.path(), &none());
    assert_eq!(store.next_theme(1).map(Theme::id), Some("light"), "Dark, Light are the loose ones");
    assert_eq!(store.next_theme(-1).map(Theme::id), Some("light"), "and it wraps round");
    let pack: Vec<String> =
        store.themes().iter().filter(|t| !t.folder().is_empty()).map(|t| t.id().to_owned()).collect();
    store.apply(&pack[0]);
    assert_eq!(store.next_theme(1).map(Theme::id), Some(pack[1].as_str()));
    assert_eq!(store.next_theme(-1).map(Theme::id), Some(pack[pack.len() - 1].as_str()));
}

#[test]
fn deleting_and_folders() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = open(dir.path(), &none());
    assert!(!store.can_delete("dark"), "built-in themes stay");
    let t = store.new_theme("Mine", None, "");
    assert!(store.can_delete(t.id()));
    store.apply(t.id());
    store.delete(t.id());
    assert_eq!(store.active_id(), "dark", "deleting the active theme switches to the first one");
    let f = store.add_folder("  Work ", "#3DBE96");
    assert_eq!(f.name, "Work");
    let a = store.new_theme("A", None, &f.id);
    store.move_to_folder("light", &f.id);
    let groups = store.groups();
    let keys: Vec<&str> = groups.iter().map(|g| g.key.as_str()).collect();
    assert_eq!(keys[0], BUILTIN_GROUP);
    assert!(!keys.contains(&MINE_GROUP), "an empty My themes hides when there are folders");
    let work = groups.iter().find(|g| g.key == f.id).unwrap();
    assert_eq!(work.themes, ["light".to_owned(), a.id().to_owned()]);
    store.delete_folder(&f.id);
    assert_eq!(store.get("light").unwrap().folder(), "");
    let names: Vec<String> = store.folders().into_iter().map(|f| f.name).collect();
    assert_eq!(names, ["More themes"]);
}

#[test]
fn group_order_and_folding() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = open(dir.path(), &none());
    let f = store.add_folder("Work", "#3DBE96");
    let more = store.folders()[0].id.clone();
    store.set_group_order(&[f.id.clone(), BUILTIN_GROUP.into(), more.clone()]);
    let keys: Vec<String> = store.groups().into_iter().map(|g| g.key).collect();
    assert_eq!(keys[..3], [f.id.clone(), BUILTIN_GROUP.into(), more]);
    assert_eq!(store.folders()[0].id, f.id, "the folders follow the order too");
    let builtin = store.groups().into_iter().find(|g| g.key == BUILTIN_GROUP).unwrap();
    assert!(!store.group_collapsed(&builtin));
    store.toggle_group(&builtin);
    assert!(store.group_collapsed(&builtin));
    let work = store.groups().into_iter().find(|g| g.key == f.id).unwrap();
    store.toggle_group(&work);
    assert!(store.groups().into_iter().find(|g| g.key == f.id).map(|g| store.group_collapsed(&g)).unwrap());
}

#[test]
fn themes_made_from_one_colour() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = open(dir.path(), &none());
    let t = store.from_accent(Rgb::from_u32(0x7B6CF6), false, "");
    assert_eq!(t.name(), "#7B6CF6 dark");
    assert_eq!(t.colours()["bg_base"], "#121118");
    let made = store
        .from_colours("From Atlas", json!({"primary": "#CBA6F7", "bg": "#141218"}).as_object().unwrap())
        .unwrap();
    let again = store
        .from_colours("From Atlas", json!({"primary": "#8839EF", "bg": "#EFF1F5"}).as_object().unwrap())
        .unwrap();
    assert_eq!(made, again, "a theme with the same name is updated");
    assert_eq!(store.get(&made).unwrap().colours()["primary"], "#8839EF");
    assert!(store.from_colours("Nope", json!({"primary": "bad"}).as_object().unwrap()).is_none());
    store.set_accent(&made, Rgb::from_u32(0x30A46C));
    let c = store.get(&made).unwrap().colours();
    assert_eq!(c["primary"], "#30A46C");
    assert_eq!(c["folder_border"], c["primary_text"]);
}

#[test]
fn a_built_in_goes_back_to_its_colours() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = open(dir.path(), &none());
    let mut t = store.get("light").unwrap().clone();
    t.colours_mut().insert("primary".into(), json!("#00FF00"));
    t.set("folder", json!("x"));
    store.update_theme(t);
    store.reset_builtin("light");
    let t = store.get("light").unwrap();
    assert_eq!(t.colours()["primary"], "#F2B705");
    assert_eq!(t.folder(), "x", "it keeps its folder");
}

#[test]
fn theme_keys_come_from_the_themes() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = open(dir.path(), &none());
    let mut t = store.get("light").unwrap().clone();
    t.set("hotkey", json!("ctrl+alt+7"));
    store.update_theme(t);
    assert_eq!(store.hotkeys(), [("light".to_owned(), "Light".to_owned(), "ctrl+alt+7".to_owned())]);
    let copy = store.duplicate("light").unwrap();
    assert_eq!(copy.name(), "Light copy");
    assert_eq!(copy.hotkey(), "", "a copy does not take the key");
    assert!(!copy.builtin_flag());
}

fn share() -> Value {
    read_json("themes_share_cases.json")
}

fn pictures(into: &Path) {
    fs::create_dir_all(into).unwrap();
    fs::copy(fixture_path("lumtheme/wall.png"), into.join("wall.png")).unwrap();
    fs::copy(fixture_path("lumtheme/card-icon.png"), into.join("card icon.png")).unwrap();
}

#[test]
fn a_theme_exported_by_rust_is_the_file_python_writes() {
    let case = share();
    let dir = tempfile::tempdir().unwrap();
    pictures(dir.path());
    let mut source = case["source"].clone();
    let at = |name: &str| dir.path().join(name).to_string_lossy().into_owned();
    source["icon"] = json!(at("card icon.png"));
    source["background_image"] = json!(at("wall.png"));
    let mut store = open(&dir.path().join("home"), &none());
    store.add(Theme(source.as_object().unwrap().clone()));
    let out = dir.path().join("out.lumtheme");
    store.export(source["id"].as_str().unwrap(), &out).unwrap();
    let want = fs::read_to_string(fixture_path("lumtheme/python-export.lumtheme")).unwrap();
    assert_eq!(fs::read_to_string(&out).unwrap(), want);
}

#[test]
fn a_theme_exported_by_python_imports_like_it_does_there() {
    let case = share();
    let dir = tempfile::tempdir().unwrap();
    let mut store = open(dir.path(), &none());
    let t = store.import_file(&fixture_path("lumtheme/python-export.lumtheme")).unwrap();
    let id = t.id().to_owned();
    let mut got = Value::Object(t.0.clone());
    let images = dir.path().join("theme-images").to_string_lossy().into_owned();
    // JSON writes a Windows path's backslashes doubled, so look for the path as JSON writes it, then make the
    // separators after it slashes
    let escaped = serde_json::to_string(&images).unwrap().trim_matches('"').to_owned();
    let text = got
        .to_string()
        .replace(&escaped, "<SCRATCH>/share/theme-images")
        .replace(&id, "<ID>")
        .replace("images\\\\<ID>", "images/<ID>");
    got = serde_json::from_str(&text).unwrap();
    let want_id = case["imported_id"].as_str().unwrap();
    let want: Value = serde_json::from_str(&case["imported"].to_string().replace(want_id, "<ID>")).unwrap();
    assert_eq!(got, want);
    for name in ["wall.png", "card icon.png"] {
        assert!(
            dir.path().join("theme-images").join(format!("{id}-{name}")).is_file(),
            "{name} was unpacked"
        );
    }
}

#[test]
fn what_is_not_a_theme_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = open(dir.path(), &none());
    let file = dir.path().join("x.lumtheme");
    fs::write(&file, r#"{"hello": 1}"#).unwrap();
    let err = store.import_file(&file).unwrap_err();
    assert_eq!(err.to_string(), "This file is not a Luminescent theme (.lumtheme).");
    fs::write(&file, "not json").unwrap();
    assert!(store.import_file(&file).is_err());
}

#[test]
fn recent_changes_stay_with_the_theme() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = open(dir.path(), &none());
    let mut list: Vec<Recent> = Vec::new();
    let one = |a: &str, b: &str| {
        (
            json!({"primary": a}).as_object().unwrap().clone(),
            json!({"primary": b}).as_object().unwrap().clone(),
        )
    };
    let (o, n) = one("#111111", "#222222");
    note_change(&mut list, "lum", o, n, "Accent", Map::new());
    store.set_recent_changes("dark", &list);
    store.flush();
    let again = open(dir.path(), &none());
    let back = again.recent_changes("dark");
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].new["primary"], "#222222");
    assert!(again.recent_changes("light").is_empty());
}

#[test]
fn copying_a_fixture_folder_works_for_the_app_too() {
    let dir = tempfile::tempdir().unwrap();
    copy_dir("sample_home", dir.path());
    let store = open(&dir.path().join("data"), &none());
    assert!(store.themes().len() >= 2);
}
