//! The Today page: a summary of the day, its timeline by the hour and the things used most. ‹ and › step to
//! earlier days.

use egui::{Align2, Id, Rect, Role, Sense, Ui, UiBuilder, WidgetInfo, pos2, vec2};
use jiff::ToSpan;
use jiff::civil::Date;
use lum_calc::timeparse::fmt_secs;
use lum_core::overview::{self, Entry};
use serde_json::Value;

use super::{Env, returning};
use crate::fonts::Weight;
use crate::overlay::PageFrame;
use crate::theme::Tokens;
use crate::tools::draw;
use crate::widgets::IconButton;

const NARROW: f32 = 640.0;
const HOUR_H: f32 = 30.0;
const ROW_H: f32 = 30.0;

#[derive(Debug, Default)]
pub struct TodayPage {
    /// The day shown; today until ‹ is pressed.
    day: Option<Date>,
    /// The log of that day, read when the page comes up or the day changes.
    loaded: Option<(Date, Vec<Value>)>,
    pass: Option<u64>,
}

enum Line {
    Hour(String),
    At(Entry),
}

impl TodayPage {
    fn load(&mut self, env: &mut Env<'_>) {
        let day = self.day.unwrap_or(env.today).min(env.today);
        self.day = (day != env.today).then_some(day);
        env.usage.flush();
        self.loaded = Some((day, env.usage.day_activity(Some(day))));
    }

    fn step(&mut self, env: &mut Env<'_>, days: i64) {
        let day = self.day.unwrap_or(env.today);
        let next = day.checked_add(days.days()).unwrap_or(day).min(env.today);
        self.day = (next != env.today).then_some(next);
        self.loaded = None;
    }

