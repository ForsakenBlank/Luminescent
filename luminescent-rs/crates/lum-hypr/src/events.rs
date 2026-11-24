//! The event socket (`.socket2.sock`): one line per event, `name>>data`. It tells us when our windows map (so
//! they can be placed), when monitors come and go, and when the config was reloaded, which throws away every
//! bind and rule added at runtime.

use crate::Hypr;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixStream;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A window mapped. `addr` carries the `0x` prefix the other calls want.
    OpenWindow {
        addr: String,
        workspace: String,
        class: String,
        title: String,
    },
    CloseWindow {
        addr: String,
    },
    /// A monitor was plugged in or pulled out.
    MonitorsChanged,
    /// The config was reloaded. By the time this arrives our Lua is installed again; binds must follow.
    ConfigReloaded,
    ActiveWindow {
        class: String,
        title: String,
    },
    Other {
        name: String,
        data: String,
    },
}

impl Event {
    pub fn parse(line: &str) -> Event {
        let (name, data) = line.split_once(">>").unwrap_or((line, ""));
        match name {
            "openwindow" => {
                let mut parts = data.splitn(4, ',');
                match (parts.next(), parts.next(), parts.next(), parts.next()) {
                    (Some(addr), Some(workspace), Some(class), Some(title)) => Event::OpenWindow {
                        addr: format!("0x{addr}"),
                        workspace: workspace.to_owned(),
                        class: class.to_owned(),
                        title: title.to_owned(),
                    },
                    _ => Event::Other { name: name.to_owned(), data: data.to_owned() },
                }
            }
            "closewindow" => Event::CloseWindow { addr: format!("0x{}", data.trim()) },
            "monitoraddedv2" | "monitorremovedv2" => Event::MonitorsChanged,
            "configreloaded" => Event::ConfigReloaded,
            "activewindow" => {
                let (class, title) = data.split_once(',').unwrap_or((data, ""));
                Event::ActiveWindow { class: class.to_owned(), title: title.to_owned() }
            }
            _ => Event::Other { name: name.to_owned(), data: data.to_owned() },
        }
    }
}

/// Reads events for as long as the app runs and hands each one to `sink`. On a config reload it puts our Lua
/// back before passing the event on. A dropped connection is tried again after 2 s, then less and less often,
/// so a Hyprland that went away costs nothing.
pub async fn listen(hypr: Hypr, mut sink: impl FnMut(Event) + Send) {
    let mut wait = Duration::from_secs(2);
    loop {
        match UnixStream::connect(hypr.event_socket_path()).await {
            Ok(stream) => {
                wait = Duration::from_secs(2);
                let mut lines = BufReader::new(stream).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let event = Event::parse(&line);
                    if event == Event::ConfigReloaded {
                        tracing::info!("Hyprland config reloaded: reinstalling binds and window rules");
                        let again = hypr.clone();
                        let done = tokio::task::spawn_blocking(move || again.bootstrap()).await;
                        if let Ok(Err(e)) = done {
                            tracing::warn!("could not reinstall the Hyprland helpers: {e}");
                        }
                    }
                    sink(event);
                }
                tracing::info!("the Hyprland event socket closed");
            }
            Err(e) => tracing::debug!("Hyprland event socket: {e}"),
        }
        tokio::time::sleep(wait).await;
        wait = (wait * 2).min(Duration::from_secs(60));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_parse() {
        assert_eq!(
            Event::parse("openwindow>>55d1,2,luminescent,Luminescent Notes, with a comma"),
            Event::OpenWindow {
                addr: "0x55d1".into(),
                workspace: "2".into(),
                class: "luminescent".into(),
                title: "Luminescent Notes, with a comma".into()
            }
        );
        assert_eq!(Event::parse("closewindow>>55d1\n"), Event::CloseWindow { addr: "0x55d1".into() });
        assert_eq!(Event::parse("monitorremovedv2>>1,DP-2,Dell"), Event::MonitorsChanged);
        assert_eq!(Event::parse("configreloaded>>"), Event::ConfigReloaded);
        assert_eq!(
            Event::parse("openwindow>>short"),
            Event::Other { name: "openwindow".into(), data: "short".into() }
        );
        assert_eq!(
            Event::parse("activewindow>>kitty,~"),
            Event::ActiveWindow { class: "kitty".into(), title: "~".into() }
        );
    }
}
