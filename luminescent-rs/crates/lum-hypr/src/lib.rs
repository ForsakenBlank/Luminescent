//! Hyprland: the compositor does for us what Win32 does on Windows (placing windows, focus rules, binds).
//!
//! Phase 2 only needs the control socket, for the UI spike's window rules. Phase 3 adds the event socket,
//! the LUMI bootstrap, binds through the FIFO and the WindowPlacer.

#[cfg(unix)]
pub mod socket;
