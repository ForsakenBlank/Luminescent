//! The Today, Usage and Calendar pages: what you did today, what you use most, and Atlas's calendar with your
//! reminders. They are not tools with panels, only pages in the overlay and boxes on the dashboard, drawn from
//! the activity log, the usage counts and the calendar Atlas last sent (`lum_core::overview`,
//! `lum_calc::calendar`).

mod calendar;
mod today;
mod usage;

use egui::{Rect, Ui};
use jiff::civil::{Date, DateTime};
use lum_core::usage::Usage;

use crate::overlay::PageFrame;
use calendar::CalendarPage;
use today::TodayPage;
use usage::UsagePage;

/// What the pages need from the app.
pub struct Env<'a> {
    pub usage: &'a mut Usage,
    pub today: Date,
    pub now: DateTime,
    /// The reminders in Settings, when Reminders is on (the Calendar shows them next to Atlas's events).
    pub reminders: Option<&'a [serde_json::Value]>,
    /// Atlas Sync is turned on, so its stored calendar is shown.
    pub atlas_on: bool,
    /// `atlas_sync.json`.
    pub atlas_file: std::path::PathBuf,
}

/// What a page asks the app for.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Out {
    /// A Settings section to open ("atlas_sync").
    pub open_settings: Option<String>,
}

pub const KEYS: [&str; 3] = ["today", "stats", "calendar"];

#[derive(Debug, Default)]
pub struct Overview {
    today: TodayPage,
    usage: UsagePage,
    calendar: CalendarPage,
    out: Out,
}

impl Overview {
    pub fn new() -> Overview {
        Overview::default()
    }

    pub fn handles(key: &str) -> bool {
        KEYS.contains(&key)
    }

    /// Draws page `key` into `rect`. Gives `busy` while a question is open.
    pub fn show(&mut self, key: &str, ui: &mut Ui, rect: Rect, env: &mut Env<'_>) -> PageFrame {
        match key {
            "today" => self.today.show(ui, rect, env),
            "stats" => self.usage.show(ui, rect, env),
            "calendar" => self.calendar.show(ui, rect, env, &mut self.out),
            _ => PageFrame::default(),
        }
    }

    /// A page that panicked starts again from scratch.
    pub fn reset(&mut self, key: &str) {
        match key {
            "today" => self.today = TodayPage::default(),
            "stats" => self.usage = UsagePage::default(),
            "calendar" => self.calendar = CalendarPage::default(),
            _ => {}
        }
    }

    pub fn take_out(&mut self) -> Out {
        std::mem::take(&mut self.out)
    }
}

/// Whether the page has come back after being away: it was not drawn in the pass before this one.
pub(crate) fn returning(ctx: &egui::Context, last: &mut Option<u64>) -> bool {
    let pass = ctx.cumulative_pass_nr();
    let back = last.is_none_or(|l| l + 1 < pass);
    *last = Some(pass);
    back
}
