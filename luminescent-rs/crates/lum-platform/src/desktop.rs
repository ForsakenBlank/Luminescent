//! Screens, the window rules our windows need and the desktop session the app runs in.
//!
//! Wayland lets no program place its own windows or keep them from taking the focus, so on Hyprland each
//! window carries an invisible marker in its title and window rules (run as Lua inside Hyprland, from
//! lum-hypr) do the rest, the same way `core/hypr.py` does it. Everywhere else most of this does nothing.

use crate::window::{self, WindowPlacer};
use std::sync::Arc;

/// The Wayland app id, which Hyprland calls the window class.
pub const APP_ID: &str = "luminescent";

/// Title markers, the same characters as the Python app. They are invisible in a title.
pub mod marker {
    pub use lum_hypr::marker::*;
}

/// The unseen main window's title. It is a transparent pixel; on Hyprland a rule keeps it small, unfocused and
/// in a corner.
pub const ROOT_TITLE: &str = "Luminescent root";

#[derive(Debug, Clone, PartialEq)]
pub struct Monitor {
    pub name: String,
    /// Top-left corner in desktop coordinates (logical pixels on Hyprland, physical on Windows).
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub primary: bool,
    /// What bars and docks keep for themselves at the top and the bottom (they stay above our windows).
    pub top_inset: u32,
    pub bottom_inset: u32,
}

/// A window to open full size on one monitor (Hyprland only places it if a rule says so before it maps).
#[derive(Debug, Clone)]
pub struct Cover {
    pub title: String,
    pub monitor: String,
}

/// The title of the window a cover prefix opens on `monitor` (see [`Desktop::start`]). The space at the end
/// keeps `DP-1` from also matching `DP-10`.
pub fn cover_title(prefix: &str, monitor: &str) -> String {
    format!("{prefix} on {monitor} ")
}

/// Every screen, the primary one first. Empty when the system can not say (other Linux desktops).
pub fn monitors() -> Vec<Monitor> {
    let mut list = imp::monitors();
    list.sort_by_key(|m| !m.primary);
    list
}

/// Where the mouse pointer is, in desktop coordinates, when the system says.
pub fn cursor() -> Option<(i32, i32)> {
    imp::cursor()
}

/// The compositor places our windows through rules (Hyprland), so a window's own position means nothing.
pub fn places_windows_by_rule() -> bool {
    #[cfg(target_os = "linux")]
    return lum_hypr::Hypr::from_env().is_some();
    #[cfg(not(target_os = "linux"))]
    return false;
}

/// Installs the rules for a separate app id (the spikes'): the same rules as the app's, the hidden main
/// window and `covers`, in a table of their own. `Ok(false)` means there is nothing to do here.
pub fn install_rules(app_id: &str, covers: &[Cover]) -> Result<bool, String> {
    imp::install_rules(app_id, covers)
}

/// Switches off the rules `install_rules` made for this app id.
pub fn remove_rules(app_id: &str) {
    imp::remove_rules(app_id)
}

/// Where the compositor really put this process's windows, one line each, for the spike's report. Empty when
/// the system does not say.
pub fn describe_windows() -> Vec<String> {
    imp::describe_windows()
}

/// Something the desktop told us.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesktopEvent {
    /// A bind of ours fired: its id, and when we heard of it (for the hotkey latency counter).
    Bind { id: String, at: std::time::Instant },
    /// The compositor reloaded its config. Our rules are back already; binds must be made again.
    Reloaded,
    /// A monitor was plugged in or pulled out.
    MonitorsChanged,
    /// Another program opened a window.
    ForeignWindow { addr: String, class: String },
}

/// The desktop the app runs in: who places the windows and where desktop events come from.
pub struct Desktop {
    placer: Arc<dyn WindowPlacer>,
    #[cfg(target_os = "linux")]
    hyprland: Option<hyprland::Session>,
}

impl Desktop {
    /// Sets the desktop up for our windows; call it before the first one opens. Each of `cover_prefixes`
    /// opens full size on a monitor when its title is [`cover_title`] (the dims and the overlay backdrops).
    pub fn start(cover_prefixes: &[&str]) -> Desktop {
        #[cfg(target_os = "linux")]
        if let Some(session) = hyprland::Session::start(cover_prefixes) {
            return Desktop { placer: session.placer(), hyprland: Some(session) };
        }
        let _ = cover_prefixes;
        #[cfg(windows)]
        return Desktop { placer: window::windows() };
        #[cfg(not(windows))]
        Desktop::none()
    }

