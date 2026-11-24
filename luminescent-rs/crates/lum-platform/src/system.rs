//! Small system readings: how long since the last key press or mouse move, and which programs are running.

use std::collections::BTreeSet;
use std::time::Instant;

/// Seconds since the user last did anything. Windows knows; Wayland tells no program about key presses
/// elsewhere, so there the mouse standing still is the best guess (from Hyprland, which knows where it is).
pub struct IdleClock {
    /// Where the pointer last was, and since when (Wayland's guess; Windows asks the system).
    #[cfg_attr(windows, allow(dead_code))]
    last: Option<((i32, i32), Instant)>,
}

impl Default for IdleClock {
    fn default() -> IdleClock {
        IdleClock::new()
    }
}

impl IdleClock {
    pub fn new() -> IdleClock {
        IdleClock { last: None }
    }

    /// Asks the system (a socket call on Hyprland), so call it off the UI thread.
    pub fn seconds(&mut self) -> f64 {
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::SystemInformation::GetTickCount;
            use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
            let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
            // SAFETY: the struct is sized as the call wants and lives for it.
            if unsafe { GetLastInputInfo(&mut info) } != 0 {
                // SAFETY: no preconditions.
                let now = unsafe { GetTickCount() };
                return f64::from(now.wrapping_sub(info.dwTime)) / 1000.0;
            }
            0.0
        }
        #[cfg(not(windows))]
        {
            let pos = cursor();
            let now = Instant::now();
            match (&self.last, pos) {
                (Some((was, since)), Some(p)) if *was == p => {
                    return now.duration_since(*since).as_secs_f64();
                }
                (_, Some(p)) => self.last = Some((p, now)),
                _ => {}
            }
            self.last.map_or(0.0, |(_, since)| now.duration_since(since).as_secs_f64())
        }
    }
}

#[cfg(target_os = "linux")]
fn cursor() -> Option<(i32, i32)> {
    lum_hypr::Hypr::from_env()?.cursor_pos()
}

#[cfg(not(any(target_os = "linux", windows)))]
fn cursor() -> Option<(i32, i32)> {
    None
}

/// The lowercase names of every running program, with and without `.exe` on Windows, for "is it running?"
pub fn process_names() -> BTreeSet<String> {
    imp::process_names()
}

#[cfg(target_os = "linux")]
mod imp {
    use std::collections::BTreeSet;

    pub fn process_names() -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        let Ok(dir) = std::fs::read_dir("/proc") else { return names };
        for entry in dir.flatten() {
            let name = entry.file_name();
            if !name.to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            let path = entry.path();
            if let Ok(comm) = std::fs::read_to_string(path.join("comm")) {
                names.insert(comm.trim().to_lowercase());
            }
            if let Ok(exe) = std::fs::read_link(path.join("exe"))
                && let Some(file) = exe.file_name()
            {
                names.insert(file.to_string_lossy().to_lowercase());
            }
        }
        names
    }
}

#[cfg(windows)]
mod imp {
    use std::collections::BTreeSet;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };

    pub fn process_names() -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        // SAFETY: a snapshot handle we close below; the entry struct is sized as the calls want.
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE {
                return names;
            }
            let mut entry = std::mem::zeroed::<PROCESSENTRY32W>();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut ok = Process32FirstW(snap, &mut entry) != 0;
            while ok {
                let end = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..end]).to_lowercase();
                if !name.is_empty() {
                    if let Some(bare) = name.strip_suffix(".exe") {
                        names.insert(bare.to_owned());
                    }
                    names.insert(name);
                }
                ok = Process32NextW(snap, &mut entry) != 0;
            }
            CloseHandle(snap);
        }
        names
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod imp {
    pub fn process_names() -> std::collections::BTreeSet<String> {
        Default::default()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(target_os = "linux")]
    fn this_test_is_a_running_program() {
        let names = super::process_names();
        let me = std::env::current_exe().expect("exe");
        let me = me.file_name().expect("name").to_string_lossy().to_lowercase();
        assert!(names.contains(&me), "{me} in {} names", names.len());
    }
}
