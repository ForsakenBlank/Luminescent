//! Window Snapper: hotkeys that put the active window in a half, a third or a corner of its screen.
//!
//! Hyprland tiles windows, so a snapped window is made floating first and then moved and sized to the spot
//! (the screen minus your bars and dock, with a small gap). Pressing the same direction again steps through
//! the sizes: left half, left third, left two thirds, back to the half. "Back into the tiling layout" undoes
//! it. On Windows the window is moved inside its screen's work area and "back" puts it where it was before
//! it was snapped. Every spot is also in the command palette. The geometry is `lum_screen::snap`; moving the
//! window is `lum_platform::snap`, on a worker thread because it asks the system.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use lum_core::actions::Action;
use lum_core::module::{Module, ModuleCx, StartError};
use lum_platform::snap::{self as platform, Area, Found};
use lum_screen::snap::{self, Cycler, SPOT_KEYS, SPOTS};

use crate::{Bind, Inbox, Request, Services, UiCx, UiModule};

const TITLE: &str = "Window Snapper";

#[derive(Debug, Clone)]
enum Cmd {
    /// A direction: the first press snaps, pressing it again steps through the sizes.
    Step(&'static str),
    Snap(&'static str),
    Tile,
}

/// What the worker threads share: which step each window is on, and (Windows) where it was before.
#[derive(Debug, Default)]
struct Memory {
    cycler: Cycler,
    before: HashMap<String, Area>,
}

/// Moves windows for the snapper. The real one asks the system; tests hand in their own.
pub trait Mover: Send + Sync {
    fn active(&self) -> Found;
    fn place(&self, window: &platform::Window, rect: Area);
    fn rect(&self, window: &platform::Window) -> Option<Area>;
    fn put_back(&self, window: &platform::Window, before: Option<Area>);
}

struct System;

impl Mover for System {
    fn active(&self) -> Found {
        platform::active()
    }

    fn place(&self, window: &platform::Window, rect: Area) {
        platform::place(window, rect);
    }

    fn rect(&self, window: &platform::Window) -> Option<Area> {
        platform::rect(window)
    }

    fn put_back(&self, window: &platform::Window, before: Option<Area>) {
        platform::put_back(window, before);
    }
}

pub struct SnapperModule {
    inbox: Inbox<Cmd>,
    services: Services,
    running: bool,
    gap: i32,
    memory: Arc<Mutex<Memory>>,
    mover: Arc<dyn Mover>,
}

impl std::fmt::Debug for SnapperModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SnapperModule").field("running", &self.running).finish_non_exhaustive()
    }
}

/// The undo action's label: Linux puts the window back into tiling, Windows back where it was.
fn tile_label() -> &'static str {
    if cfg!(windows) {
        "Snap window: put it back where it was"
    } else {
        "Snap window: back into the tiling layout"
    }
}

impl SnapperModule {
    pub fn new(services: &Services) -> SnapperModule {
        SnapperModule {
            inbox: Inbox::new(services),
            services: services.clone(),
            running: false,
            gap: 8,
            memory: Arc::default(),
            mover: Arc::new(System),
        }
    }

    /// Moves windows with `mover` instead of the system (tests).
    pub fn set_mover(&mut self, mover: Arc<dyn Mover>) {
        self.mover = mover;
    }

    fn run(&self, cmd: Cmd) {
        let (memory, mover, services, gap) =
            (self.memory.clone(), self.mover.clone(), self.services.clone(), self.gap);
        let spawned = std::thread::Builder::new().name("snapper".into()).spawn(move || {
            let window = match mover.active() {
                Found::Window(w) => w,
                Found::Nothing => return,
                Found::Unsupported => {
                    let text = "Snapping windows needs Hyprland.".to_owned();
                    services.send(Request::Notify { title: TITLE.into(), text, ms: 3000 });
                    return;
                }
            };
            let Ok(mut memory) = memory.lock() else { return };
            let spot = match cmd {
                Cmd::Step(direction) => memory.cycler.step(&window.id, direction),
                Cmd::Snap(spot) => Some(spot),
                Cmd::Tile => {
                    memory.cycler.forget(&window.id);
                    let before = memory.before.remove(&window.id);
                    mover.put_back(&window, before);
                    return;
                }
            };
            let Some(spot) = spot else { return };
            if cfg!(windows)
                && !memory.before.contains_key(&window.id)
                && let Some(rect) = mover.rect(&window)
            {
                memory.before.insert(window.id.clone(), rect);
            }
            // Windows lists the work area in real pixels, so the gap grows with the screen's scale
            let gap = if cfg!(windows) { (f64::from(gap) * window.scale).round() as i32 } else { gap };
            if let Some(rect) = snap::spot_rect(window.area, spot, gap) {
                mover.place(&window, rect);
                tracing::info!("snapped {} to {spot}", window.id);
            }
        });
        if let Err(e) = spawned {
            tracing::warn!("could not start snapping: {e}");
        }
    }
}

impl Module for SnapperModule {
    fn key(&self) -> &'static str {
        "snapper"
    }

    fn title(&self) -> &'static str {
        TITLE
    }

    fn start(&mut self, cx: &mut ModuleCx<'_>) -> Result<(), StartError> {
        self.gap = cx.settings.get_i64("snapper.gap").unwrap_or(8).clamp(0, 100) as i32;
        self.running = true;
        Ok(())
    }

    fn stop(&mut self, _cx: &mut ModuleCx<'_>) {
        self.running = false;
    }

    fn actions(&self) -> Vec<Action> {
        let mut out: Vec<Action> = SPOTS
            .iter()
            .map(|&(id, label, _)| {
                self.inbox
                    .action(&format!("snap_{id}"), &format!("Snap window: {label}"), move || Cmd::Snap(id))
            })
            .collect();
        out.push(self.inbox.action("tile", tile_label(), || Cmd::Tile));
        out
    }
}

