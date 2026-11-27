//! Colour Picker (the eyedropper): the hotkey grabs every screen once and shows that frozen picture full
//! screen with a loupe (11 x 11 pixels at 13 times, ringed centre, HEX and RGB) that follows the mouse. Click
//! copies the colour in the chosen format; arrow keys nudge the pointer 1 px; Esc or right-click cancels. The
//! last 10 colours are kept (tray menu and command palette), and each can become the theme's accent or a new
//! theme. The colour dialog borrows it as its eyedropper: the colour then goes back to the dialog instead of
//! the clipboard.

use std::time::Instant;

use egui::{Color32, Context, CornerRadius, CursorIcon, Id, Key, Rect, Stroke, StrokeKind, Ui, pos2, vec2};
use lum_core::actions::Action;
use lum_core::module::{Module, ModuleCx, StartError};
use lum_platform::desktop;
use lum_platform::tray::MenuItem;
use lum_screen::colour::{self, Format, Recent};
use lum_screen::{IRect, Point, loupe};
use lum_theme::colour::Rgb;

use crate::fonts::Weight;
use crate::palette::Entry;
use crate::theme::Tokens;
use crate::tools::frozen::{self, FrozenScreens, ScreenCx, Shot};
use crate::{Bind, Inbox, Request, Services, UiCx, UiModule};

const TITLE: &str = "Colour Picker";
const ICON: &str = "🎯";

enum Cmd {
    /// The hotkey or the action; `for_dialog` is the colour dialog that asked, if one did.
    Pick {
        asked: Instant,
        for_dialog: Option<Id>,
    },
    Grabbed {
        shots: Result<Vec<Shot>, String>,
        asked: Instant,
        for_dialog: Option<Id>,
    },
}

/// The screens while a colour is being picked.
struct Picking {
    screens: FrozenScreens,
    /// The screen the mouse is on and where on it, in layout pixels.
    pointer: Option<(usize, Point)>,
    /// Where the mouse was last seen in the window, so a nudge is not undone by a mouse that did not move.
    last_hover: Option<egui::Pos2>,
    for_dialog: Option<Id>,
}

/// What happened on the frozen screens this frame.
enum Outcome {
    Picked(Rgb),
    Cancelled,
}

pub struct ColorPickerModule {
    inbox: Inbox<Cmd>,
    services: Services,
    running: bool,
    recent: Recent,
    format: Format,
    upper: bool,
    picking: Option<Picking>,
    /// A grab is on its way (a second hotkey press does nothing meanwhile).
    grabbing: bool,
}

impl std::fmt::Debug for ColorPickerModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ColorPickerModule").field("running", &self.running).finish_non_exhaustive()
    }
}

impl ColorPickerModule {
    pub fn new(services: &Services) -> ColorPickerModule {
        ColorPickerModule {
            inbox: Inbox::new(services),
            services: services.clone(),
            running: false,
            recent: Recent::load(&services.data_file("colours.json")),
            format: Format::Hex,
            upper: true,
            picking: None,
            grabbing: false,
        }
    }

    /// Whether the frozen screens are up (tests).
    pub fn is_picking(&self) -> bool {
        self.picking.is_some()
    }

    pub fn recent(&self) -> &[String] {
        &self.recent.colours
    }

    fn fmt(&self, c: Rgb) -> String {
        colour::text(c, self.format, self.upper)
    }

    fn take_commands(&mut self, ctx: &Context) {
        for cmd in self.inbox.take() {
            match cmd {
                Cmd::Pick { asked, for_dialog } => {
                    if self.picking.is_some() || self.grabbing {
                        continue;
                    }
                    self.grabbing = true;
                    let inbox = self.inbox.clone();
                    frozen::grab(ctx, move |shots| inbox.push(Cmd::Grabbed { shots, asked, for_dialog }));
                }
                Cmd::Grabbed { shots, asked, for_dialog } => {
                    self.grabbing = false;
                    match shots {
                        Ok(shots) => self.open(shots, asked, for_dialog),
                        Err(e) => {
                            if let Some(who) = for_dialog {
                                eyedropper::answer(ctx, who, None);
                            }
                            self.notify(&format!("Could not grab the screen: {e}"), 4000);
                        }
                    }
                }
            }
        }
    }

    fn open(&mut self, shots: Vec<Shot>, asked: Instant, for_dialog: Option<Id>) {
        let screens = FrozenScreens::new(frozen::COLOUR_PICKER, shots, asked);
        // the loupe starts where the mouse already is, without waiting for it to move
        let pointer = desktop::cursor().and_then(|(x, y)| screens.at_desktop(x, y));
        self.picking = Some(Picking { screens, pointer, last_hover: None, for_dialog });
    }

