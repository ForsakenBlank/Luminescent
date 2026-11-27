//! How a timer looks: the panel's body (the big time with its ring, the buttons, the laps and the bar along the
//! bottom) and the larger card the overlay's Timers page shows.

use std::time::Instant;

use egui::{Align, Layout, Rect, Sense, Stroke, Ui, UiBuilder, pos2, vec2};
use lum_calc::timer::{Mode, TimerState, fmt_clock};

use crate::fonts::Weight;
use crate::text::Text;
use crate::theme::{Tokens, Tone};
use crate::widgets::{Button, TextInput};

pub const RUN_GREEN: egui::Color32 = egui::Color32::from_rgb(0x3F, 0xBF, 0x74);

/// How a countdown shows its progress (Settings → Timer → Countdown progress).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    Ring,
    Bar,
}

impl Progress {
    pub fn from_setting(value: Option<&str>) -> Progress {
        if value == Some("bar") { Progress::Bar } else { Progress::Ring }
    }
}

/// What the person did to a timer this frame.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    PlayPause,
    Reset,
    SwitchMode,
    Lap,
    /// Open the box to type a new time into.
    AskTime,
    /// The box was answered with this text.
    Submit(String),
    /// Esc in the box.
    CloseEntry,
}

/// The typing box of one panel.
#[derive(Debug, Default)]
pub struct Entry {
    pub text: String,
    pub invalid: bool,
    focus: bool,
}

impl Entry {
    pub fn open() -> Entry {
        Entry { focus: true, ..Entry::default() }
    }
}

/// The size of the big time for a panel of this size (`resizeEvent`).
pub fn time_px(width: f32, height: f32, show_ms: bool, ring: bool) -> f32 {
    let px = (width / if show_ms { 5.6 } else { 4.4 }).floor().min((height / 4.0).floor()).clamp(18.0, 72.0);
    if ring { (px * 0.72).floor().max(14.0) } else { px }
}

fn play_text(state: &TimerState, now: Instant) -> &'static str {
    if state.is_running() {
        "❚❚  Pause"
    } else if state.elapsed_ms(now) <= 0.0 || state.finished {
        "▶  Start"
    } else {
        "▶  Resume"
    }
}

/// The big time, centred in `rect`, with the countdown's ring just left of it.
#[allow(clippy::too_many_arguments)]
fn big_time(
    ui: &mut Ui,
    salt: egui::Id,
    rect: Rect,
    state: &TimerState,
    now: Instant,
    progress: Progress,
    tokens: &Tokens,
    panel: egui::Vec2,
) -> bool {
    let ring = state.mode == Mode::Timer && progress == Progress::Ring && state.duration_ms > 0;
    let px = time_px(panel.x, panel.y, state.show_ms, ring).min(rect.height() * 0.9).max(14.0);
    let (main, frac) = fmt_clock(state.display_ms(now), state.show_ms);
    let colour = if state.finished { tokens.p.primary } else { tokens.p.text_primary };
    let font = tokens.font(px, Weight::Medium, true);
    let painter = ui.painter();
    let main_galley = painter.layout_no_wrap(main, font, colour);
    let frac_galley = (!frac.is_empty()).then(|| {
        painter.layout_no_wrap(
            frac,
            tokens.font((px * 0.6).max(10.0), Weight::Medium, true),
            tokens.p.text_muted,
        )
    });
    let text_w = main_galley.size().x + frac_galley.as_ref().map_or(0.0, |g| g.size().x);
    let d = px * 0.8;
    let ring_w = if ring { d * 1.45 } else { 0.0 };
    let left = rect.center().x - (text_w + ring_w) / 2.0;
    let y = rect.center().y;
    if ring {
        let c = pos2(left + d / 2.0, y);
        let width = (d / 9.0).max(2.5);
        painter.circle_stroke(c, d / 2.0, Stroke::new(width, tokens.p.bg_frame));
        let left_frac = 1.0 - state.progress(now) as f32;
        if left_frac > 0.0 {
            // what is left runs down clockwise from the top
            let steps = ((left_frac * 72.0).ceil() as usize).max(2);
            let points: Vec<_> = (0..=steps)
                .map(|i| {
                    let a = -std::f32::consts::FRAC_PI_2
                        + i as f32 / steps as f32 * left_frac * std::f32::consts::TAU;
                    pos2(c.x + d / 2.0 * a.cos(), c.y + d / 2.0 * a.sin())
                })
                .collect();
            painter.add(egui::Shape::line(points, Stroke::new(width, colour)));
        }
    }
    let text_left = left + ring_w;
    let hit = Rect::from_min_size(pos2(left, y - px * 0.7), vec2(text_w + ring_w, px * 1.4));
    painter.galley(pos2(text_left, y - main_galley.size().y / 2.0), main_galley.clone(), colour);
    if let Some(g) = frac_galley {
        painter.galley(
            pos2(text_left + main_galley.size().x, y - g.size().y / 2.0 + px * 0.15),
            g,
            tokens.p.text_muted,
        );
    }
    let response = ui.interact(hit, salt.with("time"), Sense::click());
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Start or pause"));
    response.clicked()
}

