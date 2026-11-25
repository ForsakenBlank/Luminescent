//! Installed programs and real icons: the app list (`.desktop` files on Linux), the icon a file, folder,
//! program or command shows in the file manager, and "show this file in the file manager"
//! (`core/desktop.py`, `core/desktop_win.py`).
//!
//! Installing Luminescent's own launcher entry and icons waits for phase 15: the Python app writes the same
//! `luminescent.desktop`, and both may run until then.

use crate::runner::Kind;
use std::path::PathBuf;

/// Where an icon's pixels come from.
#[derive(Debug, Clone, PartialEq)]
pub enum IconSource {
    /// An image file from the icon theme (PNG or SVG).
    File(PathBuf),
    /// Pixels the system handed over (Windows), straight RGBA.
    Pixels { size: u32, rgba: Vec<u8> },
}

/// Opens the file manager with `path` selected (its folder for a folder). Gives back an error text.
pub fn reveal(path: &str) -> Result<(), String> {
    let path = PathBuf::from(crate::runner::expand(path));
    let path = std::path::absolute(&path).unwrap_or(path);
    if !path.exists() {
        return Err(format!("{} does not exist.", path.display()));
    }
    imp::reveal(&path)
}

/// The real icon for a dashboard item's target, or None to use the kind's symbol.
pub fn icon_for(target: &str, kind: Kind, size: u16) -> Option<IconSource> {
    let t = target.trim().trim_matches('"');
    if t.is_empty() {
        return None;
    }
    imp::icon_for(t, kind, size)
}

#[cfg(target_os = "linux")]
pub use imp::{apps, read_entry};

#[cfg(target_os = "linux")]
mod imp {
    use super::{IconSource, Kind};
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::OnceLock;