    fn notify(&self, text: &str, ms: u32) {
        self.services.send(Request::Notify { title: TITLE.into(), text: text.into(), ms });
    }

    fn finish(&mut self, ctx: &Context, outcome: Outcome) {
        let Some(picking) = self.picking.take() else { return };
        let Outcome::Picked(c) = outcome else {
            if let Some(who) = picking.for_dialog {
                eyedropper::answer(ctx, who, None);
            }
            return;
        };
        if let Some(who) = picking.for_dialog {
            eyedropper::answer(ctx, who, Some(c));
        } else {
            self.copy(c);
        }
        self.recent.push(c);
        if let Err(e) = self.recent.save() {
            tracing::warn!("could not save the recent colours: {e}");
        }
        // the tray's Recent colours menu
        self.services.send(Request::StatusChanged);
    }

    fn copy(&self, c: Rgb) {
        let text = self.fmt(c);
        crate::tools::clip::copy(text.clone());
        self.notify(&format!("{text} copied"), 2000);
    }

    fn draw(&mut self, ctx: &Context) {
        let format = self.format;
        let Some(picking) = self.picking.as_mut() else { return };
        let Picking { screens, pointer, last_hover, .. } = picking;
        let mut outcome = None;
        screens.show(ctx, |ui, cx, _response| {
            ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
            let hover = ui.input(|i| i.pointer.hover_pos()).filter(|p| cx.area.contains(*p));
            if let Some(p) = hover {
                if *last_hover != Some(p) || pointer.is_none_or(|(i, _)| i != cx.index) {
                    *last_hover = Some(p);
                    *pointer = Some((cx.index, cx.local(p)));
                }
            } else if pointer.is_some_and(|(i, _)| i == cx.index) && ui.input(|i| i.pointer.has_pointer()) {
                // the mouse left for the other monitor: no loupe left behind on this one
                *pointer = None;
            }
            let here = pointer.filter(|(i, _)| *i == cx.index).map(|(_, p)| p);
            if cx.primary || here.is_some() {
                let (w, _) = cx.size();
                let hint = loupe::hint_rect(w);
                frozen::paint_hint(
                    ui,
                    cx.to_ui(Point::new(w / 2, 0)).x,
                    cx.area.min.y + hint.y as f32,
                    hint.w as f32,
                    &loupe::hint_parts(format),
                );
            }
            if let Some(p) = here {
                paint_loupe(ui, cx, p);
            }
            if let Some(next) = read_input(ui, cx, here) {
                outcome = Some(next);
            }
            if cx.has_keys
                && let Some((dx, dy, shift)) = arrow(ui)
                && let Some((i, p)) = *pointer
                && i == cx.index
            {
                let moved = colour::nudge(p, dx, dy, shift);
                let (x, y) = cx.desktop(moved);
                desktop::move_cursor(x, y);
                *pointer = Some((i, moved));
            }
        });
        if let Some(outcome) = outcome {
            self.finish(ctx, outcome);
        }
    }

    fn palette_colour(&self, id: &str) -> Option<(&'static str, Rgb)> {
        for prefix in ["colour:", "colour-accent:", "colour-theme:", "colour-light:"] {
            if let Some(c) = id.strip_prefix(prefix).and_then(Rgb::parse) {
                return Some((prefix, c));
            }
        }
        None
    }
}

/// Clicks and keys on one screen.
fn read_input(ui: &Ui, cx: &ScreenCx<'_>, here: Option<Point>) -> Option<Outcome> {
    let (primary, secondary) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.secondary_pressed()));
    let on_this = ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| cx.area.contains(p));
    if on_this && secondary {
        return Some(Outcome::Cancelled);
    }
    if on_this
        && primary
        && let Some(p) = here
    {
        return Some(Outcome::Picked(cx.shot.frame.at(p, cx.ratio())));
    }
    if !cx.has_keys {
        return None;
    }
    if ui.input(|i| i.key_pressed(Key::Escape)) {
        return Some(Outcome::Cancelled);
    }
    if ui.input(|i| i.key_pressed(Key::Enter) || i.key_pressed(Key::Space))
        && let Some(p) = here
    {
        return Some(Outcome::Picked(cx.shot.frame.at(p, cx.ratio())));
    }
    None
}

/// An arrow key pressed this frame: the step and whether Shift was held with it.
pub(crate) fn arrow(ui: &Ui) -> Option<(i32, i32, bool)> {
    ui.input(|i| {
        i.events.iter().find_map(|e| match e {
            egui::Event::Key { key, pressed: true, modifiers, .. } => {
                let (dx, dy) = match key {
                    Key::ArrowLeft => (-1, 0),
                    Key::ArrowRight => (1, 0),
                    Key::ArrowUp => (0, -1),
                    Key::ArrowDown => (0, 1),
                    _ => return None,
                };
                Some((dx, dy, modifiers.shift))
            }
            _ => None,
        })
    })
}

