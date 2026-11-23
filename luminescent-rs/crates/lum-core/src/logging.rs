//! Logging to `data/launcher.log` and, when it is a terminal, to stdout.
//!
//! Lines look exactly like the Python app's (`14:30:01 INFO    launcher: started notes`), so one log can hold
//! both apps' runs and the same greps work. The file rotates like Python's `RotatingFileHandler(maxBytes=512_000,
//! backupCount=2)`: launcher.log, launcher.log.1, launcher.log.2.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::Local;
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields, MakeWriter};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{Layer, Registry};

pub const MAX_BYTES: u64 = 512_000;
pub const BACKUPS: u32 = 2;

/// Starts logging. `file` is `None` when Settings → General → Write a log file is off.
pub fn init(file: Option<&Path>) -> io::Result<()> {
    let to_stdout = io::stdout().is_terminal();
    let file_layer = match file {
        Some(path) => Some(
            tracing_subscriber::fmt::layer()
                .event_format(PythonStyle)
                .with_writer(RotatingFile::open(path, MAX_BYTES, BACKUPS)?)
                .with_filter(LevelFilter::INFO),
        ),
        None => None,
    };
    let stdout_layer = to_stdout.then(|| {
        tracing_subscriber::fmt::layer()
            .event_format(PythonStyle)
            .with_writer(io::stdout)
            .with_filter(LevelFilter::INFO)
    });
    let subscriber = Registry::default().with(file_layer).with(stdout_layer);
    // a second init (tests, a restart in the same process) keeps the first one
    let _ = tracing::subscriber::set_global_default(subscriber);
    Ok(())
}

/// `%(asctime)s %(levelname)-7s %(name)s: %(message)s` with `%H:%M:%S`.
pub struct PythonStyle;

impl<S, N> FormatEvent<S, N> for PythonStyle
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let meta = event.metadata();
        write!(
            writer,
            "{} {:<7} {}: ",
            Local::now().format("%H:%M:%S"),
            level_name(*meta.level()),
            logger_name(meta.target())
        )?;
        ctx.field_format().format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}

fn level_name(level: Level) -> &'static str {
    match level {
        Level::ERROR => "ERROR",
        Level::WARN => "WARNING",
        Level::INFO => "INFO",
        Level::DEBUG | Level::TRACE => "DEBUG",
    }
}

/// The Python loggers are named after their part ("launcher", "settings", "storage"); the last piece of the
/// Rust module path gives the same names.
pub fn logger_name(target: &str) -> &str {
    target.rsplit("::").next().unwrap_or(target)
}

/// A log file that rolls over before a line would take it past `max_bytes`.
#[derive(Clone)]
pub struct RotatingFile {
    inner: Arc<Mutex<Rotating>>,
}

struct Rotating {
    path: PathBuf,
    max_bytes: u64,
    backups: u32,
    file: Option<File>,
    size: u64,
}

impl RotatingFile {
    pub fn open(path: &Path, max_bytes: u64, backups: u32) -> io::Result<RotatingFile> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        let size = file.metadata()?.len();
        let state = Rotating { path: path.to_owned(), max_bytes, backups, file: Some(file), size };
        Ok(RotatingFile { inner: Arc::new(Mutex::new(state)) })
    }

    /// Writes one whole record, rolling over first if it would not fit.
    pub fn write_record(&self, record: &[u8]) -> io::Result<()> {
        let mut state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        state.write_record(record)
    }
}

impl Rotating {
    fn write_record(&mut self, record: &[u8]) -> io::Result<()> {
        if self.max_bytes > 0 && self.size + record.len() as u64 >= self.max_bytes {
            self.roll_over()?;
        }
        if self.file.is_none() {
            self.file = Some(OpenOptions::new().create(true).append(true).open(&self.path)?);
            self.size = 0;
        }
        if let Some(file) = self.file.as_mut() {
            file.write_all(record)?;
            file.flush()?;
            self.size += record.len() as u64;
        }
        Ok(())
    }

    fn roll_over(&mut self) -> io::Result<()> {
        // the file is closed first: Windows can not rename a file that is open
        self.file = None;
        for i in (1..self.backups).rev() {
            let from = numbered(&self.path, i);
            if from.exists() {
                let to = numbered(&self.path, i + 1);
                let _ = fs::remove_file(&to);
                fs::rename(&from, &to)?;
            }
        }
        if self.backups > 0 {
            let first = numbered(&self.path, 1);
            let _ = fs::remove_file(&first);
            fs::rename(&self.path, &first)?;
        } else {
            let _ = fs::remove_file(&self.path);
        }
        Ok(())
    }
}

fn numbered(path: &Path, n: u32) -> PathBuf {
    crate::store::with_suffix(path, &format!(".{n}"))
}

/// What tracing writes one event into; the whole line reaches the file in one piece when it is dropped.
pub struct RecordWriter {
    file: RotatingFile,
    buf: Vec<u8>,
}

impl Write for RecordWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for RecordWriter {
    fn drop(&mut self) {
        if !self.buf.is_empty() {
            // nowhere left to report a failing log file
            let _ = self.file.write_record(&self.buf);
        }
    }
}

impl<'a> MakeWriter<'a> for RotatingFile {
    type Writer = RecordWriter;

    fn make_writer(&'a self) -> RecordWriter {
        RecordWriter { file: self.clone(), buf: Vec::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_the_python_loggers() {
        assert_eq!(logger_name("luminescent::launcher"), "launcher");
        assert_eq!(logger_name("lum_core::settings"), "settings");
        assert_eq!(logger_name("unhandled"), "unhandled");
        assert_eq!(level_name(Level::WARN), "WARNING");
    }

    #[test]
    fn rotates_with_two_backups() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("launcher.log");
        let log = RotatingFile::open(&path, 100, 2).unwrap();
        let line = [b'x'; 39];
        for _ in 0..10 {
            log.write_record(&line).unwrap();
        }
        // two lines fit (78 < 100), a third would reach 117, so each file holds two
        assert_eq!(fs::metadata(&path).unwrap().len(), 78);
        assert_eq!(fs::metadata(numbered(&path, 1)).unwrap().len(), 78);
        assert_eq!(fs::metadata(numbered(&path, 2)).unwrap().len(), 78);
        assert!(!numbered(&path, 3).exists());
    }

    #[test]
    fn keeps_appending_to_an_existing_log() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("launcher.log");
        fs::write(&path, "old line\n").unwrap();
        RotatingFile::open(&path, MAX_BYTES, BACKUPS).unwrap().write_record(b"new line\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "old line\nnew line\n");
    }
}
