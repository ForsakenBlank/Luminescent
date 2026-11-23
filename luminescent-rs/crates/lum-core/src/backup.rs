//! Settings backups: settings.json plus the data files that hold your setup, in one zip, as `core/backup.py`.
//!
//! The vault (its files live elsewhere anyway) and launcher.log are never included. Before an import the
//! current files are zipped to `data/backups/auto-<time>.zip`. Once a week the app also makes
//! `data/backups/weekly-<date>.zip` and keeps the newest five.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use jiff::Timestamp;
use jiff::civil::DateTime;
use jiff::tz::TimeZone;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::paths::Paths;
use crate::store;

/// The files a backup holds, relative to the home folder (plus `data/profiles/*.json`).
pub const FILES: &[&str] = &[
    "settings.json",
    "data/notes.json",
    "data/timers.json",
    "data/reminders_state.json",
    "data/hearing.json",
    "data/dashboard.json",
    "data/themes.json",
    "data/calculator.json",
    "data/colours.json",
    "data/palette.json",
    "data/profiles.json",
    "data/automation.json",
];

pub const WEEKLY_KEEP: usize = 5;
const WEEK_SECS: f64 = 7.0 * 86400.0;

#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error("This zip has no settings.json, so it is not a Luminescent backup.")]
    NotABackup,
    #[error("{name} in the backup is damaged: {reason}")]
    Damaged { name: String, reason: String },
    #[error("could not read the backup: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("{0}")]
    Io(#[from] io::Error),
}

/// What is there to back up right now.
pub fn members(paths: &Paths) -> Vec<String> {
    let mut out: Vec<String> =
        FILES.iter().filter(|f| paths.home.join(f).is_file()).map(|f| (*f).to_owned()).collect();
    let mut profiles: Vec<String> = fs::read_dir(paths.profiles_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| name.ends_with(".json"))
        .map(|name| format!("data/profiles/{name}"))
        .collect();
    profiles.sort();
    out.extend(profiles);
    out
}

/// Writes the backup. Returns the number of files in it.
pub fn export(paths: &Paths, zip_path: &Path) -> Result<usize, BackupError> {
    let members = members(paths);
    let mut zip = ZipWriter::new(File::create(zip_path)?);
    for name in &members {
        let source = paths.home.join(name);
        let mut options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        if let Some(when) = modified(&source) {
            options = options.last_modified_time(when);
        }
        zip.start_file(name.as_str(), options)?;
        zip.write_all(&fs::read(&source)?)?;
    }
    zip.finish()?.sync_all()?;
    Ok(members.len())
}

/// The file's time in local time, as Python's `zipfile` stores it.
fn modified(path: &Path) -> Option<zip::DateTime> {
    let mtime: SystemTime = fs::metadata(path).ok()?.modified().ok()?;
    let local = Timestamp::try_from(mtime).ok()?.to_zoned_datetime(TimeZone::system()).datetime();
    zip_time(local)
}

fn zip_time(at: DateTime) -> Option<zip::DateTime> {
    let part = |v: i8| u8::try_from(v).ok();
    zip::DateTime::from_date_and_time(
        u16::try_from(at.year()).ok()?,
        part(at.month())?,
        part(at.day())?,
        part(at.hour())?,
        part(at.minute())?,
        part(at.second())?,
    )
    .ok()
}

/// Names an import may restore: the backup list, and profile files directly in `data/profiles/`.
pub fn allowed(name: &str) -> bool {
    let name = name.replace('\\', "/");
    if FILES.contains(&name.as_str()) {
        return true;
    }
    name.starts_with("data/profiles/")
        && name.ends_with(".json")
        && name.matches('/').count() == 2
        && !name.contains("..")
}

/// The files an import would restore, or why the zip is not a usable backup.
pub fn check(zip_path: &Path) -> Result<Vec<String>, BackupError> {
    let mut zip = ZipArchive::new(File::open(zip_path)?)?;
    let names = allowed_names(&mut zip)?;
    if !names.iter().any(|n| n == "settings.json") {
        return Err(BackupError::NotABackup);
    }
    for name in &names {
        let bytes = read_member(&mut zip, name)?;
        let damaged = |reason: String| BackupError::Damaged { name: name.clone(), reason };
        let text = String::from_utf8(bytes).map_err(|e| damaged(e.to_string()))?;
        serde_json::from_str::<serde_json::Value>(&text).map_err(|e| damaged(e.to_string()))?;
    }
    Ok(names)
}

fn allowed_names(zip: &mut ZipArchive<File>) -> Result<Vec<String>, BackupError> {
    let mut names = Vec::new();
    for i in 0..zip.len() {
        let entry = zip.by_index(i)?;
        let name = entry.name()?.into_owned();
        if allowed(&name) {
            names.push(name);
        }
    }
    Ok(names)
}

fn read_member(zip: &mut ZipArchive<File>, name: &str) -> Result<Vec<u8>, BackupError> {
    let mut entry = zip.by_name(name)?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Backs up the current files, then writes the backup's files over them. Returns the safety backup's path.
/// The app restarts afterwards, so nothing has to be reloaded here.
pub fn restore(paths: &Paths, zip_path: &Path, now: DateTime) -> Result<PathBuf, BackupError> {
    let names = check(zip_path)?;
    fs::create_dir_all(paths.backups_dir())?;
    let auto = paths.backups_dir().join(format!("auto-{}.zip", now.strftime("%Y%m%d-%H%M%S")));
    export(paths, &auto)?;
    let mut zip = ZipArchive::new(File::open(zip_path)?)?;
    for name in names {
        let bytes = read_member(&mut zip, &name)?;
        let target = name.replace('\\', "/").split('/').fold(paths.home.clone(), |p, part| p.join(part));
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        store::write_atomically(&target, &bytes)?;
    }
    Ok(auto)
}

/// Once a week: zip the setup into `data/backups/weekly-<date>.zip` and keep the newest five. Returns the new
/// backup, or `None` when the last one is less than a week old.
pub fn weekly(paths: &Paths, now: DateTime, now_ts: f64) -> Result<Option<PathBuf>, BackupError> {
    let dir = paths.backups_dir();
    fs::create_dir_all(&dir)?;
    let weekly = weekly_backups(&dir);
    if let Some(newest) = weekly.last()
        && let Some(age) = age_secs(newest, now_ts)
        && age < WEEK_SECS
    {
        return Ok(None);
    }
    let path = dir.join(format!("weekly-{}.zip", now.strftime("%Y-%m-%d")));
    let count = export(paths, &path)?;
    tracing::info!("weekly backup: {count} files -> {}", path.display());
    let all = weekly_backups(&dir);
    let extra = all.len().saturating_sub(WEEKLY_KEEP);
    for old in &all[..extra] {
        let _ = fs::remove_file(old);
    }
    Ok(Some(path))
}

fn weekly_backups(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            name.starts_with("weekly-") && name.ends_with(".zip")
        })
        .collect();
    found.sort();
    found
}

fn age_secs(path: &Path, now_ts: f64) -> Option<f64> {
    let mtime = fs::metadata(path).ok()?.modified().ok()?;
    let secs = mtime.duration_since(SystemTime::UNIX_EPOCH).ok()?.as_secs_f64();
    Some(now_ts - secs)
}
