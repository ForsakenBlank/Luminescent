//! Spikes 1 to 4: what the OS lets our windows do. These need someone looking at the screen, so each prints
//! what to do first and what it measured at the end.

use std::time::{Duration, Instant};

use egui::{Key, ViewportBuilder, ViewportCommand, ViewportId, pos2};
use lum_platform::desktop::{self, Cover, marker};
use lum_ui::Tokens;

use super::{heading, launch, say, window};
use crate::shell::{self, Renderer};

/// One full-screen dim per monitor. On Hyprland a rule per title puts each on its monitor (a window can not
/// place itself on Wayland); elsewhere each goes borderless full screen on its monitor.
struct Dim {
    id: ViewportId,
    title: String,
    monitor: Option<usize>,
    size: [f32; 2],
    asked: Instant,
    first_frame: Option<Duration>,
    clicks: u32,
}

fn dims(prefix: &str) -> (Vec<Dim>, Vec<Cover>) {
    let screens = desktop::monitors();
    let hyprland = desktop::places_windows_by_rule();
    let now = Instant::now();
    let mut list = Vec::new();
    let mut covers = Vec::new();
    for i in 0..screens.len().max(1) {
        let title = format!("{prefix} {}", i + 1);
        let size = screens.get(i).map_or([800.0, 600.0], |m| [m.width as f32, m.height as f32]);
        if let (true, Some(m)) = (hyprland, screens.get(i)) {
            covers.push(Cover { title: title.clone(), monitor: m.name.clone() });
        }
        list.push(Dim {
            id: ViewportId::from_hash_of(&title),
            monitor: (!hyprland).then_some(i),
            title,
            size,
            asked: now,
            first_frame: None,
            clicks: 0,
        });
    }
    (list, covers)
}

fn dim_builder(dim: &Dim) -> ViewportBuilder {
    let builder = ViewportBuilder::default()
        .with_title(&dim.title)
        .with_app_id(super::APP_ID)
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top()
        .with_taskbar(false)
        .with_resizable(false)
        .with_inner_size(dim.size);
    match dim.monitor {
        Some(i) => builder.with_monitor(i),
        None => builder,
    }
}

fn print_screens() {
    let screens = desktop::monitors();
    if screens.is_empty() {
        println!("  screens: the system did not list them (one dim, on the first screen)");
    }
    for m in &screens {
        println!(
            "  screen {}: {}x{} at {},{} scale {}{}",
            m.name,
            m.width,
            m.height,
            m.x,
            m.y,
            m.scale,
            if m.primary { " (primary or focused)" } else { "" }
        );
    }
}

fn print_windows(ctx: &egui::Context, dims: &[Dim]) {
    for dim in dims {
        let info = ctx.input_for(dim.id, |i| i.viewport().clone());
        let ppp = info.native_pixels_per_point.unwrap_or(1.0);
        let rect = info.outer_rect.or(info.inner_rect);
        let place = rect.map_or("unknown (Wayland does not say)".to_owned(), |r| {
            format!(
                "{:.0}x{:.0} at {:.0},{:.0} (px)",
                r.width() * ppp,
                r.height() * ppp,
                r.min.x * ppp,
                r.min.y * ppp
            )
        });
        println!("  {}: egui sees it as {place}", dim.title);
    }
    for line in desktop::describe_windows() {
        println!("  Hyprland: {line}");
    }
}

pub fn overlay(renderer: Renderer, secs: u64) -> Result<(), String> {
    println!("{}", heading("overlay", renderer));
    println!("  Every screen should go dark (64 %) with a line of text in the middle. Nothing else of ours");
    println!(
        "  should show: no frame, no taskbar button, no 1x1 window. Space closes the dims and opens them"
    );
    println!("  again, timed (do it a few times). Esc or {secs} s ends it.");
    print_screens();
    let (dims, covers) = dims("Luminescent dim");
    let app = OverlaySpike {
        dims,
        renderer,
        deadline: Instant::now() + Duration::from_secs(secs),
        reopens: Vec::new(),
        shown: true,
        reported: false,
        describe_at: None,
        frames: 0,
    };
    launch(renderer, shell::hidden_root().with_title(desktop::ROOT_TITLE), &covers, |_| app)
}

struct OverlaySpike {
    dims: Vec<Dim>,
    renderer: Renderer,
    deadline: Instant,
    reopens: Vec<Duration>,
    shown: bool,
    reported: bool,
    /// When to print where the dims ended up (once, a moment after they first opened).
    describe_at: Option<Instant>,
    frames: u64,
}

