//! Timers and stopwatches: as many floating panels as you like, kept in `timers.json` and brought back still
//! running after a restart, plus the overlay's Timers page that shows them as cards.

mod view;

use std::time::{Duration, Instant};

use egui::{Rect, Ui, vec2};
use jiff::ZonedDateTime;
use lum_calc::timer::{Mode, TimerState, parse_entry, wall_seconds};
use lum_core::actions::Action;
use lum_core::module::{Module, ModuleCx, StartError};
use lum_core::pyjson::{self, Style};
use lum_core::settings::Settings;
use lum_core::store::{self, Writer};
use serde_json::Value;

use crate::overlay::PageFrame;
use crate::panel::{FloatingPanel, Geometry, centred_on};
use crate::text::Text;
use crate::theme::Tokens;
use crate::tools::edit;
use crate::widgets::{Button, IconButton, IconShape, TextInput};
use crate::{Bind, Inbox, Request, Services, UiCx, UiModule};
use view::{Act, Entry, Progress, RUN_GREEN};

const SAVE_GEOMETRY_AFTER: Duration = Duration::from_millis(500);

#[derive(Debug)]
enum Cmd {
    /// "ask": a new panel with a box for the time; false: a stopwatch ready to start.
    New {
        ask: bool,
    },
    ToggleEdit,
    Lap,
    /// An automation asked for a running timer (`ms` 0: a stopwatch).
    Start {
        ms: f64,
        label: String,
    },
}

#[derive(Debug)]
struct Panel {
    id: u64,
    state: TimerState,
    panel: FloatingPanel,
    entry: Option<Entry>,
}

impl Panel {
    /// "Timer", "Stopwatch", or "Timer  ·  DONE", with the label after it.
    fn retitle(&mut self) {
        let done = if self.state.finished { "  ·  DONE" } else { "" };
        self.panel.set_title(&format!("{}{done}", self.state.mode.title()), &self.state.label);
    }
}

#[derive(Debug, Default)]
struct PageState {
    typed: String,
    hint: String,
}

pub struct TimerModule {
    services: Services,
    inbox: Inbox<Cmd>,
    running: bool,
    panels: Vec<Panel>,
    next_id: u64,
    last_started: Option<u64>,
    edit: bool,
    writer: Writer,
    /// When a moved or resized panel is written to the file.
    geometry_due: Option<Instant>,
    page: PageState,
}

impl std::fmt::Debug for TimerModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TimerModule").field("panels", &self.panels.len()).finish_non_exhaustive()
    }
}

fn number(settings: &Settings, key: &str, default: f32) -> f32 {
    settings.get_f64(&format!("timer.{key}")).map_or(default, |v| v as f32)
}

impl TimerModule {
    pub fn new(services: &Services) -> TimerModule {
        TimerModule {
            services: services.clone(),
            inbox: Inbox::new(services),
            running: false,
            panels: Vec::new(),
            next_id: 1,
            last_started: None,
            edit: false,
            writer: Writer::new("timers-save"),
            geometry_due: None,
            page: PageState::default(),
        }
    }

    fn path(&self) -> std::path::PathBuf {
        self.services.data_file("timers.json")
    }

    fn status_changed(&self) {
        self.services.send(Request::StatusChanged);
    }

    /// Writes every open panel that has a time (a panel still asking for one is not kept).
    fn save(&mut self) {
        self.geometry_due = None;
        let now = Instant::now();
        let wall = wall_seconds();
        let list: Vec<Value> = self
            .panels
            .iter()
            .filter(|p| !p.state.fresh)
            .map(|p| {
                let g = p.panel.geometry;
                p.state.to_json(
                    [g.x.round() as i64, g.y.round() as i64, g.w.round() as i64, g.h.round() as i64],
                    wall,
                    now,
                )
            })
            .collect();
        self.writer.save(&self.path(), pyjson::to_string(&Value::Array(list), Style::indented(2)));
    }