    pub fn show(&mut self, ui: &mut Ui, rect: Rect, env: &mut Env<'_>) -> PageFrame {
        if returning(ui.ctx(), &mut self.pass) || self.loaded.is_none() {
            self.load(env);
        }
        let Some((day, values)) = self.loaded.as_ref() else { return PageFrame::default() };
        let tokens = Tokens::of(ui.ctx());
        let p = &tokens.p;
        let (day, today) = (*day, env.today);
        let narrow = rect.width() < NARROW;
        let area = Rect::from_min_max(
            rect.min + vec2(if narrow { 12.0 } else { 24.0 }, 12.0),
            rect.max - vec2(if narrow { 12.0 } else { 24.0 }, 18.0),
        );
        let is_today = day == today;
        let mut step = 0;

        // title, date and the arrows
        let title = overview::day_title(day, today);
        let galley = draw::line(ui, &title, 15.0, Weight::SemiBold, false, p.text_primary, area.width());
        let title_w = galley.size().x;
        ui.painter().galley(pos2(area.min.x, area.min.y + 8.0), galley, p.text_primary);
        let date = overview::date_line(day);
        let date_galley = draw::line(ui, &date, 10.0, Weight::Medium, true, p.text_muted, 220.0);
        if !narrow {
            ui.painter().galley(
                pos2(area.min.x + title_w + 12.0, area.min.y + 12.0),
                date_galley,
                p.text_muted,
            );
        }
        let arrows = Rect::from_min_size(pos2(area.max.x - 68.0, area.min.y), vec2(68.0, 34.0));
        let mut bar = ui.new_child(
            UiBuilder::new().max_rect(arrows).layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        bar.spacing_mut().item_spacing.x = 8.0;
        if bar.add(IconButton::new("‹").size(30.0).font_px(14.0).tooltip("The day before")).clicked() {
            step = -1;
        }
        let next = bar
            .add_enabled(!is_today, IconButton::new("›").size(30.0).font_px(14.0).tooltip("The day after"));
        if next.clicked() {
            step = 1;
        }

        // the four boxes
        let summary = overview::summarise(values);
        let boxes = [
            (summary.runs.to_string(), "things run"),
            (if summary.timed > 0.0 { fmt_secs(summary.timed) } else { "0m".to_owned() }, "on timers"),
            (summary.tools.to_string(), "tools used"),
            (
                match (&summary.first, &summary.last) {
                    (Some(first), Some(last)) => format!("{first} to {last}"),
                    _ => "-".to_owned(),
                },
                "first to last",
            ),
        ];
        let top = area.min.y + 46.0;
        let mut x = area.min.x;
        let mut row_y = top;
        let mut box_h = 0.0;
        for (value, label) in &boxes {
            let value_g = draw::line(ui, value, 20.0, Weight::SemiBold, true, p.text_primary, 220.0);
            let label_g = draw::caps(ui, label, 9.0, p.text_muted);
            let w = value_g.size().x.max(label_g.size().x) + 32.0;
            let h = value_g.size().y + label_g.size().y + 22.0;
            if x + w > area.max.x && x > area.min.x {
                x = area.min.x;
                row_y += box_h + 12.0;
            }
            let r = Rect::from_min_size(pos2(x, row_y), vec2(w, h));
            draw::card(ui, r, &tokens);
            ui.painter().galley(r.min + vec2(16.0, 10.0), value_g.clone(), p.text_primary);
            ui.painter().galley(r.min + vec2(16.0, 12.0 + value_g.size().y), label_g, p.text_muted);
            x += w + 12.0;
            box_h = h;
        }
        let body_top = row_y + box_h + 14.0;
        let body = Rect::from_min_max(pos2(area.min.x, body_top), area.max);

        let side_w = if narrow { 0.0 } else { 300.0 };
        let timeline = Rect::from_min_max(
            body.min,
            pos2(body.max.x - side_w - if narrow { 0.0 } else { 20.0 }, body.max.y),
        );
        self.timeline(ui, timeline, values, is_today, &tokens);
        if !narrow {
            let side = Rect::from_min_max(pos2(body.max.x - side_w, body.min.y), body.max);
            most_used(ui, side, values, &tokens);
        }
        if step != 0 {
            self.step(env, step);
        }
        PageFrame::default()
    }

    fn timeline(&self, ui: &mut Ui, rect: Rect, values: &[Value], today: bool, tokens: &Tokens) {
        let p = &tokens.p;
        let entries = overview::entries(values);
        if values.is_empty() {
            let message = if today {
                "Nothing yet today. Run something from the Dashboard or start a timer and it shows up here."
            } else {
                "Nothing was logged that day."
            };
            let galley = draw::wrapped(ui, message, 13.0, p.text_muted, rect.width());
            ui.painter().galley(rect.min, galley, p.text_muted);
            return;
        }
        let mut lines = Vec::new();
        let mut last_hour = None;
        for entry in entries {
            let hour = entry.at.hour();
            if last_hour != Some(hour) {
                last_hour = Some(hour);
                lines.push(Line::Hour(format!("{hour:02}:00")));
            }
            lines.push(Line::At(entry));
        }
        let total: f32 =
            lines.iter().map(|l| if matches!(l, Line::Hour(_)) { HOUR_H } else { ROW_H }).sum::<f32>() + 6.0;
        let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        egui::ScrollArea::vertical().id_salt("lum-today-timeline").auto_shrink([false, false]).show_viewport(
            &mut child,
            |ui, viewport| {
                let (area, _) = ui.allocate_exact_size(vec2(rect.width(), total), Sense::hover());
                let mut y = 0.0;
                for line in &lines {
                    let h = if matches!(line, Line::Hour(_)) { HOUR_H } else { ROW_H };
                    if y + h >= viewport.min.y - 20.0 && y <= viewport.max.y + 20.0 {
                        let r = Rect::from_min_size(area.min + vec2(0.0, y), vec2(area.width() - 8.0, h));
                        match line {
                            Line::Hour(text) => {
                                let g = draw::caps(ui, text, 9.5, p.text_muted);
                                ui.painter().galley(r.min + vec2(0.0, 12.0), g, p.text_muted);
                            }
                            Line::At(entry) => timeline_row(ui, r.shrink2(vec2(0.0, 1.0)), entry, tokens),
                        }
                    }
                    y += h;
                }
            },
        );
    }
}

fn timeline_row(ui: &mut Ui, r: Rect, entry: &Entry, tokens: &Tokens) {
    let p = &tokens.p;
    let id = Id::unique(("lum-today-row", r.min.x as i32, r.min.y as i32));
    let response = ui.interact(r, id, Sense::hover());
    let name = format!("{} {} {}", entry.at.strftime("%H:%M"), overview::kind_tag(&entry.kind), entry.label);
    response.widget_info(|| WidgetInfo::labeled(Role::ListItem, true, name.clone()));
    if response.hovered() {
        ui.painter().rect_filled(r, 6.0, p.bg_card_hover);
    }
    let mid = r.center().y;
    let time = ui.painter().layout_no_wrap(
        entry.at.strftime("%H:%M").to_string(),
        tokens.font(11.0, Weight::Regular, true),
        p.text_muted,
    );
    ui.painter().galley(pos2(r.min.x + 8.0, mid - time.size().y / 2.0), time, p.text_muted);
    let badge = Rect::from_center_size(pos2(r.min.x + 8.0 + 38.0 + 12.0 + 27.0, mid), vec2(54.0, 18.0));
    ui.painter().rect_filled(badge, 4.0, p.primary_muted);
    let tag = overview::kind_tag(&entry.kind);
    let tag_g = draw::line(ui, &tag, 9.0, Weight::SemiBold, true, p.primary_text, 50.0);
    ui.painter().galley(badge.center() - tag_g.size() / 2.0, tag_g, p.primary_text);
    let mut right = r.max.x - 8.0;
    if let Some(secs) = entry.secs {
        let g = ui.painter().layout_no_wrap(
            fmt_secs(secs),
            tokens.font(11.0, Weight::Regular, true),
            p.text_secondary,
        );
        right -= g.size().x;
        ui.painter().galley(pos2(right, mid - g.size().y / 2.0), g, p.text_secondary);
        right -= 12.0;
    }
    let left = badge.max.x + 12.0;
    let label = draw::line(ui, &entry.label, 13.0, Weight::Regular, false, p.text_primary, right - left);
    ui.painter().galley(pos2(left, mid - label.size().y / 2.0), label, p.text_primary);
}

fn most_used(ui: &mut Ui, rect: Rect, values: &[Value], tokens: &Tokens) {
    let p = &tokens.p;
    let head = draw::caps(ui, "most used", 9.5, p.text_muted);
    ui.painter().galley(rect.min, head, p.text_muted);
    let rows = overview::most_used(values, 8);
    if rows.is_empty() {
        ui.painter().text(
            rect.min + vec2(0.0, 26.0),
            Align2::LEFT_TOP,
            "-",
            tokens.font(12.0, Weight::Regular, false),
            p.text_muted,
        );
        return;
    }
    let mut y = rect.min.y + 26.0;
    for (label, n) in rows {
        let count = ui.painter().layout_no_wrap(
            format!("×{n}"),
            tokens.font(11.0, Weight::Regular, true),
            p.text_muted,
        );
        ui.painter().galley(pos2(rect.max.x - count.size().x, y + 1.0), count.clone(), p.text_muted);
        let name = draw::line(
            ui,
            &label,
            12.0,
            Weight::Regular,
            false,
            p.text_primary,
            rect.width() - count.size().x - 10.0,
        );
        ui.painter().galley(pos2(rect.min.x, y), name, p.text_primary);
        y += 22.0;
    }
}
