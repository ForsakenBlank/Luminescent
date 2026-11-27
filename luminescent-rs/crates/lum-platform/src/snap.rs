//! Moving another program's window for the window snapper. On Hyprland the window is made floating and then
//! sized and moved inside its monitor's free area (the bars stay clear); "back into tiling" floats it no more.
//! On Windows the window is moved inside its screen's work area, a maximised window is restored first, and
//! Windows 10 and 11's invisible resize borders are allowed for so the gaps look even; "back" puts it where
//! it was. Every call asks the system, so make them off the UI thread.

pub use lum_screen::snap::Area;

/// The window that has the keyboard, and the free area of its screen.
#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    /// Hyprland's address, or the window handle on Windows.
    pub id: String,
    pub floating: bool,
    /// The screen minus bars, docks and the taskbar, in desktop coordinates.
    pub area: Area,
    /// Real pixels per layout pixel on that screen (Windows sizes the gap by it).
    pub scale: f64,
}

/// What [`active`] found.
#[derive(Debug, Clone, PartialEq)]
pub enum Found {
    Window(Window),
    /// Nothing to snap: the desktop, nothing focused, or one of our own windows (they snap with their own
    /// buttons).
    Nothing,
    /// This desktop can not move other programs' windows.
    Unsupported,
}

pub fn active() -> Found {
    imp::active()
}

/// Puts the window at `rect` (x, y, width, height).
pub fn place(window: &Window, rect: Area) {
    imp::place(window, rect)
}

/// Where the window is now (Windows: kept so "back" can put it there again).
pub fn rect(window: &Window) -> Option<Area> {
    imp::rect(window)
}

/// Undoes the snapping: back into the tiling layout (Hyprland), or back to `before` (Windows).
pub fn put_back(window: &Window, before: Option<Area>) {
    imp::put_back(window, before)
}

/// The same calls against a given Hyprland (the fake one in tests).
#[cfg(target_os = "linux")]
pub mod hyprland {
    use super::{Area, Found, Window};
    use lum_hypr::{Hypr, lua};

    pub fn active(hypr: &Hypr) -> Found {
        let Some(win) = hypr.query("activewindow") else { return Found::Nothing };
        let text = |k: &str| win.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_owned();
        let (addr, class) = (text("address"), text("class"));
        if addr.is_empty() || class == crate::desktop::APP_ID {
            return Found::Nothing;
        }
        let floating = win.get("floating").and_then(serde_json::Value::as_bool).unwrap_or(false);
        let on = win.get("monitor").and_then(serde_json::Value::as_i64);
        let monitors = hypr.monitors();
        let Some(m) =
            monitors.iter().find(|m| Some(m.id) == on).or_else(|| monitors.iter().find(|m| m.focused))
        else {
            return Found::Nothing;
        };
        let area =
            lum_screen::snap::usable_area(m.x, m.y, m.width, m.height, m.scale, m.transform, m.reserved());
        Found::Window(Window { id: addr, floating, area, scale: m.scale })
    }

    pub fn place(hypr: &Hypr, window: &Window, (x, y, w, h): Area) {
        let mut code = String::new();
        if !window.floating {
            code.push_str(&lua::float(&window.id, true));
            code.push(' ');
        }
        code.push_str(&format!("{} {}", lua::resize(&window.id, w, h), lua::move_other(&window.id, x, y)));
        if let Err(e) = hypr.lua(&code) {
            tracing::warn!("could not snap the window: {e}");
        }
    }