    fn add_panel(&mut self, state: TimerState, geometry: Option<[f32; 4]>, settings: &Settings) {
        let (w, h) = (number(settings, "default_width", 260.0), number(settings, "default_height", 160.0));
        let min = vec2(number(settings, "min_width", 180.0), number(settings, "min_height", 120.0));
        let g = match geometry {
            Some([x, y, w, h]) => Geometry { x, y, w, h },
            None => {
                let screen = edit::cursor_screen_rect()
                    .unwrap_or(Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(1280.0, 800.0)));
                let at = centred_on(vec2(w, h), screen);
                Geometry { x: at.x, y: at.y, w, h }
            }
        };
        let id = self.next_id;
        self.next_id += 1;
        let mut panel = FloatingPanel::new(&format!("timer-{id}"), state.mode.title(), g)
            .min_size(min)
            .drag_needs_edit_mode(false)
            .activate_on_show(state.fresh);
        panel.opacity = number(settings, "bg_opacity", 0.85);
        panel.set_title(state.mode.title(), &state.label);
        panel.edit_mode = self.edit;
        panel.open();
        let entry = state.fresh.then(Entry::open);
        if state.is_running() {
            self.last_started = Some(id);
        }
        self.panels.push(Panel { id, state, panel, entry });
    }

    fn restore(&mut self, settings: &Settings) {
        let saved = store::load_json(&self.path());
        let now = Instant::now();
        let wall = wall_seconds();
        for cfg in saved.as_ref().and_then(Value::as_array).into_iter().flatten() {
            let Some(cfg) = cfg.as_object() else { continue };
            let geometry = ["x", "y", "w", "h"].map(|k| cfg.get(k).and_then(Value::as_f64).map(|v| v as f32));
            let geometry = match geometry {
                [Some(x), Some(y), Some(w), Some(h)] => Some([x, y, w, h]),
                _ => None,
            };
            let mut state = TimerState::from_json(cfg, wall, now);
            state.fresh = false;
            self.add_panel(state, geometry, settings);
        }
    }

    fn new_timer(&mut self, ask: bool, settings: &Settings) {
        if ask && let Some(panel) = self.panels.iter_mut().find(|p| p.state.fresh) {
            // the hotkey again while one is waiting for a time
            panel.entry = Some(Entry::open());
            return;
        }
        let state = if ask { TimerState::asking() } else { TimerState::new(Mode::Stopwatch, 0, "") };
        self.add_panel(state, None, settings);
        self.save();
        self.status_changed();
    }

    /// The stopwatch started last, or else the newest running one.
    fn lap_target(&self) -> Option<usize> {
        let stopwatch = |p: &Panel| p.state.mode == Mode::Stopwatch;
        let last =
            self.last_started.and_then(|id| self.panels.iter().position(|p| p.id == id && stopwatch(p)));
        last.or_else(|| self.panels.iter().rposition(|p| stopwatch(p) && p.state.is_running()))
    }

    fn take_commands(&mut self, settings: &Settings) {
        for cmd in self.inbox.take() {
            match cmd {
                Cmd::New { ask } => self.new_timer(ask, settings),
                Cmd::ToggleEdit => {
                    self.edit = !self.edit;
                    for p in &mut self.panels {
                        p.panel.edit_mode = self.edit;
                    }
                }
                Cmd::Lap => {
                    if let Some(i) = self.lap_target()
                        && self.panels[i].state.lap(Instant::now())
                    {
                        self.save();
                    }
                }
                Cmd::Start { ms, label } => {
                    let state = if ms > 0.0 {
                        TimerState::new(Mode::Timer, ms as i64, &label)
                    } else {
                        TimerState::new(Mode::Stopwatch, 0, &label)
                    };
                    self.add_panel(state, None, settings);
                    let last = self.panels.len() - 1;
                    self.act(last, Act::PlayPause);
                    self.save();
                    self.status_changed();
                }
            }
        }
    }

    fn log(&self, name: String, logged: lum_calc::timer::Logged) {
        if let Some(secs) = logged.0 {
            self.services.send(Request::Activity { kind: "timer".into(), label: name, secs: Some(secs) });
        }
    }

    /// Does what a click on a timer asked for. Gives back whether anything changed that is kept or shown in
    /// the tray.
    fn act(&mut self, index: usize, act: Act) -> bool {
        let now = Instant::now();
        let p = &mut self.panels[index];
        let name = p.state.name();
        match act {
            Act::PlayPause => {
                let logged = p.state.play_pause(now);
                if p.state.is_running() {
                    self.last_started = Some(p.id);
                }
                self.log(name, logged);
            }
            Act::Reset => {
                let logged = p.state.reset(now);
                self.log(name, logged);
            }
            Act::SwitchMode => {
                let logged = p.state.switch_mode(now);
                p.panel.set_title(p.state.mode.title(), &p.state.label);
                self.log(name, logged);
            }
            Act::Lap => {
                p.state.lap(now);
            }
            Act::AskTime => p.entry = Some(Entry::open()),
            Act::CloseEntry => {
                if p.state.fresh {
                    // Esc on a brand new panel: never mind
                    p.panel.close();
                }
                p.entry = None;
            }
            Act::Submit(text) => {
                let zoned = ZonedDateTime::now();
                match parse_entry(&text, zoned.datetime()) {
                    None => {
                        if let Some(entry) = p.entry.as_mut() {
                            entry.invalid = true;
                        }
                        return false;
                    }
                    Some((ms, label)) => {
                        let Some(logged) = p.state.set_time(ms, &label, now) else {
                            p.entry = None;
                            return false;
                        };
                        p.entry = None;
                        p.panel.set_title(p.state.mode.title(), &p.state.label);
                        self.last_started = Some(p.id);
                        self.log(name, logged);
                    }
                }
            }
        }
        true
    }

    fn finish(&mut self, index: usize, finish: lum_calc::timer::Finish, settings: &Settings) {
        let p = &mut self.panels[index];
        let name = p.state.name();
        self.services.send(Request::Emit { name: "timer.finished".into(), text: finish.text });
        if let Some(secs) = finish.logged.0 {
            self.services.send(Request::Activity { kind: "timer".into(), label: name, secs: Some(secs) });
        }
        // it comes to the front without taking the keyboard from whatever is being typed in
        p.panel.open();
        if settings.get_bool("timer.flash_on_finish", true) {
            p.panel.flash(8);
        }
        let kind = settings.get_str("timer.finish_sound").unwrap_or("alarm");
        let file = settings.get_str("timer.finish_sound_file").unwrap_or_default();
        lum_platform::sounds::play(kind, file, 1.0);
        self.save();
        self.status_changed();
    }

    /// The timers the panels hold, for the page.
    fn cards(&self) -> impl Iterator<Item = &Panel> {
        self.panels.iter().filter(|p| !p.state.fresh)
    }

    fn next_wake(&self, now: Instant) -> Option<Duration> {
        let mut soonest: Option<Duration> = None;
        for p in &self.panels {
            let wait = if p.state.is_running() {
                if p.state.show_ms {
                    Duration::from_millis(31)
                } else {
                    // the moment the seconds digit changes
                    let ms = p.state.display_ms(now) as u64 % 1000;
                    let left = if p.state.mode == Mode::Timer { ms } else { 1000 - ms };
                    Duration::from_millis(left.max(1) + 5)
                }
            } else if let Some(at) = p.state.wake_at() {
                at.saturating_duration_since(now)
            } else {
                continue;
            };
            soonest = Some(soonest.map_or(wait, |s| s.min(wait)));
        }
        soonest
    }
}

