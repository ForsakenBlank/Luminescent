//! The Calendar page: a month with a dot on every day that has something, and the chosen day's events and tasks
//! from Atlas (as of the last sync) with Luminescent's own reminders. Events and tasks are made in Atlas.

use egui::text::{LayoutJob, TextFormat};
use egui::{
    Align2, Color32, CursorIcon, Id, Rect, Role, Sense, Stroke, Ui, UiBuilder, WidgetInfo, pos2, vec2,
};
use jiff::ToSpan;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use lum_calc::calendar::{self, Calendar, Item, Kind};

use super::{Env, Out, returning};
use crate::fonts::Weight;
use crate::overlay::PageFrame;
use crate::theme::Tokens;
use crate::tools::draw;
use crate::widgets::{Button, IconButton};

const NARROW: f32 = 760.0;
const CELL: (f32, f32) = (56.0, 50.0);

/// A day button is 56 px wide, narrower when the room is.
fn cell_width(room: f32) -> f32 {
    ((room - 6.0 * 4.0) / 7.0).min(CELL.0).max(30.0)
}
const WEEKDAYS: [&str; 7] = ["MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN"];

/// What was worked out for the month and day on show; made again when either changes or the page comes back.
#[derive(Debug, Default)]
struct Cache {
    weeks: Vec<[Date; 7]>,
    counts: Vec<usize>,
    agenda: Vec<Item>,
    events: usize,
    tasks: usize,
    last_sync_ms: i64,
}

#[derive(Debug, Default)]
pub struct CalendarPage {
    month: Option<Date>,
    day: Option<Date>,
    cache: Option<Cache>,
    pass: Option<u64>,
}

impl CalendarPage {
    fn refresh(&mut self, env: &Env<'_>) {
        let today = env.today;
        let month = self.month.get_or_insert(today.first_of_month());
        let day = *self.day.get_or_insert(today);
        let calendar = if env.atlas_on { Calendar::load(&env.atlas_file) } else { Calendar::default() };
        let tz = TimeZone::system();
        let weeks = calendar::month_weeks(*month);
        let counts = weeks
            .iter()
            .flatten()
            .map(|d| calendar::items_on(*d, &calendar, env.reminders, &tz).len())
            .collect();
        self.cache = Some(Cache {
            weeks,
            counts,
            agenda: calendar::items_on(day, &calendar, env.reminders, &tz),
            events: calendar.events.len(),
            tasks: calendar.tasks.len(),
            last_sync_ms: calendar.last_sync_ms,
        });
    }

    fn step_month(&mut self, months: i32, today: Date) {
        let month = self.month.unwrap_or(today.first_of_month());
        self.month =
            month.checked_add(i64::from(months).months()).ok().map(Date::first_of_month).or(Some(month));
        self.cache = None;
    }

    fn pick(&mut self, day: Date) {
        self.day = Some(day);
        if self.month.is_none_or(|m| m.month() != day.month() || m.year() != day.year()) {
            self.month = Some(day.first_of_month());
        }
        self.cache = None;
    }

