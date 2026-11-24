//! The hotkey field from Settings (`HotkeyEdit`): the combo as key caps, a Record button and a clear ✕. Only
//! the look for now; recording keys and checking clashes arrive with the hotkey manager in phase 3.

use egui::{Color32, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

use crate::fonts::Weight;
use crate::paint;
use crate::text::Text;
use crate::theme::{DANGER, Role, Tokens, Tone};
use crate::widgets::button::IconButton;

/// The red of the Record dot and of a combo that does not work (`_REC_RED`).
pub const REC_RED: Color32 = DANGER;

/// What the field shows. `combo` is the stored form, such as `ctrl+shift+space`.
#[derive(Debug, Clone, Copy)]
pub struct HotkeyField<'a> {
    combo: &'a str,
    recording: bool,
    warning: Option<&'a str>,
}

/// Which part was clicked.
#[derive(Debug)]
pub struct HotkeyResponse {
    /// The caps (click to type a combo).
    pub field: Response,
    pub record: Response,
    pub clear: Response,
}

impl<'a> HotkeyField<'a> {
    pub fn new(combo: &'a str) -> HotkeyField<'a> {
        HotkeyField { combo, recording: false, warning: None }
    }

    /// Waiting for keys: the Record button is pressed in and the box has the accent border.
    pub fn recording(mut self, recording: bool) -> HotkeyField<'a> {
        self.recording = recording;
        self
    }

    /// Why the combo does not work, or what else uses it. Turns the box red and shows the reason.
    pub fn warning(mut self, warning: Option<&'a str>) -> HotkeyField<'a> {
        self.warning = warning.filter(|w| !w.is_empty());
        self
    }

    pub fn show(self, ui: &mut Ui) -> HotkeyResponse {
        let tokens = Tokens::of(ui.ctx());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let field = self.caps_box(ui, &tokens);
            let record = record_button(ui, &tokens, self.recording);
            let clear = ui.add(IconButton::close().tooltip("Clear (no hotkey)"));
            if let Some(warning) = self.warning {
                ui.add(Text::new(format!("⚠ {warning}")).px(12.0).tone(Tone::Danger));
            }
            HotkeyResponse { field, record, clear }
        })
        .inner
    }

    fn caps_box(&self, ui: &mut Ui, tokens: &Tokens) -> Response {
        let parts = pretty_parts(self.combo);
        let cap_font = tokens.font(11.0, Weight::Medium, true);
        let plus_font = tokens.font(11.0, Weight::Regular, false);
        let p = &tokens.p;
        let galleys: Vec<_> = parts
            .iter()
            .map(|part| ui.painter().layout_no_wrap(part.clone(), cap_font.clone(), p.text_primary))
            .collect();
        let plus = ui.painter().layout_no_wrap("+".to_owned(), plus_font, p.text_muted);
        let empty = ui.painter().layout_no_wrap(
            "Not set".to_owned(),
            tokens.font(12.0, Weight::Regular, false),
            p.text_muted,
        );
        let caps_width: f32 = if galleys.is_empty() {
            empty.size().x
        } else {
            galleys.iter().map(|g| g.size().x + 12.0).sum::<f32>()
                + (galleys.len() - 1) as f32 * (plus.size().x + 8.0)
        };
        let (rect, response) =
            ui.allocate_exact_size(vec2((caps_width + 16.0).max(200.0), 32.0), Sense::click());
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let radius = tokens.rs();
            paint::fill(painter, rect, radius, tokens, Role::BgFrame, 1.0);
            let border = if self.warning.is_some() {
                REC_RED
            } else if self.recording {
                p.primary
            } else if response.hovered() {
                p.text_muted
            } else {
                p.border
            };
            painter.rect_stroke(rect, radius, Stroke::new(1.0, border), StrokeKind::Inside);
            let mut x = rect.min.x + 8.0;
            if galleys.is_empty() {
                painter.galley(pos2(x, rect.center().y - empty.size().y / 2.0), empty, p.text_muted);
            }
            for (i, galley) in galleys.into_iter().enumerate() {
                if i > 0 {
                    x += 4.0;
                    painter.galley(
                        pos2(x, rect.center().y - plus.size().y / 2.0),
                        plus.clone(),
                        p.text_muted,
                    );
                    x += plus.size().x + 4.0;
                }
                let cap = Rect::from_min_size(
                    pos2(x, rect.center().y - galley.size().y / 2.0 - 2.0),
                    galley.size() + vec2(12.0, 4.0),
                );
                paint::fill(painter, cap, 4.0, tokens, Role::BgCard, 1.0);
                painter.rect_stroke(cap, 4.0, Stroke::new(1.0, p.border), StrokeKind::Inside);
                painter.galley(cap.min + vec2(6.0, 2.0), galley.clone(), p.text_primary);
                x = cap.max.x;
            }
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
        }
        response.on_hover_text("Click to type a combo, e.g. ctrl+shift+m")
    }
}

