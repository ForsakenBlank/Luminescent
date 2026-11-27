//! Reminders: pop-ups, sounds and notifications at a set time. The definitions live in Settings (`reminders.items`,
//! edited in Settings → Reminders); what fired last and what is snoozed lives in `reminders_state.json`. The
//! schedule maths and the state are in `lum_calc::reminder`; this module wakes at the moment the next one is due.

mod popup;

use std::time::{Duration, Instant};

use jiff::ZonedDateTime;
use jiff::civil::DateTime;
use lum_calc::reminder::{Edit, State, describe, parse_quick, quick_reminder};
use lum_core::actions::Action;
use lum_core::module::{Module, ModuleCx, StartError};
use lum_core::notes::TodoSync;
use lum_core::settings::Settings;
use serde_json::{Value, json};

use crate::palette::Entry;
use crate::{Inbox, Request, Services, UiCx, UiModule};
use popup::{Answer, Popup};

/// The longest the module sleeps without looking at the clock, so a change of time or a wake from sleep is
/// noticed within the minute.
const LONGEST_NAP: Duration = Duration::from_secs(60);
const SOUND_GAP: Duration = Duration::from_secs(3);

#[derive(Debug)]
enum Cmd {
    OpenSettings,
    /// Fire this reminder as a test: no event, no notification, the pop-up and the sound only.
    Preview(Value),
}

pub struct RemindersModule {
    services: Services,
    inbox: Inbox<Cmd>,
    state: Option<State>,
    popups: Vec<Popup>,
    serial: u64,
    /// Startup reminders go off once per run of the app, not once per switching the module on.
    startup_done: bool,
    startup_due: Vec<(Instant, String)>,
    sounds: Vec<(Instant, String, String)>,
    check_at: Instant,
    seen_items: Value,
    next: Option<(DateTime, String)>,
}

impl std::fmt::Debug for RemindersModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemindersModule").field("popups", &self.popups.len()).finish_non_exhaustive()
    }
}

fn now() -> DateTime {
    ZonedDateTime::now().datetime()
}

fn items(settings: &Settings) -> &[Value] {
    settings.get("reminders.items").and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

fn whole(settings: &Settings, key: &str, default: i64) -> i64 {
    settings.get_i64(&format!("reminders.{key}")).unwrap_or(default)
}

fn find<'a>(items: &'a [Value], id: &str) -> Option<&'a Value> {
    items.iter().find(|r| r.get("id").and_then(Value::as_str) == Some(id))
}

impl RemindersModule {
    pub fn new(services: &Services) -> RemindersModule {
        RemindersModule {
            services: services.clone(),
            inbox: Inbox::new(services),
            state: None,
            popups: Vec::new(),
            serial: 0,
            startup_done: false,
            startup_due: Vec::new(),
            sounds: Vec::new(),
            check_at: Instant::now(),
            seen_items: Value::Null,
            next: None,
        }
    }

    /// Looks at the list when it was edited, and at the clock when something may be due.
    fn look(&mut self, settings: &Settings, at: DateTime) {
        let list = items(settings);
        let Some(state) = self.state.as_mut() else { return };
        let edited = settings.get("reminders.items").unwrap_or(&Value::Null) != &self.seen_items;
        if edited {
            self.seen_items = settings.get("reminders.items").cloned().unwrap_or(Value::Null);
            state.prime(list, at);
            state.save();
        }
        if !edited && Instant::now() < self.check_at {
            return;
        }
        let (fired, changed) = state.tick(
            list,
            at,
            whole(settings, "catch_up_minutes", 120),
            whole(settings, "snooze_minutes", 10),
        );
        let catch_up = whole(settings, "catch_up_minutes", 120);
        let wake = state.next_wake(list, at, catch_up);
        self.next = state.next_reminder(list, at);
        if changed {
            state.save();
        }
        let nap = wake.map_or(LONGEST_NAP, |due| {
            let secs = (due - at).total(jiff::Unit::Second).unwrap_or(60.0).max(0.0);
            Duration::from_secs_f64(secs + 0.25).min(LONGEST_NAP)
        });
        self.check_at = Instant::now() + nap;
        for f in fired {
            if let Some(rem) = find(list, &f.id) {
                self.fire(rem.clone(), settings, false);
            }
            if f.once {
                self.services.send(Request::ReminderEdit(Edit::Disable(f.id)));
            }
        }
    }

