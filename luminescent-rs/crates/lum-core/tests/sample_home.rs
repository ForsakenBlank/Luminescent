//! Every data file in fixtures/sample_home loads and saves back unchanged.
//!
//! Later phases own most of these files and will add typed models; until then the generic store must already
//! carry them through untouched (key order, number forms, escapes), since a module that only changes one field
//! still writes the whole file.

#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};

use lum_core::pyjson::Style;
use lum_core::settings::{DefaultPlaces, Schema, Settings, SettingsFiles};
use lum_core::store;

fn sample_copy() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("sample_home");
    lum_fixtures::copy_dir("sample_home", &home);
    (dir, home)
}

/// How the Python module that owns a file wrote it: one line, or indented by the width of the second line.
fn style_of(text: &str) -> Style {
    match text.lines().nth(1) {
        Some(line) => Style::indented(line.len() - line.trim_start().len()),
        None => Style::COMPACT,
    }
}

fn json_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            json_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "json") {
            out.push(path);
        }
    }
}

#[test]
fn every_json_file_saves_back_byte_for_byte() {
    let (_dir, home) = sample_copy();
    let mut files = Vec::new();
    json_files(&home, &mut files);
    assert!(files.len() >= 24, "sample_home lost files: only {} JSON files", files.len());
    for path in files {
        let before = fs::read_to_string(&path).unwrap();
        let value = store::load_json(&path).unwrap_or_else(|| panic!("{} did not load", path.display()));
        store::save_json(&path, &value, style_of(&before)).unwrap();
        let after = fs::read_to_string(&path).unwrap();
        assert_eq!(after, before, "{} changed", path.display());
    }
}

#[test]
fn settings_json_round_trips_through_the_store() {
    let (_dir, home) = sample_copy();
    let path = home.join("settings.json");
    let before = fs::read_to_string(&path).unwrap();
    let files = SettingsFiles {
        settings: path.clone(),
        profiles: home.join("data/profiles.json"),
        profiles_dir: home.join("data/profiles"),
    };
    let schema =
        Box::leak(Box::new(Schema::parse(&lum_fixtures::read_text("settings_schema.linux.json")).unwrap()));
    let mut settings = Settings::open(files, schema, DefaultPlaces::placeholders());
    assert_eq!(settings.profiles().len(), 2, "sample_home has a second profile");
    settings.save(false).unwrap();
    let after = fs::read_to_string(&path).unwrap();
    assert_eq!(lum_fixtures::without_whitespace(&after), lum_fixtures::without_whitespace(&before));
    assert_eq!(after, before, "settings.json is written exactly as Python writes it");
}
