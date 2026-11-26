//! Luminescent's own colour picker, used everywhere a colour is picked, and its gradient editor
//! (`core/colourpick.py`).
//!
//! The dialog is a window of its own that stays open while the colour under it is changing: every change is
//! reported as it happens ([`PickerKind::Live`]) so the swatch and the preview behind it follow, and the
//! person's answer is [`PickerKind::Done`] or [`PickerKind::Cancelled`] (the caller then puts back what was
//! there before). A saturation / brightness square, a hue strip, the colour you started with (click it to go
//! back), a hex box that also takes `rgb(...)` and colour names, how readable text is on the colour, and rows of
//! recent picks, this theme's colours and basics. In gradient mode a bar shows every stop: click the bar to add
//! one, drag a stop to move it, drag it off the bar (or press Delete) to remove it.

use std::path::PathBuf;

use egui::{
    Align, Color32, Context, CursorIcon, Id, Key, Layout, Pos2, Rect, Response, Sense, Stroke, StrokeKind,
    Ui, Vec2, pos2, vec2,
};
use lum_theme::picker::{self, BASICS};
use lum_theme::{Gradient as EngineGradient, GradientKind as Kind, Rgb};

use crate::paint;
use crate::panel::{FloatingPanel, Geometry};
use crate::text::{caps, hint, mono};
use crate::theme::{Gradient, Tokens, Tone, alpha, colour};
use crate::widgets::{self, Button, Combo, IconButton, Slider, SpinBox, TextInput};

/// What the picker holds or hands back.
#[derive(Debug, Clone, PartialEq)]
pub enum Picked {
    Colour(Rgb),
    Gradient(EngineGradient),
}

