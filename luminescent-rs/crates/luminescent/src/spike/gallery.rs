//! `--spike gallery`: the widget kit in a window. For the first few seconds nothing may move or repaint (the
//! idle numbers are measured then); after that everything can be tried, including a real floating panel,
//! dialogs and the dim over the screen. Ends when the window closes.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use egui::ViewportCommand;
use lum_platform::process;
use lum_ui::gallery::Gallery;

use super::{heading, launch, window};
use crate::shell::Renderer;

pub fn run(renderer: Renderer, idle_secs: u64) -> Result<(), String> {
    println!("{}", heading("gallery", renderer));
    println!("  Keep the mouse off the window for the first {} s while idle is measured.", idle_secs + 3);
    println!("  Then try everything: both themes, the text size, the panel (drag, resize, edit mode),");
    println!("  the dialogs, the context menu on the card, the dim. Close the window to end.");
    let painted = Arc::new(AtomicU64::new(0));
    let counted = painted.clone();
    launch(renderer, window("Luminescent widget gallery", [1280.0, 900.0]), &[], move |ctx| {
        let ctx = ctx.clone();
        let measured = counted.clone();
        std::thread::spawn(move || measure(&ctx, renderer, idle_secs, &measured));
        GallerySpike { gallery: Gallery::new(), painted: counted, opened: Instant::now() }
    })
}

fn measure(ctx: &egui::Context, renderer: Renderer, secs: u64, painted: &AtomicU64) {
    std::thread::sleep(Duration::from_secs(3));
    let (before, frames, started) = (process::sample(), painted.load(Ordering::Relaxed), Instant::now());
    std::thread::sleep(Duration::from_secs(secs));
    let after = process::sample();
    let elapsed = started.elapsed().as_secs_f64();
    let frames = painted.load(Ordering::Relaxed) - frames;
    let cpu = after.cpu.saturating_sub(before.cpu).as_secs_f64() / elapsed * 100.0;
    let wakeups = match (before.context_switches, after.context_switches) {
        (Some(a), Some(b)) => format!("{:.1}", b.saturating_sub(a) as f64 / elapsed),
        _ => "n/a".to_owned(),
    };
    let rss = after.rss_bytes as f64 / 1_048_576.0;
    println!(
        "  idle with the gallery open: {cpu:.2} % CPU, {wakeups} wake-ups/s, {rss:.0} MB, {frames} frames painted"
    );
    println!(
        "| gallery idle | {} | {cpu:.2} % | {wakeups}/s | {rss:.0} MB | {frames} frames | {} |",
        renderer.name(),
        if frames == 0 { "pass" } else { "FAIL (it repainted while nothing changed)" }
    );
    println!("  Now try everything; close the window to end.");
    ctx.request_repaint();
}

struct GallerySpike {
    gallery: Gallery,
    painted: Arc<AtomicU64>,
    opened: Instant,
}

impl eframe::App for GallerySpike {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.painted.fetch_add(1, Ordering::Relaxed);
        self.gallery.ui(ui);
        if ui.input(|i| i.viewport().close_requested()) {
            println!("  gallery open for {:.0} s", self.opened.elapsed().as_secs_f32());
            ui.ctx().send_viewport_cmd(ViewportCommand::Close);
        }
    }
}