    /// A reminder goes off: the event, a theme switch, the sound, the notification and the pop-up.
    fn fire(&mut self, rem: Value, settings: &Settings, preview: bool) {
        let title = rem.get("title").and_then(Value::as_str).unwrap_or_default().to_owned();
        let acts = rem.get("actions").cloned().unwrap_or_else(|| json!({}));
        tracing::info!("firing {title:?}");
        if !preview {
            self.services.send(Request::Emit { name: "reminder.fired".into(), text: title.clone() });
        }
        if let Some(theme) = acts.get("theme").and_then(Value::as_str).filter(|t| !t.is_empty()) {
            self.services.send(Request::ApplyTheme(theme.to_owned()));
        }
        let mut kind = acts.get("sound").and_then(Value::as_str).unwrap_or("default").to_owned();
        let mut file = acts.get("sound_file").and_then(Value::as_str).unwrap_or_default().to_owned();
        if kind == "default" {
            kind = settings.get_str("reminders.default_sound").unwrap_or("chime").to_owned();
            file = settings.get_str("reminders.default_sound_file").unwrap_or_default().to_owned();
        }
        let repeat = acts.get("sound_repeat").and_then(Value::as_i64).unwrap_or(1).max(1);
        for i in 0..repeat {
            self.sounds.push((Instant::now() + SOUND_GAP * i as u32, kind.clone(), file.clone()));
        }
        let popup = acts.get("popup").and_then(Value::as_bool).unwrap_or(true);
        if acts.get("toast").and_then(Value::as_bool).unwrap_or(true) && !preview {
            let message = rem.get("message").and_then(Value::as_str).filter(|m| !m.is_empty());
            let text = message.map_or_else(|| describe(&rem), str::to_owned);
            let title = if title.is_empty() { "Reminder".to_owned() } else { title };
            // with the pop-up on, only the desktop notification (for its history): a card of ours as well
            // stacked a second, smaller copy of the same reminder in the corner
            self.services.send(if popup {
                Request::SystemNotice { title, text, ms: 8000 }
            } else {
                Request::Toast { title, text, ms: 8000 }
            });
        }
        if popup {
            self.show_popup(rem, settings);
        }
    }