    /// Nothing to place or listen to (tests, `--smoke`, no display).
    pub fn none() -> Desktop {
        Desktop {
            placer: window::none(),
            #[cfg(target_os = "linux")]
            hyprland: None,
        }
    }

    pub fn placer(&self) -> Arc<dyn WindowPlacer> {
        self.placer.clone()
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn hypr(&self) -> Option<&lum_hypr::Hypr> {
        self.hyprland.as_ref().map(|s| &s.hypr)
    }

    /// Plain words for the log and `ctl status`.
    pub fn describe(&self) -> &'static str {
        #[cfg(target_os = "linux")]
        if self.hyprland.is_some() {
            return "Hyprland";
        }
        if cfg!(windows) { "Windows" } else { "no window placement" }
    }

    /// Starts listening for desktop events on `runtime`; each one goes to `sink`.
    pub fn listen(
        &self,
        runtime: &tokio::runtime::Handle,
        sink: impl Fn(DesktopEvent) + Send + Sync + 'static,
    ) {
        #[cfg(target_os = "linux")]
        if let Some(session) = &self.hyprland {
            session.listen(runtime, Arc::new(sink));
        }
        #[cfg(not(target_os = "linux"))]
        let _ = (runtime, sink);
    }

    /// Makes another program's window float above ours, centred on the area `[x, y, width, height]` (the
    /// screen with the mouse). Only Hyprland can do this; elsewhere windows already open on top.
    pub fn float_foreign(&self, addr: &str, area: [i32; 4]) {
        #[cfg(target_os = "linux")]
        if let Some(session) = &self.hyprland {
            let [x, y, w, h] = area;
            session.sync().float_on_top(addr, lum_hypr::sync::Area { x, y, w, h });
        }
        #[cfg(not(target_os = "linux"))]
        let _ = (addr, area);
    }

    /// Puts a window floated by [`Desktop::float_foreign`] back into the tiling layout.
    pub fn unfloat_foreign(&self, addr: &str) {
        #[cfg(target_os = "linux")]
        if let Some(session) = &self.hyprland {
            session.sync().unfloat(addr);
        }
        #[cfg(not(target_os = "linux"))]
        let _ = addr;
    }

    /// Our binds go quiet; call it on the way out.
    pub fn shutdown(&self) {
        #[cfg(target_os = "linux")]
        if let Some(session) = &self.hyprland {
            session.shutdown();
        }
    }
}

#[cfg(target_os = "linux")]
mod hyprland {
    use super::{DesktopEvent, ROOT_TITLE, cover_title};
    use crate::window::WindowPlacer;
    use crate::window::hyprland::HyprPlacer;
    use lum_hypr::events::{self, Event};
    use lum_hypr::lua::Cover;
    use lum_hypr::sync::WindowSync;
    use lum_hypr::{Hypr, fifo};
    use std::sync::Arc;

    pub struct Session {
        pub hypr: Hypr,
        sync: WindowSync,
        placer: Arc<HyprPlacer>,
        covers: Arc<Vec<String>>,
    }

    impl Session {
        pub fn start(cover_prefixes: &[&str]) -> Option<Session> {
            let hypr = Hypr::from_env()?;
            // a test copy (LUMINESCENT_NO_HOOK) must not wipe the binds of the real one
            if std::env::var_os("LUMINESCENT_NO_HOOK").is_none()
                && let Err(e) = hypr.bootstrap()
            {
                tracing::warn!("could not install the Hyprland helpers: {e}");
            }
            let covers: Arc<Vec<String>> = Arc::new(cover_prefixes.iter().map(|p| (*p).to_owned()).collect());
            install_extras(&hypr, &covers);
            let (sync, _thread) = WindowSync::start(hypr.clone());
            let placer = Arc::new(HyprPlacer::new(sync.clone()));
            Some(Session { hypr, sync, placer, covers })
        }

        pub fn placer(&self) -> Arc<dyn WindowPlacer> {
            self.placer.clone()
        }

        pub fn sync(&self) -> &WindowSync {
            &self.sync
        }

