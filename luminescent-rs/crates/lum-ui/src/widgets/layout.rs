//! Surfaces content sits on: cards, section headers, rules and tooltips.

use egui::{InnerResponse, Margin, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, UiBuilder, vec2};

use crate::paint;
use crate::text::{Text, caps};
use crate::theme::{Role, Tokens, Tone};

/// How a surface looks: a fill role, a border and a corner radius.
#[derive(Debug, Clone, Copy)]
pub struct Surface {
    pub fill: Option<Role>,
    pub fill_alpha: f32,
    pub border: Option<egui::Color32>,
    /// The border colour while the pointer is over it (cards lean towards the accent).
    pub hover_border: Option<egui::Color32>,
    pub radius: f32,
    pub margin: Margin,
}

impl Surface {
    /// `#lumCard`: a card-coloured box with a border, leaning towards the accent when hovered.
    pub fn card(tokens: &Tokens) -> Surface {
        Surface {
            fill: Some(Role::BgCard),
            fill_alpha: 1.0,
            border: Some(tokens.p.border),
            hover_border: tokens.hover.then(|| tokens.hover_border(0.45)),
            radius: tokens.rb(),
            margin: Margin::same(12),
        }
    }

    /// An input-coloured well (`bg_frame`), for lists and drop areas.
    pub fn well(tokens: &Tokens) -> Surface {
        Surface {
            fill: Some(Role::BgFrame),
            fill_alpha: 1.0,
            border: Some(tokens.p.border),
            hover_border: None,
            radius: tokens.rs(),
            margin: Margin::same(8),
        }
    }

    pub fn margin(mut self, margin: impl Into<Margin>) -> Surface {
        self.margin = margin.into();
        self
    }

    /// Lays `add` out on the surface. The background is painted once the content's size is known, so it can be
    /// a gradient and the border can follow the pointer.
    pub fn show<R>(self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let tokens = Tokens::of(ui.ctx());
        let background = ui.painter().add(Shape::Noop);
        let outer = ui.available_rect_before_wrap();
        let inner_max = Rect::from_min_max(
            outer.min + self.margin.left_top(),
            (outer.max - self.margin.right_bottom()).max(outer.min + self.margin.left_top()),
        );
        let mut child = ui.new_child(UiBuilder::new().max_rect(inner_max).layout(*ui.layout()));
        let inner = add(&mut child);
        let rect = Rect::from_min_max(
            child.min_rect().min - self.margin.left_top(),
            child.min_rect().max + self.margin.right_bottom(),
        );
        let response = ui.allocate_rect(rect, Sense::hover());
        let mut shapes = Vec::new();
        if let Some(role) = self.fill {
            shapes.push(paint::fill_shape(ui.ctx(), rect, self.radius, &tokens, role, self.fill_alpha));
        }
        let border =
            if response.contains_pointer() { self.hover_border.or(self.border) } else { self.border };
        if let Some(colour) = border {
            shapes.push(Shape::rect_stroke(rect, self.radius, Stroke::new(1.0, colour), StrokeKind::Inside));
        }
        ui.painter().set(background, Shape::Vec(shapes));
        InnerResponse::new(inner, response)
    }
}

/// A card around `add`.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    Surface::card(&Tokens::of(ui.ctx())).show(ui, add)
}

/// A muted mono caps heading with a line running to the right edge: "APPEARANCE ────────".
pub fn section_header(ui: &mut Ui, title: &str) -> Response {
    let tokens = Tokens::of(ui.ctx());
    ui.add_space(14.0);
    let response = ui
        .horizontal(|ui| {
            ui.add(caps(title).px(10.0).spacing(0.14).tone(Tone::Muted));
            let room = ui.available_rect_before_wrap();
            let y = ui.min_rect().center().y;
            ui.painter().hline(room.x_range(), y, Stroke::new(1.0, tokens.p.border));
        })
        .response;
    ui.add_space(6.0);
    response
}

/// A 1 px line in the border colour across the available width (`rule()`).
pub fn rule(ui: &mut Ui) -> Response {
    let tokens = Tokens::of(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, tokens.p.border);
    response
}

/// A 1 px upright line `height` tall, for separating groups in a toolbar.
pub fn vrule(ui: &mut Ui, height: f32) -> Response {
    let tokens = Tokens::of(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(vec2(1.0, height), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, tokens.p.border);
    response
}

/// A tooltip in the small type the stylesheet gives `QToolTip`.
pub fn tip(response: Response, text: impl Into<String>) -> Response {
    let text = text.into();
    response.on_hover_ui(|ui| {
        ui.add(Text::new(text).px(12.0).wrap());
    })
}
