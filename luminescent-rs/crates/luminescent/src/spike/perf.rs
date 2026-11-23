//! Spikes 5 to 8: what things cost. Each runs a timed part by itself (so two machines give comparable
//! numbers) and then lets you try it by hand.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use egui::epaint::CubicBezierShape;
use egui::text::{CCursor, CCursorRange};
use egui::{Color32, Key, Pos2, Rect, Stroke, StrokeKind, ViewportCommand, pos2, vec2};
use lum_platform::process;
use lum_ui::Tokens;

use super::{FrameStats, heading, launch, window};
use crate::shell::Renderer;

pub fn idle(renderer: Renderer, secs: u64) -> Result<(), String> {
    println!("{}", heading("idle", renderer));
    println!(
        "  A window opens and nothing in it moves. Leave the mouse off it: hovering is input and repaints."
    );
    println!(
        "  It measures for {secs} s after a 3 s settle, then closes. (--idle-check measures the real app,"
    );
    println!("  whose main window is hidden.)");
    let passes = Arc::new(AtomicU64::new(0));
    let counted = passes.clone();
    launch(renderer, window("Luminescent idle check", [640.0, 420.0]), &[], move |ctx| {
        let ctx = ctx.clone();
        std::thread::spawn(move || measure_idle(ctx, renderer, secs, &counted));
        IdleSpike { passes }
    })
}

fn measure_idle(ctx: egui::Context, renderer: Renderer, secs: u64, passes: &AtomicU64) {
    std::thread::sleep(Duration::from_secs(3));
    let (before, painted, started) = (process::sample(), passes.load(Ordering::Relaxed), Instant::now());
    std::thread::sleep(Duration::from_secs(secs));
    let after = process::sample();
    let elapsed = started.elapsed().as_secs_f64();
    let painted = passes.load(Ordering::Relaxed) - painted;
    let cpu = after.cpu.saturating_sub(before.cpu).as_secs_f64() / elapsed * 100.0;
    let wakeups = match (before.context_switches, after.context_switches) {
        (Some(a), Some(b)) => format!("{:.1}", b.saturating_sub(a) as f64 / elapsed),
        _ => "n/a".to_owned(),
    };
    let rss = after.rss_bytes as f64 / 1_048_576.0;
    let threads = after.threads.map_or("n/a".to_owned(), |t| t.to_string());
    println!("  CPU          {cpu:.2} % of one core");
    println!("  wake-ups     {wakeups} a second");
    println!("  memory       {rss:.0} MB RSS");
    println!("  threads      {threads}");
    println!("  frames       {painted} painted while idle (should be 0)");
    println!(
        "| 5 idle, window open | {} | {cpu:.2} % | {wakeups}/s | {rss:.0} MB | {painted} frames |",
        renderer.name()
    );
    ctx.send_viewport_cmd(ViewportCommand::Close);
    ctx.request_repaint();
}

struct IdleSpike {
    passes: Arc<AtomicU64>,
}

impl eframe::App for IdleSpike {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.passes.fetch_add(1, Ordering::Relaxed);
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Nothing here moves");
            ui.label("CPU, wake-ups and memory are being measured. Keep the mouse away from this window.");
            ui.add_space(12.0);
            for i in 0..12 {
                ui.horizontal(|ui| {
                    ui.label(format!("Row {i:02}"));
                    let _ = ui.button("A button");
                    let mut on = i % 2 == 0;
                    ui.checkbox(&mut on, "A switch");
                });
            }
        });
    }
}

const FIELDS: usize = 300;

pub fn form(renderer: Renderer, secs: u64) -> Result<(), String> {
    println!("{}", heading("form", renderer));
    println!(
        "  A {FIELDS}-field form (like a long Settings page) scrolls down and up by itself for 8 s while"
    );
    println!(
        "  frames are timed. Then scroll it yourself and say whether it feels smooth. Ends after {secs} s,"
    );
    println!("  on Esc or when you close it.");
    let app = FormSpike {
        renderer,
        started: Instant::now(),
        secs,
        texts: (0..FIELDS).map(|i| format!("Value {i}")).collect(),
        ticks: (0..FIELDS).map(|i| i % 3 == 0).collect(),
        numbers: (0..FIELDS).map(|i| (i % 100) as f32).collect(),
        choices: vec![0; FIELDS],
        max_offset: 0.0,
        timed: FrameStats::default(),
        by_hand: FrameStats::default(),
    };
    launch(renderer, window("Luminescent form check", [900.0, 700.0]), &[], |_| app)
}

struct FormSpike {
    renderer: Renderer,
    started: Instant,
    secs: u64,
    texts: Vec<String>,
    ticks: Vec<bool>,
    numbers: Vec<f32>,
    choices: Vec<usize>,
    max_offset: f32,
    timed: FrameStats,
    by_hand: FrameStats,
}