        pub fn listen(
            &self,
            runtime: &tokio::runtime::Handle,
            sink: Arc<dyn Fn(DesktopEvent) + Send + Sync>,
        ) {
            let (hypr, sync, placer, covers) =
                (self.hypr.clone(), self.sync.clone(), self.placer.clone(), self.covers.clone());
            let app_id = hypr.names().app_id.clone();
            let to_app = sink.clone();
            let again = hypr.clone();
            runtime.spawn(events::listen(hypr, move |event| {
                sync.event(&event);
                match event {
                    Event::OpenWindow { addr, class, .. } if class != app_id => {
                        to_app(DesktopEvent::ForeignWindow { addr, class })
                    }
                    Event::CloseWindow { .. } => placer.forget_all(),
                    Event::MonitorsChanged => {
                        install_extras(&again, &covers);
                        to_app(DesktopEvent::MonitorsChanged);
                    }
                    Event::ConfigReloaded => {
                        install_extras(&again, &covers);
                        to_app(DesktopEvent::Reloaded);
                    }
                    _ => {}
                }
            }));
            let path = self.hypr.names().fifo.clone();
            runtime.spawn(async move {
                if let Err(e) = fifo::listen(&path, move |id| {
                    sink(DesktopEvent::Bind { id, at: std::time::Instant::now() })
                })
                .await
                {
                    tracing::error!("could not read {}: {e}", path.display());
                }
            });
        }

        pub fn shutdown(&self) {
            self.sync.stop();
            self.hypr.shutdown();
        }
    }

    /// The rules the Python app does not have: the hidden main window's, and a cover rule per prefix and
    /// monitor. They go after the bootstrap and again whenever monitors change or the config reloads.
    fn install_extras(hypr: &Hypr, prefixes: &[String]) {
        if let Err(e) = hypr.root_rule(ROOT_TITLE) {
            tracing::warn!("could not set the main window's rule: {e}");
        }
        let covers: Vec<Cover> = hypr
            .monitors()
            .iter()
            .flat_map(|m| {
                let (width, height) = m.logical_size();
                prefixes.iter().map(move |p| Cover {
                    title: cover_title(p, &m.name),
                    monitor: m.name.clone(),
                    width,
                    height,
                })
            })
            .collect();
        if let Err(e) = hypr.cover_monitors(&covers) {
            tracing::warn!("could not set the cover rules: {e}");
        }
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{Cover, Monitor, ROOT_TITLE};
    use lum_hypr::lua::{self, Names};
    use lum_hypr::{Hypr, marker};

    pub fn monitors() -> Vec<Monitor> {
        let Some(hypr) = Hypr::from_env() else { return Vec::new() };
        hypr.monitors()
            .into_iter()
            .map(|m| {
                let (width, height) = m.logical_size();
                let [_, top, _, bottom] = m.reserved();
                Monitor {
                    name: m.name,
                    x: m.x,
                    y: m.y,
                    width,
                    height,
                    scale: m.scale.max(0.1) as f32,
                    primary: m.focused,
                    top_inset: top.max(0) as u32,
                    bottom_inset: bottom.max(0) as u32,
                }
            })
            .collect()
    }

    pub fn cursor() -> Option<(i32, i32)> {
        Hypr::from_env()?.cursor_pos()
    }

    fn spike(app_id: &str) -> Option<Hypr> {
        let runtime = lum_hypr::socket::runtime_dir()?;
        let hypr = Hypr::from_env()?;
        // its own table and class, so it never touches a running Luminescent
        let names = Names {
            table: "LUMSPIKE".to_owned(),
            app_id: app_id.to_owned(),
            fifo: runtime.join("luminescent").join("spike.fifo"),
        };
        Some(Hypr::at(hypr.socket_path().parent()?, names))
    }

    pub fn install_rules(app_id: &str, covers: &[Cover]) -> Result<bool, String> {
        let Some(hypr) = spike(app_id) else { return Ok(false) };
        hypr.bootstrap()?;
        hypr.root_rule(ROOT_TITLE)?;
        let screens = hypr.monitors();
        let covers: Vec<lua::Cover> = covers
            .iter()
            .filter_map(|c| {
                let m = screens.iter().find(|m| m.name == c.monitor)?;
                let (width, height) = m.logical_size();
                Some(lua::Cover { title: c.title.clone(), monitor: c.monitor.clone(), width, height })
            })
            .collect();
        hypr.cover_monitors(&covers)?;
        Ok(true)
    }

    pub fn remove_rules(app_id: &str) {
        let Some(hypr) = spike(app_id) else { return };
        let t = &hypr.names().table;
        let code = format!(
            "if {t} then for _, r in pairs({t}.rules or {{}}) do pcall(function() r:set_enabled(false) end) end \
             {t}.rules_version = nil if {t}.root_rule then pcall(function() {t}.root_rule:set_enabled(false) end) end end"
        );
        let cleared = hypr.lua(&code).and_then(|()| hypr.lua(&lua::clear_cover_rules(hypr.names())));
        if let Err(e) = cleared {
            tracing::warn!("could not switch off the window rules: {e}");
        }
    }

    pub fn describe_windows() -> Vec<String> {
        let Some(hypr) = Hypr::from_env() else { return Vec::new() };
        let me = i64::from(std::process::id());
        hypr.clients()
            .into_iter()
            .filter(|c| c.pid == me)
            .map(|c| {
                let title = marker::strip(&c.title);
                format!(
                    "\"{title}\" at {},{} size {}x{} on monitor {} floating={} pinned={} focused={}",
                    c.at.0,
                    c.at.1,
                    c.size.0,
                    c.size.1,
                    c.monitor,
                    c.floating,
                    c.pinned,
                    c.focus_history_id == 0
                )
            })
            .collect()
    }
}

#[cfg(windows)]
mod imp {
    use super::{Cover, Monitor};
    use windows_sys::Win32::Foundation::{LPARAM, RECT, TRUE};
    use windows_sys::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
    };
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows_sys::Win32::UI::WindowsAndMessaging::MONITORINFOF_PRIMARY;
    use windows_sys::core::BOOL;

