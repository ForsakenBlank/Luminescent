//! The editor for `reminders.items` (`RemindersEditor`): the reminders in a list on the left, and the one picked
//! as three cards on the right: what it says, when it goes off and how it tells you. It changes only the keys
//! that were edited, so a reminder keeps whatever else it has and its schedule keeps the signature the state
//! file recorded.

use egui::{Align, Id, Layout, Rect, Sense, Ui, UiBuilder, pos2, vec2};
use jiff::civil::Time;
use lum_calc::reminder::{DAY_NAMES, describe, new_reminder, parse_date, parse_time};
use lum_core::settings::{Field, Section};
use lum_platform::dialogs::Request;
use serde_json::{Value, json};

use super::{Env, FileAsk, FileJob, SettingsUi};
use crate::text::{Text, caps};
use crate::theme::{Tokens, Tone};
use crate::widgets::{Button, Checkbox, Combo, SpinBox, TextArea, TextInput, Variant, card};

const REPEAT: [(&str, &str); 7] = [
    ("once", "Once (a specific day)"),
    ("daily", "Every day"),
    ("weekly", "Certain weekdays"),
    ("monthly", "Every month"),
    ("every_days", "Every N days"),
    ("interval", "Every N minutes"),
    ("startup", "When the launcher starts (PC startup)"),
];

const POSITIONS: [(&str, &str); 9] = [
    ("tl", "Top left"),
    ("tc", "Top centre"),
    ("tr", "Top right"),
    ("ml", "Middle left"),
    ("mc", "Centre of screen"),
    ("mr", "Middle right"),
    ("bl", "Bottom left"),
    ("bc", "Bottom centre"),
    ("br", "Bottom right"),
];

const SOUNDS: [(&str, &str); 4] =
    [("none", "No sound"), ("chime", "Chime"), ("alarm", "Alarm"), ("custom", "Custom sound file")];

/// What the editor remembers between frames: which reminder is open.
#[derive(Debug, Default)]
pub struct RemindersEditor {
    pub current: Option<String>,
}

fn id_of(rem: &Value) -> String {
    rem.get("id").and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn text<'a>(map: &'a Value, key: &str, default: &'a str) -> &'a str {
    map.get(key).and_then(Value::as_str).unwrap_or(default)
}

fn whole(map: &Value, key: &str, default: i64) -> i64 {
    map.get(key).and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64))).unwrap_or(default)
}

fn flag(map: &Value, key: &str, default: bool) -> bool {
    map.get(key).and_then(Value::as_bool).unwrap_or(default)
}

/// One line of the list: a dot for on or off, the title and the schedule in words.
fn line(rem: &Value) -> (String, String) {
    let dot = if flag(rem, "enabled", true) { "\u{25cf}" } else { "\u{25cb}" };
    let title = text(rem, "title", "");
    (format!("{dot}  {}", if title.is_empty() { "(untitled)" } else { title }), describe(rem))
}

/// `9:30` or `09:30`.
fn valid_time(text: &str) -> bool {
    let Some((hour, minute)) = text.trim().split_once(':') else { return false };
    let digits = |s: &str, len: std::ops::RangeInclusive<usize>| {
        len.contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit())
    };
    digits(hour, 1..=2)
        && digits(minute, 2..=2)
        && hour.parse::<u8>().is_ok_and(|h| h < 24)
        && minute.parse::<u8>().is_ok_and(|m| m < 60)
}

fn valid_date(text: &str) -> bool {
    parse_date(text).is_some()
}

/// A text box for a date or a time: what is typed stays until it is valid, then it is written to the reminder.
fn checked_text(
    ui: &mut Ui,
    id: Id,
    name: &str,
    current: &str,
    valid: fn(&str) -> bool,
    width: f32,
) -> Option<String> {
    let focused = ui.memory(|m| m.has_focus(id.with("box")));
    let mut buffer: String = ui.data_mut(|d| d.get_temp(id)).unwrap_or_else(|| current.to_owned());
    if !focused && buffer != current {
        buffer = current.to_owned();
    }
    let invalid = !valid(&buffer);
    let response =
        ui.add(TextInput::new(&mut buffer).id(id.with("box")).name(name).width(width).invalid(invalid));
    let changed = response.changed();
    ui.data_mut(|d| d.insert_temp(id, buffer.clone()));
    (changed && valid(&buffer)).then_some(buffer.trim().to_owned())
}

