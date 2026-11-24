//! Buttons in the stylesheet's variants (`QPushButton[variant=...]`) and the round icon button.

use egui::{Color32, Response, Sense, Stroke, StrokeKind, Ui, Vec2, Widget, vec2};

use crate::fonts::Weight;
use crate::paint;
use crate::theme::{DANGER, Role, Tokens, alpha};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Variant {
    /// A card-coloured button with a border.
    #[default]
    Normal,
    /// The accent fill: the main action of a dialog or page.
    Primary,
    /// No fill until hovered; secondary text.
    Ghost,
    /// A border and no fill.
    Outline,
    /// Red text, a red wash when hovered.
    Danger,
    /// Round ends, a little bigger: the overlay's dock and toolbars.
    Pill,
    /// A pill in the accent colour.
    PillAccent,
}

#[derive(Debug, Clone)]
#[must_use = "add it with ui.add(...)"]
pub struct Button {
    text: String,
    variant: Variant,
    checked: bool,
    min_width: f32,
}

impl Button {
    pub fn new(text: impl Into<String>) -> Button {
        Button { text: text.into(), variant: Variant::Normal, checked: false, min_width: 0.0 }
    }

    pub fn primary(text: impl Into<String>) -> Button {
        Button::new(text).variant(Variant::Primary)
    }

    pub fn ghost(text: impl Into<String>) -> Button {
        Button::new(text).variant(Variant::Ghost)
    }

    pub fn danger(text: impl Into<String>) -> Button {
        Button::new(text).variant(Variant::Danger)
    }

    pub fn variant(mut self, variant: Variant) -> Button {
        self.variant = variant;
        self
    }

    /// Shown pressed in, like a checkable QPushButton that is on.
    pub fn checked(mut self, checked: bool) -> Button {
        self.checked = checked;
        self
    }

    pub fn min_width(mut self, width: f32) -> Button {
        self.min_width = width;
        self
    }
}

enum Fill {
    None,
    Role(Role, f32),
    Colour(Color32),
}

/// Background, border and text colour of a button in one state, as the stylesheet gives them.
fn look(
    t: &Tokens,
    variant: Variant,
    checked: bool,
    enabled: bool,
    hovered: bool,
    pressed: bool,
) -> (Fill, Color32, Color32) {
    let p = &t.p;
    let clear = Color32::TRANSPARENT;
    if !enabled {
        return match variant {
            Variant::Primary | Variant::PillAccent => {
                (Fill::Colour(alpha(p.primary, 0.28)), clear, alpha(p.on_primary, 0.55))
            }
            Variant::Ghost | Variant::Outline => (
                Fill::None,
                if variant == Variant::Outline { p.border } else { clear },
                alpha(p.text_muted, 0.7),
            ),
            Variant::Danger => (Fill::None, clear, alpha(DANGER, 0.4)),
            Variant::Normal | Variant::Pill => (Fill::Role(Role::BgFrame, 1.0), p.border, p.text_muted),
        };
    }
    match variant {
        Variant::Primary | Variant::PillAccent => {
            if hovered {
                (Fill::Role(Role::PrimaryHover, 1.0), p.primary_hover, p.on_primary)
            } else {
                (Fill::Role(Role::Primary, 1.0), p.primary, p.on_primary)
            }
        }
        Variant::Ghost if checked => (Fill::Role(Role::PrimaryMuted, 1.0), clear, p.primary_text),
        Variant::Ghost if hovered => (Fill::Role(Role::BgCardHover, 1.0), clear, p.text_primary),
        Variant::Ghost => (Fill::None, clear, p.text_secondary),
        Variant::Outline if hovered => (Fill::Role(Role::BgCardHover, 1.0), p.border, p.text_primary),
        Variant::Outline => (Fill::None, p.border, p.text_primary),
        Variant::Danger if hovered => (Fill::Colour(alpha(DANGER, 0.12)), clear, DANGER),
        Variant::Danger => (Fill::None, clear, DANGER),
        Variant::Normal | Variant::Pill if checked => {
            (Fill::Role(Role::PrimaryMuted, 1.0), p.primary, p.text_primary)
        }
        Variant::Normal | Variant::Pill if pressed => {
            (Fill::Role(Role::BgCardHover, 1.0), p.text_muted, p.text_primary)
        }
        Variant::Normal | Variant::Pill if hovered => {
            (Fill::Role(Role::BgCardHover, 1.0), p.border, p.text_primary)
        }
        Variant::Normal | Variant::Pill => (Fill::Role(Role::BgCard, 1.0), p.border, p.text_primary),
    }
}

