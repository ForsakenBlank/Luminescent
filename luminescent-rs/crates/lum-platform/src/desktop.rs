//! Screens and the window rules our windows need, in the small form the UI spike and the hidden main window
//! use. Phase 3 replaces the Hyprland half with the full LUMI bootstrap and the WindowPlacer.
//!
//! Wayland lets no program place its own windows or keep them from taking the focus, so on Hyprland each
//! window carries an invisible marker in its title and window rules (run as Lua inside Hyprland) do the rest,
//! the same way `core/hypr.py` does it. Everywhere else the functions here do nothing.

/// The Wayland app id, which Hyprland calls the window class.
pub const APP_ID: &str = "luminescent";

/// Title markers, the same characters as the Python app. They are invisible in a title.
pub mod marker {
    /// Do not focus it when it opens.
    pub const NO_ACTIVATE: char = '\u{200b}';
    /// Never give it the keyboard (Hyprland then sends it no mouse input either).
    pub const NO_FOCUS: char = '\u{200c}';
}

/// The hidden main window's title. winit can not hide a window on Wayland, so a rule keeps it out of sight.
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
}

/// A window to open full size on one monitor (Hyprland only places it if a rule says so before it maps).
#[derive(Debug, Clone)]
pub struct Cover {
    pub title: String,
    pub monitor: String,
}

/// Every screen, the primary one first. Empty when the system can not say (other Linux desktops).
pub fn monitors() -> Vec<Monitor> {
    let mut list = imp::monitors();
    list.sort_by_key(|m| !m.primary);
    list
}

/// The compositor places our windows through rules (Hyprland), so a window's own position means nothing.
pub fn places_windows_by_rule() -> bool {
    #[cfg(target_os = "linux")]
    return lum_hypr::socket::available();
    #[cfg(not(target_os = "linux"))]
    return false;
}

/// Installs the rules for windows with this app id: floating, on top, no borders, the title markers, the
/// hidden main window and `covers`. Call it before the windows open. `Ok(false)` means there is nothing to
/// do here (not Hyprland).
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

#[cfg(target_os = "linux")]
mod imp {
    use super::{Cover, Monitor, ROOT_TITLE, marker};
    use lum_hypr::socket::{self, lstr};
    use serde_json::Value;

    pub fn monitors() -> Vec<Monitor> {
        let Some(Value::Array(list)) = socket::query("monitors") else {
            return Vec::new();
        };
        list.iter()
            .filter_map(|m| {
                let scale = m.get("scale")?.as_f64()?.max(0.1);
                let int = |key: &str| m.get(key).and_then(Value::as_i64);
                // Hyprland reports the mode in pixels; the layout uses them divided by the scale
                let (w, h) = (int("width")? as f64, int("height")? as f64);
                let turned = int("transform").is_some_and(|t| t % 2 == 1);
                let (w, h) = if turned { (h, w) } else { (w, h) };
                Some(Monitor {
                    name: m.get("name")?.as_str()?.to_owned(),
                    x: int("x")? as i32,
                    y: int("y")? as i32,
                    width: (w / scale).round() as u32,
                    height: (h / scale).round() as u32,
                    scale: scale as f32,
                    primary: m.get("focused").and_then(Value::as_bool).unwrap_or(false),
                })
            })
            .collect()
    }

    fn table(app_id: &str) -> String {
        // the Python app keeps its rules in LUMI; ours live apart so the two never switch off each other's
        format!("LUMRS[{}]", lstr(app_id))
    }

