//! Putting our windows where the app wants them, keeping some from taking the keyboard, and raising and
//! focusing them. Windows are known by their title (each viewport's title is unique, markers included).
//!
//! On Windows winit already places windows, keeps them on top, out of the taskbar and click-through; the one
//! thing it can not do is stop a click from activating a window, so that backend adds `WS_EX_NOACTIVATE` and
//! takes the foreground the way `core/widgets.py` does. On Hyprland a window can not place itself at all, so
//! that backend hands every wish to lum-hypr's `WindowSync`.

#[cfg(windows)]
use crate::desktop::marker;
use std::sync::Arc;

/// Where one of our windows should be, as drawn this frame.
#[derive(Debug, Clone, Copy)]
pub struct Placement<'a> {
    /// The full title, markers included.
    pub title: &'a str,
    /// Top-left corner in desktop coordinates. None: a rule or the system places it.
    pub at: Option<(i32, i32)>,
    /// Give it the keyboard once it has opened.
    pub focus: bool,
}

impl Placement<'_> {
    #[cfg(windows)]
    fn quiet(&self) -> bool {
        marker::has(self.title, marker::NO_ACTIVATE) || marker::has(self.title, marker::NO_FOCUS)
    }
}

pub trait WindowPlacer: Send + Sync {
    /// Called every frame the window is drawn. Cheap when nothing changed, so callers need not remember.
    fn shown(&self, window: &Placement<'_>);
    /// Gives the window the keyboard.
    fn focus(&self, title: &str);
    /// Puts the window above our other windows.
    fn raise(&self, title: &str);
}

/// For systems where the windows place themselves (X11, other Wayland desktops) and for tests.
pub fn none() -> Arc<dyn WindowPlacer> {
    Arc::new(NoPlacer)
}

struct NoPlacer;

impl WindowPlacer for NoPlacer {
    fn shown(&self, _window: &Placement<'_>) {}
    fn focus(&self, _title: &str) {}
    fn raise(&self, _title: &str) {}
}

#[cfg(target_os = "linux")]
pub(crate) mod hyprland {
    use super::{Placement, WindowPlacer};
    use lum_hypr::sync::{Want, WindowSync};
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Remembers the last wish per title, so drawing a window again sends nothing.
    pub struct HyprPlacer {
        sync: WindowSync,
        last: Mutex<HashMap<String, Option<(i32, i32)>>>,
    }

    impl HyprPlacer {
        pub fn new(sync: WindowSync) -> HyprPlacer {
            HyprPlacer { sync, last: Mutex::new(HashMap::new()) }
        }

        /// A window closed: the next time a window with this title is drawn it is a new one.
        pub fn forget_all(&self) {
            self.last.lock().expect("the placer's lock is never poisoned").clear();
        }
    }

    impl WindowPlacer for HyprPlacer {
        fn shown(&self, window: &Placement<'_>) {
            let mut last = self.last.lock().expect("the placer's lock is never poisoned");
            if last.get(window.title) == Some(&window.at) {
                return;
            }
            last.insert(window.title.to_owned(), window.at);
            self.sync.want(Want { title: window.title.to_owned(), at: window.at, focus: window.focus });
        }

        fn focus(&self, title: &str) {
            self.sync.focus(title);
        }