    pub fn show(&mut self, ui: &mut Ui, rect: Rect, env: &mut Env<'_>, out: &mut Out) -> PageFrame {
        if returning(ui.ctx(), &mut self.pass) {
            self.cache = None;
        }
        if self.cache.is_none() {
            self.refresh(env);
        }
        let tokens = Tokens::of(ui.ctx());
        let p = &tokens.p;
        let today = env.today;
        let narrow = rect.width() < NARROW;
        let pad = if narrow { 12.0 } else { 24.0 };
        let area = Rect::from_min_max(rect.min + vec2(pad, 12.0), rect.max - vec2(pad, 18.0));
        let month = self.month.unwrap_or(today.first_of_month());
        let day = self.day.unwrap_or(today);

        let title = month.strftime("%B %Y").to_string();
        let title_room = if narrow { area.width() } else { area.width() - 330.0 };
        let galley = draw::line(ui, &title, 15.0, Weight::SemiBold, false, p.text_primary, title_room);
        ui.painter().galley(pos2(area.min.x, area.min.y + 7.0), galley, p.text_primary);
        // in a small box the buttons go on a line of their own under the month
        let controls = if narrow {
            Rect::from_min_max(pos2(area.min.x, area.min.y + 34.0), pos2(area.max.x, area.min.y + 68.0))
        } else {
            Rect::from_min_max(pos2(area.max.x - 330.0, area.min.y), pos2(area.max.x, area.min.y + 34.0))
        };
        let mut bar = ui.new_child(
            UiBuilder::new().max_rect(controls).layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        bar.spacing_mut().item_spacing.x = 8.0;
        if bar.add(Button::primary("Sync with Atlas")).clicked() {
            out.open_settings = Some("atlas_sync".to_owned());
        }
        if bar.add(IconButton::new("›").size(30.0).font_px(14.0).tooltip("Month after")).clicked() {
            self.step_month(1, today);
        }
        if bar.add(Button::ghost("Today")).clicked() {
            self.day = Some(today);
            self.month = Some(today.first_of_month());
            self.cache = None;
        }
        if bar.add(IconButton::new("‹").size(30.0).font_px(14.0).tooltip("Month before")).clicked() {
            self.step_month(-1, today);
        }

        let body_top = area.min.y + if narrow { 80.0 } else { 48.0 };
        let body = Rect::from_min_max(pos2(area.min.x, body_top), area.max);
        let cell_w = cell_width(body.width() - 8.0);
        let grid_w = (cell_w + 4.0) * 7.0 - 4.0;
        let mut picked = None;
        let mut child = ui.new_child(UiBuilder::new().max_rect(body));
        child.set_clip_rect(body.intersect(ui.clip_rect()));
        let Some(cache) = self.cache.as_ref() else { return PageFrame::default() };
        if narrow {
            egui::ScrollArea::vertical().id_salt("lum-calendar-stacked").auto_shrink([false, false]).show(
                &mut child,
                |ui| {
                    let (r, _) = ui.allocate_exact_size(
                        vec2(body.width() - 8.0, self.left_height(cache, grid_w, env)),
                        Sense::hover(),
                    );
                    picked = self.left(ui, r, cache, month, day, today, env, cell_w, &tokens);
                    let (agenda, _) = ui.allocate_exact_size(
                        vec2(body.width() - 8.0, agenda_height(cache, day, today) + 8.0),
                        Sense::hover(),
                    );
                    paint_agenda(ui, agenda, cache, day, today, &tokens);
                },
            );
        } else {
            let left = Rect::from_min_size(body.min, vec2(grid_w.max(420.0), body.height()));
            picked = self.left(&mut child, left, cache, month, day, today, env, cell_w, &tokens);
            let right = Rect::from_min_max(pos2(left.max.x + 24.0, body.min.y), body.max);
            let mut agenda_ui = ui.new_child(UiBuilder::new().max_rect(right));
            agenda_ui.set_clip_rect(right.intersect(ui.clip_rect()));
            egui::ScrollArea::vertical().id_salt("lum-calendar-agenda").auto_shrink([false, false]).show(
                &mut agenda_ui,
                |ui| {
                    let (r, _) = ui.allocate_exact_size(
                        vec2(right.width() - 8.0, agenda_height(cache, day, today)),
                        Sense::hover(),
                    );
                    paint_agenda(ui, r, cache, day, today, &tokens);
                },
            );
        }
        if let Some(day) = picked {
            self.pick(day);
        }
        PageFrame::default()
    }

    fn left_height(&self, cache: &Cache, _grid_w: f32, _env: &Env<'_>) -> f32 {
        20.0 + cache.weeks.len() as f32 * (CELL.1 + 4.0) + 20.0 + 40.0
    }

    #[allow(clippy::too_many_arguments)]
    fn left(
        &self,
        ui: &mut Ui,
        rect: Rect,
        cache: &Cache,
        month: Date,
        day: Date,
        today: Date,
        env: &Env<'_>,
        cell_w: f32,
        tokens: &Tokens,
    ) -> Option<Date> {
        let p = &tokens.p;
        let mut picked = None;
        let grid_w = (cell_w + 4.0) * 7.0 - 4.0;
        let left = rect.min.x;
        for (i, name) in WEEKDAYS.iter().enumerate() {
            let g = draw::caps(ui, name, 9.5, p.text_muted);
            let cx = left + i as f32 * (cell_w + 4.0) + cell_w / 2.0;
            ui.painter().galley(pos2(cx - g.size().x / 2.0, rect.min.y + 4.0), g, p.text_muted);
        }
        let mut n = 0;
        for (r, week) in cache.weeks.iter().enumerate() {
            for (c, d) in week.iter().enumerate() {
                let cell = Rect::from_min_size(
                    pos2(left + c as f32 * (cell_w + 4.0), rect.min.y + 20.0 + r as f32 * (CELL.1 + 4.0)),
                    vec2(cell_w, CELL.1),
                );
                let busy = cache.counts.get(n).copied().unwrap_or(0) > 0;
                n += 1;
                let (selected, is_today, other) = (*d == day, *d == today, d.month() != month.month());
                let name = format!("{}", d.strftime("%A %-d %B"));
                let response = ui.interact(
                    cell,
                    Id::unique(("lum-calendar-day", cell.min.x as i32, cell.min.y as i32)),
                    Sense::click(),
                );
                response.widget_info(|| WidgetInfo::selected(Role::Button, true, selected, name.clone()));
                if response.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                let fill = if selected {
                    p.primary_muted
                } else if response.hovered() {
                    p.bg_card_hover
                } else {
                    Color32::TRANSPARENT
                };
                ui.painter().rect_filled(cell, 10.0, fill);
                if is_today {
                    ui.painter().rect_stroke(
                        cell,
                        10.0,
                        Stroke::new(1.0, p.primary),
                        egui::StrokeKind::Inside,
                    );
                }
                let colour = if selected || is_today {
                    p.primary_text
                } else if other {
                    p.text_muted
                } else {
                    p.text_primary
                };
                let weight = if is_today { Weight::SemiBold } else { Weight::Regular };
                let number = ui.painter().layout_no_wrap(
                    d.day().to_string(),
                    tokens.font(14.0, weight, false),
                    colour,
                );
                ui.painter().galley(
                    pos2(cell.center().x - number.size().x / 2.0, cell.min.y + 8.0),
                    number,
                    colour,
                );
                if busy {
                    ui.painter().circle_filled(pos2(cell.center().x, cell.max.y - 11.0), 2.5, colour);
                }
                if response.clicked() {
                    picked = Some(*d);
                }
            }
        }
        let y = rect.min.y + 20.0 + cache.weeks.len() as f32 * (CELL.1 + 4.0) + 8.0;
        let line = if !env.atlas_on {
            "Turn on Atlas Sync (Settings → Atlas Sync) to see Atlas's calendar here.".to_owned()
        } else if cache.last_sync_ms > 0 {
            let at = jiff::Timestamp::from_millisecond(cache.last_sync_ms)
                .map(|t| {
                    t.to_zoned_datetime(TimeZone::system()).datetime().strftime("%d %b %H:%M").to_string()
                })
                .unwrap_or_default();
            format!("{} events and {} tasks from Atlas · last synced {at}", cache.events, cache.tasks)
        } else {
            "Not synced with Atlas yet. Press Sync with Atlas, then connect from Atlas → Sync.".to_owned()
        };
        let galley = draw::wrapped(ui, &line, 11.0, p.text_muted, grid_w.max(rect.width().min(420.0)));
        ui.painter().galley(pos2(rect.min.x, y), galley, p.text_muted);
        picked
    }
}

fn day_title(day: Date, today: Date) -> String {
    let relative = if day == today {
        Some("Today")
    } else if today.checked_add(1.day()).is_ok_and(|d| d == day) {
        Some("Tomorrow")
    } else if today.checked_sub(1.day()).is_ok_and(|d| d == day) {
        Some("Yesterday")
    } else {
        None
    };
    let long = day.strftime("%A %d %B").to_string().replace(" 0", " ");
    match relative {
        Some(word) => format!("{word} · {long}"),
        None => long,
    }
}

const AGENDA_ROW: f32 = 46.0;

fn agenda_height(cache: &Cache, _day: Date, _today: Date) -> f32 {
    28.0 + cache.agenda.len().max(1) as f32 * AGENDA_ROW
}

fn paint_agenda(ui: &mut Ui, rect: Rect, cache: &Cache, day: Date, today: Date, tokens: &Tokens) {
    let p = &tokens.p;
    let title =
        draw::line(ui, &day_title(day, today), 13.0, Weight::SemiBold, false, p.text_primary, rect.width());
    ui.painter().galley(rect.min, title, p.text_primary);
    if cache.agenda.is_empty() {
        ui.painter().text(
            rect.min + vec2(0.0, 32.0),
            Align2::LEFT_TOP,
            "Nothing on this day.",
            tokens.font(13.0, Weight::Regular, false),
            p.text_muted,
        );
        return;
    }
    for (i, item) in cache.agenda.iter().enumerate() {
        let r = Rect::from_min_size(
            rect.min + vec2(0.0, 28.0 + i as f32 * AGENDA_ROW),
            vec2(rect.width(), AGENDA_ROW - 6.0),
        );
        let row =
            ui.interact(r, Id::unique(("lum-calendar-row", r.min.x as i32, r.min.y as i32)), Sense::hover());
        row.widget_info(|| {
            WidgetInfo::labeled(Role::ListItem, true, format!("{} {}", item.when, item.title))
        });
        draw::card(ui, r, tokens);
        let colour = item.colour.as_deref().and_then(draw::hex).unwrap_or(match item.kind {
            Kind::Event => p.primary,
            Kind::Task | Kind::Done => p.text_secondary,
            Kind::Reminder => p.primary_text,
        });
        ui.painter().rect_filled(
            Rect::from_center_size(pos2(r.min.x + 14.0, r.center().y), vec2(4.0, 22.0)),
            2.0,
            colour,
        );
        let when = ui.painter().layout_no_wrap(
            item.when.clone(),
            tokens.font(11.5, Weight::Regular, true),
            p.text_secondary,
        );
        ui.painter().galley(pos2(r.min.x + 28.0, r.center().y - when.size().y / 2.0), when, p.text_secondary);
        let kind = draw::caps(ui, item.kind.label(), 9.0, p.text_muted);
        let kind_x = r.max.x - 12.0 - kind.size().x;
        ui.painter().galley(pos2(kind_x, r.center().y - kind.size().y / 2.0), kind, p.text_muted);
        let left = r.min.x + 28.0 + 64.0 + 12.0;
        let done = item.kind == Kind::Done;
        let mut job = LayoutJob::single_section(
            item.title.clone(),
            TextFormat {
                font_id: tokens.font(13.0, Weight::Regular, false),
                color: if done { p.text_muted } else { p.text_primary },
                strikethrough: if done { Stroke::new(1.0, p.text_muted) } else { Stroke::NONE },
                ..Default::default()
            },
        );
        job.wrap.max_width = (kind_x - left - 8.0).max(8.0);
        job.wrap.max_rows = 1;
        let galley = ui.painter().layout_job(job);
        ui.painter().galley(pos2(left, r.center().y - galley.size().y / 2.0), galley, p.text_primary);
    }
}