fn record_button(ui: &mut Ui, tokens: &Tokens, recording: bool) -> Response {
    let p = &tokens.p;
    let label = if recording { "Press keys…" } else { "Record" };
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        tokens.font(13.0, Weight::Medium, false),
        p.text_secondary,
    );
    let size = vec2(12.0 + 7.0 + 7.0 + galley.size().x + 12.0, 32.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let radius = tokens.rs();
        if recording {
            paint::fill(painter, rect, radius, tokens, Role::PrimaryMuted, 1.0);
        } else if response.hovered() {
            paint::fill(painter, rect, radius, tokens, Role::BgCardHover, 1.0);
        }
        painter.circle_filled(pos2(rect.min.x + 15.5, rect.center().y), 3.5, REC_RED);
        let colour = if recording {
            p.primary_text
        } else if response.hovered() {
            p.text_primary
        } else {
            p.text_secondary
        };
        painter.galley(pos2(rect.min.x + 26.0, rect.center().y - galley.size().y / 2.0), galley, colour);
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// `ctrl+shift+space` as the caps show it: Ctrl, Shift, Space. A stand-in for `hotkeys.pretty` until phase 3
/// brings the key name tables.
pub fn pretty_parts(combo: &str) -> Vec<String> {
    combo
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => "Ctrl".to_owned(),
            "alt" => "Alt".to_owned(),
            "shift" => "Shift".to_owned(),
            "super" | "win" | "meta" | "cmd" => "Super".to_owned(),
            other => {
                let mut chars = other.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().chain(chars).collect(),
                    None => String::new(),
                }
            }
        })
        .collect()
}

/// Lays out what a key cap looks like at the given place, for pages that list hotkeys (the cheat sheet,
/// palette hints) without the field around it. Returns the cap's rect.
pub fn key_cap(ui: &Ui, at: egui::Pos2, text: &str) -> Rect {
    let tokens = Tokens::of(ui.ctx());
    let p = &tokens.p;
    let galley =
        ui.painter().layout_no_wrap(text.to_owned(), tokens.font(11.0, Weight::Medium, true), p.text_primary);
    let cap = Rect::from_min_size(at, galley.size() + Vec2::new(12.0, 4.0));
    let painter = ui.painter();
    paint::fill(painter, cap, 4.0, &tokens, Role::BgCard, 1.0);
    painter.rect_stroke(cap, 4.0, Stroke::new(1.0, p.border), StrokeKind::Inside);
    painter.galley(cap.min + vec2(6.0, 2.0), galley, p.text_primary);
    cap
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps() {
        assert_eq!(pretty_parts("ctrl+shift+space"), ["Ctrl", "Shift", "Space"]);
        assert_eq!(pretty_parts("super + m"), ["Super", "M"]);
        assert!(pretty_parts("").is_empty());
    }
}
