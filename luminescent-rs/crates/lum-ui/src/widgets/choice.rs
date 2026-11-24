//! Controls that pick a value: check box, switch, slider, opacity slider, spin box, combo and colour button.
//!
//! Like `core/ux.py`, a slider jumps to where you click, and the mouse wheel only changes a spin box or slider
//! once you have clicked it; before that the wheel scrolls the page, so scrolling down a page never changes a
//! value on the way past.

use std::ops::RangeInclusive;

use egui::{
    Align2, Color32, CursorIcon, Event, Frame, Id, Key, Margin, Pos2, Rect, Response, Sense, Shape, Stroke,
    StrokeKind, TextEdit, Ui, Vec2, Widget, pos2, vec2,
};

use crate::anim;
use crate::fonts::Weight;
use crate::paint;
use crate::text::Text;
use crate::theme::{Role, Tokens, Tone};

/// A small chevron, pointing up or down, like the stylesheet's generated arrow images.
pub fn chevron(painter: &egui::Painter, centre: Pos2, size: Vec2, up: bool, colour: Color32) {
    let (half, h) = (size.x / 2.0, size.y / 2.0);
    let dir = if up { -1.0 } else { 1.0 };
    let points =
        vec![centre + vec2(-half, -h * dir), centre + vec2(0.0, h * dir), centre + vec2(half, -h * dir)];
    painter.add(Shape::line(points, Stroke::new(1.5, colour)));
}

fn pointing(ui: &Ui, response: &Response) {
    if response.hovered() && ui.is_enabled() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
}

/// How far the wheel turned over this control in this frame, if it may take the wheel (it has the focus).
fn wheel_steps(ui: &Ui, response: &Response) -> f32 {
    if !(response.hovered() && response.has_focus()) {
        return 0.0;
    }
    ui.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                Event::MouseWheel { delta, .. } => Some(delta.y.signum()),
                _ => None,
            })
            .sum()
    })
}

#[must_use = "add it with ui.add(...)"]
pub struct Checkbox<'a> {
    on: &'a mut bool,
    text: String,
}

impl<'a> Checkbox<'a> {
    pub fn new(on: &'a mut bool, text: impl Into<String>) -> Checkbox<'a> {
        Checkbox { on, text: text.into() }
    }
}

impl Widget for Checkbox<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::of(ui.ctx());
        let tone = if ui.is_enabled() { Tone::Primary } else { Tone::Muted };
        let galley = (!self.text.is_empty())
            .then(|| ui.painter().layout_job(Text::new(&self.text).tone(tone).job(&tokens)));
        let box_size = 16.0;
        let text_width = galley.as_ref().map_or(0.0, |g| 9.0 + g.size().x);
        let height = galley.as_ref().map_or(box_size, |g| g.size().y.max(box_size)).max(22.0);
        let (rect, mut response) =
            ui.allocate_exact_size(vec2(box_size + text_width, height), Sense::click());
        if response.clicked() {
            *self.on = !*self.on;
            response.mark_changed();
        }
        if ui.is_rect_visible(rect) {
            let p = &tokens.p;
            let indicator = Rect::from_center_size(
                pos2(rect.min.x + box_size / 2.0, rect.center().y),
                Vec2::splat(box_size),
            );
            let painter = ui.painter();
            if *self.on {
                paint::fill(painter, indicator, 4.0, &tokens, Role::Primary, 1.0);
                let at = |x: f32, y: f32| indicator.min + vec2(x, y) * box_size;
                painter.add(Shape::line(
                    vec![at(0.27, 0.52), at(0.44, 0.69), at(0.75, 0.34)],
                    Stroke::new(1.8, p.on_primary),
                ));
            } else {
                let border = if response.hovered() && ui.is_enabled() { p.primary } else { p.text_muted };
                painter.rect_stroke(indicator, 4.0, Stroke::new(2.0, border), StrokeKind::Inside);
            }
            if let Some(galley) = galley {
                let at = pos2(indicator.max.x + 9.0, rect.center().y - galley.size().y / 2.0);
                painter.galley(at, galley, p.text_primary);
            }
        }
        pointing(ui, &response);
        let (enabled, on, text) = (ui.is_enabled(), *self.on, self.text);
        response.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, enabled, on, &text));
        response
    }
}

/// An on / off switch: a pill whose knob slides across. The Python app uses check boxes everywhere; this is
/// for places where the change takes effect at once.
#[must_use = "add it with ui.add(...)"]
pub struct Switch<'a> {
    on: &'a mut bool,
    text: String,
}

