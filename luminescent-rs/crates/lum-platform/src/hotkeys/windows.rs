//! Hotkeys on Windows: one low-level keyboard hook (`WH_KEYBOARD_LL`) for the whole app.
//!
//! Windows silently removes a low-level hook whose callback is slow, so the callback does nothing but note the
//! key, ask whether it was already down (one cheap call) and hand that to the matcher thread. The hook never
//! swallows keys: hotkeys also reach the focused app, as they did in the Python app.

use super::matcher::{KeyState, Matcher, Raw};
use super::{Backend, Options, Signal, Wake, vk};
use crossbeam_channel::{Receiver, Sender};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
    SendInput,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, MSG, PostThreadMessageW,
    SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN, WM_QUIT, WM_SYSKEYDOWN,
};

/// Keys we send carry this, so the hook does not read them back as the user's.
const OURS: usize = 0x4C55_4D49;

enum Msg {
    Key(Raw),
    Add(String, bool),
    Remove(String),
    AddHold(String),
    RemoveHold(String),
    Suspend(bool),
    Reset,
    WatchRelease(Option<String>),
    Stop,
}

/// Where the hook callback forwards to. A low-level hook callback gets no user data, so it is a static; there
/// is one hook per process.
static FORWARD: OnceLock<Sender<Msg>> = OnceLock::new();

unsafe extern "system" fn hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION Windows passes a valid KBDLLHOOKSTRUCT that lives for this call.
        let k = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        if k.dwExtraInfo != OURS
            && let Some(tx) = FORWARD.get()
        {
            let vk = k.vkCode as u16;
            // SAFETY: no preconditions. Windows has not counted this event yet, so this is the state before it.
            let was_down = unsafe { GetAsyncKeyState(i32::from(vk)) } < 0;
            let down = wparam == WM_KEYDOWN as usize || wparam == WM_SYSKEYDOWN as usize;
            let extended = k.flags & LLKHF_EXTENDED != 0;
            let _ = tx.send(Msg::Key(Raw { vk, extended, down, was_down, at: Instant::now() }));
        }
    }
    // SAFETY: passing the event on unchanged, as every hook must.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

/// Ctrl, Shift or a Windows key is down, other than `hold` itself.
pub fn modifiers_down(hold: &str) -> bool {
    let groups: [(&str, &[u16]); 3] =
        [("ctrl", &[0xA2, 0xA3]), ("shift", &[0xA0, 0xA1]), ("windows", &[0x5B, 0x5C])];
    groups
        .iter()
        .filter(|(name, _)| *name != hold)
        .flat_map(|(_, keys)| keys.iter())
        .any(|&vk| System.is_down(vk))
}

pub fn mask_menu_key() {
    // 0xE8 is unassigned: nothing reacts to it, but Windows no longer sees the Alt release as a lone tap
    send_inputs(&[key_input(0xE8, false), key_input(0xE8, true)]);
}

struct System;

impl KeyState for System {
    fn is_down(&self, vk: u16) -> bool {
        // SAFETY: no preconditions.
        unsafe { GetAsyncKeyState(i32::from(vk)) < 0 }
    }
}

pub struct HookKeys {
    tx: Sender<Msg>,
    hook_thread: u32,
}

impl HookKeys {
    pub fn start(signals: Sender<Signal>, wake: Wake) -> Result<HookKeys, String> {
        let (tx, rx) = crossbeam_channel::unbounded();
        FORWARD.set(tx.clone()).map_err(|_| "the keyboard is hooked already".to_owned())?;
        let (ready_tx, ready_rx) = crossbeam_channel::bounded(1);
        std::thread::Builder::new()
            .name("keyboard-hook".into())
            .spawn(move || run_hook(ready_tx))
            .map_err(|e| e.to_string())?;
        let hook_thread = ready_rx.recv_timeout(Duration::from_secs(5)).map_err(|e| e.to_string())??;
        std::thread::Builder::new()
            .name("hotkeys".into())
            .spawn(move || run_matcher(rx, signals, wake))
            .map_err(|e| e.to_string())?;
        Ok(HookKeys { tx, hook_thread })
    }
}

/// The hook lives on this thread, which only pumps messages (Windows calls the hook from its message loop).
fn run_hook(ready: Sender<Result<u32, String>>) {
    // SAFETY: installing a global hook with a callback that lives for the whole program; the module handle of
    // our own executable is what Windows wants for it.
    let handle =
        unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook), GetModuleHandleW(std::ptr::null()), 0) };
    if handle.is_null() {
        let _ = ready.send(Err(std::io::Error::last_os_error().to_string()));
        return;
    }
    // SAFETY: no preconditions.
    let _ = ready.send(Ok(unsafe { GetCurrentThreadId() }));
    // SAFETY: MSG is plain data; GetMessageW fills it and returns 0 on WM_QUIT, -1 on error.
    unsafe {
        let mut msg = std::mem::zeroed::<MSG>();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {}
        UnhookWindowsHookEx(handle);
    }
}