    pub fn install_rules(app_id: &str, covers: &[Cover]) -> Result<bool, String> {
        if !socket::available() {
            return Ok(false);
        }
        let class = lstr(&format!("^{app_id}$"));
        let title = |pattern: &str| lstr(&format!("^{pattern}$"));
        let mut rules = vec![
            format!(
                "{{ match = {{ class = {class} }}, float = true, pin = true, decorate = false, no_anim = true, \
                 no_shadow = true, no_blur = true, no_dim = true, border_size = 0, rounding = 0, \
                 opacity = \"1 override 1 override\" }}"
            ),
            format!(
                "{{ match = {{ class = {class}, title = {} }}, no_initial_focus = true }}",
                title(&format!(".*{}.*", marker::NO_ACTIVATE))
            ),
            format!(
                "{{ match = {{ class = {class}, title = {} }}, no_initial_focus = true, no_focus = true }}",
                title(&format!(".*{}.*", marker::NO_FOCUS))
            ),
            // the main window has to exist for the UI to run, but nobody should ever see it
            format!(
                "{{ match = {{ class = {class}, title = {} }}, no_initial_focus = true, no_focus = true, \
                 pin = false, size = \"1 1\", move = \"0 0\", opacity = \"0 override 0 override\" }}",
                title(&format!("{ROOT_TITLE}.*"))
            ),
        ];
        let screens = monitors();
        for cover in covers {
            let Some(m) = screens.iter().find(|m| m.name == cover.monitor) else { continue };
            // move is measured from the monitor's own corner
            rules.push(format!(
                "{{ match = {{ class = {class}, title = {} }}, monitor = {}, move = \"0 0\", size = \"{} {}\" }}",
                title(&format!("{}.*", cover.title)),
                lstr(&m.name),
                m.width,
                m.height
            ));
        }
        let t = table(app_id);
        let code = format!(
            "LUMRS = LUMRS or {{}} for _, r in pairs({t} or {{}}) do pcall(function() r:set_enabled(false) end) end \
             {t} = {{}} {}",
            rules
                .iter()
                .map(|r| format!("{t}[#{t} + 1] = hl.window_rule({r})"))
                .collect::<Vec<_>>()
                .join(" ")
        );
        socket::lua(&code).map(|()| true)
    }

    pub fn remove_rules(app_id: &str) {
        if !socket::available() {
            return;
        }
        let t = table(app_id);
        let code = format!(
            "if LUMRS then for _, r in pairs({t} or {{}}) do pcall(function() r:set_enabled(false) end) end \
             {t} = nil end"
        );
        if let Err(e) = socket::lua(&code) {
            tracing::warn!("could not switch off the window rules: {e}");
        }
    }

    pub fn describe_windows() -> Vec<String> {
        let Some(Value::Array(clients)) = socket::query("clients") else {
            return Vec::new();
        };
        let me = i64::from(std::process::id());
        clients
            .iter()
            .filter(|c| c.get("pid").and_then(Value::as_i64) == Some(me))
            .map(|c| {
                let pair = |key: &str| {
                    let v = c.get(key).and_then(Value::as_array);
                    let n = |i: usize| v.and_then(|v| v.get(i)).and_then(Value::as_i64).unwrap_or(0);
                    (n(0), n(1))
                };
                let ((x, y), (w, h)) = (pair("at"), pair("size"));
                let title: String = c
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .chars()
                    .filter(|ch| !matches!(ch, '\u{200b}'..='\u{2064}'))
                    .collect();
                let flag = |key: &str| c.get(key).and_then(Value::as_bool).unwrap_or(false);
                let focused = c.get("focusHistoryID").and_then(Value::as_i64) == Some(0);
                format!(
                    "\"{title}\" at {x},{y} size {w}x{h} on monitor {} floating={} pinned={} focused={focused}",
                    c.get("monitor").and_then(Value::as_i64).unwrap_or(-1),
                    flag("floating"),
                    flag("pinned"),
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
        EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW, MONITORINFOF_PRIMARY,
    };
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
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
            let end = info.szDevice.iter().position(|&c| c == 0).unwrap_or(info.szDevice.len());
            list.push(Monitor {
                name: String::from_utf16_lossy(&info.szDevice[..end]),
                x: r.left,
                y: r.top,
                width: (r.right - r.left).max(0) as u32,
                height: (r.bottom - r.top).max(0) as u32,
                scale: dpi_x as f32 / 96.0,
                primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
            });
        }
        TRUE
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