impl eframe::App for OverlaySpike {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.frames += 1;
        if self.frames == 1 {
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }
        let tokens = Tokens::of(&ctx);
        let mut close = Instant::now() >= self.deadline;
        let mut reopen = false;
        if self.shown {
            for dim in &mut self.dims {
                let builder = dim_builder(dim);
                ctx.show_viewport_immediate(dim.id, builder, |ui, _| {
                    if dim.first_frame.is_none() {
                        dim.first_frame = Some(dim.asked.elapsed());
                    }
                    let rect = ui.max_rect();
                    let painter = ui.painter();
                    painter.rect_filled(rect, 0.0, lum_ui::theme::alpha(tokens.scrim(), 0.64));
                    let text = format!("{}: Space reopens, Esc ends", dim.title);
                    say(painter, rect.center(), &text, 22.0, tokens.p.text_primary);
                    ui.input(|i| {
                        close |= i.key_pressed(Key::Escape) || i.viewport().close_requested();
                        reopen |= i.key_pressed(Key::Space);
                        if i.pointer.any_click() {
                            dim.clicks += 1;
                        }
                    });
                });
            }
        }
        let all_up = self.dims.iter().all(|d| d.first_frame.is_some());
        if self.shown && all_up && !self.reported {
            self.reported = true;
            let slowest = self.dims.iter().filter_map(|d| d.first_frame).max().unwrap_or_default();
            if self.reopens.is_empty() {
                println!("  first open: every dim drew its first frame within {:.1} ms", ms(slowest));
            } else {
                println!("  reopen {}: {:.1} ms", self.reopens.len(), ms(slowest));
            }
            if self.reopens.is_empty() {
                // a moment later the compositor has placed them, so where they are means something
                self.describe_at = Some(Instant::now() + Duration::from_millis(600));
                ctx.request_repaint_after(Duration::from_millis(600));
            }
            self.reopens.push(slowest);
        }
        if self.describe_at.is_some_and(|at| Instant::now() >= at) {
            self.describe_at = None;
            print_windows(&ctx, &self.dims);
        }
        if reopen {
            // closing a viewport destroys its window: on Wayland that is the only way to hide one
            self.shown = false;
            self.reported = false;
            ctx.request_repaint();
        } else if !self.shown {
            let now = Instant::now();
            for dim in &mut self.dims {
                dim.asked = now;
                dim.first_frame = None;
            }
            self.shown = true;
            ctx.send_viewport_cmd(ViewportCommand::RequestPaintWhileHidden);
            ctx.request_repaint();
        }
        if close {
            let reopens: Vec<f32> = self.reopens.iter().skip(1).map(|d| ms(*d)).collect();
            let worst = reopens.iter().copied().fold(0.0, f32::max);
            println!(
                "  result: {} dims, first open {:.1} ms, {} reopens, slowest {:.1} ms",
                self.dims.len(),
                self.reopens.first().map_or(0.0, |d| ms(*d)),
                reopens.len(),
                worst
            );
            println!(
                "| 1 overlay | {} | {} screens | first open {:.0} ms, reopen worst {:.0} ms | (did every screen dim? anything extra on screen?) |",
                self.renderer.name(),
                self.dims.len(),
                self.reopens.first().map_or(0.0, |d| ms(*d)),
                worst
            );
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }
}

pub fn click_through(renderer: Renderer, secs: u64) -> Result<(), String> {
    println!("{}", heading("clickthrough", renderer));
    println!("  The first screen dims. Every 4 seconds it switches between CLICK-THROUGH (clicks go to the");
    println!("  window behind it) and NORMAL (it catches clicks). Click on it a few times in both states,");
    println!(
        "  for example on a window behind it. It counts the clicks it caught. Ends after {secs} s or Esc."
    );
    let (mut dims, covers) = dims("Luminescent click-through");
    dims.truncate(1);
    let covers: Vec<Cover> = covers.into_iter().take(1).collect();
    let app = ClickSpike {
        dim: dims.remove(0),
        renderer,
        started: Instant::now(),
        secs,
        through: false,
        caught: [0, 0],
        frames: 0,
    };
    launch(renderer, shell::hidden_root(), &covers, |_| app)
}

struct ClickSpike {
    dim: Dim,
    renderer: Renderer,
    started: Instant,
    secs: u64,
    through: bool,
    /// Clicks the dim caught while normal, and while click-through (that one should stay 0).
    caught: [u32; 2],
    frames: u64,
}