fn form_row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(112.0, 24.0), Sense::hover());
        ui.painter().text(
            rect.left_center(),
            egui::Align2::LEFT_CENTER,
            label,
            Tokens::of(ui.ctx()).font(13.0, crate::fonts::Weight::Regular, false),
            Tokens::of(ui.ctx()).p.text_secondary,
        );
        add(ui);
    });
}

fn heading(ui: &mut Ui, title: &str) {
    ui.add(caps(title).px(9.5).spacing(0.12).tone(Tone::Muted));
}

impl SettingsUi {
    pub(super) fn reminders_editor(&mut self, ui: &mut Ui, section: &Section, field: &Field, env: &Env<'_>) {
        let mut items: Vec<Value> =
            self.value(&section.key, &field.key).and_then(Value::as_array).cloned().unwrap_or_default();
        let before = items.clone();
        let current = self
            .reminders
            .current
            .clone()
            .filter(|id| items.iter().any(|r| id_of(r) == *id))
            .or_else(|| items.first().map(id_of));
        self.reminders.current = current.clone();
        let wide = ui.available_width() >= 620.0;
        let mut preview = None;
        let mut browse = None;
        if wide {
            // two columns laid out by hand: the list is a fixed 250 wide and the form takes the rest
            let room = ui.available_rect_before_wrap();
            let tall = 20_000.0;
            let left = Rect::from_min_size(room.min, vec2(250.0, tall));
            let right = Rect::from_min_max(room.min + vec2(262.0, 0.0), pos2(room.max.x, room.min.y + tall));
            let mut list_ui = ui.new_child(UiBuilder::new().max_rect(left));
            self.reminder_list(&mut list_ui, &mut items);
            let mut form_ui = ui.new_child(UiBuilder::new().max_rect(right));
            let current = self.reminders.current.clone();
            form(&mut form_ui, &mut items, current.as_deref(), env, &mut preview, &mut browse);
            let bottom = list_ui.min_rect().max.y.max(form_ui.min_rect().max.y);
            ui.allocate_rect(Rect::from_min_max(room.min, pos2(room.max.x, bottom)), Sense::hover());
        } else {
            self.reminder_list(ui, &mut items);
            ui.add_space(8.0);
            let current = self.reminders.current.clone();
            form(ui, &mut items, current.as_deref(), env, &mut preview, &mut browse);
        }
        if items != before {
            self.set_value(&section.key, &field.key, Value::Array(items));
        }
        if let Some(rem) = preview {
            self.out.preview_reminder = Some(rem);
        }
        if let Some(id) = browse {
            let request = Request::new("Choose a sound")
                .folder(Some(lum_core::paths::user_home()))
                .filter("Audio", &["wav", "mp3", "ogg", "flac", "m4a"]);
            self.out.files.push(FileAsk {
                job: FileJob::ReminderSound(id),
                save: false,
                folder: false,
                request,
            });
        }
    }

