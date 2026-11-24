//! Desktop notifications: D-Bus on Linux (Noctalia, mako, dunst, Caelestia all listen), toasts on Windows.
//! Sending one talks to another process, so it happens on a short-lived thread and never holds anything up.

use std::path::PathBuf;

/// Shows a desktop notification for `ms` milliseconds, with the app's icon when `icon` exists.
pub fn show(title: &str, message: &str, ms: u32, icon: Option<PathBuf>) {
    let (title, message) = (title.to_owned(), message.to_owned());
    let spawned = std::thread::Builder::new().name("notify".into()).spawn(move || {
        let mut n = notify_rust::Notification::new();
        n.appname(lum_core::APP_NAME).summary(&title).body(&message);
        n.timeout(notify_rust::Timeout::Milliseconds(ms));
        if let Some(icon) = icon.filter(|p| p.is_file()) {
            n.icon(&icon.to_string_lossy());
        }
        if let Err(e) = n.show() {
            tracing::warn!("could not show the notification {title:?}: {e}");
        }
    });
    if let Err(e) = spawned {
        tracing::warn!("could not show a notification: {e}");
    }
}
