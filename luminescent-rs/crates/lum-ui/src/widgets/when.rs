//! A date and time picker (`QDateTimeEdit` with its calendar pop-up): a month of days to click and two boxes for the
//! time. Due dates in Notes and the date of a one-off reminder use it.

use egui::{Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use jiff::ToSpan;
use jiff::civil::{Date, DateTime};

use crate::fonts::Weight;
use crate::text::Text;
use crate::theme::{Tokens, Tone};
use crate::widgets::{IconButton, SpinBox};

const CELL: egui::Vec2 = vec2(34.0, 26.0);

#[derive(Debug, Clone, PartialEq)]
pub struct WhenState {
    pub date: Date,
    pub hour: f64,
    pub minute: f64,
    /// The first day of the month on show.
    month: Date,
}

impl WhenState {
    pub fn new(at: DateTime) -> WhenState {
        let month = at.date().first_of_month();
        WhenState { date: at.date(), hour: f64::from(at.hour()), minute: f64::from(at.minute()), month }
    }

    /// The picked moment, to the minute.
    pub fn value(&self) -> DateTime {
        self.date.at(
            self.hour.round().clamp(0.0, 23.0) as i8,
            self.minute.round().clamp(0.0, 59.0) as i8,
            0,
            0,
        )
    }

    /// "Tue 24 Nov 2026  09:00", as the Python box shows it.
    pub fn summary(&self) -> String {
        self.value().strftime("%a %-d %b %Y  %H:%M").to_string()
    }

    fn step_month(&mut self, by: i32) {
        if let Ok(next) = self.month.checked_add(i64::from(by).months()) {
            self.month = next;
        }
    }
}

/// Monday-first weeks, as the British locale draws them.
const WEEKDAYS: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

/// The 42 days a month's grid shows, starting with the Monday on or before the 1st.
pub fn grid(month: Date) -> Vec<Date> {
    let first = month.first_of_month();
    let back = i64::from(first.weekday().to_monday_zero_offset());
    (0..42).filter_map(|i| first.checked_add((i - back).days()).ok()).collect()
}

/// Draws the picker. Gives back whether the value changed.
pub fn show(ui: &mut Ui, state: &mut WhenState, today: Date) -> bool {
    let tokens = Tokens::of(ui.ctx());
    let before = state.clone();
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing = vec2(12.0, 8.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if ui.add(IconButton::new("‹").size(26.0).ghost().tooltip("Previous month")).clicked() {
                    state.step_month(-1);
                }
                let (title, _) = ui.allocate_exact_size(vec2(CELL.x * 7.0 - 52.0, 26.0), Sense::hover());
                ui.painter().text(
                    title.center(),
                    egui::Align2::CENTER_CENTER,
                    state.month.strftime("%B %Y").to_string(),
                    tokens.font(13.0, Weight::SemiBold, false),
                    tokens.p.text_primary,
                );
                if ui.add(IconButton::new("›").size(26.0).ghost().tooltip("Next month")).clicked() {
                    state.step_month(1);
                }
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for name in WEEKDAYS {
                    let (cell, _) = ui.allocate_exact_size(vec2(CELL.x, 20.0), Sense::hover());
                    ui.painter().text(
                        cell.center(),
                        egui::Align2::CENTER_CENTER,
                        name,
                        tokens.font(10.5, Weight::Medium, true),
                        tokens.p.text_muted,
                    );
                }
            });
            for week in grid(state.month).chunks(7) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for &day in week {
                        day_cell(ui, state, day, today, &tokens);
                    }
                });
            }
        });
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.add(Text::new("Time").px(11.0).tone(Tone::Muted));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.add(SpinBox::new(&mut state.hour, 0.0..=23.0).width(64.0));
                ui.add(Text::new(":").px(14.0));
                ui.add(SpinBox::new(&mut state.minute, 0.0..=59.0).width(64.0));
            });
        });
    });
    ui.add(Text::new(state.summary()).px(12.5).medium());
    *state != before
}

fn day_cell(ui: &mut Ui, state: &mut WhenState, day: Date, today: Date, tokens: &Tokens) {
    let (rect, response) = ui.allocate_exact_size(CELL, Sense::click());
    let this_month = day.month() == state.month.month();
    let selected = day == state.date;
    let p = &tokens.p;
    let inner = Rect::from_center_size(rect.center(), vec2(CELL.x - 4.0, CELL.y - 2.0));
    if selected {
        ui.painter().rect_filled(inner, tokens.rs(), p.primary);
    } else if response.hovered() {
        ui.painter().rect_filled(inner, tokens.rs(), p.bg_card_hover);
    }
    if day == today && !selected {
        ui.painter().rect_stroke(inner, tokens.rs(), Stroke::new(1.0, p.primary), StrokeKind::Inside);
    }
    let colour = if selected {
        p.on_primary
    } else if this_month {
        p.text_primary
    } else {
        p.text_muted
    };
    ui.painter().text(
        pos2(rect.center().x, rect.center().y),
        egui::Align2::CENTER_CENTER,
        day.day().to_string(),
        tokens.font(12.5, Weight::Regular, false),
        colour,
    );
    let name = day.strftime("%A %-d %B %Y").to_string();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
    if response.clicked() {
        state.date = day;
        state.month = day.first_of_month();
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> DateTime {
        text.parse().expect("a date and time")
    }

    #[test]
    fn the_grid_starts_on_the_monday_before_the_first_and_has_six_weeks() {
        let days = grid("2026-11-01".parse().expect("a date"));
        assert_eq!(days.len(), 42);
        assert_eq!(days[0].to_string(), "2026-10-26", "1 November 2026 is a Sunday");
        assert_eq!(days[6].to_string(), "2026-11-01");
        assert_eq!(days[41].to_string(), "2026-12-06");
        let days = grid("2026-06-15".parse().expect("a date"));
        assert_eq!(days[0].to_string(), "2026-06-01", "a month that starts on a Monday has no lead-in");
    }

    #[test]
    fn the_value_keeps_the_minute_and_the_summary_reads_like_the_python_box() {
        let mut state = WhenState::new(at("2026-11-24T09:05"));
        assert_eq!(state.value(), at("2026-11-24T09:05"));
        assert_eq!(state.summary(), "Tue 24 Nov 2026  09:05");
        state.hour = 23.4;
        state.minute = 59.6;
        assert_eq!(state.value(), at("2026-11-24T23:59"));
    }

    #[test]
    fn months_step_forwards_and_back_over_a_year_end() {
        let mut state = WhenState::new(at("2026-12-31T10:00"));
        state.step_month(1);
        assert_eq!(state.month.to_string(), "2027-01-01");
        state.step_month(-2);
        assert_eq!(state.month.to_string(), "2026-11-01");
    }
}