impl Picked {
    /// One colour for it: a gradient's first stop.
    pub fn first(&self) -> Rgb {
        match self {
            Picked::Colour(c) => *c,
            Picked::Gradient(g) => g.stops[0].1,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PickerKind {
    /// The colour is being changed; the dialog is still open.
    Live(Picked),
    /// Done was pressed.
    Done(Picked),
    /// Cancelled or closed: put back what was there before.
    Cancelled,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PickerEvent {
    /// What the caller named the colour when it opened the dialog.
    pub target: String,
    pub kind: PickerKind,
}

const WIDTH: f32 = 380.0;
const SOLID_HEIGHT: f32 = 560.0;
const GRADIENT_HEIGHT: f32 = 1020.0;

pub struct ColourDialog {
    panel: FloatingPanel,
    target: String,
    allow_gradient: bool,
    start: Picked,
    gradient_mode: bool,
    solid: Rgb,
    kind: Kind,
    angle: f64,
    /// The stops in the order they were made; `current` is the one being edited.
    stops: Vec<(f64, Rgb)>,
    current: usize,
    drag: Option<usize>,
    removing: bool,
    hsv: (f64, f64, f64),
    hex: String,
    stop_edit: Vec<Option<String>>,
    recent: Vec<Rgb>,
    files: (PathBuf, PathBuf),
    events: Vec<PickerEvent>,
    /// The screen colour picker (phase 9) is there to ask.
    pub screen_picker: bool,
    wants_screen: bool,
}

impl std::fmt::Debug for ColourDialog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ColourDialog").field("target", &self.target).finish()
    }
}

impl ColourDialog {
    /// `picker_file` is `picker.json` (its own recent picks), `eyedropper_file` the screen colour picker's
    /// `colours.json`, which the recent row reads too.
    pub fn new(picker_file: PathBuf, eyedropper_file: PathBuf) -> ColourDialog {
        ColourDialog {
            panel: FloatingPanel::new(
                "colour-picker",
                "Pick a colour",
                Geometry { x: 200.0, y: 120.0, w: WIDTH, h: SOLID_HEIGHT },
            )
            .resizable(false)
            .min_size(vec2(WIDTH, 300.0)),
            target: String::new(),
            allow_gradient: false,
            start: Picked::Colour(Rgb::BLACK),
            gradient_mode: false,
            solid: Rgb::BLACK,
            kind: Kind::Linear,
            angle: 135.0,
            stops: vec![(0.0, Rgb::BLACK), (1.0, Rgb::WHITE)],
            current: 0,
            drag: None,
            removing: false,
            hsv: (0.0, 1.0, 1.0),
            hex: String::new(),
            stop_edit: Vec::new(),
            recent: Vec::new(),
            files: (picker_file, eyedropper_file),
            events: Vec::new(),
            screen_picker: false,
            wants_screen: false,
        }
    }

    pub fn is_open(&self) -> bool {
        self.panel.is_open()
    }

    /// What the dialog is open for (empty while closed).
    pub fn target(&self) -> &str {
        if self.is_open() { &self.target } else { "" }
    }

    /// Opens the dialog on `start`. `near` is the window it opens in front of (it is centred on it). With
    /// `gradient` the person can switch to a gradient.
    pub fn open(&mut self, target: &str, title: &str, start: Picked, gradient: bool, near: Rect) {
        self.target = target.to_owned();
        self.allow_gradient = gradient;
        self.recent = picker::recent(&self.files.0, &self.files.1);
        self.start = match &start {
            Picked::Gradient(_) if !gradient => Picked::Colour(start.first()),
            _ => start,
        };
        match self.start.clone() {
            Picked::Gradient(g) => {
                self.gradient_mode = true;
                self.solid = g.at(0.0);
                self.kind = g.kind;
                self.angle = g.angle;
                self.stops = g.stops.clone();
            }
            Picked::Colour(c) => {
                self.gradient_mode = false;
                self.solid = c;
                let g = EngineGradient::from_colour(c);
                self.kind = g.kind;
                self.angle = g.angle;
                self.stops = g.stops;
            }
        }
        self.current = 0;
        self.drag = None;
        self.removing = false;
        self.stop_edit.clear();
        self.panel.set_title(if title.is_empty() { "Pick a colour" } else { title }, "");
        let size = self.size();
        self.panel.geometry = Geometry::from_rect(Rect::from_center_size(near.center(), size));
        let start_colour = if self.gradient_mode { self.stops[0].1 } else { self.solid };
        self.hsv = picker::hsv_of(start_colour);
        self.hex = start_colour.hex();
        self.panel.open();
    }

    pub fn close(&mut self) {
        self.panel.close();
    }

    /// A colour came back from the screen colour picker.
    pub fn picked_from_screen(&mut self, c: Rgb) {
        self.wants_screen = false;
        self.set_colour(c, None, true);
    }

    /// Whether the person asked to pick from the screen since the last call.
    pub fn take_screen_request(&mut self) -> bool {
        std::mem::take(&mut self.wants_screen)
    }

    fn size(&self) -> Vec2 {
        vec2(WIDTH, if self.gradient_mode { GRADIENT_HEIGHT } else { SOLID_HEIGHT })
    }

    /// What the dialog holds now.
    pub fn value(&self) -> Option<Picked> {
        if self.gradient_mode {
            EngineGradient::new(self.kind, self.angle, self.stops.clone()).map(Picked::Gradient)
        } else {
            Some(Picked::Colour(self.solid))
        }
    }

    fn emit_live(&mut self) {
        if let Some(v) = self.value() {
            self.events.push(PickerEvent { target: self.target.clone(), kind: PickerKind::Live(v) });
        }
    }

    fn finish(&mut self, accepted: bool) {
        if accepted {
            match self.value() {
                Some(Picked::Colour(c)) => picker::remember(&self.files.0, c),
                Some(Picked::Gradient(ref g)) => {
                    for (_, c) in &g.stops {
                        picker::remember(&self.files.0, *c);
                    }
                }
                None => {}
            }
            if let Some(v) = self.value() {
                self.events.push(PickerEvent { target: self.target.clone(), kind: PickerKind::Done(v) });
            }
        } else {
            self.events.push(PickerEvent { target: self.target.clone(), kind: PickerKind::Cancelled });
        }
        self.panel.close();
    }

    /// Makes `c` the colour being edited (the solid colour, or the selected gradient stop).
    fn set_colour(&mut self, c: Rgb, from_hsv: Option<(f64, f64, f64)>, emit: bool) {
        let mut hsv = from_hsv.unwrap_or_else(|| picker::hsv_of(c));
        // greys have no hue: keep the one you had
        if from_hsv.is_none() && (hsv.1 <= 0.001 || hsv.2 <= 0.001) {
            hsv.0 = self.hsv.0;
        }
        self.hsv = hsv;
        self.hex = c.hex();
        if self.gradient_mode {
            if let Some(stop) = self.stops.get_mut(self.current) {
                stop.1 = c;
            }
            self.stop_edit.clear();
        } else {
            self.solid = c;
        }
        if emit {
            self.emit_live();
        }
    }

    fn set_mode(&mut self, gradient: bool) {
        self.gradient_mode = gradient && self.allow_gradient;
        let c = if self.gradient_mode {
            self.stops[self.current.min(self.stops.len() - 1)].1
        } else {
            self.solid
        };
        self.set_colour(c, None, false);
        self.panel.geometry =
            Geometry::from_rect(Rect::from_min_size(self.panel.geometry.rect().min, self.size()));
        self.emit_live();
    }

    fn sorted(&self) -> Vec<(f64, Rgb)> {
        let mut s = self.stops.clone();
        s.sort_by(|a, b| a.0.total_cmp(&b.0));
        s
    }

    fn remove_stop(&mut self, i: usize) {
        if self.stops.len() <= 2 || i >= self.stops.len() {
            return;
        }
        self.stops.remove(i);
        self.current = i.saturating_sub(1).min(self.stops.len() - 1);
        self.stop_edit.clear();
        let c = self.stops[self.current].1;
        self.set_colour(c, None, false);
        self.emit_live();
    }

    fn use_gradient(&mut self, g: &EngineGradient) {
        self.kind = g.kind;
        self.angle = g.angle;
        self.stops = g.stops.clone();
        self.current = 0;
        self.stop_edit.clear();
        let c = self.stops[0].1;
        self.set_colour(c, None, false);
        self.emit_live();
    }

    /// Draws the dialog if it is open and collects what happened since the last frame.
    pub fn show(&mut self, ctx: &Context) -> Vec<PickerEvent> {
        if self.panel.is_open() {
            let tokens = Tokens::of(ctx);
            let mut panel = std::mem::replace(
                &mut self.panel,
                FloatingPanel::new("taken", "", Geometry { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }),
            );
            let allow = self.allow_gradient;
            let mut mode_change = None;
            let gradient_mode = self.gradient_mode;
            let mut done = false;
            let mut cancel = false;
            let response = panel.show_with(
                ctx,
                |ui| {
                    if allow {
                        mode_change =
                            widgets::segmented(ui, &["Solid", "Gradient"], usize::from(gradient_mode));
                    }
                },
                |ui| {
                    let (accept, dismiss) = self.body(ui, &tokens);
                    done = accept;
                    cancel = dismiss;
                },
            );
            self.panel = panel;
            if let Some(i) = mode_change {
                self.set_mode(i == 1);
            }
            if done {
                self.finish(true);
            } else if cancel || response.closed || response.escape {
                self.finish(false);
            }
        }
        std::mem::take(&mut self.events)
    }

    /// The dialog's content. Gives back whether Done and Cancel were pressed.
    fn body(&mut self, ui: &mut Ui, tokens: &Tokens) -> (bool, bool) {
        let mut accept = false;
        let mut dismiss = false;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let margin = egui::Margin { left: 16, right: 16, top: 12, bottom: 14 };
            egui::Frame::NONE.inner_margin(margin).show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 10.0);
                if self.gradient_mode {
                    self.gradient_tools(ui, tokens);
                }
                self.picker_area(ui, tokens);
                self.hex_row(ui, tokens, &mut accept);
                self.readable_line(ui, tokens);
                self.chip_rows(ui, tokens);
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(Button::primary("Done")).clicked() {
                            accept = true;
                        }
                        if ui.add(Button::ghost("Cancel")).clicked() {
                            dismiss = true;
                        }
                    });
                });
            });
        });
        (accept, dismiss)
    }

    fn picker_area(&mut self, ui: &mut Ui, tokens: &Tokens) {
        let width = ui.available_width();
        let (h, s, v) = self.hsv;
        let square = Rect::from_min_size(ui.cursor().min, vec2(width, 170.0));
        if let Some((ns, nv)) = sv_square(ui, square, h, s, v, tokens) {
            let c = picker::rgb_of_hsv(h, ns, nv);
            self.set_colour(c, Some((h, ns, nv)), true);
        }
        if let Some(nh) = hue_strip(ui, h, tokens) {
            let (_, s, v) = self.hsv;
            let c = picker::rgb_of_hsv(nh, s, v);
            self.set_colour(c, Some((nh, s, v)), true);
        }
    }

    fn hex_row(&mut self, ui: &mut Ui, tokens: &Tokens, accept: &mut bool) {
        let now = if self.gradient_mode {
            self.stops[self.current.min(self.stops.len() - 1)].1
        } else {
            self.solid
        };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let old = self.start.clone();
            let new = self.value().unwrap_or(Picked::Colour(now));
            if old_new(ui, &old, &new, tokens).clicked() {
                self.revert();
            }
            let id = Id::unique("lum-picker-hex");
            let parsed = picker::parse(&self.hex);
            let invalid = parsed.is_none() && !self.hex.trim().is_empty();
            let response = ui.add(
                TextInput::new(&mut self.hex)
                    .id(id)
                    .mono()
                    .hint("#RRGGBB, rgb(…) or a name")
                    .invalid(invalid)
                    .width(ui.available_width() - 76.0)
                    .name("Colour code"),
            );
            if response.changed()
                && let Some(c) = picker::parse(&self.hex)
            {
                let typed = self.hex.clone();
                self.set_colour(c, None, true);
                self.hex = typed;
            }
            if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                *accept = true;
            }
            if !response.has_focus() && !response.changed() {
                self.hex = now.hex();
            }
            if ui.add(IconButton::new("⎘").size(30.0).tooltip("Copy the hex code")).clicked() {
                ui.ctx().copy_text(now.hex());
            }
            let eye = ui.add_enabled(
                self.screen_picker,
                IconButton::new("⌖").size(30.0).font_px(15.0).tooltip("Pick a colour from the screen"),
            );
            if eye.clicked() {
                self.wants_screen = true;
            }
        });
    }

    /// Undoes everything: back to the colour (or gradient) the dialog opened with.
    fn revert(&mut self) {
        match self.start.clone() {
            Picked::Gradient(g) if self.allow_gradient => {
                self.use_gradient(&g);
                self.gradient_mode = true;
                self.panel.geometry =
                    Geometry::from_rect(Rect::from_min_size(self.panel.geometry.rect().min, self.size()));
            }
            Picked::Colour(c) => {
                if self.gradient_mode {
                    self.gradient_mode = false;
                    self.panel.geometry =
                        Geometry::from_rect(Rect::from_min_size(self.panel.geometry.rect().min, self.size()));
                }
                self.set_colour(c, None, true);
            }
            Picked::Gradient(g) => self.set_colour(g.at(0.0), None, true),
        }
    }

    fn readable_line(&self, ui: &mut Ui, tokens: &Tokens) {
        let c = if self.gradient_mode {
            self.stops[self.current.min(self.stops.len() - 1)].1
        } else {
            self.solid
        };
        let white_on = picker::contrast(c, Rgb::WHITE);
        let black_on = picker::contrast(c, Rgb::from_u32(0x111111));
        let (name, ratio) = if white_on >= black_on { ("WHITE", white_on) } else { ("BLACK", black_on) };
        let background = Rgb::new(tokens.p.bg_base.r(), tokens.p.bg_base.g(), tokens.p.bg_base.b());
        ui.add(
            mono(format!(
                "TEXT ON IT: {name} {ratio:.1}:1   ·   AGAINST THE BACKGROUND {:.1}:1",
                picker::contrast(c, background)
            ))
            .px(10.5)
            .tone(Tone::Muted)
            .wrap(),
        );
    }

    fn chip_rows(&mut self, ui: &mut Ui, tokens: &Tokens) {
        let current = if self.gradient_mode {
            self.stops[self.current.min(self.stops.len() - 1)].1
        } else {
            self.solid
        };
        let theme: Vec<Rgb> = {
            let p = &tokens.p;
            let mut out: Vec<Rgb> = Vec::new();
            for c in [
                p.primary,
                p.primary_text,
                p.primary_muted,
                p.bg_base,
                p.bg_card,
                p.bg_header,
                p.border,
                p.text_primary,
                p.text_secondary,
                p.text_muted,
                p.folder_border,
            ] {
                let rgb = Rgb::new(c.r(), c.g(), c.b());
                if !out.contains(&rgb) {
                    out.push(rgb);
                }
            }
            out
        };
        let basics: Vec<Rgb> = BASICS.iter().map(|&c| Rgb::from_u32(c)).collect();
        let rows = [("RECENT", self.recent.clone()), ("THEME", theme), ("BASICS", basics)];
        let mut chosen = None;
        for (name, colours) in rows {
            if colours.is_empty() {
                continue;
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let (rect, _) = ui.allocate_exact_size(vec2(50.0, 20.0), Sense::hover());
                ui.painter().text(
                    rect.left_center(),
                    egui::Align2::LEFT_CENTER,
                    name,
                    tokens.font(9.0, crate::fonts::Weight::SemiBold, true),
                    tokens.p.text_muted,
                );
                for c in colours.iter().take(11) {
                    if chip(ui, ChipFill::Colour(colour(*c)), 20.0, *c == current, &c.hex(), tokens).clicked()
                    {
                        chosen = Some(*c);
                    }
                }
            });
        }
        if let Some(c) = chosen {
            self.set_colour(c, None, true);
        }
    }

    // ── gradient tools ─────────────────────────────────────────────────────

    fn gradient_tools(&mut self, ui: &mut Ui, tokens: &Tokens) {
        let width = ui.available_width();
        // the preview, as big as it will be
        let (preview, _) = ui.allocate_exact_size(vec2(width, 56.0), Sense::hover());
        if let Some(g) = EngineGradient::new(self.kind, self.angle, self.stops.clone()) {
            paint::gradient(ui.painter(), preview, 8.0, &Gradient::from(&g), 1.0);
        }
        ui.painter().rect_stroke(preview, 8.0, Stroke::new(1.0, tokens.p.border), StrokeKind::Inside);

        let mut changed = false;
        let bar =
            gradient_bar(ui, &mut self.stops, &mut self.current, &mut self.drag, &mut self.removing, tokens);
        if bar.stops_changed {
            changed = true;
            self.stop_edit.clear();
        }
        if bar.selected {
            let c = self.stops[self.current].1;
            self.set_colour(c, None, false);
        }
        if let Some(i) = bar.remove {
            self.remove_stop(i);
        }
        ui.add(
            hint("Click the bar to add a colour · drag a dot to move it, down off the bar to remove it")
                .wrap(),
        );

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let mut index = Kind::ALL.iter().position(|k| *k == self.kind).unwrap_or(0);
            let names: Vec<&str> = Kind::ALL.iter().map(|k| k.label()).collect();
            if ui.add(Combo::new(Id::unique("lum-picker-kind"), &mut index, &names).width(96.0)).changed() {
                self.kind = Kind::ALL[index];
                changed = true;
            }
            ui.add_enabled_ui(self.kind != Kind::Radial, |ui| {
                let mut angle = self.angle.floor();
                if ui
                    .add(
                        Slider::new(&mut angle, 0.0..=359.0)
                            .step(1.0)
                            .width(ui.available_width() - 48.0)
                            .name("Angle"),
                    )
                    .changed()
                {
                    self.angle = angle;
                    changed = true;
                }
            });
            ui.add(mono(format!("{}°", self.angle as i64 % 360)).px(11.0).tone(Tone::Secondary));
        });

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            if ui.add(Button::ghost("↻ 45°")).on_hover_text("Turn it a little").clicked() {
                self.angle = (self.angle.floor() + 45.0).rem_euclid(360.0);
                changed = true;
            }
            if ui.add(Button::ghost("⇄ Reverse")).on_hover_text("Swap the ends").clicked() {
                for stop in &mut self.stops {
                    stop.0 = 1.0 - stop.0;
                }
                changed = true;
            }
            if ui.add(Button::ghost("Even out")).on_hover_text("Space the colours evenly").clicked() {
                let order = {
                    let mut idx: Vec<usize> = (0..self.stops.len()).collect();
                    idx.sort_by(|&a, &b| self.stops[a].0.total_cmp(&self.stops[b].0));
                    idx
                };
                let n = (order.len() - 1) as f64;
                for (place, i) in order.into_iter().enumerate() {
                    self.stops[i].0 = place as f64 / n;
                }
                changed = true;
            }
            if ui
                .add_enabled(self.stops.len() > 2, Button::ghost("✕ Stop"))
                .on_hover_text("Remove the selected colour (Delete)")
                .clicked()
            {
                self.remove_stop(self.current);
            }
            if ui.add(Button::ghost("+ Colour")).on_hover_text("Add a colour in the biggest gap").clicked() {
                let stops = self.sorted();
                let gap = (0..stops.len() - 1)
                    .max_by(|&a, &b| (stops[a + 1].0 - stops[a].0).total_cmp(&(stops[b + 1].0 - stops[b].0)));
                if let Some(i) = gap {
                    let pos = (stops[i].0 + stops[i + 1].0) / 2.0;
                    self.stops.push((pos, stops[i].1.mix(stops[i + 1].1, 0.5)));
                    self.current = self.stops.len() - 1;
                    self.stop_edit.clear();
                    let c = self.stops[self.current].1;
                    self.set_colour(c, None, false);
                    changed = true;
                }
            }
        });

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            ui.add(caps("DIRECTION").px(9.5).spacing(0.12).tone(Tone::Muted));
            ui.add_space(4.0);
            for (arrow, angle) in [
                ("↑", 0.0),
                ("↗", 45.0),
                ("→", 90.0),
                ("↘", 135.0),
                ("↓", 180.0),
                ("↙", 225.0),
                ("←", 270.0),
                ("↖", 315.0),
            ] {
                if ui
                    .add(IconButton::new(arrow).size(24.0).font_px(12.0).ghost().tooltip(format!("{angle}°")))
                    .clicked()
                {
                    self.angle = angle;
                    changed = true;
                }
            }
        });

        self.stop_rows(ui, tokens, &mut changed);

        let first = self.sorted().first().map_or(self.solid, |s| s.1);
        let mut chosen: Option<EngineGradient> = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.add(caps("IDEAS").px(9.5).spacing(0.12).tone(Tone::Muted));
            ui.add_space(4.0);
            for (name, g) in picker::ideas_for(first) {
                if chip(ui, ChipFill::Gradient(Gradient::from(&g)), 22.0, false, name, tokens).clicked() {
                    chosen = Some(g);
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.add(caps("PRESETS").px(9.5).spacing(0.12).tone(Tone::Muted));
            ui.add_space(4.0);
            let accent = EngineGradient::new(
                Kind::Linear,
                135.0,
                vec![(0.0, rgb_of(tokens.p.primary)), (1.0, rgb_of(tokens.p.primary_text))],
            );
            let list = accent.into_iter().map(|g| ("This theme's accent", g)).chain(picker::presets());
            for (name, g) in list {
                if chip(ui, ChipFill::Gradient(Gradient::from(&g)), 22.0, false, name, tokens).clicked() {
                    chosen = Some(g);
                }
            }
        });
        if let Some(g) = chosen {
            self.use_gradient(&g);
        } else if changed {
            self.emit_live();
        }
    }

    /// One row per colour: a dot to select it, its hex code, where it sits along the gradient and a way to
    /// remove it.
    fn stop_rows(&mut self, ui: &mut Ui, tokens: &Tokens, changed: &mut bool) {
        let mut order: Vec<usize> = (0..self.stops.len()).collect();
        order.sort_by(|&a, &b| self.stops[a].0.total_cmp(&self.stops[b].0));
        if self.stop_edit.len() != self.stops.len() {
            self.stop_edit = vec![None; self.stops.len()];
        }
        let mut select = None;
        let mut remove = None;
        let can_remove = self.stops.len() > 2;
        for i in order {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let (pos, c) = self.stops[i];
                if chip(ui, ChipFill::Colour(colour(c)), 18.0, i == self.current, "Edit this colour", tokens)
                    .clicked()
                {
                    select = Some(i);
                }
                let id = Id::unique(("lum-picker-stop", i));
                let mut text = self.stop_edit[i].clone().unwrap_or_else(|| c.hex());
                let response = ui.add(
                    TextInput::new(&mut text)
                        .id(id)
                        .mono()
                        .width(ui.available_width() - 150.0)
                        .name("Colour of this stop"),
                );
                if response.has_focus() || response.changed() {
                    self.stop_edit[i] = Some(text.clone());
                }
                if response.lost_focus() {
                    if let Some(c) = picker::parse(&text) {
                        self.current = i;
                        self.set_colour(c, None, false);
                        *changed = true;
                    }
                    self.stop_edit[i] = None;
                }
                let mut percent = (pos * 100.0).round();
                if ui
                    .add(
                        SpinBox::new(&mut percent, 0.0..=100.0)
                            .suffix(" %")
                            .width(72.0)
                            .name("Place along the gradient"),
                    )
                    .changed()
                {
                    self.stops[i].0 = percent / 100.0;
                    *changed = true;
                }
                if ui
                    .add_enabled(
                        can_remove,
                        IconButton::new("✕").size(22.0).font_px(10.0).ghost().tooltip("Remove this colour"),
                    )
                    .clicked()
                {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = select {
            self.current = i;
            let c = self.stops[i].1;
            self.set_colour(c, None, false);
        }
        if let Some(i) = remove {
            self.remove_stop(i);
        }
    }
}

fn rgb_of(c: Color32) -> Rgb {
    Rgb::new(c.r(), c.g(), c.b())
}

// ── the pieces ────────────────────────────────────────────────────────────────

/// Saturation left to right, brightness bottom to top, for one hue. Gives back the new (saturation,
/// brightness) while it is being dragged or moved with the arrow keys.
fn sv_square(ui: &mut Ui, area: Rect, h: f64, s: f64, v: f64, tokens: &Tokens) -> Option<(f64, f64)> {
    let response = ui.allocate_rect(area, Sense::click_and_drag());
    // room for the knob at the edges
    let r = area.shrink(9.0);
    let hue = colour(picker::rgb_of_hsv(h, 1.0, 1.0));
    let painter = ui.painter();
    if let Some(across) =
        Gradient::new(crate::theme::GradientKind::Linear, 90.0, vec![(0.0, Color32::WHITE), (1.0, hue)])
    {
        paint::gradient(painter, r, 8.0, &across, 1.0);
    }
    if let Some(down) = Gradient::new(
        crate::theme::GradientKind::Linear,
        180.0,
        vec![(0.0, Color32::TRANSPARENT), (1.0, Color32::BLACK)],
    ) {
        paint::gradient(painter, r, 8.0, &down, 1.0);
    }
    painter.rect_stroke(r, 8.0, Stroke::new(1.0, tokens.p.border), StrokeKind::Inside);
    let knob = pos2(r.min.x + s as f32 * r.width(), r.min.y + (1.0 - v as f32) * r.height());
    painter.circle(
        knob,
        8.0,
        colour(picker::rgb_of_hsv(h, s, v)),
        Stroke::new(3.0, Color32::from_black_alpha(120)),
    );
    painter.circle_stroke(knob, 8.0, Stroke::new(2.0, Color32::WHITE));
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
    }
    let mut out = None;
    if response.is_pointer_button_down_on() || response.dragged() {
        response.request_focus();
        if let Some(p) = response.interact_pointer_pos() {
            let ns = f64::from(((p.x - r.min.x) / r.width().max(1.0)).clamp(0.0, 1.0));
            let nv = 1.0 - f64::from(((p.y - r.min.y) / r.height().max(1.0)).clamp(0.0, 1.0));
            out = Some((ns, nv));
        }
    } else if response.has_focus() {
        let (step, d) = ui.input(|i| {
            let step = if i.modifiers.shift { 0.1 } else { 0.01 };
            let dx =
                f64::from(i8::from(i.key_pressed(Key::ArrowRight)) - i8::from(i.key_pressed(Key::ArrowLeft)));
            let dy =
                f64::from(i8::from(i.key_pressed(Key::ArrowUp)) - i8::from(i.key_pressed(Key::ArrowDown)));
            (step, (dx, dy))
        });
        if d != (0.0, 0.0) {
            out = Some(((s + d.0 * step).clamp(0.0, 1.0), (v + d.1 * step).clamp(0.0, 1.0)));
        }
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Slider, true, "Saturation and brightness"));
    out
}