    fn show_popup(&mut self, rem: Value, settings: &Settings) {
        let mut anchor =
            rem["actions"].get("position").and_then(Value::as_str).unwrap_or("default").to_owned();
        if anchor == "default" {
            anchor = settings.get_str("reminders.popup_position").unwrap_or("br").to_owned();
        }
        let screens = crate::window::screens();
        let at = if settings.get_str("reminders.popup_screen") == Some("cursor") {
            crate::window::cursor_screen(&screens).or((!screens.is_empty()).then_some(0))
        } else {
            (!screens.is_empty()).then_some(0)
        };
        let screen = at
            .map_or(egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 800.0)), |i| {
                screens[i].work
            });
        self.serial += 1;
        let snooze = whole(settings, "snooze_minutes", 10);
        let timeout = whole(settings, "popup_timeout", 0).max(0) as u64;
        let time = now().strftime("%H:%M").to_string();
        self.popups.push(Popup::new(self.serial, rem, snooze, timeout, &anchor, screen, &time));
    }

    fn answer(&mut self, serial: u64, answer: Answer) {
        let Some(at) = self.popups.iter().position(|p| p.serial == serial) else { return };
        let popup = self.popups.remove(at);
        if let Answer::Snooze(minutes) = answer
            && let Some(id) = popup.rem.get("id").and_then(Value::as_str)
            && let Some(state) = self.state.as_mut()
        {
            let until = now().checked_add(jiff::Span::new().minutes(minutes)).unwrap_or_else(|_| now());
            state.snooze(id, until);
            state.save();
            self.check_at = Instant::now();
        }
    }

    fn start_startup_reminders(&mut self, settings: &Settings) {
        if std::mem::replace(&mut self.startup_done, true) {
            return;
        }
        for rem in items(settings) {
            let on = rem.get("enabled").and_then(Value::as_bool).unwrap_or(true);
            let sch = &rem["schedule"];
            if on && sch.get("type").and_then(Value::as_str) == Some("startup") {
                let delay = sch.get("startup_delay").and_then(Value::as_i64).unwrap_or(10).max(1) as u64;
                if let Some(id) = rem.get("id").and_then(Value::as_str) {
                    self.startup_due.push((Instant::now() + Duration::from_secs(delay), id.to_owned()));
                }
            }
        }
    }

    fn run_due_sounds_and_startups(&mut self, settings: &Settings, ctx: &egui::Context) {
        let at = Instant::now();
        let (due, later): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.sounds).into_iter().partition(|(when, ..)| *when <= at);
        self.sounds = later;
        for (_, kind, file) in due {
            lum_platform::sounds::play(&kind, &file, 1.0);
        }
        let (due, later): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.startup_due).into_iter().partition(|(when, _)| *when <= at);
        self.startup_due = later;
        for (_, id) in due {
            if let Some(rem) = find(items(settings), &id) {
                self.fire(rem.clone(), settings, false);
            }
        }
        let soonest =
            self.sounds.iter().map(|(w, ..)| *w).chain(self.startup_due.iter().map(|(w, _)| *w)).min();
        if let Some(when) = soonest {
            ctx.request_repaint_after(when.saturating_duration_since(at) + Duration::from_millis(20));
        }
    }

    fn take_commands(&mut self, settings: &Settings) {
        for cmd in self.inbox.take() {
            match cmd {
                Cmd::OpenSettings => self.services.send(Request::OpenSettings("reminders".into())),
                Cmd::Preview(rem) => self.fire(rem, settings, true),
            }
        }
    }

    /// Adds a reminder another part of the app asked for, and starts its clock.
    fn add_quick(&mut self, title: &str, message: &str, when: DateTime, id: &str) {
        let rem = quick_reminder(title, message, when, id, &uuid_hex());
        if let Some(state) = self.state.as_mut() {
            state.started(&rem, now());
            state.save();
        }
        self.services.send(Request::ReminderEdit(Edit::Add(rem)));
        self.check_at = Instant::now();
    }
}

fn uuid_hex() -> String {
    lum_core::notes::new_id() + &lum_core::notes::new_id() + &lum_core::notes::new_id()
}

impl Module for RemindersModule {
    fn key(&self) -> &'static str {
        "reminders"
    }

    fn title(&self) -> &'static str {
        "Reminders"
    }

    fn start(&mut self, cx: &mut ModuleCx<'_>) -> Result<(), StartError> {
        let mut state = State::load(&self.services.data_file("reminders_state.json"));
        let list = items(cx.settings);
        state.prime(list, now());
        state.save();
        self.seen_items = cx.settings.get("reminders.items").cloned().unwrap_or(Value::Null);
        self.state = Some(state);
        self.check_at = Instant::now() + Duration::from_millis(1500);
        self.start_startup_reminders(cx.settings);
        tracing::info!("{} reminder(s) loaded", items(cx.settings).len());
        Ok(())
    }

    fn stop(&mut self, _cx: &mut ModuleCx<'_>) {
        self.popups.clear();
        self.sounds.clear();
        self.next = None;
        if let Some(state) = self.state.take() {
            state.save();
            state.flush();
        }
    }

    /// Edits to the list are noticed by the next look; nothing has to restart.
    fn apply_settings(&mut self, _cx: &mut ModuleCx<'_>) -> Result<(), StartError> {
        self.check_at = Instant::now();
        Ok(())
    }

    fn actions(&self) -> Vec<Action> {
        vec![self.inbox.action("settings", "Reminders…", || Cmd::OpenSettings)]
    }
}

