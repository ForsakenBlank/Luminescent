//! Everything that talks to the operating system. So far: the single-instance lock, the ctl socket, quit
//! signals, the screens, the window rules the UI needs and the process's own CPU and memory. Phase 3 adds
//! hotkeys, the tray, notifications, the clipboard and launching.

pub mod desktop;
pub mod instance;
pub mod ipc;
pub mod process;
pub mod signals;