/// The hue strip. Gives back the new hue while it is being dragged.
fn hue_strip(ui: &mut Ui, h: f64, tokens: &Tokens) -> Option<f64> {
    let (area, response) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::click_and_drag());
    let track = area.shrink2(vec2(9.0, 3.0));
    let stops: Vec<(f32, Color32)> =
        (0..7).map(|i| (i as f32 / 6.0, colour(picker::rgb_of_hsv(i as f64 * 60.0, 1.0, 1.0)))).collect();
    if let Some(g) = Gradient::new(crate::theme::GradientKind::Linear, 90.0, stops) {
        paint::gradient(ui.painter(), track, track.height() / 2.0, &g, 1.0);
    }
    let x = track.min.x + (h.rem_euclid(360.0) / 360.0) as f32 * track.width();
    let centre = pos2(x, area.center().y);
    let painter = ui.painter();
    painter.circle(
        centre,
        8.0,
        colour(picker::rgb_of_hsv(h, 1.0, 1.0)),
        Stroke::new(3.0, Color32::from_black_alpha(110)),
    );
    painter.circle_stroke(centre, 8.0, Stroke::new(2.0, Color32::WHITE));
    let _ = tokens;
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    let mut out = None;
    if response.is_pointer_button_down_on() || response.dragged() {
        response.request_focus();
        if let Some(p) = response.interact_pointer_pos() {
            out = Some(f64::from(((p.x - track.min.x) / track.width().max(1.0)).clamp(0.0, 1.0)) * 359.9);
        }
    } else if response.has_focus() {
        let step = ui.input(|i| {
            let step = if i.modifiers.shift { 10.0 } else { 1.0 };
            step * f64::from(
                i8::from(i.key_pressed(Key::ArrowRight)) - i8::from(i.key_pressed(Key::ArrowLeft)),
            )
        });
        if step != 0.0 {
            out = Some((h + step).rem_euclid(360.0));
        }
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Slider, true, "Hue"));
    out
}