impl eframe::App for ClickSpike {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.frames += 1;
        if self.frames == 1 {
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }
        let tokens = Tokens::of(&ctx);
        let elapsed = self.started.elapsed();
        let through = (elapsed.as_secs() / 4) % 2 == 1;
        let mut close = elapsed.as_secs() >= self.secs;
        let switched = through != self.through;
        self.through = through;
        let (dim, caught) = (&mut self.dim, &mut self.caught);
        ctx.show_viewport_immediate(dim.id, dim_builder(dim), |ui, _| {
            if switched {
                ui.ctx().send_viewport_cmd(ViewportCommand::MousePassthrough(through));
            }
            let rect = ui.max_rect();
            let painter = ui.painter();
            painter.rect_filled(
                rect,
                0.0,
                lum_ui::theme::alpha(tokens.scrim(), if through { 0.35 } else { 0.64 }),
            );
            let state = if through {
                "CLICK-THROUGH: clicks should reach the window behind"
            } else {
                "NORMAL: this dim catches clicks"
            };
            let left = 4 - elapsed.as_secs() % 4;
            say(painter, rect.center(), state, 26.0, tokens.p.text_primary);
            say(
                painter,
                rect.center() + egui::vec2(0.0, 40.0),
                &format!(
                    "switches in {left} s, caught {} clicks while normal, {} while click-through",
                    caught[0], caught[1]
                ),
                16.0,
                tokens.p.text_secondary,
            );
            ui.input(|i| {
                close |= i.key_pressed(Key::Escape);
                if i.pointer.any_click() {
                    caught[usize::from(through)] += 1;
                }
            });
        });
        if close {
            let ok = self.caught[1] == 0;
            println!(
                "  clicks caught while normal: {}, while click-through: {} ({})",
                self.caught[0],
                self.caught[1],
                if ok { "good" } else { "click-through leaked clicks" }
            );
            println!(
                "| 2 click-through | {} | caught {} normal / {} through | {} | (did clicks reach the window behind?) |",
                self.renderer.name(),
                self.caught[0],
                self.caught[1],
                if ok { "pass" } else { "FAIL" }
            );
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
        } else {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }
}

pub fn no_focus(renderer: Renderer) -> Result<(), String> {
    println!("{}", heading("nofocus", renderer));
    println!(
        "  Click into another program now (a text editor) and keep typing. In 5 seconds a panel opens in"
    );
    println!("  the middle of the screen; your typing should stay in the other program. 5 seconds later a");
    println!("  second panel opens that should take the keyboard (as Notes does when you open it).");
    let app = FocusSpike {
        started: Instant::now(),
        renderer,
        quiet: Sample::default(),
        loud: Sample::default(),
        frames: 0,
    };
    launch(renderer, shell::hidden_root(), &[], |_| app)
}

#[derive(Default)]
struct Sample {
    opened: Option<Instant>,
    focused_after: Option<Duration>,
    checks: u32,
}

struct FocusSpike {
    started: Instant,
    renderer: Renderer,
    quiet: Sample,
    loud: Sample,
    frames: u64,
}

impl FocusSpike {
    fn panel(ctx: &egui::Context, title: &str, sample: &mut Sample, take_focus: bool) {
        let tokens = Tokens::of(ctx);
        let builder = ViewportBuilder::default()
            .with_title(title)
            .with_app_id(super::APP_ID)
            .with_inner_size([420.0, 160.0])
            .with_decorations(false)
            .with_always_on_top()
            .with_taskbar(false)
            .with_active(take_focus);
        let id = ViewportId::from_hash_of(title);
        let first = sample.opened.is_none();
        ctx.show_viewport_immediate(id, builder, |ui, _| {
            let rect = ui.max_rect();
            ui.painter().rect_filled(rect, 0.0, tokens.p.bg_base);
            let text = if take_focus {
                "This one should have the keyboard"
            } else {
                "This one should not have the keyboard"
            };
            say(ui.painter(), rect.center(), text, 16.0, tokens.p.text_primary);
            if first && take_focus {
                ui.ctx().send_viewport_cmd(ViewportCommand::Focus);
            }
        });
        let opened = *sample.opened.get_or_insert_with(Instant::now);
        if sample.focused_after.is_none() && ctx.input_for(id, |i| i.viewport().focused) == Some(true) {
            sample.focused_after = Some(opened.elapsed());
        }
        sample.checks += 1;
    }
}