/// The loupe card next to the mouse at `p`.
fn paint_loupe(ui: &Ui, cx: &ScreenCx<'_>, p: Point) {
    let tokens = Tokens::of(ui.ctx());
    let pal = &tokens.p;
    let painter = ui.painter();
    let (w, h) = cx.size();
    let card = cx.rect(loupe::card_rect(p, w, h));
    let rad = tokens.r();
    painter.rect(card, rad, pal.bg_base, Stroke::new(1.0, pal.border), StrokeKind::Inside);

    let zoom = loupe::zoom_rect(loupe::card_rect(p, w, h));
    let cells = loupe::cells(&cx.shot.frame, p, cx.ratio());
    let inner = (rad - 5.0).max(0.0) as u8;
    let last = loupe::CELLS - 1;
    for (n, c) in cells.iter().enumerate() {
        let Some(c) = c else { continue };
        let (col, row) = (n as i32 % loupe::CELLS, n as i32 / loupe::CELLS);
        let cell =
            IRect::new(zoom.x + col * loupe::ZOOM, zoom.y + row * loupe::ZOOM, loupe::ZOOM, loupe::ZOOM);
        // the corner cells are rounded like the zoom's frame
        let corner =
            |want_col: i32, want_row: i32| if col == want_col && row == want_row { inner } else { 0 };
        let radius = CornerRadius {
            nw: corner(0, 0),
            ne: corner(last, 0),
            sw: corner(0, last),
            se: corner(last, last),
        };
        painter.rect_filled(cx.rect(cell), radius, crate::theme::colour(*c));
    }
    let mid = cx.rect(loupe::centre_cell(zoom));
    painter.rect_stroke(mid.expand(2.5), 0.0, Stroke::new(1.0, Color32::BLACK), StrokeKind::Middle);
    painter.rect_stroke(mid.expand(1.0), 0.0, Stroke::new(1.5, Color32::WHITE), StrokeKind::Middle);

    let c = cx.shot.frame.at(p, cx.ratio());
    let fy = cx.rect(zoom).max.y + loupe::PAD as f32 + 1.0;
    painter
        .line_segment([pos2(card.min.x + 1.0, fy), pos2(card.max.x - 1.0, fy)], Stroke::new(1.0, pal.border));
    let swatch = Rect::from_min_size(pos2(card.min.x + 10.0, fy + 8.0), vec2(24.0, 24.0));
    painter.rect_filled(swatch, 5.0, crate::theme::colour(c));
    let text_x = card.min.x + 42.0;
    painter.text(
        pos2(text_x, fy + 14.0),
        egui::Align2::LEFT_CENTER,
        c.hex(),
        tokens.font(12.0, Weight::SemiBold, true),
        pal.text_primary,
    );
    painter.text(
        pos2(text_x, fy + 28.0),
        egui::Align2::LEFT_CENTER,
        format!("RGB {} {} {}", c.r, c.g, c.b),
        tokens.font(10.0, Weight::Regular, true),
        pal.text_muted,
    );
    let info = egui::WidgetInfo::labeled(egui::Role::Image, true, format!("Loupe {}", c.hex()));
    ui.interact(card, Id::unique(("lum-loupe", cx.index)), egui::Sense::hover()).widget_info(|| info.clone());
}

/// The colour dialog's way to borrow the picker: it asks, the picker answers, the dialog takes the answer.
/// They live in different windows and modules, so they meet in the context's memory.
pub mod eyedropper {
    use egui::{Context, Id};
    use lum_theme::colour::Rgb;

    fn asked_id() -> Id {
        Id::unique("lum-eyedropper-asked")
    }

    fn answer_id() -> Id {
        Id::unique("lum-eyedropper-answer")
    }

    /// The dialog `who` wants a colour from the screen.
    pub fn ask(ctx: &Context, who: Id) {
        ctx.data_mut(|d| d.insert_temp(asked_id(), who));
        ctx.request_repaint();
    }

    pub(super) fn take_ask(ctx: &Context) -> Option<Id> {
        ctx.data_mut(|d| {
            let who = d.get_temp::<Id>(asked_id());
            d.remove::<Id>(asked_id());
            who
        })
    }

    /// The colour for `who`, or `None` when the pick was cancelled.
    pub(super) fn answer(ctx: &Context, who: Id, colour: Option<Rgb>) {
        ctx.data_mut(|d| d.insert_temp(answer_id(), (who, colour)));
        ctx.request_repaint();
    }