    fn reminder_list(&mut self, ui: &mut Ui, items: &mut Vec<Value>) {
        if items.is_empty() {
            ui.add(Text::new("No reminders yet. Press Add to make one.").px(12.0).tone(Tone::Muted));
        }
        for rem in items.iter() {
            let id = id_of(rem);
            let (title, when) = line(rem);
            let on = self.reminders.current.as_deref() == Some(id.as_str());
            let response = crate::widgets::row(ui, on, |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.add(Text::new(title).px(13.0).medium());
                    ui.add(Text::new(when).px(11.0).tone(Tone::Muted));
                });
            });
            if response.response.clicked() {
                self.reminders.current = Some(id);
            }
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if ui.add(Button::primary("+ Add")).clicked() {
                let id = lum_core::notes::new_id() + &lum_core::notes::new_id() + &lum_core::notes::new_id();
                let rem = new_reminder(jiff::ZonedDateTime::now().date(), &id);
                items.push(rem);
                self.reminders.current = Some(id);
            }
            let at = items.iter().position(|r| Some(id_of(r)) == self.reminders.current);
            if ui.add_enabled(at.is_some(), Button::new("Duplicate")).clicked()
                && let Some(i) = at
            {
                let mut copy = items[i].clone();
                let id = lum_core::notes::new_id() + &lum_core::notes::new_id() + &lum_core::notes::new_id();
                copy["id"] = json!(id);
                let title = text(&copy, "title", "").to_owned();
                copy["title"] = json!(format!("{title} (copy)"));
                items.insert(i + 1, copy);
                self.reminders.current = Some(id);
            }
            if ui.add_enabled(at.is_some(), Button::new("Delete").variant(Variant::Danger)).clicked()
                && let Some(i) = at
            {
                items.remove(i);
                self.reminders.current = items.get(i.min(items.len().saturating_sub(1))).map(id_of);
            }
        });
    }
}

fn set(rem: &mut Value, section: &str, key: &str, value: Value) {
    if section.is_empty() {
        rem[key] = value;
        return;
    }
    if !rem.get(section).is_some_and(Value::is_object) {
        rem[section] = json!({});
    }
    rem[section][key] = value;
}

