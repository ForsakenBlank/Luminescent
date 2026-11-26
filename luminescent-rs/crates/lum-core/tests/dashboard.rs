//! The dashboard store against the Python app's `_load` (fixtures/dashboard_migration_cases.json) and the
//! sample home's real file.

#![allow(clippy::unwrap_used)]

use std::fs;

use lum_core::dashboard::{DashboardStore, ToolSource, snap_to};
use serde_json::{Map, Value, json};

/// Python's ids are random, so a generated one (ten hex digits) is compared as a placeholder.
fn mask_ids(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, v) in map.iter_mut() {
                match v.as_str() {
                    Some(id)
                        if key == "id" && id.len() == 10 && id.chars().all(|c| c.is_ascii_hexdigit()) =>
                    {
                        *v = json!("<id>");
                    }
                    _ => mask_ids(v),
                }
            }
        }
        Value::Array(list) => list.iter_mut().for_each(mask_ids),
        _ => {}
    }
}

fn masked(map: &Map<String, Value>) -> Value {
    let mut value = Value::Object(map.clone());
    mask_ids(&mut value);
    value
}

fn tools(fake: &Value) -> Vec<ToolSource> {
    fake.as_object()
        .unwrap()
        .iter()
        .map(|(key, m)| ToolSource {
            key: key.clone(),
            icon: m["icon"].as_str().unwrap().to_owned(),
            actions: m["actions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| (a[0].as_str().unwrap().to_owned(), a[1].as_str().unwrap().to_owned()))
                .collect(),
        })
        .collect()
}

#[test]
fn every_migration_case_matches_the_python_load() {
    let doc = lum_fixtures::read_json("dashboard_migration_cases.json");
    let sources = tools(&doc["fake_modules"]);
    for case in doc["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dashboard.json");
        if !case["saved"].is_null() {
            fs::write(&path, case["saved"].to_string()).unwrap();
        }
        let raw = case["raw_settings"].as_object().cloned().unwrap_or_default();
        let store = DashboardStore::load(&path, &sources, &raw);
        assert_eq!(
            masked(store.document()),
            {
                let mut expected = case["result"].clone();
                mask_ids(&mut expected);
                expected
            },
            "{name}"
        );
    }
}

#[test]
fn a_first_start_writes_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dashboard.json");
    let _ = DashboardStore::load(&path, &[], &Map::new());
    let written: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(written["layout"], 3);
    assert_eq!(written["boards"][0]["tabs"][0]["id"], "tools");
}

#[test]
fn the_sample_home_dashboard_round_trips_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dashboard.json");
    let before = lum_fixtures::read_text("sample_home/data/dashboard.json");
    fs::write(&path, &before).unwrap();
    let store = DashboardStore::load(&path, &[], &Map::new());
    store.save();
    store.flush();
    assert_eq!(fs::read_to_string(&path).unwrap(), before);
}

fn store_with(json_text: &str) -> (tempfile::TempDir, DashboardStore) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dashboard.json");
    fs::write(&path, json_text).unwrap();
    let mut n = 0;
    let ids = Box::new(move || {
        n += 1;
        format!("{n:010x}")
    });
    let store = DashboardStore::load_with(&path, &[], &Map::new(), ids);
    (dir, store)
}

const TWO_BOXES: &str = r#"{"layout": 3, "active": "main", "boards": [{"id": "main", "name": "Main", "tabs": [
  {"id": "a", "title": "A", "items": [{"id": "i1", "name": "One", "hotkey": "ctrl+1"}, {"id": "i2", "name": "Two"},
     {"id": "i3", "name": "Three", "kind": "group", "group": ["i1", "i2"]}]},
  {"id": "b", "title": "B", "items": []}]}]}"#;

fn names(store: &DashboardStore, tab: &str) -> Vec<String> {
    store.tab(tab).unwrap().items().map(|i| i.name().to_owned()).collect()
}

#[test]
fn items_move_within_a_box_and_between_boxes() {
    let (_dir, mut store) = store_with(TWO_BOXES);
    store.move_item("i1", "", 1, -1);
    assert_eq!(names(&store, "a"), ["Two", "One", "Three"]);
    store.move_item("i1", "", -5, -1);
    assert_eq!(names(&store, "a"), ["Two", "One", "Three"], "out of range does nothing");
    // dropped below the last row: the list shrank above the drop point, so it still ends up last
    store.move_item("i2", "", 0, 3);
    assert_eq!(names(&store, "a"), ["One", "Three", "Two"]);
    store.move_item("i3", "b", 0, -1);
    assert_eq!(names(&store, "b"), ["Three"]);
    store.move_item("i1", "b", 0, 0);
    assert_eq!(names(&store, "b"), ["One", "Three"]);
}

#[test]
fn deleting_an_item_takes_it_out_of_groups() {
    let (_dir, mut store) = store_with(TWO_BOXES);
    store.delete_item("i1");
    let group = store.find_item("i3").unwrap().1.group();
    assert_eq!(group, ["i2"]);
}

#[test]
fn a_copied_board_gets_new_ids_and_no_hotkeys() {
    let (_dir, mut store) = store_with(TWO_BOXES);
    let copy = store.add_board("Work", "main", false, &[]);
    assert_eq!(store.active_id(), copy);
    let board = store.board(&copy).unwrap();
    let ids: Vec<&str> = board.tabs().map(|t| t.id()).collect();
    assert!(!ids.contains(&"a") && !ids.contains(&"b"));
    let first = board.tabs().next().unwrap().items().next().unwrap();
    assert_ne!(first.id(), "i1");
    assert_eq!(first.hotkey(), "");
    assert_eq!(store.board_count(), 2);
}

#[test]
fn screens_follow_their_board_and_the_last_board_stays() {
    let (_dir, mut store) = store_with(TWO_BOXES);
    let second = store.add_board("Second", "", true, &[]);
    store.set_screen_board("DP-1", &second);
    assert_eq!(store.screen_board("DP-1").unwrap().id(), second);
    assert_eq!(store.pinned_screens(&second), ["DP-1"]);
    store.delete_board(&second);
    assert!(store.screen_board("DP-1").is_none());
    assert_eq!(store.active_id(), "main");
    store.delete_board("main");
    assert_eq!(store.board_count(), 1, "the last board can not go");
}

#[test]
fn files_dropped_in_become_named_items() {
    let (dir, mut store) = store_with(TWO_BOXES);
    let file = dir.path().join("backup.sh");
    fs::write(&file, "").unwrap();
    let folder = dir.path().join("Games");
    fs::create_dir(&folder).unwrap();
    let paths = [file.to_string_lossy().into_owned(), format!("{}/", folder.display())];
    store.add_paths("b", &paths, -1, &|_| None);
    assert_eq!(names(&store, "b"), ["backup", "Games"]);
    assert_eq!(store.tab("b").unwrap().items().nth(1).unwrap().path(), folder.to_string_lossy());
}

#[test]
fn hotkey_users_name_the_item() {
    let (_dir, store) = store_with(TWO_BOXES);
    let users = store.hotkey_users("", &|c| c.to_lowercase());
    assert_eq!(users["ctrl+1"], "Dashboard → A → One");
    assert!(store.hotkey_users("i1", &|c| c.to_owned()).is_empty());
}

#[test]
fn snapping_rounds_halves_to_even_like_python() {
    assert_eq!(snap_to(4.0, 8), 0);
    assert_eq!(snap_to(12.0, 8), 16);
    assert_eq!(snap_to(20.0, 8), 16);
    assert_eq!(snap_to(5.5, 0), 6);
    assert_eq!(snap_to(6.5, 1), 6);
}