fn run_matcher(rx: Receiver<Msg>, signals: Sender<Signal>, wake: Wake) {
    let mut m = Matcher::new(System);
    // the cheat sheet's keys, checked every 50 ms until they are all up
    let mut release: Option<(String, Vec<Vec<u16>>)> = None;
    let emit = |out: Vec<Signal>| {
        if !out.is_empty() {
            for s in out {
                let _ = signals.send(s);
            }
            wake();
        }
    };
    loop {
        // the 100 ms check runs only while a key is held
        let msg = if m.busy() || release.is_some() {
            let every = if release.is_some() { 50 } else { 100 };
            match rx.recv_timeout(Duration::from_millis(every)) {
                Ok(msg) => msg,
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    emit(m.tick(Instant::now()));
                    // a key is down when any of its codes is (left or right Ctrl)
                    let up = release.as_ref().is_some_and(|(_, keys)| {
                        keys.iter().any(|codes| !codes.iter().any(|&c| System.is_down(c)))
                    });
                    if up && let Some((combo, _)) = release.take() {
                        emit(vec![Signal::Released { combo, unknown: false }]);
                    }
                    continue;
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return,
            }
        } else {
            match rx.recv() {
                Ok(msg) => msg,
                Err(_) => return,
            }
        };
        match msg {
            Msg::Key(raw) => emit(m.key(raw)),
            Msg::Add(combo, always) => m.add(&combo, always),
            Msg::Remove(combo) => m.remove(&combo),
            Msg::AddHold(key) => m.add_hold(&key),
            Msg::RemoveHold(key) => m.remove_hold(&key),
            Msg::Suspend(on) => m.suspend(on),
            Msg::Reset => emit(m.reset()),
            Msg::WatchRelease(combo) => {
                release = combo.map(|c| {
                    let keys = c.split('+').filter(|k| !k.is_empty()).map(vk::codes).collect();
                    (c, keys)
                })
            }
            Msg::Stop => return,
        }
    }
}

fn key_input(vk: u16, up: bool) -> INPUT {
    let extended = matches!(vk, 0x21..=0x28 | 0x2D | 0x2E | 0x5B | 0x5C | 0x5D | 0xA3 | 0xA5);
    let mut flags = if up { KEYEVENTF_KEYUP } else { 0 };
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: OURS },
        },
    }
}

fn send_inputs(inputs: &[INPUT]) {
    if inputs.is_empty() {
        return;
    }
    // SAFETY: the slice is valid for the call and each INPUT is fully set up as a keyboard event.
    unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
}

impl Backend for HookKeys {
    fn add(&mut self, combo: &str, opts: Options, _check_clash: bool) {
        let _ = self.tx.send(Msg::Add(combo.to_owned(), opts.always));
    }

    fn remove(&mut self, combo: &str) {
        let _ = self.tx.send(Msg::Remove(combo.to_owned()));
    }

    fn add_hold(&mut self, key: &str) {
        let _ = self.tx.send(Msg::AddHold(key.to_owned()));
    }

    fn remove_hold(&mut self, key: &str) {
        let _ = self.tx.send(Msg::RemoveHold(key.to_owned()));
    }

    fn suspend(&mut self, on: bool) {
        let _ = self.tx.send(Msg::Suspend(on));
    }

    fn reset(&mut self) {
        let _ = self.tx.send(Msg::Reset);
    }

    /// Presses the keys in order and lets go of them the other way round.
    fn send(&self, combo: &str) {
        let codes: Vec<u16> = combo.split('+').filter_map(vk::of).collect();
        let mut inputs: Vec<INPUT> = codes.iter().map(|&c| key_input(c, false)).collect();
        inputs.extend(codes.iter().rev().map(|&c| key_input(c, true)));
        send_inputs(&inputs);
    }

    fn watch_release(&mut self, combo: Option<&str>) {
        let _ = self.tx.send(Msg::WatchRelease(combo.map(str::to_owned)));
    }

    fn release_modifiers(&self) {
        let ups: Vec<INPUT> = vk::MODIFIER_KEYS
            .iter()
            .flat_map(|(_, sides)| sides.iter().copied())
            .filter(|&c| System.is_down(c))
            .map(|c| key_input(c, true))
            .collect();
        send_inputs(&ups);
    }

    fn shutdown(&mut self) {
        let _ = self.tx.send(Msg::Stop);
        // SAFETY: posting to a thread id we got from that thread; a thread that already ended just fails.
        unsafe { PostThreadMessageW(self.hook_thread, WM_QUIT, 0, 0) };
    }
}
