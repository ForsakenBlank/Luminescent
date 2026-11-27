//! Screen Ruler: the hotkey freezes every screen (like the colour picker) and shows crosshairs. Drag from one
//! point to another: the tag shows its width x height and the straight distance, in real screen pixels. Hold
//! Shift to keep the line level or upright. Letting go copies "W x H" (Ctrl: the distance) and Enter copies and
//! closes; arrow keys nudge the pointer 1 px (Shift: 10 px). Esc or right-click closes.

use std::time::Instant;

use egui::{Color32, Context, CursorIcon, Id, Key, Pos2, Rect, Shape, Stroke, StrokeKind, Ui, pos2, vec2};
use lum_core::actions::Action;
use lum_core::module::{Module, ModuleCx, StartError};
use lum_platform::desktop;
use lum_screen::ruler::{self, Measure};
use lum_screen::{IRect, Point};

use crate::fonts::Weight;
use crate::theme::{Tokens, alpha};
use crate::tools::colorpicker::arrow;
use crate::tools::frozen::{self, FrozenScreens, ScreenCx, Shot};
use crate::{Bind, Inbox, Request, Services, UiCx, UiModule};

const TITLE: &str = "Screen Ruler";

enum Cmd {
    Measure(Instant),
    Grabbed(Result<Vec<Shot>, String>, Instant),
}

/// One screen's pointer and drag, as each of Python's ruler windows kept its own.
#[derive(Debug, Clone, Copy, Default)]
struct Marks {
    pos: Option<Point>,
    start: Option<Point>,
    end: Option<Point>,
}

impl Marks {
    fn measure(&self, ratio: f64) -> Option<Measure> {
        Some(ruler::measure(self.start?, self.end?, ratio))
    }
}

struct Measuring {
    screens: FrozenScreens,
    marks: Vec<Marks>,
    /// Where the mouse was last seen, so a nudge is not undone by a mouse that did not move.
    last_hover: Option<Pos2>,
}

enum Outcome {
    Copy { got: Measure, distance: bool, close: bool },
    Close,
}

pub struct RulerModule {
    inbox: Inbox<Cmd>,
    services: Services,
    running: bool,
    measuring: Option<Measuring>,
    grabbing: bool,
}

impl std::fmt::Debug for RulerModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RulerModule").field("running", &self.running).finish_non_exhaustive()
    }
}

impl RulerModule {
    pub fn new(services: &Services) -> RulerModule {
        RulerModule {
            inbox: Inbox::new(services),
            services: services.clone(),
            running: false,
            measuring: None,
            grabbing: false,
        }
    }

    pub fn is_measuring(&self) -> bool {
        self.measuring.is_some()
    }

    fn take_commands(&mut self, ctx: &Context) {
        for cmd in self.inbox.take() {
            match cmd {
                Cmd::Measure(asked) => {
                    if self.measuring.is_some() || self.grabbing {
                        continue;
                    }
                    self.grabbing = true;
                    let inbox = self.inbox.clone();
                    frozen::grab(ctx, move |shots| inbox.push(Cmd::Grabbed(shots, asked)));
                }
                Cmd::Grabbed(shots, asked) => {
                    self.grabbing = false;
                    match shots {
                        Ok(shots) => {
                            let marks = vec![Marks::default(); shots.len()];
                            let screens = FrozenScreens::new(frozen::RULER, shots, asked);
                            self.measuring = Some(Measuring { screens, marks, last_hover: None });
                        }
                        Err(e) => self.notify(&format!("Could not grab the screen: {e}"), 4000),
                    }
                }
            }
        }
    }

    fn notify(&self, text: &str, ms: u32) {
        self.services.send(Request::Notify { title: TITLE.into(), text: text.into(), ms });
    }

    fn copy(&self, got: Measure, distance: bool) {
        let (clip, shown) = got.copy_text(distance);
        crate::tools::clip::copy(clip);
        self.notify(&shown, 1500);
    }

    fn draw(&mut self, ctx: &Context) {
        let Some(Measuring { screens, marks, last_hover }) = self.measuring.as_mut() else { return };
        let mut outcome = None;
        screens.show(ctx, |ui, cx, _response| {
            ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
            let m = &mut marks[cx.index];
            if let Some(o) = handle_input(ui, cx, m, last_hover) {
                outcome = Some(o);
            }
            paint(ui, cx, m);
        });
        match outcome {
            Some(Outcome::Copy { got, distance, close }) => {
                self.copy(got, distance);
                if close {
                    self.measuring = None;
                }
            }
            Some(Outcome::Close) => self.measuring = None,
            None => {}
        }
    }
}

