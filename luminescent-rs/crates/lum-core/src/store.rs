//! Reading and writing the JSON data files, with the same contract as the Python helpers in `core/__init__.py`.
//!
//! Saving writes `<file>.tmp`, flushes it to disk and renames it over the old file, so a crash or power cut
//! mid-write leaves the previous file, never an empty one. Loading gives `None` for a missing file; a file that
//! can not be read is kept as `<file>.bad` (so a later save does not overwrite it) and also gives `None`.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::pyjson::{self, Style};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("could not write {path}: {source}")]
    Write { path: PathBuf, source: io::Error },
}

pub fn save_json(path: &Path, value: &Value, style: Style) -> Result<(), StoreError> {
    save_text(path, &pyjson::to_string(value, style))
}

/// Writes any text atomically (tmp + fsync + rename).
pub fn save_text(path: &Path, text: &str) -> Result<(), StoreError> {
    write_atomically(path, text.as_bytes())
        .map_err(|source| StoreError::Write { path: path.to_owned(), source })
}

pub fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = with_suffix(path, ".tmp");
    {
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)
}

pub fn load_json(path: &Path) -> Option<Value> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
        Err(e) => {
            tracing::error!("could not read {}: {e}", path.display());
            keep_as_bad(path);
            return None;
        }
    };
    match serde_json::from_str(&text) {
        Ok(value) => Some(value),
        Err(e) => {
            tracing::error!("could not read {}: {e}", path.display());
            keep_as_bad(path);
            None
        }
    }
}

/// Moves an unreadable file out of the way as `<file>.bad`, replacing an older `.bad`.
pub fn keep_as_bad(path: &Path) {
    let bad = with_suffix(path, ".bad");
    if let Err(e) = fs::rename(path, &bad) {
        tracing::warn!("could not keep {} as {}: {e}", path.display(), bad.display());
    }
}

/// `settings.json` + `.tmp` gives `settings.json.tmp`, as Python's `path + ".tmp"` does.
pub fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Saves files on a thread of its own, so the UI thread never waits for the disk. A newer text for the same
/// file replaces one that has not been written yet. `flush` waits until everything queued is on disk (used at
/// quit and by tests); dropping the writer does the same.
#[derive(Debug)]
pub struct Writer {
    tx: Option<std::sync::mpsc::Sender<WriteJob>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[derive(Debug)]
enum WriteJob {
    Save(PathBuf, String),
    Flush(std::sync::mpsc::Sender<()>),
}

impl Writer {
    pub fn new(name: &str) -> Writer {
        let (tx, rx) = std::sync::mpsc::channel::<WriteJob>();
        let thread = std::thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || {
                while let Ok(first) = rx.recv() {
                    let mut batch = vec![first];
                    batch.extend(rx.try_iter());
                    let mut latest: Vec<(PathBuf, String)> = Vec::new();
                    let mut waiting = Vec::new();
                    for job in batch {
                        match job {
                            WriteJob::Save(path, text) => match latest.iter_mut().find(|(p, _)| *p == path) {
                                Some(slot) => slot.1 = text,
                                None => latest.push((path, text)),
                            },
                            WriteJob::Flush(done) => waiting.push(done),
                        }
                    }
                    for (path, text) in latest {
                        if let Err(e) = save_text(&path, &text) {
                            tracing::error!("{e}");
                        }
                    }
                    for done in waiting {
                        let _ = done.send(());
                    }
                }
            })
            .ok();
        Writer { tx: Some(tx), thread }
    }

    pub fn save(&self, path: &Path, text: String) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(WriteJob::Save(path.to_owned(), text));
        }
    }

    pub fn flush(&self) {
        let Some(tx) = &self.tx else { return };
        let (done, wait) = std::sync::mpsc::channel();
        if tx.send(WriteJob::Flush(done)).is_ok() {
            let _ = wait.recv();
        }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.tx.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn saves_atomically_and_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.json");
        save_json(&path, &json!({"a": 1}), Style::indented(2)).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "{\n  \"a\": 1\n}");
        assert!(!with_suffix(&path, ".tmp").exists());
        assert_eq!(load_json(&path), Some(json!({"a": 1})));
    }

    #[test]
    fn missing_file_gives_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_json(&dir.path().join("nope.json")), None);
    }

    #[test]
    fn unreadable_file_is_kept_as_bad() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.json");
        fs::write(&path, "{broken").unwrap();
        assert_eq!(load_json(&path), None);
        assert!(!path.exists());
        assert_eq!(fs::read_to_string(dir.path().join("notes.json.bad")).unwrap(), "{broken");
    }
}