pub enum ChipFill {
    Colour(Color32),
    Gradient(Gradient),
}

/// A small round swatch (a colour or a gradient) in the rows under the picker.
pub fn chip(ui: &mut Ui, fill: ChipFill, size: f32, active: bool, tip: &str, tokens: &Tokens) -> Response {
    let (area, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let r = area.shrink(2.0);
    let radius = r.height() / 2.0;
    match &fill {
        ChipFill::Colour(c) => {
            ui.painter().circle_filled(r.center(), radius, *c);
        }
        ChipFill::Gradient(g) => paint::gradient(ui.painter(), r, radius, g, 1.0),
    }
    let ring = if active {
        Stroke::new(2.0, tokens.p.primary)
    } else if response.hovered() {
        Stroke::new(1.0, tokens.p.text_muted)
    } else {
        Stroke::new(1.0, tokens.p.border)
    };
    ui.painter().circle_stroke(r.center(), if active { radius + 1.0 } else { radius }, ring);
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    let name = tip.to_owned();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
    if tip.is_empty() { response } else { response.on_hover_text(tip) }
}

fn fill_of(p: &Picked) -> ChipFill {
    match p {
        Picked::Colour(c) => ChipFill::Colour(colour(*c)),
        Picked::Gradient(g) => ChipFill::Gradient(Gradient::from(g)),
    }
}

/// The colour you started with (left half, click it to go back) next to the new one (right half).
fn old_new(ui: &mut Ui, old: &Picked, new: &Picked, tokens: &Tokens) -> Response {
    let (area, response) = ui.allocate_exact_size(vec2(64.0, 30.0), Sense::click());
    let r = area.shrink(1.0);
    let half = r.width() / 2.0;
    let left = Rect::from_min_size(r.min, vec2(half, r.height()));
    let right = Rect::from_min_size(pos2(r.min.x + half, r.min.y), vec2(half, r.height()));
    let radius = 8.0;
    let painter = ui.painter();
    for (rect, value, corners) in
        [(left, old, [radius, 0.0, 0.0, radius]), (right, new, [0.0, radius, radius, 0.0])]
    {
        let rounding = egui::CornerRadius {
            nw: corners[0] as u8,
            ne: corners[1] as u8,
            se: corners[2] as u8,
            sw: corners[3] as u8,
        };
        match fill_of(value) {
            ChipFill::Colour(c) => {
                painter.rect_filled(rect, rounding, c);
            }
            ChipFill::Gradient(g) => {
                // a gradient keeps its rounded ends by being painted whole and covered on the other half
                let whole = if std::ptr::eq(value, old) {
                    Rect::from_min_size(r.min, vec2(r.width(), r.height()))
                } else {
                    r
                };
                let clip = painter.with_clip_rect(rect);
                paint::gradient(&clip, whole, radius, &g, 1.0);
            }
        }
    }
    let border = if response.hovered() { tokens.p.text_muted } else { tokens.p.border };
    painter.rect_stroke(r, radius, Stroke::new(1.0, border), StrokeKind::Inside);
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Button, true, "Go back to the colour you started with")
    });
    response.on_hover_text("Left: the colour you started with (click to go back) · right: the new one")
}

