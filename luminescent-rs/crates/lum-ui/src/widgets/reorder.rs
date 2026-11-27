//! Dragging the rows of a list into a new order (`DragReorder` in the Python app): grab a row or its handle, a line
//! shows where it will land, let go to drop it. A press that does not move is left alone, so clicks still work.

use egui::{CursorIcon, Id, Rect, Response, Stroke, Ui};

use crate::theme::{Tokens, alpha};

/// Where the dragged row would land among the others: the number of other rows that centre above the pointer.
fn slot(rows: &[Rect], from: usize, pointer_y: f32) -> usize {
    rows.iter().enumerate().filter(|&(i, r)| i != from && r.center().y < pointer_y).count()
}

/// The y of the gap the row would be dropped into, among the rows that stay.
fn gap_y(rows: &[Rect], from: usize, to: usize) -> Option<f32> {
    let others: Vec<&Rect> = rows.iter().enumerate().filter(|&(i, _)| i != from).map(|(_, r)| r).collect();
    match (to.checked_sub(1).and_then(|i| others.get(i)), others.get(to)) {
        (Some(above), Some(below)) => Some((above.max.y + below.min.y) / 2.0),
        (Some(above), None) => Some(above.max.y + 1.0),
        (None, Some(below)) => Some(below.min.y - 1.0),
        (None, None) => None,
    }
}

/// Call once per frame after laying out the rows: `rows` are each row's rectangle and the response that starts
/// a drag (the row itself or its handle). Gives back `(from, to)` when a row was dropped in a new place, with
/// `to` counted among the rows once the dragged one is taken out, which is where it ends up.
pub fn drag_rows(ui: &mut Ui, id: Id, rows: &[(Rect, Response)]) -> Option<(usize, usize)> {
    let tokens = Tokens::of(ui.ctx());
    let state_id = id.with("dragging");
    let mut from: Option<usize> = ui.ctx().data(|d| d.get_temp(state_id));
    for (i, (_, response)) in rows.iter().enumerate() {
        if response.drag_started() {
            from = Some(i);
            ui.ctx().data_mut(|d| d.insert_temp(state_id, i));
        }
        if response.hovered() && from.is_none() {
            ui.ctx().set_cursor_icon(CursorIcon::Grab);
        }
    }
    let from = from.filter(|&i| i < rows.len())?;
    let (source_rect, source) = &rows[from];
    let rects: Vec<Rect> = rows.iter().map(|(r, _)| *r).collect();
    let pointer = ui.ctx().pointer_interact_pos();
    let to = pointer.map_or(from, |p| slot(&rects, from, p.y));
    if source.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        ui.painter().rect_filled(*source_rect, tokens.rs(), alpha(tokens.p.bg_base, 0.5));
        if let Some(y) = gap_y(&rects, from, to) {
            let x = source_rect.x_range();
            ui.painter().hline(x, y, Stroke::new(2.0, tokens.p.primary));
        }
    }
    if source.drag_stopped() {
        ui.ctx().data_mut(|d| d.remove::<usize>(state_id));
        return (to != from).then_some((from, to));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2};

    fn rows(n: usize) -> Vec<Rect> {
        (0..n).map(|i| Rect::from_min_size(pos2(0.0, i as f32 * 40.0), vec2(100.0, 36.0))).collect()
    }

    #[test]
    fn the_slot_counts_the_other_rows_above_the_pointer() {
        let r = rows(4);
        assert_eq!(slot(&r, 0, 5.0), 0, "still at the top: first among the rest");
        assert_eq!(slot(&r, 0, 100.0), 2, "below the third row's middle, before the fourth");
        assert_eq!(slot(&r, 3, 5.0), 0);
        assert_eq!(slot(&r, 1, 500.0), 3);
    }

    #[test]
    fn the_line_sits_in_the_gap_it_would_drop_into() {
        let r = rows(3);
        assert_eq!(gap_y(&r, 0, 0), Some(39.0), "above the second row, which is the first that stays");
        assert_eq!(gap_y(&r, 0, 1), Some(78.0), "between the second and third row");
        assert_eq!(gap_y(&r, 0, 2), Some(117.0), "under the last row");
        assert_eq!(gap_y(&rows(1), 0, 0), None);
    }
}