impl Module for TimerModule {
    fn key(&self) -> &'static str {
        "timer"
    }

    fn title(&self) -> &'static str {
        "Timer / Stopwatch"
    }

    fn start(&mut self, cx: &mut ModuleCx<'_>) -> Result<(), StartError> {
        self.running = true;
        if cx.settings.get_bool("timer.restore_on_start", true) {
            self.restore(cx.settings);
        }
        self.status_changed();
        Ok(())
    }

    fn stop(&mut self, _cx: &mut ModuleCx<'_>) {
        self.save();
        self.writer.flush();
        // stopping is not a pause: the timers keep their time in the file and carry on at the next start
        for p in &mut self.panels {
            p.panel.close();
        }
        self.panels.clear();
        self.edit = false;
        self.last_started = None;
        self.running = false;
        self.status_changed();
    }

    /// Only hotkeys and the progress style change live; running timers are left alone.
    fn apply_settings(&mut self, _cx: &mut ModuleCx<'_>) -> Result<(), StartError> {
        Ok(())
    }

    fn actions(&self) -> Vec<Action> {
        vec![
            self.inbox.action("new_timer", "New timer", || Cmd::New { ask: true }),
            self.inbox.action("new_stopwatch", "New stopwatch", || Cmd::New { ask: false }),
            self.inbox.action("edit", "Timer edit mode (move, reset, snap)", || Cmd::ToggleEdit),
            self.inbox.action("lap", "Lap the last stopwatch", || Cmd::Lap),
        ]
    }
}

impl UiModule for TimerModule {
    fn start_timer(&mut self, ms: f64, label: &str) -> bool {
        self.inbox.push(Cmd::Start { ms, label: label.to_owned() });
        true
    }