impl eframe::App for FocusSpike {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.frames += 1;
        if self.frames == 1 {
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }
        let t = self.started.elapsed().as_secs_f32();
        if t >= 5.0 {
            // the marker makes Hyprland's no_initial_focus rule apply; elsewhere with_active(false) does it
            Self::panel(
                &ctx,
                &format!("Luminescent quiet panel{}", marker::NO_ACTIVATE),
                &mut self.quiet,
                false,
            );
        }
        if t >= 10.0 {
            Self::panel(&ctx, "Luminescent focused panel", &mut self.loud, true);
        }
        if t >= 14.0 {
            for line in desktop::describe_windows() {
                println!("  Hyprland: {line}");
            }
            let verdict = |s: &Sample| {
                s.focused_after.map_or("never had the keyboard".to_owned(), |d| {
                    format!("had the keyboard after {:.0} ms", d.as_secs_f32() * 1000.0)
                })
            };
            let quiet_ok = self.quiet.focused_after.is_none();
            let loud_ok = self.loud.focused_after.is_some();
            println!(
                "  quiet panel: {} ({})",
                verdict(&self.quiet),
                if quiet_ok { "good" } else { "it stole the focus" }
            );
            println!(
                "  focused panel: {} ({})",
                verdict(&self.loud),
                if loud_ok { "good" } else { "it could not take the focus" }
            );
            println!(
                "| 3 no focus | {} | quiet: {}, focused: {} | {} | (did your typing stay in the other program?) |",
                self.renderer.name(),
                verdict(&self.quiet),
                verdict(&self.loud),
                if quiet_ok && loud_ok { "pass" } else { "FAIL" }
            );
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

pub fn drop(renderer: Renderer, secs: u64) -> Result<(), String> {
    println!("{}", heading("drop", renderer));
    println!(
        "  A window opens. Drag one or more files from Dolphin or Explorer onto it. Every drop is listed"
    );
    println!("  here. Ends after {secs} s, on Esc or when you close the window.");
    let app = DropSpike {
        renderer,
        deadline: Instant::now() + Duration::from_secs(secs),
        hovered: 0,
        dropped: Vec::new(),
    };
    launch(renderer, window("Luminescent drop check", [560.0, 320.0]).with_drag_and_drop(true), &[], |_| app)
}

struct DropSpike {
    renderer: Renderer,
    deadline: Instant,
    hovered: u32,
    dropped: Vec<String>,
}

impl eframe::App for DropSpike {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let tokens = Tokens::of(&ctx);
        let (hovering, dropped, close) = ui.input(|i| {
            (
                !i.raw.hovered_files.is_empty(),
                i.raw.dropped_files.clone(),
                i.key_pressed(Key::Escape) || i.viewport().close_requested(),
            )
        });
        if hovering {
            self.hovered += 1;
        }
        for file in dropped {
            let name = file.path().display().to_string();
            println!("  dropped: {name}");
            self.dropped.push(name);
        }
        let rect = ui.max_rect();
        let painter = ui.painter();
        painter.rect_filled(rect, 0.0, if hovering { tokens.p.primary_muted } else { tokens.p.bg_base });
        painter.rect_stroke(
            rect.shrink(16.0),
            10.0,
            egui::Stroke::new(1.5, tokens.p.border),
            egui::StrokeKind::Inside,
        );
        let text = if hovering {
            "Let go to drop".to_owned()
        } else {
            format!("Drop files here ({} so far)", self.dropped.len())
        };
        say(painter, rect.center(), &text, 18.0, tokens.p.text_primary);
        if let Some(last) = self.dropped.last() {
            say(painter, pos2(rect.center().x, rect.center().y + 34.0), last, 12.0, tokens.p.text_muted);
        }
        if close || Instant::now() >= self.deadline {
            println!("  {} files dropped, hovering seen in {} frames", self.dropped.len(), self.hovered);
            println!(
                "| 4 file drop | {} | {} files dropped | {} | |",
                self.renderer.name(),
                self.dropped.len(),
                if self.dropped.is_empty() { "FAIL (nothing arrived)" } else { "pass" }
            );
            ctx.send_viewport_cmd(ViewportCommand::Close);
        } else {
            ctx.request_repaint_after(self.deadline.saturating_duration_since(Instant::now()));
        }
    }
}

fn ms(d: Duration) -> f32 {
    d.as_secs_f32() * 1000.0
}