impl UiModule for SnapperModule {
    fn binds(&self) -> Vec<Bind> {
        vec![
            Bind { setting: "left_hotkey", id: "left", label: "snap left" },
            Bind { setting: "right_hotkey", id: "right", label: "snap right" },
            Bind { setting: "up_hotkey", id: "up", label: "snap up" },
            Bind { setting: "down_hotkey", id: "down", label: "snap down" },
            Bind { setting: "top_left_hotkey", id: "top_left", label: "snap top-left quarter" },
            Bind { setting: "top_right_hotkey", id: "top_right", label: "snap top-right quarter" },
            Bind { setting: "bottom_left_hotkey", id: "bottom_left", label: "snap bottom-left quarter" },
            Bind { setting: "bottom_right_hotkey", id: "bottom_right", label: "snap bottom-right quarter" },
            Bind { setting: "centre_third_hotkey", id: "centre_third", label: "snap centre third" },
            Bind { setting: "tile_hotkey", id: "tile", label: "back into tiling" },
        ]
    }

    fn bind_pressed(&mut self, id: &str) {
        let cmd = if id == "tile" {
            Some(Cmd::Tile)
        } else if let Some((direction, _)) = snap::CYCLES.iter().find(|(d, _)| *d == id) {
            Some(Cmd::Step(direction))
        } else {
            SPOT_KEYS.iter().find(|s| **s == id).map(|spot| Cmd::Snap(spot))
        };
        if let Some(cmd) = cmd {
            self.inbox.push(cmd);
        }
    }

    fn show(&mut self, _cx: &mut UiCx<'_>) {
        if !self.running {
            return;
        }
        for cmd in self.inbox.take() {
            self.run(cmd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[derive(Default)]
    struct Fake {
        placed: Mutex<Vec<(String, Area)>>,
        back: Mutex<Vec<(String, Option<Area>)>>,
        unsupported: bool,
    }

    impl Mover for Fake {
        fn active(&self) -> Found {
            if self.unsupported {
                return Found::Unsupported;
            }
            Found::Window(platform::Window {
                id: "0xabc".into(),
                floating: false,
                area: (0, 30, 1920, 1050),
                scale: 1.0,
            })
        }

        fn place(&self, window: &platform::Window, rect: Area) {
            self.placed.lock().expect("lock").push((window.id.clone(), rect));
        }

        fn rect(&self, _window: &platform::Window) -> Option<Area> {
            Some((100, 100, 800, 600))
        }

        fn put_back(&self, window: &platform::Window, before: Option<Area>) {
            self.back.lock().expect("lock").push((window.id.clone(), before));
        }
    }

    fn module(fake: Arc<Fake>) -> SnapperModule {
        let dir = std::env::temp_dir();
        let mut m = SnapperModule::new(&Services::detached(dir));
        m.set_mover(fake);
        m.running = true;
        m
    }

    /// Runs one command and waits for its worker.
    fn press(m: &mut SnapperModule, id: &str) {
        m.bind_pressed(id);
        for cmd in m.inbox.take() {
            m.run(cmd);
        }
        std::thread::sleep(Duration::from_millis(60));
    }

    #[test]
    fn a_direction_pressed_again_steps_through_the_sizes() {
        let fake = Arc::new(Fake::default());
        let mut m = module(fake.clone());
        for _ in 0..4 {
            press(&mut m, "left");
        }
        let widths: Vec<i32> = fake.placed.lock().expect("lock").iter().map(|(_, r)| r.2).collect();
        // half, third, two thirds, half again (8 px gaps, 4 between windows)
        assert_eq!(widths, [948, 628, 1268, 948]);
    }

    #[test]
    fn spots_and_back_into_tiling() {
        let fake = Arc::new(Fake::default());
        let mut m = module(fake.clone());
        press(&mut m, "top_left");
        assert_eq!(fake.placed.lock().expect("lock")[0].1, (8, 38, 948, 513));
        press(&mut m, "tile");
        let back = fake.back.lock().expect("lock").clone();
        let before = if cfg!(windows) { Some((100, 100, 800, 600)) } else { None };
        assert_eq!(back, [("0xabc".to_owned(), before)]);
    }

    #[test]
    fn every_spot_is_an_action_and_other_desktops_are_told() {
        let fake = Arc::new(Fake { unsupported: true, ..Fake::default() });
        let mut m = module(fake.clone());
        let labels: Vec<String> = m.actions().iter().map(|a| a.label.clone()).collect();
        assert_eq!(labels.len(), 16);
        assert_eq!(labels[0], "Snap window: Left half");
        assert_eq!(labels[14], "Snap window: Centre, large");
        press(&mut m, "right");
        assert!(fake.placed.lock().expect("lock").is_empty());
        let said: Vec<_> = m.services.take();
        assert_eq!(
            said,
            [Request::Notify {
                title: TITLE.into(),
                text: "Snapping windows needs Hyprland.".into(),
                ms: 3000
            }]
        );
    }
}
