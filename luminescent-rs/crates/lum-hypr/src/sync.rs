//! Keeping our windows where the app wants them (`WindowSync` in `core/hypr.py`). A Wayland window can not
//! place itself, so the app says where each window should be (by title) and this moves it there once Hyprland
//! has mapped it, and again whenever the app moves it. A window nobody placed is left where Hyprland put it.
//!
//! Everything runs on one thread that owns the state and sleeps on a channel. It only wakes on a timer while
//! a window that just opened is settling (at most 2.5 s), so it costs nothing while nothing changes.

use crate::events::Event;
use crate::marker;
use crate::socket::Hypr;
use crossbeam_channel::{Receiver, Sender, select};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Gaps between the checks after a window opens (see [`Settle`]).
const CHECKS_MS: [u64; 11] = [15, 45, 90, 150, 250, 400, 600, 900, 1300, 1800, 2500];

/// Where the app wants a window. Desktop (layout) coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Want {
    pub title: String,
    /// None: leave it where Hyprland (or a rule) puts it.
    pub at: Option<(i32, i32)>,
    /// Give it the keyboard once it has opened.
    pub focus: bool,
}

#[derive(Debug)]
enum Msg {
    Want(Want),
    Focus(String),
    Raise(String),
    FloatOnTop { addr: String, area: Area },
    Unfloat(String),
    Event(Event),
    Stop,
}

/// A rectangle in layout coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Area {
    fn centre(&self) -> (i32, i32) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }
}

/// The handle the app keeps. Cheap to clone; every call only sends a message.
#[derive(Debug, Clone)]
pub struct WindowSync {
    tx: Sender<Msg>,
}

impl WindowSync {
    pub fn start(hypr: Hypr) -> (WindowSync, std::thread::JoinHandle<()>) {
        let (tx, rx) = crossbeam_channel::unbounded();
        let thread = std::thread::Builder::new()
            .name("hypr-windows".into())
            .spawn(move || Worker::new(hypr).run(rx))
            .expect("a thread can be started at launch");
        (WindowSync { tx }, thread)
    }

    /// The window titled `want.title` should be at `want.at`. Call it when the window opens and when the
    /// app moves it; repeating the same wish does nothing.
    pub fn want(&self, want: Want) {
        let _ = self.tx.send(Msg::Want(want));
    }

    pub fn focus(&self, title: &str) {
        let _ = self.tx.send(Msg::Focus(title.to_owned()));
    }

    /// Brings one of our windows to the top of the others.
    pub fn raise(&self, title: &str) {
        let _ = self.tx.send(Msg::Raise(title.to_owned()));
    }

    /// Makes another program's window float, centred on `area`, above ours.
    pub fn float_on_top(&self, addr: &str, area: Area) {
        let _ = self.tx.send(Msg::FloatOnTop { addr: addr.to_owned(), area });
    }

    pub fn unfloat(&self, addr: &str) {
        let _ = self.tx.send(Msg::Unfloat(addr.to_owned()));
    }

    /// Pass every event from the event socket on to here.
    pub fn event(&self, event: &Event) {
        if matches!(event, Event::OpenWindow { .. } | Event::CloseWindow { .. }) {
            let _ = self.tx.send(Msg::Event(event.clone()));
        }
    }

    pub fn stop(&self) {
        let _ = self.tx.send(Msg::Stop);
    }
}

#[derive(Debug)]
struct Window {
    want: Want,
    addr: Option<String>,
    /// From the title Hyprland saw, which carries the markers.
    pinned: bool,
}

/// Moving a window that has just opened, and keeping at it for a moment: Hyprland announces a new window
/// before it has finished placing it, and that first placement used to win over our move about half the time.
///
/// With two monitors Hyprland can take most of a second to let go of a new window: until then it answers "ok"
/// to a move and leaves the window at its own spot (the top-left of the other monitor, the same place every
/// time). So it is checked for 2.5 s and moved back each time it is not where it should be, and only trusted
/// once it has stayed put for two checks in a row.
#[derive(Debug)]
struct Settle {
    title: String,
    pinned: bool,
    addr: String,
    at: (i32, i32),
    next: usize,
    good: u32,
    due: Instant,
}

struct Worker {
    hypr: Hypr,
    windows: HashMap<String, Window>,
    /// Our windows that mapped before the app said anything about them (the event socket and the UI race).
    unclaimed: Vec<(String, String)>,
    settling: Vec<Settle>,
}

impl Worker {
    fn new(hypr: Hypr) -> Worker {
        Worker { hypr, windows: HashMap::new(), unclaimed: Vec::new(), settling: Vec::new() }
    }

