//! "Run at login". Linux: an XDG autostart entry (`~/.config/autostart/luminescent.desktop`), which UWSM and
//! most desktop sessions start by themselves. Windows: the per-user Run key, which is more reliable than a
//! shortcut in the Startup folder (no console window, no working-folder surprises).
//!
//! The entry always points at the running program, so it never goes stale when the folder moves: `sync`
//! rewrites it. While the Python app is still the one in use, the Rust app does not sync on its own (it would
//! point the Python app's entry at itself, D30).

use std::path::PathBuf;

fn exe() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("luminescent"))
}

/// Makes the entry match the setting (and repoints it if the program moved). Gives back an error text.
pub fn sync(want: bool) -> Result<(), String> {
    imp::sync(want)
}

pub fn is_enabled() -> bool {
    imp::current().is_some()
}

pub fn set_enabled(on: bool) -> Result<(), String> {
    imp::set_enabled(on)
}

/// The command written into the entry.
pub fn command() -> String {
    imp::command()
}

#[cfg(not(windows))]
mod imp {
    use super::exe;
    use std::path::PathBuf;

    fn entry() -> PathBuf {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::home_dir().unwrap_or_default().join(".config"));
        config.join("autostart").join("luminescent.desktop")
    }

    /// Desktop-entry quoting: the folder may have a space in it.
    pub fn quote(arg: &str) -> String {
        if arg.is_empty() || arg.chars().any(|c| " \t\n\"'\\><~|&;$*?#()`".contains(c)) {
            let escaped: String = arg
                .chars()
                .flat_map(|c| match c {
                    '"' | '`' | '$' | '\\' => vec!['\\', c],
                    _ => vec![c],
                })
                .collect();
            format!("\"{escaped}\"")
        } else {
            arg.to_owned()
        }
    }

    pub fn command() -> String {
        quote(&exe().to_string_lossy())
    }

    fn text() -> String {
        let dir = exe().parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        [
            "[Desktop Entry]".to_owned(),
            "Type=Application".to_owned(),
            format!("Name={}", lum_core::APP_NAME),
            "Comment=Hotkey tools hub".to_owned(),
            format!("Exec={}", command()),
            format!("Path={dir}"),
            "Icon=luminescent".to_owned(),
            "Terminal=false".to_owned(),
            "X-GNOME-Autostart-enabled=true".to_owned(),
            String::new(),
        ]
        .join("\n")
    }

    pub fn current() -> Option<String> {
        let text = std::fs::read_to_string(entry()).ok()?;
        text.lines().find_map(|l| l.strip_prefix("Exec=")).map(|c| c.trim().to_owned())
    }

    pub fn set_enabled(on: bool) -> Result<(), String> {
        let path = entry();
        let done = if on {
            path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&path, text()))
        } else {
            match std::fs::remove_file(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                other => other,
            }
        };
        match &done {
            Ok(()) if on => tracing::info!("autostart enabled: {}", command()),
            Ok(()) => tracing::info!("autostart disabled"),
            Err(e) => tracing::error!("could not write {}: {e}", path.display()),
        }
        done.map_err(|e| e.to_string())
    }

    pub fn sync(want: bool) -> Result<(), String> {
        match current() {
            cur if want && cur.as_deref() != Some(command().as_str()) => set_enabled(true),
            Some(_) if !want => set_enabled(false),
            _ => Ok(()),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::quote;

        #[test]
        fn paths_with_spaces_are_quoted() {
            assert_eq!(quote("/usr/bin/luminescent"), "/usr/bin/luminescent");
            assert_eq!(quote("/home/me/My Apps/luminescent"), "\"/home/me/My Apps/luminescent\"");
            assert_eq!(quote("/a$b"), "\"/a\\$b\"");
            assert_eq!(quote(""), "\"\"");
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::exe;
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    /// Entries written before the rename; removed so it never starts twice.
    const OLD_NAMES: [&str; 1] = ["UsefulScripts"];

    pub fn command() -> String {
        format!("\"{}\"", exe().display())
    }

    pub fn current() -> Option<String> {
        let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_READ).ok()?;
        key.get_value(lum_core::APP_NAME).ok()
    }

    pub fn set_enabled(on: bool) -> Result<(), String> {
        let key = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE)
            .map_err(|e| e.to_string())?;
        let done = if on {
            key.set_value(lum_core::APP_NAME, &command())
        } else {
            match key.delete_value(lum_core::APP_NAME) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                other => other,
            }
        };
        match &done {
            Ok(()) if on => tracing::info!("autostart enabled: {}", command()),
            Ok(()) => tracing::info!("autostart disabled"),
            Err(e) => tracing::error!("could not write the Run key: {e}"),
        }
        done.map_err(|e| e.to_string())
    }

    pub fn sync(want: bool) -> Result<(), String> {
        if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE) {
            for name in OLD_NAMES {
                if key.delete_value(name).is_ok() {
                    tracing::info!("removed old startup entry {name:?}");
                }
            }
        }
        match current() {
            cur if want && cur.as_deref() != Some(command().as_str()) => set_enabled(true),
            Some(_) if !want => set_enabled(false),
            _ => Ok(()),
        }
    }
}
