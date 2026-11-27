//! The fields that are not one box: the lists with their own editors and the buttons that do things
//! (Backup, Quit everything, the notification test, Updates).

use egui::{Align, Layout, Sense, Stroke, StrokeKind, Ui, UiBuilder, vec2};
use lum_core::settings::{Field, Section};
use lum_platform::dialogs::Request;
use serde_json::Value;

use super::model::{START_PANELS, TRAY_ITEMS, ticked};
use super::{Env, FileAsk, FileJob, SettingsUi, full_key, strings};
use crate::overlay::pages::{REQUIRED_PAGES, title};
use crate::paint;
use crate::text::{Text, hint, label};
use crate::theme::{Role, Tokens, Tone};
use crate::widgets::{Button, Checkbox, IconButton};

const LONG_NOTIFICATION: &str = "A longer message, the kind an error has, so you can see how the card grows to fit it instead of cutting it off. ";

impl SettingsUi {
    pub(super) fn custom_control(&mut self, ui: &mut Ui, section: &Section, field: &Field, env: &Env<'_>) {
        match full_key(section, field).as_str() {
            "ui.nav_order" => self.nav_editor(ui, section, field),
            "general.start_panels" => self.check_list(ui, section, field, START_PANELS),
            "general.tray_items" => self.check_list(ui, section, field, TRAY_ITEMS),
            "general.backup" => self.backup_buttons(ui, env),
            "general.quit" => self.quit_buttons(ui),
            "general.updates" => updates(ui, env),
            "general.toast_test" => self.toast_buttons(ui),
            "reminders.items" => {
                ui.add(hint("The reminders editor comes with the Reminders page.").px(12.5));
            }
            other => {
                ui.add(hint(format!("(unsupported field {other})")).px(12.5));
            }
        }
    }