    fn data_home() -> PathBuf {
        std::env::var_os("XDG_DATA_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::home_dir().unwrap_or_default().join(".local/share"))
    }

    fn desktop_dirs() -> Vec<PathBuf> {
        let mut dirs = vec![data_home().join("applications")];
        let shared =
            std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".to_owned());
        dirs.extend(shared.split(':').filter(|d| !d.is_empty()).map(|d| Path::new(d).join("applications")));
        let home = std::env::home_dir().unwrap_or_default();
        dirs.push(home.join(".local/share/flatpak/exports/share/applications"));
        dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));
        dirs
    }

    /// The `[Desktop Entry]` group of a `.desktop` file; the first of each key wins.
    pub fn read_entry(path: &Path) -> HashMap<String, String> {
        let mut out = HashMap::new();
        let Ok(bytes) = std::fs::read(path) else { return out };
        let mut in_main = false;
        for line in String::from_utf8_lossy(&bytes).lines() {
            let line = line.trim();
            if line.starts_with('[') {
                in_main = line == "[Desktop Entry]";
                continue;
            }
            if in_main && let Some((k, v)) = line.split_once('=') {
                out.entry(k.trim().to_owned()).or_insert_with(|| v.trim().to_owned());
            }
        }
        out
    }

    /// Every app launchers show, as (name, its `.desktop` file), sorted by name. The first folder wins, like
    /// the launchers do.
    pub fn apps() -> Vec<(String, PathBuf)> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for dir in desktop_dirs() {
            let Ok(list) = std::fs::read_dir(&dir) else { continue };
            let mut names: Vec<String> =
                list.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
            names.sort();
            for n in names {
                if !n.ends_with(".desktop") || !seen.insert(n.clone()) {
                    continue;
                }
                let path = dir.join(&n);
                let e = read_entry(&path);
                let shown = e.get("Type").is_none_or(|t| t == "Application")
                    && e.get("NoDisplay").is_none_or(|v| v != "true")
                    && e.get("Hidden").is_none_or(|v| v != "true");
                if let (true, Some(name)) = (shown, e.get("Name").filter(|n| !n.is_empty())) {
                    out.push((name.clone(), path));
                }
            }
        }
        out.sort_by_key(|(name, _)| name.to_lowercase());
        out
    }

    /// Program name (and desktop id) to icon name, from every installed `.desktop` file. Built once.
    fn app_icons() -> &'static HashMap<String, String> {
        static INDEX: OnceLock<HashMap<String, String>> = OnceLock::new();
        INDEX.get_or_init(|| {
            let mut index = HashMap::new();
            for dir in desktop_dirs() {
                let Ok(list) = std::fs::read_dir(&dir) else { continue };
                for entry in list.flatten() {
                    let n = entry.file_name().to_string_lossy().into_owned();
                    let Some(id) = n.strip_suffix(".desktop") else { continue };
                    let e = read_entry(&entry.path());
                    let Some(icon) = e.get("Icon").filter(|i| !i.is_empty()) else { continue };
                    index.entry(id.to_lowercase()).or_insert_with(|| icon.clone());
                    let exec = e.get("Exec").map(String::as_str).unwrap_or("");
                    let argv = shlex::split(exec)
                        .unwrap_or_else(|| exec.split_whitespace().map(str::to_owned).collect());
                    if let Some(prog) = argv.iter().find(|a| *a != "env" && !a.contains('=')) {
                        let base = Path::new(prog).file_name().map(|b| b.to_string_lossy().to_lowercase());
                        index.entry(base.unwrap_or_default()).or_insert_with(|| icon.clone());
                    }
                }
            }
            index
        })
    }

    fn theme() -> &'static str {
        static THEME: OnceLock<String> = OnceLock::new();
        THEME.get_or_init(|| freedesktop_icons::default_theme_gtk().unwrap_or_else(|| "hicolor".to_owned()))
    }

    fn themed(name: &str, size: u16) -> Option<IconSource> {
        if name.is_empty() {
            return None;
        }
        let path = Path::new(name);
        if path.is_absolute() {
            return path.is_file().then(|| IconSource::File(path.to_path_buf()));
        }
        freedesktop_icons::lookup(name)
            .with_size(size)
            .with_theme(theme())
            .with_cache()
            .find()
            .map(IconSource::File)
    }

    pub fn icon_for(t: &str, kind: Kind, size: u16) -> Option<IconSource> {
        let path = PathBuf::from(crate::runner::expand(t));
        match kind {
            Kind::Url => {
                themed("internet-web-browser", size).or_else(|| themed("applications-internet", size))
            }
            Kind::Folder => {
                let home = std::env::home_dir().unwrap_or_default();
                let special = [
                    (home.clone(), "user-home"),
                    (home.join("Desktop"), "user-desktop"),
                    (home.join("Downloads"), "folder-download"),
                    (home.join("Documents"), "folder-documents"),
                    (home.join("Pictures"), "folder-pictures"),
                    (home.join("Music"), "folder-music"),
                    (home.join("Videos"), "folder-videos"),
                ];
                let clean: PathBuf = path.components().collect();
                let name = special.iter().find(|(p, _)| *p == clean).map_or("", |(_, n)| n);
                themed(name, size).or_else(|| themed("folder", size))
            }
            _ if path.extension().is_some_and(|e| e == "desktop") && path.is_file() => {
                themed(read_entry(&path).get("Icon").map_or("", String::as_str), size)
            }
            Kind::Program | Kind::Python | Kind::File if path.is_file() => {
                let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
                let stem = path.file_stem().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
                if let Some(icon) = app_icons().get(&name).or_else(|| app_icons().get(&stem))
                    && let Some(found) = themed(icon, size)
                {
                    return Some(found);
                }
                let mime = mime_guess::from_path(&path).first_or_octet_stream();
                let exact = format!("{}-{}", mime.type_(), mime.subtype());
                themed(&exact, size).or_else(|| themed(&format!("{}-x-generic", mime.type_()), size))
            }
            Kind::Command => {
                let first = shlex::split(t).and_then(|a| a.into_iter().next())?;
                let prog = Path::new(&first).file_name()?.to_string_lossy().to_lowercase();
                themed(app_icons().get(&prog).map_or("", String::as_str), size)
                    .or_else(|| themed(&prog, size))
                    .or_else(|| themed("utilities-terminal", size))
            }
            _ => None,
        }
    }

    fn file_url(path: &Path) -> String {
        let mut url = String::from("file://");
        for b in path.to_string_lossy().bytes() {
            if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
                url.push(b as char);
            } else {
                url.push_str(&format!("%{b:02X}"));
            }
        }
        url
    }

    pub fn reveal(path: &Path) -> Result<(), String> {
        let url = file_url(path);
        let asked = std::thread::scope(|s| {
            s.spawn(|| -> Result<(), String> {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| e.to_string())?;
                runtime.block_on(async {
                    let bus = zbus::Connection::session().await.map_err(|e| e.to_string())?;
                    let body = (vec![url.as_str()], "");
                    let call = bus.call_method(
                        Some("org.freedesktop.FileManager1"),
                        "/org/freedesktop/FileManager1",
                        Some("org.freedesktop.FileManager1"),
                        "ShowItems",
                        &body,
                    );
                    tokio::time::timeout(std::time::Duration::from_secs(2), call)
                        .await
                        .map_err(|e| e.to_string())?
                        .map(|_| ())
                        .map_err(|e| e.to_string())
                })
            })
            .join()
            .unwrap_or_else(|_| Err("the file manager call panicked".to_owned()))
        });
        if asked.is_ok() {
            return Ok(());
        }
        let text = path.to_string_lossy().into_owned();
        let parent = path.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        let tries: [(&str, Vec<String>); 4] = [
            ("dolphin", vec!["--select".into(), text.clone()]),
            ("nautilus", vec!["--select".into(), text.clone()]),
            ("nemo", vec![text.clone()]),
            ("thunar", vec![parent.clone()]),
        ];
        for (fm, args) in tries {
            if crate::terminal::which(fm).is_some() {
                let spawned = std::process::Command::new(fm)
                    .args(args)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
                if spawned.is_ok() {
                    return Ok(());
                }
            }
        }
        crate::runner::open_path(if path.is_dir() { &text } else { &parent })
    }
}