        fn raise(&self, title: &str) {
            self.sync.raise(title);
        }
    }
}

#[cfg(windows)]
pub fn windows() -> Arc<dyn WindowPlacer> {
    Arc::new(win::WinPlacer::default())
}

#[cfg(windows)]
mod win {
    use super::{Placement, WindowPlacer};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, TRUE};
    use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentProcessId, GetCurrentThreadId};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, EnumWindows, GWL_EXSTYLE, GetForegroundWindow, GetWindowLongPtrW, GetWindowTextW,
        GetWindowThreadProcessId, HWND_TOPMOST, IsWindow, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
        SWP_NOSIZE, SWP_NOZORDER, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, WS_EX_NOACTIVATE,
    };
    use windows_sys::core::BOOL;

    /// Window handles by title. A handle is kept as a number: it is only ever handed back to the system.
    #[derive(Default)]
    pub struct WinPlacer {
        known: Mutex<HashMap<String, isize>>,
    }

    fn title_of(hwnd: HWND) -> String {
        let mut buf = [0u16; 512];
        // SAFETY: the buffer is as long as we say; a stale handle just gives back 0 characters.
        let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }

    struct Search {
        title: Vec<u16>,
        pid: u32,
        found: HWND,
    }

    unsafe extern "system" fn visit(hwnd: HWND, data: LPARAM) -> BOOL {
        // SAFETY: `data` is the &mut Search find() passed in, alive for the whole enumeration.
        let search = unsafe { &mut *(data as *mut Search) };
        let mut pid = 0u32;
        // SAFETY: Windows hands us a live handle for the length of the callback; pid is a valid out pointer.
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if pid == search.pid && title_of(hwnd).encode_utf16().eq(search.title.iter().copied()) {
            search.found = hwnd;
            return 0;
        }
        TRUE
    }

    fn find(title: &str) -> Option<HWND> {
        // SAFETY: no preconditions.
        let pid = unsafe { GetCurrentProcessId() };
        let mut search = Search { title: title.encode_utf16().collect(), pid, found: std::ptr::null_mut() };
        // SAFETY: the callback only runs during this call, while `search` is borrowed mutably here.
        unsafe { EnumWindows(Some(visit), &mut search as *mut Search as LPARAM) };
        (!search.found.is_null()).then_some(search.found)
    }

    impl WinPlacer {
        /// The window's handle, found once and checked each time after (a window that closed and opened
        /// again under the same title has a new one).
        fn handle(&self, title: &str) -> (Option<HWND>, bool) {
            let mut known = self.known.lock().expect("the placer's lock is never poisoned");
            if let Some(&h) = known.get(title) {
                let hwnd = h as HWND;
                // SAFETY: IsWindow accepts any value and says whether it is a live window.
                if unsafe { IsWindow(hwnd) } != 0 && title_of(hwnd) == title {
                    return (Some(hwnd), false);
                }
            }
            let found = find(title);
            match found {
                Some(hwnd) => known.insert(title.to_owned(), hwnd as isize),
                None => known.remove(title),
            };
            (found, true)
        }
    }

    impl WindowPlacer for WinPlacer {
        fn shown(&self, window: &Placement<'_>) {
            if !window.quiet() {
                return;
            }
            let (Some(hwnd), true) = self.handle(window.title) else { return };
            // SAFETY: `hwnd` is a live window of this process; changing its extended style is allowed from any
            // thread of the process, and SetWindowPos with these flags only makes the change take effect.
            unsafe {
                let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | WS_EX_NOACTIVATE as isize);
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );
            }
        }

        /// Windows refuses SetForegroundWindow from a background process, so a window opened by a global
        /// hotkey used to appear without keyboard focus. Attaching to the foreground thread's input lifts that.
        fn focus(&self, title: &str) {
            let (Some(hwnd), _) = self.handle(title) else { return };
            // SAFETY: all handles are live or null (the calls then fail harmlessly). The thread attachment is
            // undone before returning, and it is made from this thread, which owns our windows (the UI thread).
            unsafe {
                let fg = GetForegroundWindow();
                if fg.is_null() || fg == hwnd {
                    return;
                }
                let me = GetCurrentThreadId();
                let other = GetWindowThreadProcessId(fg, std::ptr::null_mut());
                let attached = other != 0 && other != me && AttachThreadInput(other, me, TRUE) != 0;
                BringWindowToTop(hwnd);
                SetForegroundWindow(hwnd);
                if attached {
                    AttachThreadInput(other, me, 0);
                }
            }
        }

        fn raise(&self, title: &str) {
            let (Some(hwnd), _) = self.handle(title) else { return };
            // SAFETY: a live window of ours; the flags keep its place, size and focus.
            unsafe { SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) };
        }
    }
}