struct BarOutcome {
    stops_changed: bool,
    selected: bool,
    remove: Option<usize>,
}

const BAR_H: f32 = 22.0;
const KNOB: f32 = 9.0;

/// The stops of a gradient on a bar: click the bar to add a stop, drag one to move it, drag it down off the bar
/// or press Delete to remove it.
fn gradient_bar(
    ui: &mut Ui,
    stops: &mut Vec<(f64, Rgb)>,
    current: &mut usize,
    drag: &mut Option<usize>,
    removing: &mut bool,
    tokens: &Tokens,
) -> BarOutcome {
    let mut out = BarOutcome { stops_changed: false, selected: false, remove: None };
    let (area, response) = ui
        .allocate_exact_size(vec2(ui.available_width(), BAR_H + 2.0 * KNOB + 10.0), Sense::click_and_drag());
    let track = Rect::from_min_size(
        pos2(area.min.x + KNOB + 1.0, area.min.y + 2.0),
        vec2(area.width() - 2.0 * KNOB - 2.0, BAR_H),
    );
    let x_of = |pos: f64| track.min.x + pos as f32 * track.width();
    let pos_of = |x: f32| f64::from(((x - track.min.x) / track.width().max(1.0)).clamp(0.0, 1.0));
    let knob_y = track.max.y + KNOB + 3.0;

    let hit = |p: Pos2, stops: &[(f64, Rgb)]| -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for (i, (pos, _)) in stops.iter().enumerate() {
            let d = (p.x - x_of(*pos)).abs();
            if d <= KNOB + 3.0 && (p.y - knob_y).abs() <= KNOB + 6.0 && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((i, d));
            }
        }
        best.map(|(i, _)| i)
    };

    if (response.drag_started() || response.clicked())
        && let Some(p) = response.interact_pointer_pos()
    {
        {
            let mut i = hit(p, stops);
            if i.is_none() {
                let pos = pos_of(p.x);
                let mut sorted = stops.clone();
                sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
                let colour_here = EngineGradient::new(Kind::Linear, 90.0, sorted).map(|g| g.at(pos));
                if let Some(c) = colour_here {
                    stops.push((pos, c));
                    i = Some(stops.len() - 1);
                    out.stops_changed = true;
                }
            }
            if let Some(i) = i {
                *current = i;
                *drag = Some(i);
                *removing = false;
                out.selected = true;
            }
            response.request_focus();
        }
    }
    if response.dragged()
        && let (Some(i), Some(p)) = (*drag, response.interact_pointer_pos())
    {
        *removing = stops.len() > 2 && p.y > area.max.y + 24.0;
        if let Some(stop) = stops.get_mut(i) {
            stop.0 = pos_of(p.x);
            out.stops_changed = true;
        }
    }
    if response.drag_stopped() {
        if *removing && let Some(i) = *drag {
            out.remove = Some(i);
        }
        *drag = None;
        *removing = false;
    }
    if response.has_focus() {
        let (delete, dx, big) = ui.input(|i| {
            (
                i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace),
                i32::from(i.key_pressed(Key::ArrowRight)) - i32::from(i.key_pressed(Key::ArrowLeft)),
                i.modifiers.shift,
            )
        });
        if delete {
            out.remove = Some(*current);
        } else if dx != 0
            && let Some(stop) = stops.get_mut(*current)
        {
            stop.0 = (stop.0 + f64::from(dx) * if big { 0.05 } else { 0.01 }).clamp(0.0, 1.0);
            out.stops_changed = true;
        }
    }

    let mut sorted = stops.clone();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    if let Some(g) = EngineGradient::new(Kind::Linear, 90.0, sorted) {
        paint::gradient(ui.painter(), track, 6.0, &Gradient::from(&g), 1.0);
    }
    let painter = ui.painter();
    painter.rect_stroke(track, 6.0, Stroke::new(1.0, tokens.p.border), StrokeKind::Inside);
    for (i, (pos, c)) in stops.iter().enumerate() {
        let x = x_of(*pos);
        let selected = i == *current;
        let gone = selected && *removing;
        painter.line_segment(
            [pos2(x, track.max.y - 4.0), pos2(x, knob_y - KNOB + 1.0)],
            Stroke::new(1.0, tokens.p.text_muted),
        );
        let mut fill = colour(*c);
        if gone {
            fill = alpha(fill, 0.3);
        }
        let ring =
            if selected { Stroke::new(3.0, tokens.p.primary) } else { Stroke::new(2.0, tokens.p.bg_base) };
        let radius = if selected { KNOB } else { KNOB - 2.0 };
        painter.circle(pos2(x, knob_y), radius, fill, ring);
        if selected {
            painter.circle_stroke(pos2(x, knob_y), radius - 2.5, Stroke::new(1.0, Color32::WHITE));
        }
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Slider, true, "Gradient colours"));
    out
}