/// Draws a panel's body into `ui` (its `max_rect`).
#[allow(clippy::too_many_arguments)]
pub fn panel_body(
    ui: &mut Ui,
    salt: egui::Id,
    state: &TimerState,
    entry: &mut Option<Entry>,
    edit_mode: bool,
    progress: Progress,
    now: Instant,
    tokens: &Tokens,
) -> Vec<Act> {
    let mut acts = Vec::new();
    let rect = ui.max_rect();
    let stopwatch = state.mode == Mode::Stopwatch;
    let bar = !stopwatch && progress == Progress::Bar;
    let lap_rows = if stopwatch { state.laps.len() } else { 0 };
    let laps_h = if lap_rows > 0 { 20.0 + (lap_rows as f32 * 28.0).min(96.0) } else { 0.0 };
    let bar_h = if bar { 2.0 } else { 0.0 };
    let top = Rect::from_min_max(rect.min, pos2(rect.max.x, rect.max.y - laps_h - bar_h));
    let inner = Rect::from_min_max(top.min + vec2(18.0, 16.0), top.max - vec2(18.0, 16.0));

    // from the bottom up: buttons, edit controls, caption; the time takes what is left
    let mut y = inner.max.y;
    let buttons = Rect::from_min_max(pos2(inner.min.x, y - 34.0), pos2(inner.max.x, y));
    y -= 34.0 + 10.0;
    let edit_row = edit_mode.then(|| {
        let r = Rect::from_min_max(pos2(inner.min.x, y - 28.0), pos2(inner.max.x, y));
        y -= 28.0 + 10.0;
        r
    });
    // a short panel in edit mode has no room for the caption as well
    let room_for_caption = y - inner.min.y > 70.0;
    let caption = (!stopwatch && room_for_caption).then(|| {
        let r = Rect::from_min_max(pos2(inner.min.x, y - 16.0), pos2(inner.max.x, y));
        y -= 16.0 + 6.0;
        r
    });
    let time = Rect::from_min_max(inner.min, pos2(inner.max.x, y.max(inner.min.y + 20.0)));

    if let Some(typing) = entry.as_mut() {
        let mut row = ui.new_child(
            UiBuilder::new().max_rect(Rect::from_center_size(time.center(), vec2(time.width(), 36.0))),
        );
        let id = salt.with("entry");
        let response = row.add(
            TextInput::new(&mut typing.text)
                .hint("5m, 1h30, 5m tea, at 17:30…  (empty: stopwatch)")
                .invalid(typing.invalid)
                .id(id),
        );
        if std::mem::take(&mut typing.focus) {
            response.request_focus();
        }
        if typing.invalid {
            response.clone().on_hover_text("Could not read a time. Try 5m, 1h30, 90s, 2:30 or at 17:30.");
        }
        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            acts.push(Act::Submit(typing.text.clone()));
            typing.focus = true;
        }
    } else if big_time(ui, salt, time, state, now, progress, tokens, vec2(rect.width(), rect.height() + 36.0))
    {
        acts.push(Act::PlayPause);
    }

    if let Some(caption) = caption {
        let of = format!("of {}", fmt_clock(state.duration_ms as f64, false).0);
        let text = if state.label.is_empty() { of } else { format!("{of} · {}", state.label) };
        ui.put(caption, Text::new(text).px(12.0).tone(Tone::Muted));
    }

    let mut row =
        ui.new_child(UiBuilder::new().max_rect(buttons).layout(Layout::left_to_right(Align::Center)));
    row.spacing_mut().item_spacing.x = 8.0;
    let lap = stopwatch;
    let widths = 118.0 + if lap { 8.0 + 64.0 } else { 0.0 };
    row.add_space(((buttons.width() - widths) / 2.0).max(0.0));
    if row.add(Button::primary(play_text(state, now)).min_width(118.0)).clicked() {
        acts.push(Act::PlayPause);
    }
    if lap && row.add(Button::ghost("Lap").min_width(64.0)).clicked() {
        acts.push(Act::Lap);
    }

    if let Some(r) = edit_row {
        let mut row = ui.new_child(UiBuilder::new().max_rect(r).layout(Layout::left_to_right(Align::Center)));
        row.spacing_mut().item_spacing.x = 6.0;
        let switch = if stopwatch { "→ Timer" } else { "→ Stopwatch" };
        row.add_space(((r.width() - 150.0) / 2.0).max(0.0));
        if row.add(Button::ghost("Reset")).clicked() {
            acts.push(Act::Reset);
        }
        if row.add(Button::ghost(switch)).clicked() {
            acts.push(Act::SwitchMode);
        }
    }

    if laps_h > 0.0 {
        let area = Rect::from_min_max(pos2(rect.min.x, top.max.y), pos2(rect.max.x, top.max.y + laps_h));
        laps_box(ui, area, state, tokens);
    }
    if bar {
        let line = Rect::from_min_max(
            pos2(rect.min.x + 1.0, rect.max.y - 3.0),
            pos2(rect.max.x - 1.0, rect.max.y - 1.0),
        );
        ui.painter().rect_filled(line, 0.0, tokens.p.bg_frame);
        let done = line.width() * state.progress(now) as f32;
        ui.painter().rect_filled(Rect::from_min_size(line.min, vec2(done, 2.0)), 0.0, tokens.p.primary);
    }
    acts
}