fn handle_input(ui: &Ui, cx: &ScreenCx<'_>, m: &mut Marks, last_hover: &mut Option<Pos2>) -> Option<Outcome> {
    let (hover, press, secondary, down, released, shift, ctrl) = ui.input(|i| {
        (
            i.pointer.hover_pos().filter(|p| cx.area.contains(*p)),
            i.pointer.primary_pressed(),
            i.pointer.secondary_pressed(),
            i.pointer.primary_down(),
            i.pointer.primary_released(),
            i.modifiers.shift,
            i.modifiers.command || i.modifiers.ctrl,
        )
    });
    if let Some(p) = hover
        && (*last_hover != Some(p) || m.pos.is_none())
    {
        *last_hover = Some(p);
        m.pos = Some(cx.local(p));
        if let (true, Some(start)) = (down, m.start) {
            m.end = Some(ruler::locked(start, cx.local(p), shift));
        }
    } else if hover.is_none() && ui.input(|i| i.pointer.has_pointer()) {
        m.pos = None;
    }
    let here = hover.is_some();
    if here && secondary {
        return Some(Outcome::Close);
    }
    if here && press {
        let p = m.pos?;
        (m.start, m.end) = (Some(p), Some(p));
    }
    if here
        && released
        && let (Some(start), Some(p)) = (m.start, m.pos)
    {
        m.end = Some(ruler::locked(start, p, shift));
        let got = m.measure(cx.ratio())?;
        if !got.is_nothing() {
            return Some(Outcome::Copy { got, distance: ctrl, close: false });
        }
    }
    if !cx.has_keys {
        return None;
    }
    if ui.input(|i| i.key_pressed(Key::Escape)) {
        return Some(Outcome::Close);
    }
    if let Some((dx, dy, shift)) = arrow(ui)
        && let Some(p) = m.pos
    {
        let moved = lum_screen::colour::nudge(p, dx, dy, shift);
        let (x, y) = cx.desktop(moved);
        desktop::move_cursor(x, y);
        m.pos = Some(moved);
    }
    let enter = ui.input(|i| {
        i.events.iter().find_map(|e| match e {
            egui::Event::Key { key: Key::Enter, pressed: true, modifiers, .. } => {
                Some(modifiers.command || modifiers.ctrl)
            }
            _ => None,
        })
    });
    if let Some(distance) = enter {
        let got = m.measure(cx.ratio())?;
        return Some(Outcome::Copy { got, distance, close: true });
    }
    None
}

fn paint(ui: &Ui, cx: &ScreenCx<'_>, m: &Marks) {
    let tokens = Tokens::of(ui.ctx());
    let pal = &tokens.p;
    let painter = ui.painter();
    painter.rect_filled(cx.area, 0.0, Color32::from_black_alpha(40));
    let accent = pal.primary;
    if let (Some(p), None) = (m.pos, m.start) {
        let at = cx.to_ui(p) + vec2(0.5, 0.5);
        let stroke = Stroke::new(1.0, Color32::from_white_alpha(150));
        let across = [pos2(cx.area.min.x, at.y), pos2(cx.area.max.x, at.y)];
        let down = [pos2(at.x, cx.area.min.y), pos2(at.x, cx.area.max.y)];
        painter.extend(Shape::dashed_line(&across, stroke, 4.0, 2.0));
        painter.extend(Shape::dashed_line(&down, stroke, 4.0, 2.0));
    }
    if let (Some(a), Some(b), Some(got)) = (m.start, m.end, m.measure(cx.ratio())) {
        let span = cx.rect(IRect::spanning(a, b));
        painter.rect(span, 0.0, alpha(accent, 40.0 / 255.0), Stroke::new(1.0, accent), StrokeKind::Inside);
        let (pa, pb) = (cx.to_ui(a), cx.to_ui(b));
        painter.line_segment([pa, pb], Stroke::new(2.0, Color32::WHITE));
        for pt in [pa, pb] {
            painter.circle(pt, 4.0, accent, Stroke::new(2.0, Color32::WHITE));
        }
        let font = tokens.font(12.0, Weight::SemiBold, true);
        let galley = painter.layout_no_wrap(got.label(), font, pal.text_primary);
        let (tw, th) = (galley.size().x.ceil() as i32 + 20, galley.size().y.ceil() as i32 + 10);
        let (w, h) = cx.size();
        let tag = cx.rect(ruler::tag_rect(b, tw, th, w, h));
        painter.rect(tag, 8.0, pal.bg_base, Stroke::new(1.0, pal.border), StrokeKind::Inside);
        let text_at = tag.center() - galley.size() / 2.0;
        painter.galley(text_at, galley, pal.text_primary);
        let label = got.label();
        ui.interact(tag, Id::unique(("lum-ruler-tag", cx.index)), egui::Sense::hover())
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, label.clone()));
    }
    if cx.primary || m.pos.is_some() {
        let font = tokens.font(11.0, Weight::Regular, true);
        let galley = painter.layout_no_wrap(ruler::HINT.to_owned(), font, pal.text_secondary);
        let w = galley.size().x + 40.0;
        let r = Rect::from_min_size(pos2(cx.area.center().x - w / 2.0, cx.area.min.y + 22.0), vec2(w, 32.0));
        painter.rect(r, 16.0, pal.bg_base, Stroke::new(1.0, pal.border), StrokeKind::Inside);
        painter.galley(r.center() - galley.size() / 2.0, galley, pal.text_secondary);
    }
}

impl Module for RulerModule {
    fn key(&self) -> &'static str {
        "ruler"
    }

    fn title(&self) -> &'static str {
        TITLE
    }

    fn start(&mut self, _cx: &mut ModuleCx<'_>) -> Result<(), StartError> {
        self.running = true;
        Ok(())
    }

    fn stop(&mut self, _cx: &mut ModuleCx<'_>) {
        self.measuring = None;
        self.running = false;
    }

    fn actions(&self) -> Vec<Action> {
        vec![self.inbox.action("measure", "Measure pixels on screen", || Cmd::Measure(Instant::now()))]
    }
}

impl UiModule for RulerModule {
    fn binds(&self) -> Vec<Bind> {
        vec![Bind { setting: "hotkey", id: "measure", label: "screen ruler" }]
    }

    fn bind_pressed(&mut self, id: &str) {
        if id == "measure" {
            self.inbox.push(Cmd::Measure(Instant::now()));
        }
    }

    fn show(&mut self, cx: &mut UiCx<'_>) {
        if !self.running {
            return;
        }
        let ctx = cx.ctx.clone();
        self.take_commands(&ctx);
        if self.measuring.is_some() {
            self.draw(&ctx);
        }
    }
}
