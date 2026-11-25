//! Hyprland: the compositor does for us what Win32 does on Windows. Wayland does not let a program place its
//! own windows, read the cursor outside its windows or see keys pressed in other programs; Hyprland can do
//! all of that through its control socket. So this crate:
//!
//! * talks to the socket directly (a request is about 1 ms; no `hyprctl` process),
//! * runs a little Lua inside Hyprland (the Python app's `LUMI` code, under our own table name) that owns our
//!   keybinds and window rules. A bind reports back by writing one line into a FIFO,
//! * listens to the event socket, so binds and rules come back after a config reload (which throws away
//!   everything added at runtime) and new windows can be placed,
//! * keeps our windows where the app wants them ([`sync::WindowSync`]).
//!
//! Window titles carry invisible [`marker`]s so the rules can tell our windows apart.

pub mod lua;
pub mod marker;

#[cfg(unix)]
pub mod events;
#[cfg(all(unix, feature = "fake"))]
pub mod fake;
#[cfg(unix)]
pub mod fifo;
#[cfg(unix)]
pub mod socket;
#[cfg(unix)]
pub mod sync;

#[cfg(unix)]
pub use socket::Hypr;