fn laps_box(ui: &mut Ui, area: Rect, state: &TimerState, tokens: &Tokens) {
    ui.painter().hline(area.x_range(), area.min.y + 0.5, Stroke::new(1.0, tokens.p.border));
    let inner = Rect::from_min_max(area.min + vec2(18.0, 8.0), area.max - vec2(18.0, 12.0));
    let mut list = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Min)));
    list.set_clip_rect(inner.intersect(ui.clip_rect()));
    egui::ScrollArea::vertical().id_salt("laps").auto_shrink([false, false]).show(&mut list, |ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        let mut previous = 0.0;
        let mut rows = Vec::new();
        for (i, total) in state.laps.iter().enumerate() {
            rows.push((format!("{:02}", i + 1), fmt_clock(total - previous, true), fmt_clock(*total, true)));
            previous = *total;
        }
        // newest on top
        for (n, lap, total) in rows.into_iter().rev() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add_sized([44.0, 16.0], Text::new(n).px(12.0).mono().tone(Tone::Muted));
                ui.add(Text::new(format!("{}{}", lap.0, lap.1)).px(12.0).mono());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add(Text::new(format!("{}{}", total.0, total.1)).px(12.0).mono().tone(Tone::Muted));
                });
            });
        }
    });
}

/// One timer as a card on the Timers page.
pub fn card(
    ui: &mut Ui,
    rect: Rect,
    state: &TimerState,
    now: Instant,
    tokens: &Tokens,
) -> (Vec<Act>, bool, bool) {
    let mut acts = Vec::new();
    let (mut close, mut show_panel) = (false, false);
    ui.painter().rect_filled(rect, tokens.r(), tokens.p.bg_card);
    ui.painter().rect_stroke(rect, tokens.r(), Stroke::new(1.0, tokens.p.border), egui::StrokeKind::Inside);
    let inner = rect.shrink2(vec2(18.0, 14.0));
    let mut card = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Min)));
    card.spacing_mut().item_spacing.y = 8.0;
    card.horizontal(|ui| {
        let kind = state.mode.title().to_uppercase();
        let label = if state.label.is_empty() {
            String::new()
        } else {
            format!("  ·  {}", state.label.to_uppercase())
        };
        let done = if state.finished { "  ·  DONE" } else { "" };
        ui.add(
            crate::text::caps(format!("{kind}{label}{done}"))
                .px(9.5)
                .semibold()
                .spacing(0.12)
                .tone(Tone::Muted),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add(
                    crate::widgets::IconButton::new("✕")
                        .size(24.0)
                        .font_px(10.0)
                        .ghost()
                        .tooltip("Close this timer"),
                )
                .clicked()
            {
                close = true;
            }
        });
    });
    let tone = if state.finished { Tone::Accent2 } else { Tone::Primary };
    card.add(Text::new(fmt_clock(state.display_ms(now), false).0).px(40.0).medium().mono().tone(tone));
    let (line, _) = card.allocate_exact_size(vec2(inner.width(), 3.0), Sense::hover());
    if state.mode == Mode::Timer && state.duration_ms > 0 {
        card.painter().rect_filled(line, 1.0, tokens.p.bg_frame);
        let done = line.width() * state.progress(now) as f32;
        card.painter().rect_filled(Rect::from_min_size(line.min, vec2(done, 3.0)), 1.0, tokens.p.primary);
    }
    card.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        let play = if state.is_running() {
            "Pause"
        } else if state.elapsed_ms(now) <= 0.0 || state.finished {
            "Start"
        } else {
            "Resume"
        };
        if ui.add(Button::primary(play)).clicked() {
            acts.push(Act::PlayPause);
        }
        if ui.add(Button::ghost("Reset")).clicked() {
            acts.push(Act::Reset);
        }
        if state.mode == Mode::Stopwatch && ui.add(Button::ghost("Lap")).clicked() {
            acts.push(Act::Lap);
        }
        if ui.add(Button::ghost("Panel")).on_hover_text("Show its floating panel").clicked() {
            show_panel = true;
        }
    });
    (acts, close, show_panel)
}