    pub fn put_back(hypr: &Hypr, window: &Window) {
        if window.floating
            && let Err(e) = hypr.lua(&lua::float(&window.id, false))
        {
            tracing::warn!("could not put the window back into tiling: {e}");
        }
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{Area, Found, Window, hyprland};
    use lum_hypr::Hypr;

    pub fn active() -> Found {
        match Hypr::from_env() {
            Some(hypr) => hyprland::active(&hypr),
            None => Found::Unsupported,
        }
    }

    pub fn place(window: &Window, rect: Area) {
        if let Some(hypr) = Hypr::from_env() {
            hyprland::place(&hypr, window, rect);
        }
    }

    pub fn rect(_window: &Window) -> Option<Area> {
        None
    }

    pub fn put_back(window: &Window, _before: Option<Area>) {
        if let Some(hypr) = Hypr::from_env() {
            hyprland::put_back(&hypr, window);
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::{Area, Found, Window};
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId, IsIconic,
        IsWindowVisible, IsZoomed, SW_RESTORE, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos, ShowWindow,
    };

    fn hwnd_of(window: &Window) -> HWND {
        window.id.parse::<isize>().unwrap_or(0) as HWND
    }

    fn empty() -> RECT {
        RECT { left: 0, top: 0, right: 0, bottom: 0 }
    }

    pub fn active() -> Found {
        // SAFETY: plain queries about the foreground window into buffers and structs sized for each call.
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() || IsWindowVisible(hwnd) == 0 {
                return Found::Nothing;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == GetCurrentProcessId() {
                return Found::Nothing;
            }
            let mut class = [0u16; 64];
            let n = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32).max(0) as usize;
            let class = String::from_utf16_lossy(&class[..n]);
            if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd") {
                return Found::Nothing;
            }
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut info) == 0 {
                return Found::Nothing;
            }
            let r = info.rcWork;
            let dpi = GetDpiForWindow(hwnd);
            let scale = if dpi == 0 { 1.0 } else { f64::from(dpi) / 96.0 };
            Found::Window(Window {
                id: (hwnd as isize).to_string(),
                floating: true,
                area: (r.left, r.top, r.right - r.left, r.bottom - r.top),
                scale,
            })
        }
    }

    pub fn rect(window: &Window) -> Option<Area> {
        let mut r = empty();
        // SAFETY: a RECT to write into; a stale handle just fails.
        (unsafe { GetWindowRect(hwnd_of(window), &mut r) } != 0).then_some((
            r.left,
            r.top,
            r.right - r.left,
            r.bottom - r.top,
        ))
    }

    /// Windows 10 and 11 windows have invisible resize borders: left, top, right and bottom of them.
    fn borders(hwnd: HWND) -> [i32; 4] {
        let (mut outer, mut frame) = (empty(), empty());
        // SAFETY: both RECTs are written by the calls, sized as they expect.
        unsafe {
            GetWindowRect(hwnd, &mut outer);
            let got = DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS as u32,
                (&mut frame as *mut RECT).cast(),
                std::mem::size_of::<RECT>() as u32,
            );
            if got != 0 {
                return [0; 4];
            }
        }
        [
            frame.left - outer.left,
            frame.top - outer.top,
            outer.right - frame.right,
            outer.bottom - frame.bottom,
        ]
    }

    fn move_to(hwnd: HWND, (x, y, w, h): Area) {
        // SAFETY: a live window of another program; these calls only restore and move it.
        unsafe {
            if IsZoomed(hwnd) != 0 || IsIconic(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            }
            SetWindowPos(hwnd, std::ptr::null_mut(), x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }

    pub fn place(window: &Window, rect: Area) {
        let hwnd = hwnd_of(window);
        // SAFETY: as above; a maximised window ignores SetWindowPos, so it is restored before the borders are read
        unsafe {
            if IsZoomed(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            }
        }
        move_to(hwnd, lum_screen::snap::with_borders(rect, borders(hwnd)));
    }

    pub fn put_back(window: &Window, before: Option<Area>) {
        if let Some(rect) = before {
            move_to(hwnd_of(window), rect);
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod imp {
    use super::{Area, Found, Window};

    pub fn active() -> Found {
        Found::Unsupported
    }

    pub fn place(_window: &Window, _rect: Area) {}

    pub fn rect(_window: &Window) -> Option<Area> {
        None
    }

    pub fn put_back(_window: &Window, _before: Option<Area>) {}
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use lum_hypr::fake::FakeHyprland;

    const MONITORS: &str = r#"[
        {"id": 0, "name": "DP-1", "x": 0, "y": 0, "width": 1920, "height": 1080, "scale": 1.0, "transform": 0,
         "focused": false, "reserved": [0, 30, 0, 0]},
        {"id": 1, "name": "DP-2", "x": 1920, "y": 0, "width": 3840, "height": 2160, "scale": 1.5, "transform": 0,
         "focused": true, "reserved": [0, 40, 0, 0]}
    ]"#;

    fn fake(active: &'static str) -> FakeHyprland {
        let fake = FakeHyprland::start();
        fake.set_monitors(MONITORS);
        fake.answer(move |c| (c == "j/activewindow").then(|| active.to_owned()));
        fake
    }

    #[test]
    fn the_active_window_and_its_monitor_s_free_area() {
        let fake = fake(r#"{"address": "0xabc", "class": "kitty", "floating": false, "monitor": 1}"#);
        let found = hyprland::active(&fake.hypr());
        let want = Window { id: "0xabc".into(), floating: false, area: (1920, 40, 2560, 1400), scale: 1.5 };
        assert_eq!(found, Found::Window(want));
    }

    #[test]
    fn our_own_windows_and_nothing_focused_are_left_alone() {
        let ours = fake(r#"{"address": "0x1", "class": "luminescent", "monitor": 0}"#);
        assert_eq!(hyprland::active(&ours.hypr()), Found::Nothing);
        let none = fake(r#"{}"#);
        assert_eq!(hyprland::active(&none.hypr()), Found::Nothing);
    }

    #[test]
    fn a_tiled_window_is_floated_sized_and_moved_in_one_go() {
        let fake = fake("{}");
        let w = Window { id: "0xabc".into(), floating: false, area: (0, 30, 1920, 1050), scale: 1.0 };
        hyprland::place(&fake.hypr(), &w, (8, 38, 948, 1034));
        let sent = fake.wait_for(|c| c.contains("resize")).expect("a command");
        let float = sent.find("window.float").expect("floated first");
        let resize = sent.find("window.resize").expect("then sized");
        let moved = sent.find("window.move").expect("then moved");
        assert!(float < resize && resize < moved, "{sent}");
        assert!(sent.contains("x = 948, y = 1034") && sent.contains("x = 8, y = 38"), "{sent}");
        fake.clear();
        hyprland::put_back(&fake.hypr(), &Window { floating: true, ..w });
        let back = fake.wait_for(|c| c.contains("window.float")).expect("unfloated");
        assert!(back.contains("disable"), "{back}");
    }
}