impl UiModule for RemindersModule {
    fn tray_actions(&self) -> Vec<(String, String)> {
        vec![("settings".into(), "Reminders…".into())]
    }

    fn chips(&self) -> Vec<(String, String)> {
        let Some((when, title)) = &self.next else { return Vec::new() };
        let day = if when.date() == now().date() { String::new() } else { when.strftime("%a ").to_string() };
        vec![("⏰".to_owned(), format!("{day}{}  {title}", when.strftime("%H:%M")))]
    }

    fn show(&mut self, cx: &mut UiCx<'_>) {
        self.take_commands(cx.settings);
        if self.state.is_none() {
            return;
        }
        let at = now();
        self.look(cx.settings, at);
        self.run_due_sounds_and_startups(cx.settings, cx.ctx);
        let mut answers = Vec::new();
        let mut offsets: Vec<(String, f32)> = Vec::new();
        for popup in &mut self.popups {
            let used = offsets.iter().position(|(anchor, _)| *anchor == popup.anchor);
            let offset = used.map_or(0.0, |i| offsets[i].1);
            let place = popup.place(offset);
            if let Some(answer) = popup.show(cx.ctx, place) {
                answers.push((popup.serial, answer));
            }
            let next = offset + popup.height + 8.0;
            match used {
                Some(i) => offsets[i].1 = next,
                None => offsets.push((popup.anchor.clone(), next)),
            }
        }
        for (serial, answer) in answers {
            self.answer(serial, answer);
        }
        let wait = self.check_at.saturating_duration_since(Instant::now());
        cx.ctx.request_repaint_after(wait + Duration::from_millis(20));
    }

    fn palette_live(&self, text: &str) -> Vec<Entry> {
        let Some((when, title)) = parse_quick(text, now()) else { return Vec::new() };
        let today = now().date();
        let day = if when.date() == today { String::new() } else { when.strftime("%a %d %b ").to_string() };
        let iso = when.strftime("%Y-%m-%dT%H:%M:%S");
        vec![Entry::new(
            format!("remind:quick:{iso}:{title}"),
            format!("Remind me: {title}"),
            format!("REMINDER \u{b7} {day}{}", when.strftime("%H:%M")),
            "reminder",
        )]
    }

    fn run_palette(&mut self, id: &str, services: &Services) -> bool {
        let Some(rest) = id.strip_prefix("remind:quick:") else { return id.starts_with("remind:") };
        let Some((iso, title)) = rest.split_at_checked(19) else { return true };
        let (Ok(when), Some(title)) = (iso.parse::<DateTime>(), title.strip_prefix(':')) else { return true };
        self.add_quick(title, "", when, "");
        let day = if when.date() == now().date() {
            String::new()
        } else {
            when.strftime(" on %a %d %b").to_string()
        };
        services.send(Request::Notify {
            title: "Reminders".into(),
            text: format!("{title} at {}{day}", when.strftime("%H:%M")),
            ms: 3000,
        });
        true
    }

    fn todo_reminder(&mut self, sync: &TodoSync) -> bool {
        if self.state.is_none() {
            return false;
        }
        match sync {
            TodoSync::Set { id, title, when, message } => self.add_quick(title, message, *when, id),
            TodoSync::Drop { id } => {
                if let Some(state) = self.state.as_mut() {
                    state.forget(id);
                    state.save();
                }
                self.services.send(Request::ReminderEdit(Edit::Remove(id.clone())));
            }
        }
        true
    }

    fn preview_reminder(&mut self, rem: &Value) -> bool {
        self.inbox.push(Cmd::Preview(rem.clone()));
        true
    }
}