    /// The answer for `who`, once: `Some(None)` when the pick was cancelled.
    pub fn take_answer(ctx: &Context, who: Id) -> Option<Option<Rgb>> {
        let (to, colour) = ctx.data(|d| d.get_temp::<(Id, Option<Rgb>)>(answer_id()))?;
        if to != who {
            return None;
        }
        ctx.data_mut(|d| d.remove::<(Id, Option<Rgb>)>(answer_id()));
        Some(colour)
    }

    /// The picker runs, so the dialog's eyedropper button can be used.
    pub fn available(ctx: &Context) -> bool {
        let pass = ctx.cumulative_pass_nr();
        ctx.data(|d| d.get_temp::<u64>(super::running_id())).is_some_and(|seen| seen + 2 >= pass)
    }
}

fn running_id() -> Id {
    Id::unique("lum-colour-picker-running")
}

impl Module for ColorPickerModule {
    fn key(&self) -> &'static str {
        "colorpicker"
    }

    fn title(&self) -> &'static str {
        TITLE
    }

    fn start(&mut self, cx: &mut ModuleCx<'_>) -> Result<(), StartError> {
        self.format = Format::from_setting(cx.settings.get_str("colorpicker.format").unwrap_or("hex"));
        self.upper = cx.settings.get_bool("colorpicker.uppercase", true);
        self.recent = Recent::load(self.recent.path());
        self.running = true;
        Ok(())
    }

    fn stop(&mut self, _cx: &mut ModuleCx<'_>) {
        self.picking = None;
        self.running = false;
    }

    fn actions(&self) -> Vec<Action> {
        vec![self.inbox.action("pick", "Pick a colour from the screen", || Cmd::Pick {
            asked: Instant::now(),
            for_dialog: None,
        })]
    }
}

impl UiModule for ColorPickerModule {
    fn binds(&self) -> Vec<Bind> {
        vec![Bind { setting: "hotkey", id: "pick", label: "pick a colour" }]
    }

    fn bind_pressed(&mut self, id: &str) {
        if id == "pick" {
            self.inbox.push(Cmd::Pick { asked: Instant::now(), for_dialog: None });
        }
    }

    fn show(&mut self, cx: &mut UiCx<'_>) {
        if !self.running {
            return;
        }
        let ctx = cx.ctx.clone();
        let pass = ctx.cumulative_pass_nr();
        ctx.data_mut(|d| d.insert_temp(running_id(), pass));
        if let Some(who) = eyedropper::take_ask(&ctx) {
            self.inbox.push(Cmd::Pick { asked: Instant::now(), for_dialog: Some(who) });
        }
        self.take_commands(&ctx);
        if self.picking.is_some() {
            self.draw(&ctx);
        }
    }

    fn tray_menu(&self) -> Vec<MenuItem> {
        if self.recent.colours.is_empty() {
            return Vec::new();
        }
        let items = self
            .recent
            .colours
            .iter()
            .filter_map(|c| Some((c, Rgb::parse(c)?)))
            .map(|(c, rgb)| MenuItem::Sub {
                text: self.fmt(rgb),
                items: vec![
                    MenuItem::action(&format!("row:colour:{c}"), "Copy"),
                    MenuItem::Separator,
                    MenuItem::action(&format!("row:colour-accent:{c}"), "Use as the theme's accent"),
                    MenuItem::action(&format!("row:colour-theme:{c}"), "New dark theme from it"),
                    MenuItem::action(&format!("row:colour-light:{c}"), "New light theme from it"),
                ],
            })
            .collect();
        vec![MenuItem::Sub { text: format!("{ICON}  Recent colours"), items }]
    }

    fn palette_entries(&self) -> Vec<Entry> {
        let mut out = Vec::new();
        for c in &self.recent.colours {
            let Some(rgb) = Rgb::parse(c) else { continue };
            out.push(Entry::new(
                format!("colour:{c}"),
                format!("Colour {}", self.fmt(rgb)),
                "copy",
                "colour",
            ));
            out.push(Entry::new(
                format!("colour-accent:{c}"),
                format!("Accent colour {c}"),
                "use as the theme's accent",
                "colour",
            ));
            out.push(Entry::new(
                format!("colour-theme:{c}"),
                format!("New theme from {c}"),
                "a dark theme around this colour",
                "colour",
            ));
        }
        out
    }

    fn run_palette(&mut self, id: &str, services: &Services) -> bool {
        let Some((prefix, c)) = self.palette_colour(id) else { return false };
        match prefix {
            "colour:" => self.copy(c),
            "colour-accent:" => services.send(Request::ThemeAccent(c.hex())),
            "colour-theme:" => services.send(Request::ThemeFromColour { colour: c.hex(), light: false }),
            _ => services.send(Request::ThemeFromColour { colour: c.hex(), light: true }),
        }
        true
    }
}