    fn binds(&self) -> Vec<Bind> {
        vec![
            Bind { setting: "new_hotkey", id: "new", label: "new timer" },
            Bind { setting: "edit_hotkey", id: "edit", label: "timer edit mode" },
            Bind { setting: "lap_hotkey", id: "lap", label: "lap" },
        ]
    }

    fn bind_pressed(&mut self, id: &str) {
        match id {
            "new" => self.inbox.push(Cmd::New { ask: true }),
            "edit" => self.inbox.push(Cmd::ToggleEdit),
            "lap" => self.inbox.push(Cmd::Lap),
            _ => {}
        }
    }

    fn page_key(&self) -> Option<&'static str> {
        Some("timers")
    }

    fn reset_page(&mut self) {
        self.page = PageState::default();
    }

    fn tray_actions(&self) -> Vec<(String, String)> {
        vec![("new_timer".into(), "New timer / stopwatch".into())]
    }

    fn status_lines(&self) -> Vec<String> {
        let now = Instant::now();
        self.panels
            .iter()
            .filter(|p| p.state.is_running() || p.state.finished)
            .map(|p| p.state.status_line(now))
            .collect()
    }

    fn show(&mut self, cx: &mut UiCx<'_>) {
        self.take_commands(cx.settings);
        if !self.running {
            return;
        }
        let now = Instant::now();
        let progress = Progress::from_setting(cx.settings.get_str("timer.progress"));
        let mut changed = false;
        for i in 0..self.panels.len() {
            if let Some(finish) = self.panels[i].state.tick(now) {
                self.finish(i, finish, cx.settings);
            }
        }
        let mut acts = Vec::new();
        let mut moved = false;
        for (i, p) in self.panels.iter_mut().enumerate() {
            if !p.panel.is_open() {
                continue;
            }
            p.retitle();
            let (state, entry, edit, id) = (&p.state, &mut p.entry, p.panel.edit_mode, p.panel.id());
            let tokens = cx.tokens.clone();
            let running = state.is_running();
            let ask = std::cell::Cell::new(false);
            let mut body_acts = Vec::new();
            let response = p.panel.show_with(
                cx.ctx,
                |ui| {
                    if ui
                        .add(
                            IconButton::shape(IconShape::Clock)
                                .size(24.0)
                                .ghost()
                                .tooltip("Type a new time (5m, 1h30, at 17:30…)"),
                        )
                        .clicked()
                    {
                        ask.set(true);
                    }
                    if running {
                        let (dot, _) = ui.allocate_exact_size(vec2(6.0, 6.0), egui::Sense::hover());
                        ui.painter().circle_filled(dot.center(), 3.0, RUN_GREEN);
                    }
                },
                |ui| body_acts = view::panel_body(ui, id, state, entry, edit, progress, now, &tokens),
            );
            if ask.get() {
                acts.push((i, Act::AskTime));
            }
            acts.extend(body_acts.into_iter().map(|a| (i, a)));
            if response.escape && p.entry.is_some() {
                acts.push((i, Act::CloseEntry));
            }
            moved |= response.geometry_changed;
        }
        for (i, act) in acts {
            changed |= self.act(i, act);
        }
        // closed panels go: what ran is logged and the file and tray are brought up to date
        let before = self.panels.len();
        let mut logged = Vec::new();
        self.panels.retain_mut(|p| {
            let gone = !p.panel.is_open();
            if gone && p.state.is_running() {
                logged.push((p.state.name(), p.state.reset(now)));
            }
            !gone
        });
        for (name, l) in logged {
            self.log(name, l);
        }
        if self.panels.len() != before {
            changed = true;
        }
        if changed {
            self.save();
            self.status_changed();
        } else if moved {
            self.geometry_due.get_or_insert(now + SAVE_GEOMETRY_AFTER);
        }
        if let Some(due) = self.geometry_due {
            if now >= due {
                self.save();
            } else {
                cx.ctx.request_repaint_after(due - now);
            }
        }
        self.edit_bar(cx);
        if let Some(wait) = self.next_wake(now) {
            cx.ctx.request_repaint_after(wait);
        }
    }

    fn page(&mut self, ui: &mut Ui, rect: Rect, cx: &mut UiCx<'_>) -> PageFrame {
        self.take_commands(cx.settings);
        if !self.running {
            return PageFrame::default();
        }
        let now = Instant::now();
        let tokens = Tokens::of(cx.ctx);
        let inner = rect.shrink2(vec2(24.0, 0.0));
        let mut page = ui.new_child(
            egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)),
        );
        page.add_space(12.0);
        let mut typed = std::mem::take(&mut self.page.typed);
        let mut submit = false;
        let mut stopwatch = false;
        page.horizontal(|ui| {
            ui.add(Text::new("Timers").px(15.0).semibold());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                stopwatch = ui.add(Button::ghost("New stopwatch")).clicked();
                let response = ui.add(
                    TextInput::new(&mut typed)
                        .hint("New timer: 5m, 1h30, 25m focus, at 17:30…   (Enter)")
                        .width(380.0),
                );
                submit = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if submit {
                    response.request_focus();
                }
            });
        });
        self.page.typed = typed;
        let mut closed = None;
        let mut shown = None;
        let mut page_acts = Vec::new();
        let items: Vec<(u64, TimerState)> = self.cards().map(|p| (p.id, p.state.clone())).collect();
        if items.is_empty() {
            let hint = if self.page.hint.is_empty() {
                "No timers yet. Type a time above, or start a stopwatch."
            } else {
                &self.page.hint
            };
            page.add_space(16.0);
            page.add(Text::new(hint).px(12.0).tone(crate::theme::Tone::Muted));
        } else if !self.page.hint.is_empty() {
            page.add(Text::new(self.page.hint.clone()).px(12.0).tone(crate::theme::Tone::Muted));
        }
        page.add_space(16.0);
        let width = (inner.width() - 32.0) / 3.0;
        let top = page.cursor().min;
        for (n, (id, state)) in items.iter().enumerate() {
            let col = (n % 3) as f32;
            let row = (n / 3) as f32;
            let card = Rect::from_min_size(
                top + vec2(col * (width + 16.0), row * (190.0 + 16.0)),
                vec2(width, 190.0),
            );
            let (acts, close, show_panel) = view::card(&mut page, card, state, now, &tokens);
            page_acts.extend(acts.into_iter().map(|a| (*id, a)));
            if close {
                closed = Some(*id);
            }
            if show_panel {
                shown = Some(*id);
            }
        }
        let mut changed = false;
        if submit {
            match parse_entry(&self.page.typed, ZonedDateTime::now().datetime()) {
                None => {
                    self.page.hint = "Could not read a time. Try 5m, 1h30, 90s, 2:30 or at 17:30.".to_owned()
                }
                Some((ms, label)) => {
                    let state = if ms > 0.0 {
                        TimerState::new(Mode::Timer, ms as i64, &label)
                    } else {
                        TimerState::new(Mode::Stopwatch, 0, &label)
                    };
                    self.page.typed.clear();
                    self.page.hint.clear();
                    self.add_panel(state, None, cx.settings);
                    let last = self.panels.len() - 1;
                    self.act(last, Act::PlayPause);
                    changed = true;
                }
            }
        }
        if stopwatch {
            self.add_panel(TimerState::new(Mode::Stopwatch, 0, ""), None, cx.settings);
            changed = true;
        }
        for (id, act) in page_acts {
            if let Some(i) = self.panels.iter().position(|p| p.id == id) {
                changed |= self.act(i, act);
            }
        }
        if let Some(id) = shown
            && let Some(p) = self.panels.iter_mut().find(|p| p.id == id)
        {
            p.panel.open();
        }
        if let Some(id) = closed
            && let Some(p) = self.panels.iter_mut().find(|p| p.id == id)
        {
            p.panel.close();
        }
        if changed {
            self.save();
            self.status_changed();
        }
        if self.panels.iter().any(|p| p.state.is_running()) {
            cx.ctx.request_repaint_after(Duration::from_millis(250));
        }
        PageFrame { busy: false }
    }
}

impl TimerModule {
    /// Edit mode: the bar of nine anchors that snaps every panel.
    fn edit_bar(&mut self, cx: &mut UiCx<'_>) {
        if !self.edit || self.panels.is_empty() {
            return;
        }
        let screen =
            edit::cursor_screen_rect().unwrap_or_else(|| crate::window::fallback_screen(cx.ctx).rect);
        let Some(anchor) = edit::anchor_window(cx.ctx, "timers", "Snap timers to:", screen) else { return };
        let padding = number(cx.settings, "anchor_padding", 12.0);
        let sizes: Vec<_> =
            self.panels.iter().map(|p| vec2(p.panel.geometry.w, p.panel.geometry.h)).collect();
        for (p, at) in self.panels.iter_mut().zip(edit::snap_all(anchor, &sizes, padding, screen)) {
            p.panel.geometry.x = at.x;
            p.panel.geometry.y = at.y;
        }
        self.save();
    }
}