impl<'a> Switch<'a> {
    pub fn new(on: &'a mut bool, text: impl Into<String>) -> Switch<'a> {
        Switch { on, text: text.into() }
    }
}

impl Widget for Switch<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::of(ui.ctx());
        let tone = if ui.is_enabled() { Tone::Primary } else { Tone::Muted };
        let galley = (!self.text.is_empty())
            .then(|| ui.painter().layout_job(Text::new(&self.text).tone(tone).job(&tokens)));
        let track = vec2(30.0, 16.0);
        let text_width = galley.as_ref().map_or(0.0, |g| 9.0 + g.size().x);
        let (rect, mut response) = ui.allocate_exact_size(vec2(track.x + text_width, 22.0), Sense::click());
        if response.clicked() {
            *self.on = !*self.on;
            response.mark_changed();
        }
        if ui.is_rect_visible(rect) {
            let p = &tokens.p;
            let t = anim::small(ui.ctx(), response.id, if *self.on { 1.0 } else { 0.0 });
            let track_rect = Rect::from_min_size(pos2(rect.min.x, rect.center().y - track.y / 2.0), track);
            let painter = ui.painter();
            painter.rect_filled(track_rect, 8.0, p.bg_frame);
            if t > 0.0 {
                painter.rect_filled(track_rect, 8.0, p.primary.gamma_multiply(t));
            }
            if t < 1.0 {
                let border = if response.hovered() && ui.is_enabled() { p.text_muted } else { p.border };
                painter.rect_stroke(
                    track_rect,
                    8.0,
                    Stroke::new(1.0, border.gamma_multiply(1.0 - t)),
                    StrokeKind::Inside,
                );
            }
            let knob_x = track_rect.min.x + 8.0 + t * (track.x - 16.0);
            let knob = crate::theme::mix(p.text_muted, p.on_primary, t);
            painter.circle_filled(pos2(knob_x, track_rect.center().y), 6.0, knob);
            if let Some(galley) = galley {
                let at = pos2(track_rect.max.x + 9.0, rect.center().y - galley.size().y / 2.0);
                painter.galley(at, galley, p.text_primary);
            }
        }
        pointing(ui, &response);
        let (enabled, on, text) = (ui.is_enabled(), *self.on, self.text);
        response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Switch, enabled, on, &text));
        response
    }
}

#[must_use = "add it with ui.add(...)"]
pub struct Slider<'a> {
    value: &'a mut f64,
    range: RangeInclusive<f64>,
    step: f64,
    width: Option<f32>,
    name: String,
}

impl<'a> Slider<'a> {
    pub fn new(value: &'a mut f64, range: RangeInclusive<f64>) -> Slider<'a> {
        Slider { value, range, step: 0.0, width: None, name: String::new() }
    }

    /// Values snap to multiples of `step` (0 = any value).
    pub fn step(mut self, step: f64) -> Slider<'a> {
        self.step = step.max(0.0);
        self
    }

    pub fn width(mut self, width: f32) -> Slider<'a> {
        self.width = Some(width);
        self
    }

    /// What a screen reader calls it.
    pub fn name(mut self, name: impl Into<String>) -> Slider<'a> {
        self.name = name.into();
        self
    }
}

impl Slider<'_> {
    fn snap(&self, v: f64) -> f64 {
        let (lo, hi) = (*self.range.start(), *self.range.end());
        let v = if self.step > 0.0 { lo + ((v - lo) / self.step).round() * self.step } else { v };
        v.clamp(lo.min(hi), hi.max(lo))
    }
}