    unsafe extern "system" fn collect(monitor: HMONITOR, _dc: HDC, _rect: *mut RECT, data: LPARAM) -> BOOL {
        // SAFETY: `data` is the &mut Vec<Monitor> monitors() passed in, alive for the whole enumeration.
        let list = unsafe { &mut *(data as *mut Vec<Monitor>) };
        // SAFETY: plain reads of a monitor handle Windows just gave us, into structs sized for the call.
        unsafe {
            let mut info = std::mem::zeroed::<MONITORINFOEXW>();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            if GetMonitorInfoW(monitor, &mut info.monitorInfo) == 0 {
                return TRUE;
            }
            let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
            if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) != 0 {
                dpi_x = 96;
            }
            let r = info.monitorInfo.rcMonitor;
            let work = info.monitorInfo.rcWork;
            let end = info.szDevice.iter().position(|&c| c == 0).unwrap_or(info.szDevice.len());
            list.push(Monitor {
                name: String::from_utf16_lossy(&info.szDevice[..end]),
                x: r.left,
                y: r.top,
                width: (r.right - r.left).max(0) as u32,
                height: (r.bottom - r.top).max(0) as u32,
                scale: dpi_x as f32 / 96.0,
                primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
                top_inset: (work.top - r.top).max(0) as u32,
                bottom_inset: (r.bottom - work.bottom).max(0) as u32,
            });
        }
        TRUE
    }

    pub fn cursor() -> Option<(i32, i32)> {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
        let mut at = POINT { x: 0, y: 0 };
        // SAFETY: writes into the POINT it is given.
        (unsafe { GetCursorPos(&mut at) } != 0).then_some((at.x, at.y))
    }

    pub fn monitors() -> Vec<Monitor> {
        let mut list: Vec<Monitor> = Vec::new();
        // SAFETY: the callback only runs during this call, while `list` is borrowed mutably here.
        unsafe {
            EnumDisplayMonitors(
                std::ptr::null_mut(),
                std::ptr::null(),
                Some(collect),
                &mut list as *mut _ as LPARAM,
            );
        }
        list
    }

    pub fn install_rules(_app_id: &str, _covers: &[Cover]) -> Result<bool, String> {
        Ok(false)
    }

    pub fn remove_rules(_app_id: &str) {}

    pub fn describe_windows() -> Vec<String> {
        Vec::new()
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod imp {
    use super::{Cover, Monitor};

    pub fn monitors() -> Vec<Monitor> {
        Vec::new()
    }

    pub fn cursor() -> Option<(i32, i32)> {
        None
    }

    pub fn install_rules(_app_id: &str, _covers: &[Cover]) -> Result<bool, String> {
        Ok(false)
    }

    pub fn remove_rules(_app_id: &str) {}

    pub fn describe_windows() -> Vec<String> {
        Vec::new()
    }
}

/// Whether windows can be opened at all. On Linux that needs a Wayland or X11 display; CI and SSH sessions
/// have none, and the app then runs without windows.
pub fn has_display() -> bool {
    if cfg!(target_os = "linux") {
        ["WAYLAND_DISPLAY", "DISPLAY"].iter().any(|v| std::env::var_os(v).is_some_and(|v| !v.is_empty()))
    } else {
        true
    }
}
