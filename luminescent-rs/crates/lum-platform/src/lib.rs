//! Everything that talks to the operating system: the single-instance lock, the ctl socket, quit signals, the
//! screens and the desktop session (window rules, placing windows), hotkeys, the tray, notifications, the
//! clipboard, launching and the small system services.

pub mod autostart;
pub mod clipboard;
pub mod desktop;
pub mod hotkeys;
pub mod instance;
pub mod keepawake;
pub mod notify;
pub mod ipc;
pub mod process;
pub mod signals;
pub mod sounds;
pub mod system;
pub mod tray;
pub mod window;
