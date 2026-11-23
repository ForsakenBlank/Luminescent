//! A tiny event bus: Luminescent says what happened ("timer.finished", "overlay.opened"...) and the
//! Automation page's event triggers listen. Emitting costs next to nothing when no one listens.

use std::sync::{Arc, Mutex};

use crossbeam_channel::{Receiver, Sender};

/// Every event name, with what the Automation page calls it (`core/events.py` `NAMES`).
pub const NAMES: &[(&str, &str)] = &[
    ("luminescent.started", "Luminescent started"),
    ("overlay.opened", "Overlay opened"),
    ("overlay.closed", "Overlay closed"),
    ("theme.changed", "Theme changed"),
    ("dashboard.switched", "Dashboard switched"),
    ("dashboard.item_run", "A dashboard item ran"),
    ("timer.finished", "A timer finished"),
    ("reminder.fired", "A reminder went off"),
    ("hearing.over_limit", "Hearing Guard: over the limit"),
    ("clipboard.changed", "Clipboard changed"),
    ("keepawake.changed", "Keep awake turned on or off"),
];

pub fn label(name: &str) -> Option<&'static str> {
    NAMES.iter().find(|(n, _)| *n == name).map(|(_, label)| *label)
}

/// What happened, and a short text that goes along ("Focus" for a timer called Focus).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub name: String,
    pub text: String,
}

/// Cheap to clone; every clone emits to the same listeners.
#[derive(Clone, Default)]
pub struct EventBus {
    listeners: Arc<Mutex<Vec<Sender<Event>>>>,
}

impl EventBus {
    pub fn new() -> EventBus {
        EventBus::default()
    }

    /// A new listener. Dropping the receiver unsubscribes it.
    pub fn subscribe(&self) -> Receiver<Event> {
        let (tx, rx) = crossbeam_channel::unbounded();
        self.lock().push(tx);
        rx
    }

    pub fn emit(&self, name: &str, text: impl Into<String>) {
        let mut listeners = self.lock();
        if listeners.is_empty() {
            return;
        }
        let event = Event { name: name.to_owned(), text: text.into() };
        listeners.retain(|tx| tx.send(event.clone()).is_ok());
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Sender<Event>>> {
        self.listeners.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listeners_hear_events_until_they_go() {
        let bus = EventBus::new();
        bus.emit("overlay.opened", "nobody listens yet");
        let rx = bus.subscribe();
        bus.clone().emit("timer.finished", "Focus");
        assert_eq!(rx.try_recv().unwrap(), Event { name: "timer.finished".into(), text: "Focus".into() });
        assert!(rx.try_recv().is_err());
        drop(rx);
        bus.emit("overlay.closed", "");
        assert!(bus.lock().is_empty());
    }

    #[test]
    fn names_match_the_python_app() {
        let fixture = lum_fixtures::read_json("events.json");
        let expected: Vec<(String, String)> = fixture
            .as_array()
            .unwrap()
            .iter()
            .map(|e| (e["name"].as_str().unwrap().to_owned(), e["label"].as_str().unwrap().to_owned()))
            .collect();
        let ours: Vec<(String, String)> = NAMES.iter().map(|(n, l)| (n.to_string(), l.to_string())).collect();
        assert_eq!(ours, expected);
        assert_eq!(label("reminder.fired"), Some("A reminder went off"));
    }
}
