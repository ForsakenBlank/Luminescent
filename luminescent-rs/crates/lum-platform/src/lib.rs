//! Everything that talks to the operating system: the single-instance lock, the ctl socket, quit signals, the
//! screens and the desktop session (window rules, placing windows), hotkeys, the tray, notifications, the
//! clipboard, launching and the small system services.

pub mod apps;
pub mod autostart;
pub mod capture;
pub mod clipboard;
pub mod desktop;
pub mod dialogs;
pub mod frame;
pub mod hotkeys;
pub mod instance;
pub mod ipc;
pub mod keepawake;
pub mod notify;
pub mod process;
pub mod runner;
pub mod signals;
pub mod sounds;
pub mod system;
pub mod terminal;
pub mod trash;
pub mod tray;
pub mod window;
