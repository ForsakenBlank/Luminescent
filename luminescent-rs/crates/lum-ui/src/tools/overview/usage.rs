//! The Usage page: what you use most. Dashboard items, hotkeys and tools, each with how often and when last, and
//! a bar for each of the last 30 days. Reset forgets the counts (the Today log stays).

use egui::{Align2, Id, Rect, Role, Sense, Stroke, Ui, UiBuilder, WidgetInfo, pos2, vec2};
use lum_core::overview::{self, Row};
use lum_platform::hotkeys;

use super::{Env, returning};
use crate::dialog::{AskDialog, DialogWindow, Outcome};
use crate::fonts::Weight;
use crate::overlay::PageFrame;
use crate::theme::Tokens;
use crate::tools::{dialog_id, dialog_window, draw};
use crate::widgets::Button;

const NARROW: f32 = 640.0;
const ROW_H: f32 = 34.0;
const ROWS_SHOWN: usize = 25;
const COLUMNS: [(&str, &str); 3] = [("items", "dashboard items"), ("hotkeys", "hotkeys"), ("tools", "tools")];

#[derive(Default)]
pub struct UsagePage {
    dialog: Option<DialogWindow>,
    pass: Option<u64>,
}

impl std::fmt::Debug for UsagePage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UsagePage").finish_non_exhaustive()
    }
}