#[cfg(windows)]
mod imp {
    use super::{IconSource, Kind};
    use std::path::{Path, PathBuf};
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits,
        GetObjectW, ReleaseDC,
    };
    use windows_sys::Win32::UI::Shell::{SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGetFileInfoW};
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

    pub fn reveal(path: &Path) -> Result<(), String> {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub fn icon_for(t: &str, kind: Kind, _size: u16) -> Option<IconSource> {
        let path = match kind {
            Kind::Url => return None,
            Kind::Command => crate::terminal::which(t.split_whitespace().next()?)?,
            _ => PathBuf::from(crate::runner::expand(t)),
        };
        if !path.exists() {
            return None;
        }
        pixels(&path)
    }

    /// What Explorer shows for the file, as RGBA.
    fn pixels(path: &Path) -> Option<IconSource> {
        let wide: Vec<u16> = path.as_os_str().encode_wide_lossy();
        // SAFETY: the info struct is plain data sized for the call; the path is nul-terminated and lives for it.
        let icon: HICON = unsafe {
            let mut info = std::mem::zeroed::<SHFILEINFOW>();
            let ok = SHGetFileInfoW(
                wide.as_ptr(),
                0,
                &mut info,
                std::mem::size_of::<SHFILEINFOW>() as u32,
                SHGFI_ICON | SHGFI_LARGEICON,
            );
            if ok == 0 || info.hIcon.is_null() {
                return None;
            }
            info.hIcon
        };
        // SAFETY: `icon` is ours to destroy; the bitmaps GetIconInfo hands back are ours to delete, and every
        // buffer is sized from the bitmap's own description.
        unsafe {
            let mut ii = std::mem::zeroed::<ICONINFO>();
            let got = GetIconInfo(icon, &mut ii) != 0;
            let mut out = None;
            if got && !ii.hbmColor.is_null() {
                let mut bm = std::mem::zeroed::<BITMAP>();
                GetObjectW(
                    ii.hbmColor,
                    std::mem::size_of::<BITMAP>() as i32,
                    (&mut bm as *mut BITMAP).cast(),
                );
                let (w, h) = (bm.bmWidth.max(0) as u32, bm.bmHeight.max(0) as u32);
                let mut header = std::mem::zeroed::<BITMAPINFO>();
                header.bmiHeader = BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w as i32,
                    // negative: top-down rows
                    biHeight: -(h as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..std::mem::zeroed()
                };
                let mut bgra = vec![0u8; (w * h * 4) as usize];
                let dc = GetDC(std::ptr::null_mut());
                let rows =
                    GetDIBits(dc, ii.hbmColor, 0, h, bgra.as_mut_ptr().cast(), &mut header, DIB_RGB_COLORS);
                ReleaseDC(std::ptr::null_mut(), dc);
                if rows > 0 && w == h {
                    // old icons have no alpha at all: then every pixel counts as solid
                    let no_alpha = bgra.chunks_exact(4).all(|p| p[3] == 0);
                    let rgba = bgra
                        .chunks_exact(4)
                        .flat_map(|p| [p[2], p[1], p[0], if no_alpha { 255 } else { p[3] }]);
                    out = Some(IconSource::Pixels { size: w, rgba: rgba.collect() });
                }
            }
            if !ii.hbmColor.is_null() {
                DeleteObject(ii.hbmColor);
            }
            if !ii.hbmMask.is_null() {
                DeleteObject(ii.hbmMask);
            }
            DestroyIcon(icon);
            out
        }
    }

    trait WideLossy {
        fn encode_wide_lossy(&self) -> Vec<u16>;
    }

    impl WideLossy for std::ffi::OsStr {
        fn encode_wide_lossy(&self) -> Vec<u16> {
            use std::os::windows::ffi::OsStrExt;
            self.encode_wide().chain([0]).collect()
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod imp {
    use super::{IconSource, Kind};
    use std::path::Path;

    pub fn reveal(_path: &Path) -> Result<(), String> {
        Err("not available here".to_owned())
    }

    pub fn icon_for(_t: &str, _kind: Kind, _size: u16) -> Option<IconSource> {
        None
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn desktop_entries_read_their_main_group_only() {
        let dir = tempfile::tempdir().expect("temp");
        let path = dir.path().join("x.desktop");
        std::fs::write(
            &path,
            "[Desktop Entry]\nName=Calc\nIcon=calc\nName=Second\n[Desktop Action new]\nName=New\n",
        )
        .expect("write");
        let e = read_entry(&path);
        assert_eq!(e.get("Name").map(String::as_str), Some("Calc"));
        assert_eq!(e.get("Icon").map(String::as_str), Some("calc"));
        assert_eq!(e.len(), 2);
    }

    #[test]
    fn a_missing_file_can_not_be_revealed() {
        assert_eq!(reveal("/no/such/thing"), Err("/no/such/thing does not exist.".to_owned()));
    }
}
