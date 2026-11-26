//! Menu entries for context menus and pop-up menus (`QMenu` in the stylesheet): rows 7 px tall-padded with a
//! rounded hover, a tick box for on / off entries, a hint on the right and thin separators.
//!
//! The menu frame itself (background, border, corner) comes from the style the tokens install, so
//! `response.context_menu(|ui| ...)` and `ui.menu_button(...)` already look right.

use egui::{Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2};

use crate::fonts::Weight;
use crate::paint;
use crate::theme::{Role, Tokens};

fn entry(ui: &mut Ui, text: &str, hint: &str, tick: Option<bool>) -> Response {
    let tokens = Tokens::of(ui.ctx());
    let p = &tokens.p;
    let enabled = ui.is_enabled();
    let colour = if enabled { p.text_primary } else { p.text_muted };
    let galley =
        ui.painter().layout_no_wrap(text.to_owned(), tokens.font(13.0, Weight::Regular, false), colour);
    let hint_galley = (!hint.is_empty()).then(|| {
        ui.painter().layout_no_wrap(hint.to_owned(), tokens.font(11.0, Weight::Regular, true), p.text_muted)
    });
    let left = if tick.is_some() { 12.0 + 14.0 + 8.0 } else { 12.0 };
    let hint_width = hint_galley.as_ref().map_or(0.0, |g| g.size().x + 24.0);
    let width = (left + galley.size().x + hint_width + 24.0).max(ui.available_width());
    let (rect, response) = ui.allocate_exact_size(vec2(width, galley.size().y + 14.0), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, text));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if response.hovered() && enabled {
            paint::fill(painter, rect, tokens.rs(), &tokens, Role::BgCardHover, 1.0);
        }
        if let Some(on) = tick {
            let tick_rect =
                Rect::from_center_size(pos2(rect.min.x + 12.0 + 7.0, rect.center().y), vec2(14.0, 14.0));
            if on {
                paint::fill(painter, tick_rect, 4.0, &tokens, Role::Primary, 1.0);
                let at = |x: f32, y: f32| tick_rect.min + vec2(x, y) * 14.0;
                painter.add(Shape::line(
                    vec![at(0.27, 0.52), at(0.44, 0.69), at(0.75, 0.34)],
                    Stroke::new(1.6, p.on_primary),
                ));
            } else {
                painter.rect_stroke(tick_rect, 4.0, Stroke::new(1.0, p.text_muted), StrokeKind::Inside);
            }
        }
        painter.galley(pos2(rect.min.x + left, rect.center().y - galley.size().y / 2.0), galley, colour);
        if let Some(hint) = hint_galley {
            let at = pos2(rect.max.x - 12.0 - hint.size().x, rect.center().y - hint.size().y / 2.0);
            painter.galley(at, hint, p.text_muted);
        }
    }
    response
}

/// An entry. Clicking it closes the menu.
pub fn item(ui: &mut Ui, text: &str) -> Response {
    let response = entry(ui, text, "", None);
    if response.clicked() {
        ui.close();
    }
    response
}

/// An entry with a hint on the right, usually its hotkey.
pub fn item_with_hint(ui: &mut Ui, text: &str, hint: &str) -> Response {
    let response = entry(ui, text, hint, None);
    if response.clicked() {
        ui.close();
    }
    response
}

/// An on / off entry. Clicking flips it and leaves the menu open, so several can be set at once.
pub fn check_item(ui: &mut Ui, text: &str, on: &mut bool) -> Response {
    let mut response = entry(ui, text, "", Some(*on));
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    response
}

/// A thin line between groups of entries.
pub fn separator(ui: &mut Ui) {
    let tokens = Tokens::of(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 9.0), Sense::hover());
    ui.painter().hline(rect.x_range().shrink(8.0), rect.center().y, Stroke::new(1.0, tokens.p.border));
}

/// A small muted heading inside a menu ("RECENT COLOURS").
pub fn heading(ui: &mut Ui, text: &str) {
    let tokens = Tokens::of(ui.ctx());
    let galley =
        ui.painter().layout_job(crate::text::caps(text).px(9.5).tone(crate::Tone::Muted).job(&tokens));
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), galley.size().y + 10.0), Sense::hover());
    ui.painter().galley(
        pos2(rect.min.x + 12.0, rect.center().y - galley.size().y / 2.0),
        galley,
        tokens.p.text_muted,
    );
}