impl UsagePage {
    pub fn show(&mut self, ui: &mut Ui, rect: Rect, env: &mut Env<'_>) -> PageFrame {
        if returning(ui.ctx(), &mut self.pass) {
            env.usage.flush();
        }
        let mut reset = false;
        if let Some(window) = self.dialog.as_mut() {
            let outcome = window.show(ui.ctx());
            if outcome != Outcome::Open {
                reset = matches!(outcome, Outcome::Accepted(_));
                self.dialog = None;
            }
        }
        if reset {
            env.usage.reset();
        }
        let tokens = Tokens::of(ui.ctx());
        let p = &tokens.p;
        let narrow = rect.width() < NARROW;
        let pad = if narrow { 12.0 } else { 24.0 };
        let area = Rect::from_min_max(rect.min + vec2(pad, 12.0), rect.max - vec2(pad, 18.0));
        let stats = env.usage.stats().clone();

        let title = draw::line(ui, "Usage", 15.0, Weight::SemiBold, false, p.text_primary, 200.0);
        let title_w = title.size().x;
        ui.painter().galley(pos2(area.min.x, area.min.y + 7.0), title, p.text_primary);
        let days = stats.get("days").and_then(serde_json::Value::as_object).cloned().unwrap_or_default();
        let total = overview::total_line(&days);
        if !total.is_empty() && !narrow {
            let g = draw::line(ui, &total, 10.0, Weight::Medium, true, p.text_muted, 240.0);
            ui.painter().galley(pos2(area.min.x + title_w + 12.0, area.min.y + 12.0), g, p.text_muted);
        }
        let button = Rect::from_min_size(pos2(area.max.x - 90.0, area.min.y), vec2(90.0, 30.0));
        let mut bar = ui.new_child(
            UiBuilder::new().max_rect(button).layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        let reset_button = bar.add(Button::ghost("Reset…")).on_hover_text("Start counting from zero");
        if reset_button.clicked() {
            let dialog = AskDialog::confirm(
                dialog_id("usage-reset"),
                "Reset usage",
                "Forget every count (the Today log stays)?",
                "Reset",
                "Cancel",
                true,
            );
            self.dialog = Some(dialog_window(dialog));
        }

        let head = draw::caps(ui, "last 30 days", 9.5, p.text_muted);
        ui.painter().galley(pos2(area.min.x, area.min.y + 44.0), head, p.text_muted);
        let chart = Rect::from_min_size(pos2(area.min.x, area.min.y + 62.0), vec2(area.width(), 90.0));
        paint_chart(ui, chart, &overview::day_series(&days, env.today), env.today, &tokens);

        let body = Rect::from_min_max(pos2(area.min.x, chart.max.y + 14.0), area.max);
        let now = env.now;
        let columns: Vec<(&str, Vec<Row>)> = COLUMNS
            .iter()
            .map(|(key, title)| (*title, overview::top_rows(stats.get(*key), ROWS_SHOWN)))
            .collect();
        if narrow {
            self.stacked(ui, body, &columns, now, &tokens);
        } else {
            let gap = 20.0;
            let w = (body.width() - gap * 2.0) / 3.0;
            for (i, (title, rows)) in columns.iter().enumerate() {
                let r =
                    Rect::from_min_size(body.min + vec2(i as f32 * (w + gap), 0.0), vec2(w, body.height()));
                let hotkeys = i == 1;
                column(ui, r, title, rows, hotkeys, now, &tokens, i);
            }
        }
        PageFrame { busy: self.dialog.is_some() }
    }

    /// In a small box the three columns go one under the other in a single list.
    fn stacked(
        &self,
        ui: &mut Ui,
        body: Rect,
        columns: &[(&str, Vec<Row>)],
        now: jiff::civil::DateTime,
        tokens: &Tokens,
    ) {
        let mut child = ui.new_child(UiBuilder::new().max_rect(body));
        child.set_clip_rect(body.intersect(ui.clip_rect()));
        egui::ScrollArea::vertical().id_salt("lum-usage-stacked").auto_shrink([false, false]).show(
            &mut child,
            |ui| {
                for (i, (title, rows)) in columns.iter().enumerate() {
                    let h = 24.0 + (rows.len().max(1)) as f32 * ROW_H + 12.0;
                    let (r, _) = ui.allocate_exact_size(vec2(body.width() - 8.0, h), Sense::hover());
                    paint_column(ui, r, title, rows, i == 1, now, tokens);
                }
            },
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn column(
    ui: &mut Ui,
    rect: Rect,
    title: &str,
    rows: &[Row],
    hotkeys: bool,
    now: jiff::civil::DateTime,
    tokens: &Tokens,
    index: usize,
) {
    let p = &tokens.p;
    let head = draw::caps(ui, title, 9.5, p.text_muted);
    ui.painter().galley(rect.min, head, p.text_muted);
    let list = Rect::from_min_max(pos2(rect.min.x, rect.min.y + 24.0), rect.max);
    let mut child = ui.new_child(UiBuilder::new().max_rect(list));
    child.set_clip_rect(list.intersect(ui.clip_rect()));
    egui::ScrollArea::vertical().id_salt(("lum-usage-column", index)).auto_shrink([false, false]).show(
        &mut child,
        |ui| {
            let h = (rows.len().max(1)) as f32 * ROW_H;
            let (r, _) = ui.allocate_exact_size(vec2(list.width() - 8.0, h), Sense::hover());
            paint_rows(ui, r, rows, hotkeys, now, tokens);
        },
    );
}

fn paint_column(
    ui: &mut Ui,
    rect: Rect,
    title: &str,
    rows: &[Row],
    hotkeys: bool,
    now: jiff::civil::DateTime,
    tokens: &Tokens,
) {
    let p = &tokens.p;
    let head = draw::caps(ui, title, 9.5, p.text_muted);
    ui.painter().galley(rect.min, head, p.text_muted);
    let list = Rect::from_min_max(pos2(rect.min.x, rect.min.y + 24.0), rect.max);
    paint_rows(ui, list, rows, hotkeys, now, tokens);
}

fn paint_rows(
    ui: &mut Ui,
    rect: Rect,
    rows: &[Row],
    hotkeys: bool,
    now: jiff::civil::DateTime,
    tokens: &Tokens,
) {
    let p = &tokens.p;
    if rows.is_empty() {
        ui.painter().text(
            rect.min,
            Align2::LEFT_TOP,
            "Nothing counted yet.",
            tokens.font(12.0, Weight::Regular, false),
            p.text_muted,
        );
        return;
    }
    for (i, row) in rows.iter().enumerate() {
        let r = Rect::from_min_size(rect.min + vec2(0.0, i as f32 * ROW_H), vec2(rect.width(), ROW_H));
        let label = if hotkeys {
            overview::hotkey_label(&hotkeys::pretty(&row.key, hotkeys::Os::current()), &row.label)
        } else {
            row.label.clone()
        };
        let response =
            ui.interact(r, Id::unique(("lum-usage-row", r.min.x as i32, r.min.y as i32)), Sense::hover());
        response.widget_info(|| WidgetInfo::labeled(Role::ListItem, true, label.clone()));
        let when = format!("{}×  {}", row.uses, overview::ago(&row.last, now));
        let when_g =
            ui.painter().layout_no_wrap(when, tokens.font(10.0, Weight::Regular, true), p.text_muted);
        ui.painter().galley(pos2(r.max.x - when_g.size().x, r.min.y + 4.0), when_g.clone(), p.text_muted);
        let name = draw::line(
            ui,
            &label,
            12.0,
            Weight::Regular,
            false,
            p.text_primary,
            r.width() - when_g.size().x - 12.0,
        );
        ui.painter().galley(pos2(r.min.x, r.min.y + 3.0), name, p.text_primary);
        response.on_hover_text(&label);
        let track = Rect::from_min_size(pos2(r.min.x, r.min.y + 24.0), vec2(r.width(), 3.0));
        ui.painter().rect_filled(track, 1.0, p.bg_frame);
        let fill = Rect::from_min_size(track.min, vec2((track.width() * row.fraction).floor(), 3.0));
        ui.painter().rect_filled(fill, 1.0, p.primary);
    }
}

/// Uses per day for the last 30 days, with the date under every seventh bar and under today's.
fn paint_chart(
    ui: &mut Ui,
    rect: Rect,
    days: &[(jiff::civil::Date, i64)],
    today: jiff::civil::Date,
    tokens: &Tokens,
) {
    let p = &tokens.p;
    let plot = Rect::from_min_max(rect.min + vec2(0.0, 4.0), rect.max - vec2(0.0, 16.0));
    ui.painter().hline(plot.x_range(), plot.max.y + 0.5, Stroke::new(1.0, p.border));
    let top = days.iter().map(|(_, n)| *n).max().unwrap_or(1).max(1);
    let n = days.len().max(1) as f32;
    let gap = 4.0;
    let w = (plot.width() - gap * (n - 1.0)) / n;
    for (i, (day, count)) in days.iter().enumerate() {
        let x = plot.min.x + i as f32 * (w + gap);
        if *count > 0 {
            let h = (*count as f32 / top as f32 * plot.height()).max(2.0);
            let colour = if *day == today { p.primary_text } else { p.primary };
            ui.painter().rect_filled(Rect::from_min_size(pos2(x, plot.max.y - h), vec2(w, h)), 2.0, colour);
        }
        if i % 7 == 2 || *day == today {
            let text = if *day == today {
                "TODAY".to_owned()
            } else {
                format!("{} {}", day.day(), day.strftime("%b")).to_uppercase()
            };
            let g = ui.painter().layout_no_wrap(text, tokens.font(9.0, Weight::Regular, true), p.text_muted);
            ui.painter().galley(pos2(x + w / 2.0 - g.size().x / 2.0, plot.max.y + 3.0), g, p.text_muted);
        }
    }
}