    fn run(mut self, rx: Receiver<Msg>) {
        loop {
            let due = self.settling.iter().map(|s| s.due).min();
            let msg = match due {
                Some(due) => select! {
                    recv(rx) -> msg => msg.ok(),
                    default(due.saturating_duration_since(Instant::now())) => {
                        self.check_settling();
                        continue;
                    }
                },
                None => rx.recv().ok(),
            };
            match msg {
                None | Some(Msg::Stop) => return,
                Some(msg) => self.handle(msg),
            }
        }
    }

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Want(want) => self.want(want),
            Msg::Focus(title) => match self.windows.get_mut(&title) {
                Some(Window { addr: Some(addr), .. }) => {
                    let _ = self.hypr.lua(&crate::lua::focus(addr));
                }
                Some(w) => w.want.focus = true,
                None => {}
            },
            Msg::Raise(title) => {
                if let Some(Window { addr: Some(addr), .. }) = self.windows.get(&title) {
                    let _ = self.hypr.lua(&crate::lua::raise(addr));
                }
            }
            Msg::FloatOnTop { addr, area } => self.float_on_top(&addr, area),
            Msg::Unfloat(addr) => {
                if self.hypr.client(&addr).is_some() {
                    let _ = self.hypr.lua(&crate::lua::float(&addr, false));
                }
            }
            Msg::Event(Event::OpenWindow { addr, class, title, .. }) if class == self.hypr.names().app_id => {
                self.opened(addr, &title)
            }
            Msg::Event(Event::CloseWindow { addr }) => {
                self.windows.retain(|_, w| w.addr.as_deref() != Some(&addr));
                self.settling.retain(|s| s.addr != addr);
                self.unclaimed.retain(|(a, _)| *a != addr);
            }
            Msg::Event(_) | Msg::Stop => {}
        }
    }

    fn want(&mut self, want: Want) {
        let title = want.title.clone();
        let Some(window) = self.windows.get_mut(&title) else {
            self.windows.insert(title.clone(), Window { want, addr: None, pinned: true });
            let plain = marker::strip(&title);
            if let Some(i) = self.unclaimed.iter().position(|(_, t)| marker::strip(t) == plain) {
                let (addr, seen) = self.unclaimed.remove(i);
                self.opened(addr, &seen);
            }
            return;
        };
        let moved = window.want.at != want.at;
        window.want = Want { focus: window.want.focus || want.focus, ..want };
        if let (true, Some(addr), Some((x, y))) = (moved, window.addr.clone(), window.want.at) {
            // a move made by the app takes over from any settling still going on
            self.settling.retain(|s| s.addr != addr);
            let _ = self.hypr.move_window(&addr, x, y, window.pinned);
        }
    }

    fn opened(&mut self, addr: String, title: &str) {
        let waiting = |w: &&Window| w.addr.is_none();
        let key = if self.windows.get(title).is_some_and(|w| w.addr.is_none()) {
            Some(title.to_owned())
        } else {
            let plain = marker::strip(title);
            self.windows
                .iter()
                .filter(|(_, w)| waiting(w))
                .find(|(t, _)| marker::strip(t) == plain)
                .map(|(t, _)| t.clone())
        };
        let Some(key) = key else {
            self.unclaimed.push((addr, title.to_owned()));
            return;
        };
        let window = self.windows.get_mut(&key).expect("the key was just found");
        window.addr = Some(addr.clone());
        window.pinned = !marker::has(title, marker::NO_PIN);
        if let Some(at) = window.want.at {
            let _ = self.hypr.move_window(&addr, at.0, at.1, window.pinned);
            self.settling.push(Settle {
                title: key.clone(),
                pinned: window.pinned,
                addr: addr.clone(),
                at,
                next: 0,
                good: 0,
                due: Instant::now() + Duration::from_millis(CHECKS_MS[0]),
            });
        }
        if std::mem::take(&mut window.want.focus) {
            let _ = self.hypr.lua(&crate::lua::focus(&addr));
        }
    }

    fn check_settling(&mut self) {
        let now = Instant::now();
        let (due, mut later): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.settling).into_iter().partition(|s| s.due <= now);
        if !due.is_empty() {
            let clients = self.hypr.clients();
            for mut s in due {
                let Some(client) = clients.iter().find(|c| c.address == s.addr) else { continue };
                if client.at != s.at {
                    s.good = 0;
                    let _ = self.hypr.move_window(&s.addr, s.at.0, s.at.1, s.pinned);
                } else {
                    s.good += 1;
                    // it has stayed put long enough
                    if s.good >= 2 && s.next >= 4 {
                        continue;
                    }
                }
                s.next += 1;
                match CHECKS_MS.get(s.next) {
                    Some(ms) => {
                        s.due = now + Duration::from_millis(*ms);
                        later.push(s);
                    }
                    None if client.at != s.at => tracing::info!(
                        "Hyprland kept {} at {:?} instead of {:?}",
                        marker::strip(&s.title),
                        client.at,
                        s.at
                    ),
                    None => {}
                }
            }
        }
        self.settling = later;
    }

    fn float_on_top(&mut self, addr: &str, area: Area) {
        let _ = self.hypr.lua(&crate::lua::float(addr, true));
        if let Some(c) = self.hypr.client(addr) {
            let (cw, ch) = c.size;
            // clear of bars and docks
            let (mut w, mut h) = (cw.min(area.w - 80), ch.min(area.h - 80));
            if w < 640 || h < 400 {
                // a tiny tiled leftover: give it a usable size
                (w, h) = (1200.min(area.w - 160), 800.min(area.h - 160));
            }
            if (w, h) != (cw, ch) {
                let _ = self.hypr.lua(&crate::lua::resize(addr, w, h));
            }
            let (cx, cy) = area.centre();
            let _ = self.hypr.lua(&crate::lua::move_other(addr, cx - w / 2, cy - h / 2));
        }
        let _ = self.hypr.lua(&crate::lua::raise(addr));
        let _ = self.hypr.lua(&crate::lua::focus(addr));
    }
}