fn paint_body(ui: &Ui, rect: egui::Rect, radius: f32, fill: Fill, border: Color32, tokens: &Tokens) {
    let painter = ui.painter();
    match fill {
        Fill::None => {}
        Fill::Role(role, a) => paint::fill(painter, rect, radius, tokens, role, a),
        Fill::Colour(c) => {
            painter.rect_filled(rect, radius, c);
        }
    }
    if border.a() > 0 {
        painter.rect_stroke(rect, radius, Stroke::new(1.0, border), StrokeKind::Inside);
    }
}

impl Widget for Button {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::of(ui.ctx());
        let pill = matches!(self.variant, Variant::Pill | Variant::PillAccent);
        let padding = match self.variant {
            Variant::Pill | Variant::PillAccent => vec2(18.0, 7.0),
            Variant::Primary => vec2(14.0, 6.0),
            _ => vec2(12.0, 6.0),
        };
        let font = tokens.font(13.0, Weight::Medium, false);
        let galley = ui.painter().layout_no_wrap(self.text.clone(), font, Color32::PLACEHOLDER);
        let height = (galley.size().y + 2.0 * padding.y + 2.0).max(if pill { 32.0 } else { 30.0 });
        let size = vec2((galley.size().x + 2.0 * padding.x + 2.0).max(self.min_width), height);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        if ui.is_rect_visible(rect) {
            let enabled = ui.is_enabled();
            let (fill, border, text) = look(
                &tokens,
                self.variant,
                self.checked,
                enabled,
                response.hovered(),
                response.is_pointer_button_down_on(),
            );
            let radius = if pill { 16.0 } else { tokens.rs() };
            paint_body(ui, rect, radius, fill, border, &tokens);
            let at = rect.center() - galley.size() / 2.0;
            ui.painter().galley(at, galley, text);
        }
        if response.hovered() && ui.is_enabled() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let (enabled, checked, text) = (ui.is_enabled(), self.checked, self.text);
        response.widget_info(|| {
            if checked {
                egui::WidgetInfo::selected(egui::Role::Button, enabled, true, &text)
            } else {
                egui::WidgetInfo::labeled(egui::Role::Button, enabled, &text)
            }
        });
        response
    }
}

/// A round button with a glyph (✕, ⋯, ↻), like `icon_button`: round, so the hover highlight is a circle and
/// not a box poking into a rounded corner.
#[derive(Debug, Clone)]
#[must_use = "add it with ui.add(...)"]
pub struct IconButton {
    glyph: String,
    size: f32,
    font_px: f32,
    ghost: bool,
    tooltip: Option<String>,
}

impl IconButton {
    pub fn new(glyph: impl Into<String>) -> IconButton {
        IconButton { glyph: glyph.into(), size: 26.0, font_px: 13.0, ghost: false, tooltip: None }
    }

    /// The ✕ in panel and dialog headers.
    pub fn close() -> IconButton {
        IconButton::new("✕").font_px(11.0).ghost().tooltip("Close")
    }

    pub fn size(mut self, size: f32) -> IconButton {
        self.size = size;
        self
    }

    pub fn font_px(mut self, px: f32) -> IconButton {
        self.font_px = px;
        self
    }

    pub fn ghost(mut self) -> IconButton {
        self.ghost = true;
        self
    }

    pub fn tooltip(mut self, text: impl Into<String>) -> IconButton {
        self.tooltip = Some(text.into());
        self
    }
}

impl Widget for IconButton {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::of(ui.ctx());
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(self.size), Sense::click());
        if ui.is_rect_visible(rect) {
            let variant = if self.ghost { Variant::Ghost } else { Variant::Normal };
            let (fill, border, text) = look(
                &tokens,
                variant,
                false,
                ui.is_enabled(),
                response.hovered(),
                response.is_pointer_button_down_on(),
            );
            paint_body(ui, rect, self.size / 2.0, fill, border, &tokens);
            let font = tokens.font(self.font_px, Weight::Regular, false);
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, &self.glyph, font, text);
        }
        if response.hovered() && ui.is_enabled() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let enabled = ui.is_enabled();
        let name = self.tooltip.clone().unwrap_or_else(|| self.glyph.clone());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, &name));
        match self.tooltip {
            Some(tip) => response.on_hover_text(tip),
            None => response,
        }
    }
}
