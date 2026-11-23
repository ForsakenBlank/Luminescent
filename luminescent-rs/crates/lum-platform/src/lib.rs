//! Everything that talks to the operating system. Phase 1 has the single-instance lock, the ctl socket and the
//! quit signals; phase 3 adds hotkeys, windows, the tray, notifications, the clipboard and launching.

pub mod instance;
pub mod ipc;
pub mod signals;