// ── the swatch button ─────────────────────────────────────────────────────────

/// A framed swatch (colour or gradient) with its hex code; click it to pick. With `unset_text` it can also be
/// empty (`None`), showing that text, like "Theme".
pub fn swatch(ui: &mut Ui, value: Option<&Picked>, unset_text: &str, width: f32, name: &str) -> Response {
    let tokens = Tokens::of(ui.ctx());
    let (area, response) = ui.allocate_exact_size(vec2(width, 30.0), Sense::click());
    let radius = tokens.rs();
    let p = &tokens.p;
    let painter = ui.painter();
    painter.rect_filled(area, radius, p.bg_frame);
    let border = if response.hovered() { p.text_muted } else { p.border };
    painter.rect_stroke(area, radius, Stroke::new(1.0, border), StrokeKind::Inside);
    let mut chip_rect = Rect::from_min_size(area.min + vec2(6.0, 5.0), vec2(20.0, 20.0));
    let text = match value {
        None => {
            let dash = Stroke::new(1.0, p.text_muted);
            painter.add(egui::Shape::dashed_line(&rounded_dash(chip_rect), dash, 3.0, 3.0));
            if unset_text.is_empty() { "None".to_owned() } else { unset_text.to_owned() }
        }
        Some(v) => {
            if matches!(v, Picked::Gradient(_)) {
                chip_rect = Rect::from_min_size(chip_rect.min, vec2(38.0, 20.0));
            }
            match fill_of(v) {
                ChipFill::Colour(c) => {
                    painter.rect_filled(chip_rect, 5.0, c);
                }
                ChipFill::Gradient(g) => paint::gradient(painter, chip_rect, 5.0, &g, 1.0),
            }
            painter.rect_stroke(
                chip_rect,
                5.0,
                Stroke::new(1.0, Color32::from_black_alpha(60)),
                StrokeKind::Inside,
            );
            match v {
                Picked::Colour(c) => c.hex(),
                Picked::Gradient(g) => format!("{} COLOURS", g.stops.len()),
            }
        }
    };
    let colour_text = if value.is_none() { p.text_muted } else { p.text_primary };
    let x = chip_rect.max.x + 8.0;
    painter.text(
        pos2(x, area.center().y),
        egui::Align2::LEFT_CENTER,
        text.to_uppercase(),
        tokens.font(11.5, crate::fonts::Weight::Medium, true),
        colour_text,
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    let name = name.to_owned();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
    response
}

fn rounded_dash(r: Rect) -> Vec<Pos2> {
    vec![r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()]
}