fn form(
    ui: &mut Ui,
    items: &mut [Value],
    current: Option<&str>,
    env: &Env<'_>,
    preview: &mut Option<Value>,
    browse: &mut Option<String>,
) {
    let Some(rem) = items.iter_mut().find(|r| Some(id_of(r).as_str()) == current) else { return };
    let id = Id::unique(("reminder-editor", id_of(rem)));
    ui.spacing_mut().item_spacing.y = 10.0;

    card(ui, |ui| {
        ui.set_width(ui.available_width());
        heading(ui, "REMINDER");
        let mut on = flag(rem, "enabled", true);
        if ui.add(Checkbox::new(&mut on, "Enabled")).changed() {
            set(rem, "", "enabled", json!(on));
        }
        form_row(ui, "Title", |ui| {
            let mut title = text(rem, "title", "").to_owned();
            if ui
                .add(
                    TextInput::new(&mut title)
                        .id(id.with("title"))
                        .name("Reminder title")
                        .width(ui.available_width()),
                )
                .changed()
            {
                set(rem, "", "title", json!(title));
            }
        });
        form_row(ui, "Message", |ui| {
            let mut message = text(rem, "message", "").to_owned();
            let area = TextArea::new(&mut message).rows(2).hint("Optional longer text shown in the pop-up");
            if ui.add(area).changed() {
                set(rem, "", "message", json!(message));
            }
        });
    });

    card(ui, |ui| {
        ui.set_width(ui.available_width());
        heading(ui, "WHEN");
        let schedule = rem.get("schedule").cloned().unwrap_or_else(|| json!({}));
        let kind = text(&schedule, "type", "daily").to_owned();
        let mut at = REPEAT.iter().position(|(k, _)| *k == kind).unwrap_or(1);
        let labels: Vec<&str> = REPEAT.iter().map(|(_, l)| *l).collect();
        form_row(ui, "Repeat", |ui| {
            if ui.add(Combo::new(id.with("repeat"), &mut at, &labels).width(260.0)).changed() {
                set(rem, "schedule", "type", json!(REPEAT[at].0));
            }
        });
        let kind = REPEAT[at].0;
        if matches!(kind, "once" | "every_days") {
            form_row(ui, "Date", |ui| {
                let now = text(&schedule, "date", "");
                if let Some(date) = checked_text(ui, id.with("date"), "Date", now, valid_date, 140.0) {
                    set(rem, "schedule", "date", json!(date));
                }
            });
        }
        if matches!(kind, "once" | "daily" | "weekly" | "monthly" | "every_days") {
            form_row(ui, "Time", |ui| {
                let now = text(&schedule, "time", "09:00");
                if let Some(typed) = checked_text(
                    ui,
                    id.with("time"),
                    "Time",
                    &now.chars().take(5).collect::<String>(),
                    valid_time,
                    80.0,
                ) {
                    let hhmm = parse_time(&typed, Time::constant(9, 0, 0, 0)).strftime("%H:%M").to_string();
                    // quick reminders keep their seconds unless the time was changed here
                    if !now.starts_with(&hhmm) {
                        set(rem, "schedule", "time", json!(hhmm));
                    }
                }
            });
        }
        if kind == "weekly" {
            form_row(ui, "Days", |ui| {
                let mut days: Vec<i64> = schedule
                    .get("days")
                    .and_then(Value::as_array)
                    .map(|l| l.iter().filter_map(Value::as_i64).collect())
                    .unwrap_or_default();
                let before = days.clone();
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    for (i, name) in DAY_NAMES.iter().enumerate() {
                        let mut on = days.contains(&(i as i64));
                        if ui.add(Checkbox::new(&mut on, *name)).changed() {
                            days.retain(|d| *d != i as i64);
                            if on {
                                days.push(i as i64);
                            }
                        }
                    }
                });
                if days != before {
                    days.sort_unstable();
                    set(rem, "schedule", "days", json!(days));
                }
            });
        }
        if kind == "interval" {
            form_row(ui, "Every", |ui| {
                let mut every = whole(&schedule, "every_minutes", 60) as f64;
                if ui
                    .add(
                        SpinBox::new(&mut every, 1.0..=1440.0)
                            .suffix(" minutes")
                            .width(150.0)
                            .name("Minutes between"),
                    )
                    .changed()
                {
                    set(rem, "schedule", "every_minutes", json!(every as i64));
                }
            });
            form_row(ui, "", |ui| {
                let mut window = flag(&schedule, "window", false);
                if ui.add(Checkbox::new(&mut window, "Only between")).changed() {
                    set(rem, "schedule", "window", json!(window));
                }
                for (key, default) in [("from", "09:00"), ("to", "18:00")] {
                    if key == "to" {
                        ui.add(Text::new("and").px(13.0));
                    }
                    let now = text(&schedule, key, default);
                    if let Some(typed) = checked_text(
                        ui,
                        id.with(key),
                        if key == "from" { "From" } else { "To" },
                        now,
                        valid_time,
                        86.0,
                    ) {
                        let hhmm =
                            parse_time(&typed, Time::constant(9, 0, 0, 0)).strftime("%H:%M").to_string();
                        set(rem, "schedule", key, json!(hhmm));
                    }
                }
            });
        }
        if kind == "monthly" {
            form_row(ui, "On", |ui| {
                let mut day = whole(&schedule, "monthday", 1) as f64;
                let spin = SpinBox::new(&mut day, 1.0..=31.0).width(110.0).name("Day of the month");
                if ui
                    .add(spin)
                    .on_hover_text("29, 30 and 31 fire on the last day of shorter months")
                    .changed()
                {
                    set(rem, "schedule", "monthday", json!(day as i64));
                }
            });
        }
        if kind == "every_days" {
            form_row(ui, "Every", |ui| {
                let mut every = whole(&schedule, "every_days", 2) as f64;
                if ui
                    .add(
                        SpinBox::new(&mut every, 1.0..=365.0)
                            .suffix(" days")
                            .width(130.0)
                            .name("Days between"),
                    )
                    .changed()
                {
                    set(rem, "schedule", "every_days", json!(every as i64));
                }
            });
        }
        if kind == "startup" {
            form_row(ui, "Show", |ui| {
                let mut delay = whole(&schedule, "startup_delay", 10) as f64;
                if ui
                    .add(
                        SpinBox::new(&mut delay, 1.0..=600.0)
                            .suffix(" seconds after start")
                            .width(210.0)
                            .name("Delay"),
                    )
                    .changed()
                {
                    set(rem, "schedule", "startup_delay", json!(delay as i64));
                }
            });
        }
    });

    card(ui, |ui| {
        ui.set_width(ui.available_width());
        heading(ui, "HOW IT TELLS YOU");
        let actions = rem.get("actions").cloned().unwrap_or_else(|| json!({}));
        let mut popup = flag(&actions, "popup", true);
        if ui.add(Checkbox::new(&mut popup, "Show a pop-up window")).changed() {
            set(rem, "actions", "popup", json!(popup));
        }
        form_row(ui, "Pop-up position", |ui| {
            let mut labels = vec!["Default (from settings below)"];
            labels.extend(POSITIONS.iter().map(|(_, l)| *l));
            let key = text(&actions, "position", "default");
            let mut at = POSITIONS.iter().position(|(k, _)| *k == key).map_or(0, |i| i + 1);
            if ui.add(Combo::new(id.with("position"), &mut at, &labels).width(260.0)).changed() {
                set(rem, "actions", "position", json!(if at == 0 { "default" } else { POSITIONS[at - 1].0 }));
            }
        });
        let mut toast = flag(&actions, "toast", true);
        if ui.add(Checkbox::new(&mut toast, "Desktop notification")).changed() {
            set(rem, "actions", "toast", json!(toast));
        }
        let sound = text(&actions, "sound", "default").to_owned();
        form_row(ui, "Sound", |ui| {
            let mut labels = vec!["Default (from settings below)"];
            labels.extend(SOUNDS.iter().map(|(_, l)| *l));
            let mut at = SOUNDS.iter().position(|(k, _)| *k == sound).map_or(0, |i| i + 1);
            if ui.add(Combo::new(id.with("sound"), &mut at, &labels).width(260.0)).changed() {
                set(rem, "actions", "sound", json!(if at == 0 { "default" } else { SOUNDS[at - 1].0 }));
            }
        });
        if sound == "custom" {
            form_row(ui, "Sound file", |ui| {
                let mut file = text(&actions, "sound_file", "").to_owned();
                let width = (ui.available_width() - 96.0).max(120.0);
                if ui
                    .add(
                        TextInput::new(&mut file)
                            .id(id.with("file"))
                            .hint("Path to .wav / .mp3")
                            .width(width),
                    )
                    .changed()
                {
                    set(rem, "actions", "sound_file", json!(file.trim()));
                }
                if ui.add(Button::new("Browse\u{2026}")).clicked() {
                    *browse = Some(id_of(rem));
                }
            });
        }
        form_row(ui, "Play sound", |ui| {
            let mut times = whole(&actions, "sound_repeat", 1) as f64;
            if ui
                .add(
                    SpinBox::new(&mut times, 1.0..=10.0)
                        .suffix(" time(s)")
                        .width(130.0)
                        .name("Times to play the sound"),
                )
                .changed()
            {
                set(rem, "actions", "sound_repeat", json!(times as i64));
            }
        });
        form_row(ui, "Theme", |ui| {
            let themes = (env.themes)();
            let chosen = text(&actions, "theme", "").to_owned();
            let mut labels = vec!["(don't change the theme)".to_owned()];
            labels.extend(themes.iter().map(|(_, name)| format!("Switch to: {name}")));
            let mut at = themes.iter().position(|(tid, _)| *tid == chosen).map_or(0, |i| i + 1);
            if at == 0 && !chosen.is_empty() {
                labels.push("Switch to: (deleted theme)".to_owned());
                at = labels.len() - 1;
            }
            if ui.add(Combo::new(id.with("theme"), &mut at, &labels).width(260.0)).changed() {
                let value = if at == 0 {
                    String::new()
                } else {
                    themes.get(at - 1).map_or(chosen.clone(), |(tid, _)| tid.clone())
                };
                set(rem, "actions", "theme", json!(value));
            }
        });
        ui.add(
            Text::new("Untick the pop-up and notification for a silent theme switch, for example to Light at 08:00 and Dark at 20:00.")
                .px(11.0)
                .tone(Tone::Muted)
                .wrap(),
        );
    });

    ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
        if ui.add(Button::new("\u{25b6} Test this reminder now")).clicked() {
            *preview = Some(rem.clone());
        }
    });
}
