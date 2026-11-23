//! Where settings.json and the data folder live.
//!
//! The same rules as the Python app's `core/__init__.py`: `LUMINESCENT_HOME` wins (tests, a second portable
//! copy), otherwise `%LOCALAPPDATA%\Luminescent` on Windows and `~/.local/share/Luminescent` (or
//! `$XDG_DATA_HOME/Luminescent`) on Linux. Note that settings.json sits one level above `data/`.
//!
//! One difference on purpose: a debug build uses `Luminescent-dev` instead, so working on the app can never
//! touch the real setup, even when `LUMINESCENT_HOME` is forgotten.

use std::env;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

pub const APP_NAME: &str = "Luminescent";
pub const VERSION: &str = "1.9.21";
pub const REPO: &str = "ForsakenBlank/Luminescent";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub home: PathBuf,
    pub data: PathBuf,
    /// True when `LUMINESCENT_HOME` chose the folder: a test copy that may run next to the real app.
    pub custom_home: bool,
}

impl Paths {
    /// The folders for this run, worked out from the environment.
    pub fn from_env() -> Paths {
        let custom = env::var_os("LUMINESCENT_HOME").filter(|v| !v.is_empty());
        match custom {
            Some(home) => Paths::custom(absolute(PathBuf::from(home))),
            None => Paths::at(default_home(cfg!(debug_assertions))),
        }
    }

    /// A home folder picked by the app itself.
    pub fn at(home: impl Into<PathBuf>) -> Paths {
        let home = home.into();
        Paths { data: home.join("data"), home, custom_home: false }
    }

    /// A home folder picked with `LUMINESCENT_HOME`.
    pub fn custom(home: impl Into<PathBuf>) -> Paths {
        Paths { custom_home: true, ..Paths::at(home) }
    }

    pub fn create_dirs(&self) -> io::Result<()> {
        std::fs::create_dir_all(&self.data)
    }

    pub fn settings_file(&self) -> PathBuf {
        self.home.join("settings.json")
    }

    pub fn data_file(&self, name: &str) -> PathBuf {
        self.data.join(name)
    }

    pub fn profiles_file(&self) -> PathBuf {
        self.data_file("profiles.json")
    }

    pub fn profiles_dir(&self) -> PathBuf {
        self.data_file("profiles")
    }

    pub fn log_file(&self) -> PathBuf {
        self.data_file("launcher.log")
    }

    pub fn crash_log(&self) -> PathBuf {
        self.data_file("crash.log")
    }

    pub fn start_file(&self) -> PathBuf {
        self.data_file("starting.json")
    }

    pub fn backups_dir(&self) -> PathBuf {
        self.data_file("backups")
    }

    pub fn usage_file(&self) -> PathBuf {
        self.data_file("usage.json")
    }

    pub fn activity_file(&self) -> PathBuf {
        self.data_file("activity.jsonl")
    }
}

fn default_home(dev: bool) -> PathBuf {
    let name = if dev { format!("{APP_NAME}-dev") } else { APP_NAME.to_owned() };
    user_data_dir().join(name)
}

/// `%LOCALAPPDATA%` on Windows, `$XDG_DATA_HOME` or `~/.local/share` elsewhere.
pub fn user_data_dir() -> PathBuf {
    if cfg!(windows) {
        env_dir("LOCALAPPDATA").unwrap_or_else(|| user_home().join("AppData").join("Local"))
    } else {
        env_dir("XDG_DATA_HOME").unwrap_or_else(|| user_home().join(".local").join("share"))
    }
}

/// The user's own home folder (`~`).
pub fn user_home() -> PathBuf {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    env_dir(var).unwrap_or_else(|| PathBuf::from("."))
}

fn env_dir(name: &str) -> Option<PathBuf> {
    env::var_os(name).filter(|v: &OsString| !v.is_empty()).map(PathBuf::from)
}

fn absolute(path: PathBuf) -> PathBuf {
    std::path::absolute(&path).unwrap_or(path)
}

/// A path as text for settings and logs (Python stores paths as plain strings).
pub fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_sit_above_data() {
        let p = Paths::at("/x/Luminescent");
        assert_eq!(p.settings_file(), Path::new("/x/Luminescent/settings.json"));
        assert_eq!(p.log_file(), Path::new("/x/Luminescent/data/launcher.log"));
        assert_eq!(p.profiles_dir(), Path::new("/x/Luminescent/data/profiles"));
        assert!(!p.custom_home);
        assert!(Paths::custom("/t").custom_home);
    }

    #[test]
    fn dev_builds_use_their_own_folder() {
        assert!(default_home(true).ends_with("Luminescent-dev"));
        assert!(default_home(false).ends_with("Luminescent"));
    }
}
