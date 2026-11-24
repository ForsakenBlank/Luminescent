//! Backups against fixtures/backup_cases.json and the Python-made zip in sample_home.

#![allow(clippy::unwrap_used)]

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::time::{Duration, SystemTime};

use jiff::civil::DateTime;
use lum_core::Paths;
use lum_core::backup::{self, BackupError};
use serde_json::Value;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

const NOW: DateTime = DateTime::constant(2026, 11, 23, 14, 30, 0, 0);
const NOW_TS: f64 = 1795444200.0;

fn sample_home() -> (tempfile::TempDir, Paths) {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("sample_home");
    lum_fixtures::copy_dir("sample_home", &home);
    (dir, Paths::at(home))
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_owned()).collect()
}

#[test]
fn same_file_list_and_members_as_python() {
    let cases = lum_fixtures::read_json("backup_cases.json");
    assert_eq!(backup::FILES.iter().map(|s| s.to_string()).collect::<Vec<_>>(), strings(&cases["files"]));

    let (dir, paths) = sample_home();
    let zip_path = dir.path().join("backup.zip");
    let count = backup::export(&paths, &zip_path).unwrap();
    assert_eq!(count as u64, cases["count"].as_u64().unwrap());
    let mut members = backup::check(&zip_path).unwrap();
    members.sort();
    assert_eq!(members, strings(&cases["sample_home_members"]));
}

#[test]
fn refuses_the_same_zips() {
    let cases = lum_fixtures::read_json("backup_cases.json");
    let dir = tempfile::tempdir().unwrap();
    for case in cases["check"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let zip_path = dir.path().join("case.zip");
        let mut zip = ZipWriter::new(File::create(&zip_path).unwrap());
        for entry in strings(&case["entries"]) {
            // the generator wrote "{}" everywhere except the one damaged file
            let body = if name == "damaged file" && entry == "data/notes.json" { "{nope" } else { "{}" };
            zip.start_file(entry.as_str(), SimpleFileOptions::default()).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();

        match lum_fixtures::Outcome::of(case) {
            lum_fixtures::Outcome::Ok(want) => {
                let mut got = backup::check(&zip_path).unwrap();
                let mut want = strings(&want);
                got.sort();
                want.sort();
                assert_eq!(got, want, "{name}");
            }
            lum_fixtures::Outcome::Error { kind, message } => {
                assert_eq!(kind, "ValueError");
                let err = backup::check(&zip_path).unwrap_err();
                let text = err.to_string();
                match err {
                    BackupError::NotABackup => assert_eq!(text, message, "{name}"),
                    // the reason after the colon is the JSON reader's own wording, which differs
                    BackupError::Damaged { .. } => {
                        let prefix = message.split_once(": ").unwrap().0;
                        assert!(text.starts_with(&format!("{prefix}: ")), "{name}: {text}");
                    }
                    other => panic!("{name}: unexpected {other}"),
                }
            }
        }
    }
}

#[test]
fn imports_a_python_backup_after_saving_the_current_files() {
    let python_zip = lum_fixtures::path("sample_home/data/backups/weekly-20261116.zip");
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path().join("home"));
    paths.create_dirs().unwrap();
    fs::write(paths.settings_file(), "{\"general\": {}}").unwrap();

    let auto = backup::restore(&paths, &python_zip, NOW).unwrap();
    assert_eq!(auto.file_name().unwrap(), "auto-20261123-143000.zip");
    assert_eq!(
        backup::check(&auto).unwrap(),
        vec!["settings.json"],
        "the safety backup holds what was there"
    );

    let sample = lum_fixtures::path("sample_home");
    for name in backup::check(&python_zip).unwrap() {
        let restored = fs::read(paths.home.join(&name)).unwrap();
        assert_eq!(restored, fs::read(sample.join(&name)).unwrap(), "{name}");
    }
}

fn set_age(path: &Path, days: u64) {
    let when = SystemTime::UNIX_EPOCH + Duration::from_secs_f64(NOW_TS) - Duration::from_secs(days * 86400);
    File::options().write(true).open(path).unwrap().set_modified(when).unwrap();
}

#[test]
fn weekly_backups_wait_a_week_and_keep_five() {
    let (_dir, paths) = sample_home();
    let backups = paths.backups_dir();
    set_age(&backups.join("weekly-20261116.zip"), 3);
    assert_eq!(backup::weekly(&paths, NOW, NOW_TS).unwrap(), None, "the last one is three days old");

    for (i, day) in ["2026-09-01", "2026-09-08", "2026-09-15", "2026-09-22", "2026-09-29"].iter().enumerate()
    {
        let old = backups.join(format!("weekly-{day}.zip"));
        fs::copy(backups.join("weekly-20261116.zip"), &old).unwrap();
        set_age(&old, 80 - i as u64 * 7);
    }
    set_age(&backups.join("weekly-20261116.zip"), 8);
    let made = backup::weekly(&paths, NOW, NOW_TS).unwrap().unwrap();
    assert_eq!(made.file_name().unwrap(), "weekly-2026-11-23.zip");
    let mut left: Vec<String> = fs::read_dir(&backups)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.starts_with("weekly-"))
        .collect();
    left.sort();
    assert_eq!(
        left,
        [
            "weekly-2026-09-15.zip",
            "weekly-2026-09-22.zip",
            "weekly-2026-09-29.zip",
            "weekly-2026-11-23.zip",
            "weekly-20261116.zip"
        ]
    );
}
