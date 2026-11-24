//! Lists: rows with the stylesheet's hover and selection, and a virtualised scroll list that only lays out
//! the rows on screen, however long the list is.

use egui::{Id, InnerResponse, Rect, Response, ScrollArea, Sense, Shape, Ui, UiBuilder, vec2};

use crate::paint;
use crate::theme::{Role, Tokens};
use crate::widgets::layout::Surface;

/// One row (`QListWidget::item`): 6 px by 8 px of padding, a hover wash and the selection colour.
pub fn row<R>(ui: &mut Ui, selected: bool, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    let tokens = Tokens::of(ui.ctx());
    let background = ui.painter().add(Shape::Noop);
    let width = ui.available_width();
    let top = ui.cursor().min;
    // a bounded height: a horizontal layout in an endless one works out its rows as NaN
    let inner_max = Rect::from_min_size(top + vec2(8.0, 6.0), vec2((width - 16.0).max(0.0), 1000.0));
    let mut child = ui.new_child(
        UiBuilder::new().max_rect(inner_max).layout(egui::Layout::left_to_right(egui::Align::Min)),
    );
    let inner = add(&mut child);
    let rect = Rect::from_min_size(top, vec2(width, child.min_rect().height() + 12.0));
    let response = ui.allocate_rect(rect, Sense::click());
    let role = if selected {
        Some(Role::PrimaryMuted)
    } else if response.hovered() {
        Some(Role::BgCardHover)
    } else {
        None
    };
    if let Some(role) = role {
        ui.painter().set(background, paint::fill_shape(ui.ctx(), rect, tokens.rs(), &tokens, role, 1.0));
    }
    InnerResponse::new(inner, response)
}

/// A list box (`QListWidget`): a card-coloured frame around `add`.
pub fn list_frame<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    let tokens = Tokens::of(ui.ctx());
    let mut surface = Surface::card(&tokens).margin(3);
    surface.hover_border = None;
    surface.show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        add(ui)
    })
}

/// `count` rows of `row_height`, of which only the visible ones are built. Use it for anything that can grow
/// long: processes, histories, snippets, palette results, binds.
pub fn virtual_list(
    ui: &mut Ui,
    id: Id,
    count: usize,
    row_height: f32,
    max_height: f32,
    mut add_row: impl FnMut(&mut Ui, usize) -> Response,
) -> Option<usize> {
    let mut clicked = None;
    ScrollArea::vertical().id_salt(id).max_height(max_height).auto_shrink([false, true]).show_rows(
        ui,
        row_height,
        count,
        |ui, visible| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for i in visible {
                if add_row(ui, i).clicked() {
                    clicked = Some(i);
                }
            }
        },
    );
    clicked
}
