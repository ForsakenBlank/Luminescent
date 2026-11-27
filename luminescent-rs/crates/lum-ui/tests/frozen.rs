//! The frozen screens under the colour picker, the ruler and text grab: two monitors with different scales,
//! grabbed by a stand-in for the system's screenshots, each shown at its desktop rectangle, and a point under
//! the mouse read from the right real pixel.

#![allow(clippy::unwrap_used)]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{Rect, pos2, vec2};
use egui_kittest::Harness;
use lum_platform::capture::Fake;
use lum_ui::tools::frozen::{self, FrozenScreens, Shot};
use lum_ui::window::{self, Screen};

fn two_screens() -> Vec<Screen> {
    vec![
        Screen::new("DP-1", Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0))),
        Screen::new("DP-2", Rect::from_min_size(pos2(400.0, 0.0), vec2(400.0, 300.0))).with_scale(2.0),
    ]
}

/// Grabs through the fake and waits for the worker thread.
fn grabbed(ctx: &egui::Context) -> Vec<Shot> {
    window::install_capture(ctx, Arc::new(Fake::default()));
    window::install_screens(ctx, two_screens());
    let (tx, rx) = std::sync::mpsc::channel();
    frozen::grab(ctx, move |shots| tx.send(shots).unwrap());
    rx.recv_timeout(Duration::from_secs(10)).unwrap().unwrap()
}

#[test]
fn each_monitor_is_grabbed_at_its_real_size() {
    let ctx = egui::Context::default();
    let shots = grabbed(&ctx);
    let sizes: Vec<_> =
        shots.iter().map(|s| (s.screen.name.as_str(), s.frame.width, s.frame.height)).collect();
    if cfg!(windows) {
        assert_eq!(sizes, [("DP-1", 400, 300), ("DP-2", 400, 300)]);
    } else {
        assert_eq!(sizes, [("DP-1", 400, 300), ("DP-2", 800, 600)]);
    }
}

#[test]
fn a_failed_grab_says_why() {
    let ctx = egui::Context::default();
    window::install_capture(&ctx, Arc::new(Fake { fail: Some("grim failed".into()) }));
    let (tx, rx) = std::sync::mpsc::channel();
    frozen::grab(&ctx, move |shots| tx.send(shots).unwrap());
    assert_eq!(rx.recv_timeout(Duration::from_secs(10)).unwrap().map(|s| s.len()), Err("grim failed".into()));
}

#[derive(Default)]
struct Seen {
    /// (screen, point under the mouse, ratio, colour there)
    hover: Vec<(usize, lum_screen::Point, f64, [u8; 3])>,
}

#[test]
fn the_mouse_reads_the_real_pixel_of_the_screen_it_is_on() {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let record = seen.clone();
    let mut frozen: Option<FrozenScreens> = None;
    let mut harness =
        Harness::builder().with_size([800.0, 300.0]).with_pixels_per_point(1.0).build_ui(move |ui| {
            let ctx = &ui.ctx().clone();
            let frozen =
                frozen.get_or_insert_with(|| FrozenScreens::new("test", grabbed(ctx), Instant::now()));
            frozen.show(ctx, |ui, cx, _response| {
                if let Some(p) = ui.input(|i| i.pointer.hover_pos()).filter(|p| cx.area.contains(*p)) {
                    let at = cx.local(p);
                    let c = cx.shot.frame.at(at, cx.ratio());
                    record.lock().unwrap().hover.push((cx.index, at, cx.ratio(), [c.r, c.g, c.b]));
                }
            });
        });
    harness.run();
    harness.hover_at(pos2(400.0 + 50.0, 40.0));
    harness.run();
    let seen = seen.lock().unwrap();
    let (index, at, ratio, colour) = *seen.hover.last().unwrap();
    assert_eq!((index, at), (1, lum_screen::Point::new(50, 40)));
    let k = if cfg!(windows) { 1 } else { 2 };
    assert_eq!(ratio, f64::from(k));
    assert_eq!(colour, Fake::colour_at(1, 50 * k, 40 * k));
}