impl Widget for Slider<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::of(ui.ctx());
        let width = self.width.unwrap_or_else(|| ui.available_width().min(360.0)).max(40.0);
        let (rect, mut response) = ui.allocate_exact_size(vec2(width, 22.0), Sense::click_and_drag());
        let (lo, hi) = (*self.range.start(), *self.range.end());
        let span = if hi > lo { hi - lo } else { 1.0 };
        let knob = 7.0;
        let (x0, x1) = (rect.min.x + knob, rect.max.x - knob);
        let before = *self.value;
        if response.is_pointer_button_down_on() || response.dragged() {
            // a click puts the handle right where you clicked, not a page towards it
            if let Some(pointer) = response.interact_pointer_pos() {
                let f = f64::from(((pointer.x - x0) / (x1 - x0)).clamp(0.0, 1.0));
                *self.value = self.snap(lo + f * span);
            }
            response.request_focus();
        }
        if response.has_focus() {
            let step = if self.step > 0.0 { self.step } else { span / 100.0 };
            let (left, right) = ui.input(|i| {
                (
                    i.key_pressed(Key::ArrowLeft) || i.key_pressed(Key::ArrowDown),
                    i.key_pressed(Key::ArrowRight) || i.key_pressed(Key::ArrowUp),
                )
            });
            let wheel = f64::from(wheel_steps(ui, &response));
            let moved = f64::from(i8::from(right) - i8::from(left)) + wheel;
            if moved != 0.0 {
                *self.value = self.snap(*self.value + moved * step);
            }
        }
        if *self.value != before {
            response.mark_changed();
        }
        if ui.is_rect_visible(rect) {
            let p = &tokens.p;
            let f = (((*self.value - lo) / span).clamp(0.0, 1.0)) as f32;
            let cy = rect.center().y;
            let groove = Rect::from_min_max(pos2(x0 - 2.0, cy - 3.0), pos2(x1 + 2.0, cy + 3.0));
            let painter = ui.painter();
            painter.rect(groove, 3.0, p.bg_frame, Stroke::new(1.0, p.border), StrokeKind::Inside);
            let x = x0 + f * (x1 - x0);
            let filled = Rect::from_min_max(groove.min, pos2(x, groove.max.y));
            if filled.width() > 0.0 {
                paint::fill(
                    painter,
                    filled,
                    3.0,
                    &tokens,
                    Role::Primary,
                    if ui.is_enabled() { 1.0 } else { 0.4 },
                );
            }
            painter.circle(pos2(x, cy), knob - 1.0, p.text_primary, Stroke::new(2.0, p.primary));
        }
        pointing(ui, &response);
        let (enabled, value, name) = (ui.is_enabled(), *self.value, self.name);
        response.widget_info(|| egui::WidgetInfo::slider(enabled, value, &name));
        response
    }
}

/// A 0 to 100 % slider with the value beside it (`OpacitySlider`), for opacities stored as 0.0 to 1.0.
#[must_use = "add it with ui.add(...)"]
pub struct OpacitySlider<'a> {
    value: &'a mut f32,
}

impl<'a> OpacitySlider<'a> {
    pub fn new(value: &'a mut f32) -> OpacitySlider<'a> {
        OpacitySlider { value }
    }
}

impl Widget for OpacitySlider<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let mut percent = f64::from((*self.value * 100.0).round());
        let inner = ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            let width = (ui.available_width() - 54.0).clamp(60.0, 366.0);
            let slider =
                ui.add(Slider::new(&mut percent, 0.0..=100.0).step(1.0).width(width).name("Opacity"));
            ui.add_sized([40.0, 22.0], Text::new(format!("{percent:.0}%")).px(12.0).mono());
            slider
        });
        let mut response = inner.inner;
        if response.changed() {
            *self.value = (percent / 100.0) as f32;
            response.mark_changed();
        }
        response
    }
}

#[must_use = "add it with ui.add(...)"]
pub struct SpinBox<'a> {
    value: &'a mut f64,
    range: RangeInclusive<f64>,
    step: f64,
    decimals: usize,
    suffix: String,
    width: f32,
    name: String,
}

impl<'a> SpinBox<'a> {
    pub fn new(value: &'a mut f64, range: RangeInclusive<f64>) -> SpinBox<'a> {
        SpinBox {
            value,
            range,
            step: 1.0,
            decimals: 0,
            suffix: String::new(),
            width: 110.0,
            name: "Number".to_owned(),
        }
    }

    pub fn step(mut self, step: f64) -> SpinBox<'a> {
        self.step = step;
        self
    }

    pub fn decimals(mut self, decimals: usize) -> SpinBox<'a> {
        self.decimals = decimals;
        self
    }

    /// Shown after the number: " px", " s", " %".
    pub fn suffix(mut self, suffix: impl Into<String>) -> SpinBox<'a> {
        self.suffix = suffix.into();
        self
    }

    pub fn width(mut self, width: f32) -> SpinBox<'a> {
        self.width = width;
        self
    }

    /// What a screen reader calls it.
    pub fn name(mut self, name: impl Into<String>) -> SpinBox<'a> {
        self.name = name.into();
        self
    }
}

impl SpinBox<'_> {
    fn format(&self, v: f64) -> String {
        format!("{v:.prec$}{}", self.suffix, prec = self.decimals)
    }

    fn clamp(&self, v: f64) -> f64 {
        v.clamp(*self.range.start(), *self.range.end())
    }
}

