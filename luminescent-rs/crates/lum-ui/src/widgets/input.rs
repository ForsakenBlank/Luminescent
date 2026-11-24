//! One-line and multi-line text inputs in the stylesheet's look: an input-coloured well, a border that turns
//! the accent colour while typing and red when the text is not valid.

use egui::{Frame, Margin, Response, Shape, Stroke, StrokeKind, TextEdit, Ui, Widget};

use crate::fonts::Weight;
use crate::paint;
use crate::theme::{DANGER, Role, Tokens};

/// Paints the well behind a text edit once we know whether it has the focus.
fn frame_behind(ui: &mut Ui, edit: impl FnOnce(&mut Ui) -> Response, invalid: bool) -> Response {
    let tokens = Tokens::of(ui.ctx());
    let background = ui.painter().add(Shape::Noop);
    let response = edit(ui);
    let border = if invalid {
        DANGER
    } else if response.has_focus() {
        tokens.p.primary
    } else {
        tokens.p.border
    };
    let rect = response.rect;
    let radius = tokens.rs();
    let fill = paint::fill_shape(ui.ctx(), rect, radius, &tokens, Role::BgFrame, 1.0);
    let stroke = Shape::rect_stroke(rect, radius, Stroke::new(1.0, border), StrokeKind::Inside);
    ui.painter().set(background, Shape::Vec(vec![fill, stroke]));
    response
}

#[must_use = "add it with ui.add(...)"]
pub struct TextInput<'a> {
    text: &'a mut String,
    hint: String,
    invalid: bool,
    mono: bool,
    password: bool,
    width: Option<f32>,
    id: Option<egui::Id>,
    name: Option<String>,
}

impl<'a> TextInput<'a> {
    pub fn new(text: &'a mut String) -> TextInput<'a> {
        TextInput {
            text,
            hint: String::new(),
            invalid: false,
            mono: false,
            password: false,
            width: None,
            id: None,
            name: None,
        }
    }

    /// Grey text shown while it is empty.
    pub fn hint(mut self, hint: impl Into<String>) -> TextInput<'a> {
        self.hint = hint.into();
        self
    }

    /// A red border (`[invalid="true"]`).
    pub fn invalid(mut self, invalid: bool) -> TextInput<'a> {
        self.invalid = invalid;
        self
    }

    /// Monospace text: paths, commands, hotkeys.
    pub fn mono(mut self) -> TextInput<'a> {
        self.mono = true;
        self
    }

    pub fn password(mut self) -> TextInput<'a> {
        self.password = true;
        self
    }

    pub fn width(mut self, width: f32) -> TextInput<'a> {
        self.width = Some(width);
        self
    }

    /// What a screen reader calls it. The hint stands in when there is none.
    pub fn name(mut self, name: impl Into<String>) -> TextInput<'a> {
        self.name = Some(name.into());
        self
    }

    /// A fixed id, so the focus and the cursor survive the input moving about.
    pub fn id(mut self, id: egui::Id) -> TextInput<'a> {
        self.id = Some(id);
        self
    }
}

impl Widget for TextInput<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::of(ui.ctx());
        let font = if self.mono {
            tokens.font(12.0, Weight::Regular, true)
        } else {
            tokens.font(13.0, Weight::Regular, false)
        };
        let colour = if ui.is_enabled() { tokens.p.text_primary } else { tokens.p.text_muted };
        let width = self.width.unwrap_or_else(|| ui.available_width());
        let invalid = self.invalid;
        let name = self.name.clone().or_else(|| (!self.hint.is_empty()).then(|| self.hint.clone()));
        let response = frame_behind(
            ui,
            |ui| {
                let mut edit = TextEdit::singleline(self.text)
                    .frame(Frame::NONE.inner_margin(Margin::symmetric(10, 7)))
                    .font(font)
                    .text_color(colour)
                    .password(self.password)
                    .desired_width(width - 20.0)
                    .min_size(egui::vec2(0.0, 30.0));
                if !self.hint.is_empty() {
                    edit = edit.hint_text(self.hint);
                }
                if let Some(id) = self.id {
                    edit = edit.id(id);
                }
                ui.add(edit)
            },
            invalid,
        );
        match name {
            Some(name) => response.accessible_name(name),
            None => response,
        }
    }
}

/// A multi-line input: notes, descriptions, scripts.
#[must_use = "add it with ui.add(...)"]
pub struct TextArea<'a> {
    text: &'a mut String,
    hint: String,
    rows: usize,
    mono: bool,
    name: Option<String>,
}

impl<'a> TextArea<'a> {
    pub fn new(text: &'a mut String) -> TextArea<'a> {
        TextArea { text, hint: String::new(), rows: 4, mono: false, name: None }
    }

    pub fn hint(mut self, hint: impl Into<String>) -> TextArea<'a> {
        self.hint = hint.into();
        self
    }

    pub fn rows(mut self, rows: usize) -> TextArea<'a> {
        self.rows = rows.max(1);
        self
    }

    pub fn mono(mut self) -> TextArea<'a> {
        self.mono = true;
        self
    }

    /// What a screen reader calls it. The hint stands in when there is none.
    pub fn name(mut self, name: impl Into<String>) -> TextArea<'a> {
        self.name = Some(name.into());
        self
    }
}

impl Widget for TextArea<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::of(ui.ctx());
        let font = if self.mono {
            tokens.font(12.0, Weight::Regular, true)
        } else {
            tokens.font(13.0, Weight::Regular, false)
        };
        let width = ui.available_width();
        let name = self.name.clone().or_else(|| (!self.hint.is_empty()).then(|| self.hint.clone()));
        let response = frame_behind(
            ui,
            |ui| {
                let mut edit = TextEdit::multiline(self.text)
                    .frame(Frame::NONE.inner_margin(Margin::symmetric(10, 7)))
                    .font(font)
                    .text_color(tokens.p.text_primary)
                    .desired_rows(self.rows)
                    .desired_width(width - 20.0);
                if !self.hint.is_empty() {
                    edit = edit.hint_text(self.hint);
                }
                ui.add(edit)
            },
            false,
        );
        match name {
            Some(name) => response.accessible_name(name),
            None => response,
        }
    }
}
