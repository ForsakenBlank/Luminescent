//! `--spike <name>`: the phase 2 checks that decide whether egui and eframe can carry Luminescent. Each one
//! opens its windows, measures what it can by itself, prints its numbers and a table row for
//! `rust-rewrite-plan/SPIKE_RESULTS.md`, and quits on Esc, on close or when its time is up.
//!
//! The spikes run without the app's data, lock or ctl socket, so they can run next to a real copy.

mod perf;
mod stats;
mod windows;

use egui::{Align2, Color32, FontId, ViewportBuilder};
use lum_platform::desktop;
use lum_ui::Tokens;
use lum_ui::fonts::Weight;

use crate::shell::{self, Renderer};

pub use stats::FrameStats;

/// The spikes' own app id, so their window rules never touch a running Luminescent.
pub const APP_ID: &str = "luminescent-spike";

const LIST: &[(&str, &str)] = &[
    ("overlay", "1. a transparent, frameless, always-on-top dim over every monitor; Space reopens it, timed"),
    ("clickthrough", "2. the dim switches between click-through and normal every 4 seconds; click it"),
    ("nofocus", "3. a panel opens without taking the keyboard, then one that does"),
    ("drop", "4. drop files from Dolphin or Explorer onto the window"),
    ("idle", "5. idle CPU, wake-ups and memory with a window open and nothing changing"),
    ("form", "6. a 300-field form scrolling by itself, then by you"),
    ("wires", "7. 400 bezier wires and 100 moving pulses"),
    ("note", "8. typing into a 50 000 character note"),
];

pub fn run(name: &str, renderer: Renderer, secs: Option<u64>) -> u8 {
    let outcome = match name {
        "overlay" => windows::overlay(renderer, secs.unwrap_or(60)),
        "clickthrough" => windows::click_through(renderer, secs.unwrap_or(24)),
        "nofocus" => windows::no_focus(renderer),
        "drop" => windows::drop(renderer, secs.unwrap_or(60)),
        "idle" => perf::idle(renderer, secs.unwrap_or(30)),
        "form" => perf::form(renderer, secs.unwrap_or(30)),
        "wires" => perf::wires(renderer, secs.unwrap_or(10)),
        "note" => perf::note(renderer, secs.unwrap_or(30)),
        "list" | "" => {
            println!("The UI checks (luminescent --spike <name> [--renderer glow|wgpu] [--secs N]):");
            for (name, what) in LIST {
                println!("  {name:<13} {what}");
            }
            return 0;
        }
        other => Err(format!("no spike called '{other}' (try --spike list)")),
    };
    match outcome {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("SPIKE {name} failed: {e}");
            1
        }
    }
}

/// What every spike's report starts with: the system and the renderer.
fn heading(name: &str, renderer: Renderer) -> String {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| std::env::consts::OS.to_owned());
    let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
    format!("SPIKE {name}: {desktop} {session}, renderer {}", renderer.name()).replace("  ", " ")
}

/// Runs one spike's eframe app with the Luminescent look installed.
fn launch<A: eframe::App + 'static>(
    renderer: Renderer,
    root: ViewportBuilder,
    covers: &[desktop::Cover],
    make: impl FnOnce(&egui::Context) -> A,
) -> Result<(), String> {
    let options = shell::native_options(renderer, root.with_app_id(APP_ID))?;
    match desktop::install_rules(APP_ID, covers) {
        Ok(true) => println!("  window rules set through the Hyprland socket"),
        Ok(false) => {}
        Err(e) => println!("  Hyprland did not take the window rules: {e}"),
    }
    let result = eframe::run_native(
        "Luminescent spike",
        options,
        Box::new(move |cc| {
            lum_ui::setup(&cc.egui_ctx, Tokens::dark());
            Ok(Box::new(make(&cc.egui_ctx)))
        }),
    );
    desktop::remove_rules(APP_ID);
    result.map_err(|e| e.to_string())
}

/// A visible window for the spikes that need one.
fn window(title: &str, size: [f32; 2]) -> ViewportBuilder {
    ViewportBuilder::default().with_title(title).with_inner_size(size)
}

/// Text for the spikes' own windows, centred on a point.
fn say(painter: &egui::Painter, at: egui::Pos2, text: &str, px: f32, colour: Color32) {
    painter.text(
        at,
        Align2::CENTER_CENTER,
        text,
        FontId::new(px, lum_ui::fonts::family(Weight::Medium, false)),
        colour,
    );
}