    /// The overlay's page switcher: tick the pages you want and put them in order with ↑ ↓. Dashboard and
    /// Settings are always there.
    fn nav_editor(&mut self, ui: &mut Ui, section: &Section, field: &Field) {
        let tokens = Tokens::of(ui.ctx());
        let width = ui.available_width().min(360.0);
        let keys = self.nav.keys.clone();
        let mut number = 0;
        let mut changed = false;
        ui.spacing_mut().item_spacing.y = 4.0;
        for (i, key) in keys.iter().enumerate() {
            let on = self.nav.is_on(key);
            let required = REQUIRED_PAGES.contains(&key.as_str());
            if on {
                number += 1;
            }
            let (rect, _) = ui.allocate_exact_size(vec2(width, 32.0), Sense::hover());
            let radius = tokens.rs();
            paint::fill(ui.painter(), rect, radius, &tokens, Role::BgCard, 1.0);
            ui.painter().rect_stroke(rect, radius, Stroke::new(1.0, tokens.p.border), StrokeKind::Inside);
            let inner = rect.shrink2(vec2(10.0, 0.0));
            let mut left =
                ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)));
            left.spacing_mut().item_spacing.x = 10.0;
            let mut ticked_now = on;
            let tip = if required { "Always shown" } else { "Show this page in the bar" };
            let name = title(key).unwrap_or(key);
            let checkbox = Checkbox::new(&mut ticked_now, "").name(format!("{name} in the bar"));
            if left.add_enabled(!required, checkbox).on_hover_text(tip).changed() {
                self.nav.toggle(key, ticked_now);
                changed = true;
            }
            let order = if on { format!("{number:02}") } else { "··".to_owned() };
            left.add(Text::new(order).px(10.0).mono().tone(Tone::Muted));
            left.add(Text::new(name).px(13.0).tone(if on { Tone::Primary } else { Tone::Muted }));
            let mut right =
                ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::right_to_left(Align::Center)));
            right.spacing_mut().item_spacing.x = 4.0;
            let down = IconButton::new("↓").size(22.0).font_px(10.0).ghost().tooltip("Move down");
            if right.add_enabled(i + 1 < keys.len(), down).clicked() {
                self.nav.shift(i, 1);
                changed = true;
            }
            let up = IconButton::new("↑").size(22.0).font_px(10.0).ghost().tooltip("Move up");
            if right.add_enabled(i > 0, up).clicked() {
                self.nav.shift(i, -1);
                changed = true;
            }
            if on && number <= 9 {
                right.add(Text::new(format!("Ctrl+{number}")).px(10.0).mono().tone(Tone::Muted));
            }
        }
        if changed {
            let value = Value::Array(self.nav.value().into_iter().map(Value::String).collect());
            self.set_value(&section.key, &field.key, value);
        }
    }

    /// A column of check boxes for a list setting (Show on start, Tray menu).
    fn check_list(&mut self, ui: &mut Ui, section: &Section, field: &Field, choices: &[(&str, &str)]) {
        let chosen = self.value(&section.key, &field.key).map(strings).unwrap_or_default();
        let columns = if ui.available_width() < 470.0 { 1 } else { 2 };
        let rows = choices.len().div_ceil(columns);
        let mut now = chosen.clone();
        ui.add_space(4.0);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 24.0;
            for column in 0..columns {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;
                    for (key, text) in choices.iter().skip(column * rows).take(rows) {
                        let mut on = now.iter().any(|k| k == key);
                        if ui.add(Checkbox::new(&mut on, *text)).changed() {
                            now.retain(|k| k != key);
                            if on {
                                now.push((*key).to_owned());
                            }
                        }
                    }
                });
            }
        });
        if now != chosen {
            let value = ticked(choices, &now).into_iter().map(Value::String).collect();
            self.set_value(&section.key, &field.key, Value::Array(value));
        }
    }

    fn backup_buttons(&mut self, ui: &mut Ui, env: &Env<'_>) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if ui.add(Button::new("Export backup…")).clicked() {
                let file = format!("Luminescent-backup-{}.zip", env.today);
                let documents = lum_core::paths::user_home().join("Documents");
                let request = Request::new("Export backup")
                    .file_name(&file)
                    .folder(Some(documents).filter(|d| d.is_dir()))
                    .filter("Zip", &["zip"]);
                self.out.files.push(FileAsk {
                    job: FileJob::BackupExport,
                    save: true,
                    folder: false,
                    request,
                });
            }
            if ui.add(Button::new("Import backup…")).clicked() {
                let request = Request::new("Import backup")
                    .folder(Some(lum_core::paths::user_home()))
                    .filter("Zip", &["zip"]);
                self.out.files.push(FileAsk {
                    job: FileJob::BackupImport,
                    save: false,
                    folder: false,
                    request,
                });
            }
        });
        ui.add(
            hint("Settings, notes, timers, reminders, hearing history, dashboard and themes in one zip. The vault is never included.")
                .px(12.0)
                .wrap(),
        );
    }

    fn quit_buttons(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if ui.add(Button::danger("Quit everything")).clicked() {
                self.out.quit = true;
            }
            if ui.add(Button::new("Restart")).clicked() {
                self.out.restart = true;
            }
        });
        ui.add(
            hint("The ✕ on the overlay only hides it. This closes the background process and every module.")
                .px(12.0)
                .wrap(),
        );
    }

    /// Shows a notification with the settings on this page (saved or not), so you can see them first.
    fn toast_buttons(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if ui.add(Button::new("Show a notification")).clicked() {
                let text = "This is what notifications look like.";
                self.out.toast_test = Some(("Luminescent".to_owned(), text.to_owned(), Default::default()));
            }
            if ui.add(Button::new("Show a long one")).clicked() {
                let text = LONG_NOTIFICATION.repeat(3).trim().to_owned();
                self.out.toast_test =
                    Some(("Something went wrong (error)".to_owned(), text, Default::default()));
            }
        });
    }
}

/// The version you have. Checking GitHub and installing a release come with the packaging phase.
fn updates(ui: &mut Ui, env: &Env<'_>) {
    ui.horizontal(|ui| {
        ui.add(label(format!("You have Luminescent {}", env.version)).px(14.0).semibold());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_enabled(false, Button::primary("Check for updates"));
        });
    });
    ui.add(hint("Updating from inside the app arrives with the 2.0 release.").px(12.0).wrap());
}