impl eframe::App for FormSpike {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let t = self.started.elapsed().as_secs_f32();
        let auto = t < 8.0;
        let stats = if auto { &mut self.timed } else { &mut self.by_hand };
        stats.begin(frame.info().cpu_usage);
        let mut area = egui::ScrollArea::vertical().auto_shrink(false);
        if auto && self.max_offset > 0.0 {
            // down in 4 s, up in 4 s
            let phase = if t < 4.0 { t / 4.0 } else { (8.0 - t) / 4.0 };
            area = area.vertical_scroll_offset(phase * self.max_offset);
        }
        let output = egui::CentralPanel::default()
            .show(ui, |ui| {
                area.show(ui, |ui| {
                    egui::Grid::new("form").num_columns(2).spacing([24.0, 10.0]).striped(false).show(
                        ui,
                        |ui| {
                            for i in 0..FIELDS {
                                ui.label(format!("Setting number {i}"));
                                match i % 5 {
                                    0 => ui.text_edit_singleline(&mut self.texts[i]),
                                    1 => ui.checkbox(&mut self.ticks[i], "Turn this on"),
                                    2 => ui.add(egui::Slider::new(&mut self.numbers[i], 0.0..=100.0)),
                                    3 => {
                                        egui::ComboBox::from_id_salt(i)
                                            .selected_text(["Cursor", "Primary", "Centre"][self.choices[i]])
                                            .show_ui(ui, |ui| {
                                                for (n, label) in
                                                    ["Cursor", "Primary", "Centre"].iter().enumerate()
                                                {
                                                    ui.selectable_value(&mut self.choices[i], n, *label);
                                                }
                                            })
                                            .response
                                    }
                                    _ => {
                                        ui.add(egui::DragValue::new(&mut self.numbers[i]).range(0.0..=100.0))
                                    }
                                };
                                ui.end_row();
                            }
                        },
                    );
                })
            })
            .inner;
        self.max_offset = (output.content_size.y - output.inner_rect.height()).max(0.0);
        stats.end();
        if auto {
            ui.ctx().request_repaint();
        } else if self.by_hand.frames() == 1 {
            self.by_hand.pause();
        }
        let close = ui.input(|i| i.key_pressed(Key::Escape) || i.viewport().close_requested());
        if close || t >= self.secs as f32 {
            println!("  scrolling by itself:");
            self.timed.summary().iter().for_each(|l| println!("  {l}"));
            if !self.by_hand.is_empty() {
                println!("  scrolled by hand (only frames something changed in):");
                self.by_hand.summary().iter().for_each(|l| println!("  {l}"));
            }
            println!(
                "| 6 form, {FIELDS} fields | {} | {} | (smooth by hand?) |",
                self.renderer.name(),
                self.timed.row()
            );
            ui.ctx().send_viewport_cmd(ViewportCommand::Close);
        } else if !auto {
            ui.ctx().request_repaint_after(Duration::from_secs_f32((self.secs as f32 - t).max(0.0)));
        }
    }
}

const NODES: usize = 80;
const WIRES: usize = 400;
const PULSES: usize = 100;

pub fn wires(renderer: Renderer, secs: u64) -> Result<(), String> {
    println!("{}", heading("wires", renderer));
    println!(
        "  {WIRES} bezier wires between {NODES} boxes with {PULSES} pulses running along them, for {secs} s."
    );
    println!("  Frames are timed the whole time; it should stay at your screen's refresh rate.");
    let mut seed = 0x2545_F491_u32;
    let mut next = move || {
        // a small xorshift, so every run draws the same picture
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    let nodes: Vec<Pos2> =
        (0..NODES).map(|_| pos2((next() % 1100) as f32 + 20.0, (next() % 640) as f32 + 40.0)).collect();
    let links: Vec<(usize, usize)> =
        (0..WIRES).map(|_| ((next() as usize) % NODES, (next() as usize) % NODES)).collect();
    let app =
        WireSpike { renderer, started: Instant::now(), secs, nodes, links, stats: FrameStats::default() };
    launch(renderer, window("Luminescent wires check", [1280.0, 760.0]), &[], |_| app)
}

struct WireSpike {
    renderer: Renderer,
    started: Instant,
    secs: u64,
    nodes: Vec<Pos2>,
    links: Vec<(usize, usize)>,
    stats: FrameStats,
}

impl eframe::App for WireSpike {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.stats.begin(frame.info().cpu_usage);
        let tokens = Tokens::of(ui.ctx());
        let t = self.started.elapsed().as_secs_f32();
        let painter = ui.painter();
        let origin = ui.max_rect().min.to_vec2();
        painter.rect_filled(ui.max_rect(), 0.0, tokens.p.bg_frame);
        let wire = Stroke::new(2.0, lum_ui::theme::alpha(tokens.p.text_muted, 0.8));
        let size = vec2(120.0, 44.0);
        for (i, &(a, b)) in self.links.iter().enumerate() {
            let from = self.nodes[a] + origin + vec2(size.x, size.y / 2.0);
            let to = self.nodes[b] + origin + vec2(0.0, size.y / 2.0);
            let pull = ((to.x - from.x).abs() * 0.5).max(60.0);
            let curve = CubicBezierShape::from_points_stroke(
                [from, from + vec2(pull, 0.0), to - vec2(pull, 0.0), to],
                false,
                Color32::TRANSPARENT,
                wire,
            );
            if i < PULSES {
                let at = curve.sample((t * 0.45 + i as f32 * 0.137).fract());
                painter.add(curve);
                painter.circle_filled(at, 4.0, tokens.p.primary);
            } else {
                painter.add(curve);
            }
        }
        for (i, &node) in self.nodes.iter().enumerate() {
            let rect = Rect::from_min_size(node + origin, size);
            painter.rect(rect, 8.0, tokens.p.bg_card, Stroke::new(1.0, tokens.p.border), StrokeKind::Inside);
            painter.text(
                rect.left_center() + vec2(10.0, 0.0),
                egui::Align2::LEFT_CENTER,
                format!("NODE {i:02}"),
                tokens.font(10.5, lum_ui::fonts::Weight::SemiBold, true),
                tokens.p.text_primary,
            );
        }
        self.stats.end();
        let close = ui.input(|i| i.key_pressed(Key::Escape) || i.viewport().close_requested());
        if close || t >= self.secs as f32 {
            self.stats.summary().iter().for_each(|l| println!("{l}"));
            println!(
                "| 7 wires, {WIRES} + {PULSES} pulses | {} | {} |",
                self.renderer.name(),
                self.stats.row()
            );
            ui.ctx().send_viewport_cmd(ViewportCommand::Close);
        } else {
            ui.ctx().request_repaint();
        }
    }
}