impl Widget for SpinBox<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::of(ui.ctx());
        let (rect, mut response) = ui.allocate_exact_size(vec2(self.width, 30.0), Sense::hover());
        // the well goes under the text, which is laid out first
        let background = ui.painter().add(Shape::Noop);
        let id = response.id;
        let arrows = Rect::from_min_max(pos2(rect.max.x - 22.0, rect.min.y), rect.max);
        let up_rect = Rect::from_min_max(arrows.min, pos2(arrows.max.x, arrows.center().y));
        let down_rect = Rect::from_min_max(pos2(arrows.min.x, arrows.center().y), arrows.max);
        let field_rect = Rect::from_min_max(rect.min, pos2(arrows.min.x, rect.max.y));
        let before = *self.value;

        // while you type, the text lives here; it becomes the value on Enter or when the focus moves on
        let buffer_id = id.with("typing");
        let edit_id = id.with("edit");
        let typing = ui.ctx().memory(|m| m.has_focus(edit_id));
        let mut text = if typing {
            ui.data(|d| d.get_temp::<String>(buffer_id)).unwrap_or_else(|| self.format(*self.value))
        } else {
            self.format(*self.value)
        };
        let colour = if ui.is_enabled() { tokens.p.text_primary } else { tokens.p.text_muted };
        let edit = TextEdit::singleline(&mut text)
            .id(edit_id)
            .frame(Frame::NONE.inner_margin(Margin { left: 10, right: 4, top: 7, bottom: 7 }))
            .font(tokens.font(12.0, Weight::Regular, true))
            .text_color(colour)
            .desired_width(field_rect.width() - 14.0);
        let edit_response = ui.place(field_rect, edit).accessible_name(self.name.clone());
        if edit_response.has_focus() {
            ui.data_mut(|d| d.insert_temp(buffer_id, text.clone()));
        }
        if edit_response.lost_focus() {
            let number = text.trim().trim_end_matches(self.suffix.trim()).trim().replace(',', ".");
            if let Ok(v) = number.parse::<f64>() {
                *self.value = self.clamp(v);
            }
            ui.data_mut(|d| d.remove::<String>(buffer_id));
        }

        let up = ui.interact(up_rect, id.with("up"), Sense::click());
        let down = ui.interact(down_rect, id.with("down"), Sense::click());
        let mut steps = f64::from(i8::from(up.clicked()) - i8::from(down.clicked()));
        if edit_response.has_focus() {
            steps +=
                f64::from(ui.input(|i| {
                    i8::from(i.key_pressed(Key::ArrowUp)) - i8::from(i.key_pressed(Key::ArrowDown))
                }));
            steps += f64::from(wheel_steps(ui, &edit_response));
        }
        if steps != 0.0 {
            *self.value = self.clamp(*self.value + steps * self.step);
            ui.data_mut(|d| d.insert_temp(buffer_id, self.format(*self.value)));
        }
        if *self.value != before {
            response.mark_changed();
        }

        if ui.is_rect_visible(rect) {
            let p = &tokens.p;
            let radius = tokens.rs();
            let border = if edit_response.has_focus() { p.primary } else { p.border };
            let mut shapes = vec![paint::fill_shape(ui.ctx(), rect, radius, &tokens, Role::BgFrame, 1.0)];
            for (r, hovered) in [(up_rect, up.hovered()), (down_rect, down.hovered())] {
                if hovered && ui.is_enabled() {
                    shapes.push(Shape::rect_filled(r.shrink(1.0), 0.0, p.bg_card_hover));
                }
            }
            shapes.push(Shape::rect_stroke(rect, radius, Stroke::new(1.0, border), StrokeKind::Inside));
            shapes.push(Shape::vline(arrows.min.x, arrows.y_range(), Stroke::new(1.0, p.border)));
            shapes.push(Shape::hline(arrows.x_range(), arrows.center().y, Stroke::new(1.0, p.border)));
            ui.painter().set(background, Shape::Vec(shapes));
            chevron(ui.painter(), up_rect.center(), vec2(7.0, 4.0), true, p.text_muted);
            chevron(ui.painter(), down_rect.center(), vec2(7.0, 4.0), false, p.text_muted);
        }
        pointing(ui, &up);
        pointing(ui, &down);
        response | edit_response
    }
}

#[must_use = "add it with ui.add(...)"]
pub struct Combo<'a, S: AsRef<str>> {
    id: Id,
    selected: &'a mut usize,
    options: &'a [S],
    width: f32,
}

