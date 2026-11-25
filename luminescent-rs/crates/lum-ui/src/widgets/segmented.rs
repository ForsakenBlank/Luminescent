//! A small framed set of choices, one of them picked (`#cpSeg`): the Solid / Gradient switch of the colour
//! picker.

use egui::{Id, Sense, Stroke, StrokeKind, Ui, vec2};

use crate::fonts::Weight;
use crate::paint;
use crate::theme::{Role, Tokens};

/// Draws the choices side by side and gives back the one clicked, if it is not the picked one already.
pub fn segmented(ui: &mut Ui, labels: &[&str], selected: usize) -> Option<usize> {
    let tokens = Tokens::of(ui.ctx());
    let p = &tokens.p;
    let font = tokens.font(12.0, Weight::Regular, false);
    let widths: Vec<f32> = labels
        .iter()
        .map(|l| {
            ui.painter().layout_no_wrap((*l).to_owned(), font.clone(), egui::Color32::PLACEHOLDER).size().x
                + 28.0
        })
        .collect();
    let total: f32 = widths.iter().sum::<f32>() + 4.0;
    let (frame, _) = ui.allocate_exact_size(vec2(total, 26.0), Sense::hover());
    ui.painter().rect_filled(frame, 14.0, p.bg_frame);
    ui.painter().rect_stroke(frame, 14.0, Stroke::new(1.0, p.border), StrokeKind::Inside);
    let mut x = frame.min.x + 2.0;
    let mut clicked = None;
    for (i, label) in labels.iter().enumerate() {
        let rect = egui::Rect::from_min_size(egui::pos2(x, frame.min.y + 2.0), vec2(widths[i], 22.0));
        x += widths[i];
        let response = ui.interact(rect, Id::unique(("lum-segment", labels, i)), Sense::click());
        let on = i == selected;
        if on {
            paint::fill(ui.painter(), rect, 11.0, &tokens, Role::Primary, 1.0);
        }
        let colour = if on {
            p.on_primary
        } else if response.hovered() {
            p.text_primary
        } else {
            p.text_muted
        };
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, *label, font.clone(), colour);
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let name = (*label).to_owned();
        response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, &name));
        if response.clicked() && !on {
            clicked = Some(i);
        }
    }
    clicked
}