const NOTE_CHARS: usize = 50_000;

pub fn note(renderer: Renderer, secs: u64) -> Result<(), String> {
    println!("{}", heading("note", renderer));
    println!(
        "  A note with {NOTE_CHARS} characters opens. For 8 s it types into the middle by itself (25 keys"
    );
    println!(
        "  a second) while frames are timed. Then type in it yourself: scroll, select, paste. Ends after"
    );
    println!("  {secs} s, on Esc or when you close it.");
    let line = "Luminescent keeps small tools a hotkey away. Notes are plain text with a Markdown preview. ";
    let mut text = String::with_capacity(NOTE_CHARS + 1024);
    let mut n = 0;
    while text.len() < NOTE_CHARS {
        text.push_str(line);
        n += 1;
        if n % 3 == 0 {
            text.push('\n');
        }
    }
    text.truncate(NOTE_CHARS);
    let typing = Arc::new(AtomicBool::new(true));
    let app = NoteSpike {
        renderer,
        started: Instant::now(),
        secs,
        text,
        typed_at: Instant::now(),
        timed: FrameStats::default(),
        by_hand: FrameStats::default(),
        placed: false,
        typing,
    };
    launch(renderer, window("Luminescent note check", [900.0, 700.0]), &[], |_| app)
}

struct NoteSpike {
    renderer: Renderer,
    started: Instant,
    secs: u64,
    text: String,
    typed_at: Instant,
    timed: FrameStats,
    by_hand: FrameStats,
    placed: bool,
    typing: Arc<AtomicBool>,
}

impl eframe::App for NoteSpike {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if !self.typing.load(Ordering::Relaxed) {
            return;
        }
        if self.started.elapsed() >= Duration::from_secs(8) {
            self.typing.store(false, Ordering::Relaxed);
            return;
        }
        if self.placed && self.typed_at.elapsed() >= Duration::from_millis(40) {
            self.typed_at = Instant::now();
            raw_input.events.push(egui::Event::Text("x".to_owned()));
        }
        ctx.request_repaint_after(Duration::from_millis(40));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let typing = self.typing.load(Ordering::Relaxed);
        let before = self.text.len();
        let started = Instant::now();
        let output = egui::CentralPanel::default()
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    egui::TextEdit::multiline(&mut self.text)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Body)
                        .show(ui)
                })
            })
            .inner
            .inner;
        if !self.placed {
            self.placed = true;
            output.response.request_focus();
            let mut state = output.state;
            state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(NOTE_CHARS / 2))));
            state.store(ui.ctx(), output.response.id);
        }
        if self.text.len() != before {
            // only frames that handled a key count: those are the ones a slow note would make you wait for
            let stats = if typing { &mut self.timed } else { &mut self.by_hand };
            stats.record_build(started.elapsed());
        }
        let t = self.started.elapsed().as_secs_f32();
        let close = ui.input(|i| i.key_pressed(Key::Escape) || i.viewport().close_requested());
        if close || t >= self.secs as f32 {
            println!(
                "  typing by itself, {} keys (time to handle a key and lay out the note):",
                self.timed.frames()
            );
            self.timed.summary_build().iter().for_each(|l| println!("  {l}"));
            if !self.by_hand.is_empty() {
                println!("  typing by hand, {} keys:", self.by_hand.frames());
                self.by_hand.summary_build().iter().for_each(|l| println!("  {l}"));
            }
            println!(
                "| 8 note, {NOTE_CHARS} chars | {} | {} | (typing smooth by hand?) |",
                self.renderer.name(),
                self.timed.row_build()
            );
            ui.ctx().send_viewport_cmd(ViewportCommand::Close);
        } else if !typing {
            ui.ctx().request_repaint_after(Duration::from_secs_f32((self.secs as f32 - t).max(0.0)));
        }
    }
}