impl<'a, S: AsRef<str>> Combo<'a, S> {
    pub fn new(id: Id, selected: &'a mut usize, options: &'a [S]) -> Combo<'a, S> {
        Combo { id, selected, options, width: 180.0 }
    }

    pub fn width(mut self, width: f32) -> Combo<'a, S> {
        self.width = width;
        self
    }
}

impl<S: AsRef<str>> Widget for Combo<'_, S> {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::of(ui.ctx());
        let rect = ui.allocate_space(vec2(self.width, 30.0)).1;
        let mut response = ui.interact(rect, self.id, Sense::click());
        let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&response));
        if ui.is_rect_visible(rect) {
            let p = &tokens.p;
            let radius = tokens.rs();
            let painter = ui.painter();
            paint::fill(painter, rect, radius, &tokens, Role::BgFrame, 1.0);
            let border = if open {
                p.primary
            } else if response.hovered() && ui.is_enabled() {
                p.text_muted
            } else {
                p.border
            };
            painter.rect_stroke(rect, radius, Stroke::new(1.0, border), StrokeKind::Inside);
            let current = self.options.get(*self.selected).map_or("", |s| s.as_ref());
            let colour = if ui.is_enabled() { p.text_primary } else { p.text_muted };
            let mut job = Text::new(current).medium().colour(colour).job(&tokens);
            job.wrap.max_width = rect.width() - 46.0;
            job.wrap.max_rows = 1;
            job.wrap.break_anywhere = true;
            let galley = ui.painter().layout_job(job);
            painter.galley(pos2(rect.min.x + 10.0, rect.center().y - galley.size().y / 2.0), galley, colour);
            chevron(painter, pos2(rect.max.x - 14.0, rect.center().y), vec2(8.0, 5.0), false, p.text_muted);
        }
        pointing(ui, &response);
        let mut picked = None;
        egui::Popup::from_toggle_button_response(&response).width(self.width).gap(4.0).show(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (i, option) in self.options.iter().enumerate() {
                let row = ui.allocate_response(vec2(ui.available_width(), 26.0), Sense::click());
                let p = &tokens.p;
                if row.hovered() || i == *self.selected {
                    ui.painter().rect_filled(row.rect, tokens.rs(), p.bg_card_hover);
                }
                let font = tokens.font(13.0, Weight::Regular, false);
                ui.painter().text(
                    row.rect.left_center() + vec2(8.0, 0.0),
                    Align2::LEFT_CENTER,
                    option.as_ref(),
                    font,
                    p.text_primary,
                );
                if row.clicked() {
                    picked = Some(i);
                }
            }
        });
        if let Some(i) = picked
            && i != *self.selected
        {
            *self.selected = i;
            response.mark_changed();
        }
        let (enabled, current) = (
            ui.is_enabled(),
            self.options.get(*self.selected).map_or(String::new(), |s| s.as_ref().to_owned()),
        );
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, enabled, &current));
        response
    }
}

/// A framed swatch with its hex code (`ColorButton`). Clicking it is the caller's cue to open the colour
/// picker.
#[must_use = "add it with ui.add(...)"]
pub struct ColourButton {
    colour: Color32,
}

impl ColourButton {
    pub fn new(colour: Color32) -> ColourButton {
        ColourButton { colour }
    }
}

impl Widget for ColourButton {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::of(ui.ctx());
        let (rect, response) = ui.allocate_exact_size(vec2(130.0, 32.0), Sense::click());
        if ui.is_rect_visible(rect) {
            let p = &tokens.p;
            let radius = tokens.rs();
            let painter = ui.painter();
            paint::fill(painter, rect, radius, &tokens, Role::BgFrame, 1.0);
            let border = if response.hovered() && ui.is_enabled() { p.text_muted } else { p.border };
            painter.rect_stroke(rect, radius, Stroke::new(1.0, border), StrokeKind::Inside);
            let chip = Rect::from_min_size(pos2(rect.min.x + 6.0, rect.center().y - 10.0), Vec2::splat(20.0));
            painter.rect_filled(chip, 4.0, self.colour);
            let [r, g, b, _] = self.colour.to_srgba_unmultiplied();
            let hex = format!("#{r:02X}{g:02X}{b:02X}");
            painter.text(
                pos2(chip.max.x + 8.0, rect.center().y),
                Align2::LEFT_CENTER,
                hex,
                tokens.font(12.0, Weight::Regular, true),
                p.text_primary,
            );
        }
        pointing(ui, &response);
        let enabled = ui.is_enabled();
        let [r, g, b, _] = self.colour.to_srgba_unmultiplied();
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::ColorWell, enabled, format!("#{r:02X}{g:02X}{b:02X}"))
        });
        response
    }
}
