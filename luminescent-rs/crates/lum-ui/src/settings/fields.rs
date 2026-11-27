//! The rows of a section and the control each field type gets.

use egui::text::{LayoutJob, TextFormat};
use egui::{Align, Layout, Rect, ScrollArea, Sense, Stroke, Ui, vec2};
use lum_core::settings::{Field, FieldType, Section, Settings};
use lum_platform::dialogs::Request;
use serde_json::Value;

use super::model::{self, is_shown};
use super::{Env, FileAsk, FileJob, LABEL_WIDTH, SettingsUi, full_key};
use crate::text::{Text, hint, label};
use crate::theme::{Tokens, Tone, alpha};
use crate::widgets::{
    Button, Checkbox, ColourButton, Combo, HotkeyField, OpacitySlider, SpinBox, TextInput, recorded_key,
};

/// Text with every match of the search painted over, like `set_marked`.
pub(super) fn marked_job(format: TextFormat, plain: &str, query: &str, tokens: &Tokens) -> LayoutJob {
    let mut job = LayoutJob::default();
    let lower = plain.to_lowercase();
    // lower casing can change the length of unusual letters; then there is nothing safe to mark
    if query.is_empty() || lower.len() != plain.len() || !lower.contains(query) {
        job.append(plain, 0.0, format);
        return job;
    }
    let mark = TextFormat {
        background: alpha(tokens.p.primary, 0.38),
        color: tokens.p.text_primary,
        ..format.clone()
    };
    let mut at = 0;
    while let Some(found) = lower[at..].find(query) {
        let start = at + found;
        let end = start + query.len();
        job.append(&plain[at..start], 0.0, format.clone());
        job.append(&plain[start..end], 0.0, mark.clone());
        at = end;
    }
    job.append(&plain[at..], 0.0, format);
    job
}

pub(super) fn marked(ui: &mut Ui, text: &Text, plain: &str, query: &str, wrap: bool) {
    let tokens = Tokens::of(ui.ctx());
    let job = marked_job(text.format(&tokens), plain, query, &tokens);
    let widget = egui::Label::new(job).selectable(false);
    ui.add(if wrap { widget.wrap() } else { widget.extend() });
}

impl SettingsUi {
    /// The page of the current section, drawn into `rect`.
    pub(super) fn content(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        settings: &mut Settings,
        env: &Env<'_>,
        compact: bool,
    ) {
        self.follow_search(settings);
        self.refresh_warnings(env);
        let schema = self.schema;
        let section = &schema.sections[self.current];
        let query = self.query.trim().to_lowercase();
        let only = Self::only_matching(settings);
        let margin = if compact { 14.0 } else { 36.0 };
        let id = super::base_id().with(("section", section.key.as_str()));

        let mut child =
            ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min)));
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        ScrollArea::vertical().id_salt(id).auto_shrink([false, false]).show(&mut child, |ui| {
            let width = (rect.width() - margin * 2.0).max(120.0);
            ui.add_space(if compact { 12.0 } else { 26.0 });
            ui.horizontal_top(|ui| {
                ui.add_space(margin);
                ui.vertical(|ui| {
                    ui.set_width(width);
                    self.section_head(ui, section, &query, compact);
                    self.module_bar(ui, section, env);
                    let rows: Vec<&Field> = section
                        .fields
                        .iter()
                        .filter(|f| is_shown(f))
                        .filter(|f| query.is_empty() || !only || model::field_hit(f, &query))
                        .collect();
                    for (i, field) in rows.iter().enumerate() {
                        self.row(ui, section, field, &query, i + 1 == rows.len(), env);
                    }
                    if rows.is_empty() {
                        ui.add(hint("Nothing on this page matches the search.").px(13.0));
                    }
                });
            });
            ui.add_space(26.0);
        });
    }

    fn section_head(&mut self, ui: &mut Ui, section: &Section, query: &str, compact: bool) {
        let title = label(&section.title).px(20.0).semibold();
        let describe = hint(&section.description).px(13.0);
        if compact || section.description.is_empty() {
            marked(ui, &title, &section.title, query, true);
            if !section.description.is_empty() {
                ui.add_space(4.0);
                marked(ui, &describe, &section.description, query, true);
            }
        } else {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                marked(ui, &title, &section.title, query, false);
                marked(ui, &describe, &section.description, query, true);
            });
        }
        ui.add_space(14.0);
    }

    /// Buttons for what the module does, or "Turn it on" when it is off (`_fill_open_bar`).
    fn module_bar(&mut self, ui: &mut Ui, section: &Section, env: &Env<'_>) {
        let Some(info) = (env.module)(&section.key) else { return };
        if let Some(error) = &info.error {
            ui.add(
                label(format!("⚠ This module could not be loaded: {error}"))
                    .px(13.0)
                    .tone(Tone::Danger)
                    .wrap(),
            );
            ui.add_space(8.0);
        }
        if !info.running {
            ui.horizontal_wrapped(|ui| {
                ui.add(label(format!("{} is turned off.", info.title)).px(12.5).tone(Tone::Muted));
                if ui.add(Button::primary("Turn it on")).clicked() {
                    self.turn_on(&section.key);
                }
            });
            ui.add_space(10.0);
            return;
        }
        // the reminders editor is its own list on this page
        let actions: Vec<_> =
            if section.key == "reminders" { Vec::new() } else { info.actions.iter().take(5).collect() };
        if actions.is_empty() {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            for (i, (id, text)) in actions.into_iter().enumerate() {
                let button = if i == 0 { Button::primary(text) } else { Button::new(text) };
                if ui.add(button).on_hover_text("Does the same as its hotkey").clicked() {
                    self.out.run.push(id.clone());
                }
            }
        });
        ui.add_space(10.0);
    }

    fn row(&mut self, ui: &mut Ui, section: &Section, field: &Field, query: &str, last: bool, env: &Env<'_>) {
        let tokens = Tokens::of(ui.ctx());
        let tall =
            field.kind == FieldType::Custom || (field.kind == FieldType::Bool && !field.help.is_empty());
        let pad = if tall { 14.0 } else { 10.0 };
        let stacked = ui.available_width() < 520.0;
        let hide_label = section.key == "reminders" && field.key == "items";
        let start = ui.cursor().min.y;
        ui.add_space(pad);
        if stacked {
            if !hide_label {
                self.field_label(ui, field, query, ui.available_width());
                ui.add_space(4.0);
            }
            ui.vertical(|ui| self.control_column(ui, section, field, query, env));
        } else {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if !hide_label {
                    ui.allocate_ui_with_layout(vec2(LABEL_WIDTH, 0.0), Layout::top_down(Align::Min), |ui| {
                        ui.set_width(LABEL_WIDTH);
                        if self.top_label(field) {
                            ui.add_space(8.0);
                        }
                        self.field_label(ui, field, query, LABEL_WIDTH - 16.0);
                    });
                }
                ui.vertical(|ui| self.control_column(ui, section, field, query, env));
            });
        }
        ui.add_space(pad);
        let used = ui.cursor().min.y - start;
        if used < 52.0 {
            ui.add_space(52.0 - used);
        }
        if !last {
            let y = ui.cursor().min.y;
            let x = ui.max_rect().x_range();
            ui.painter().hline(x, y, Stroke::new(1.0, tokens.p.border));
            ui.add_space(1.0);
        }
    }

    /// Labels sit at the top when the field is tall (a help line under it, a list).
    fn top_label(&self, field: &Field) -> bool {
        field.kind == FieldType::Custom || (!field.help.is_empty() && field.kind != FieldType::Bool)
    }

    fn field_label(&self, ui: &mut Ui, field: &Field, query: &str, width: f32) {
        let text = if field.restart { format!("{}  ⟳", field.label) } else { field.label.clone() };
        ui.allocate_ui(vec2(width, 0.0), |ui| {
            ui.set_width(width);
            let line = Text::new(&text).px(13.0).tone(Tone::Secondary);
            let response = ui.scope(|ui| marked(ui, &line, &text, query, true)).response;
            if field.restart {
                response.on_hover_text("Needs a restart to fully apply");
            }
        });
    }

    fn control_column(&mut self, ui: &mut Ui, section: &Section, field: &Field, query: &str, env: &Env<'_>) {
        ui.spacing_mut().item_spacing.y = 5.0;
        if let Some(control) = model::control_of(&section.key, field) {
            self.drawn.push((full_key(section, field), control));
        }
        if field.kind == FieldType::Bool {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 9.0;
                let toggled = self.bool_box(ui, section, field);
                if !field.help.is_empty() {
                    let help = hint(&field.help).px(12.0);
                    let width = ui.available_width();
                    let clicked = ui
                        .allocate_ui(vec2(width, 0.0), |ui| {
                            ui.set_width(width);
                            marked(ui, &help, &field.help, query, true);
                        })
                        .response
                        .interact(Sense::click())
                        .clicked();
                    if clicked {
                        self.toggle_bool(section, field);
                    }
                }
                let _ = toggled;
            });
            return;
        }
        self.control(ui, section, field, env);
        if !field.help.is_empty() {
            marked(ui, &hint(&field.help).px(12.0), &field.help, query, true);
        }
    }

    fn bool_box(&mut self, ui: &mut Ui, section: &Section, field: &Field) -> bool {
        let mut on = self.value(&section.key, &field.key).is_some_and(|v| v.as_bool().unwrap_or(false));
        let changed = ui.add(Checkbox::new(&mut on, "").name(&field.label)).changed();
        if changed {
            self.set_value(&section.key, &field.key, Value::Bool(on));
        }
        changed
    }

    fn toggle_bool(&mut self, section: &Section, field: &Field) {
        let on = self.value(&section.key, &field.key).is_some_and(|v| v.as_bool().unwrap_or(false));
        self.set_value(&section.key, &field.key, Value::Bool(!on));
    }

    fn control(&mut self, ui: &mut Ui, section: &Section, field: &Field, env: &Env<'_>) {
        let full = full_key(section, field);
        let current = self.value(&section.key, &field.key).cloned().unwrap_or_else(|| field.default.clone());
        match field.kind {
            FieldType::Hotkey => self.hotkey_control(ui, section, field, &full),
            FieldType::Text => {
                let mut text = current.as_str().unwrap_or_default().to_owned();
                let width = ui.available_width().min(520.0);
                if ui.add(TextInput::new(&mut text).width(width).name(&field.label)).changed() {
                    self.set_value(&section.key, &field.key, Value::String(text));
                }
            }
            FieldType::Dir | FieldType::File => self.path_control(ui, field, &full, &current),
            FieldType::Int => {
                let mut n = current.as_f64().unwrap_or(0.0);
                let range = field.min..=field.max;
                let spin =
                    SpinBox::new(&mut n, range).step(field.step.max(1.0)).width(120.0).name(&field.label);
                if ui.add(spin).changed() {
                    self.set_value(&section.key, &field.key, Value::from(n.round() as i64));
                }
            }
            FieldType::Float => {
                let mut n = current.as_f64().unwrap_or(0.0);
                let step = if field.step > 0.0 { field.step } else { 0.1 };
                let spin = SpinBox::new(&mut n, field.min..=field.max)
                    .step(step)
                    .decimals(2)
                    .width(120.0)
                    .name(&field.label);
                if ui.add(spin).changed() {
                    let rounded = (n * 100.0).round() / 100.0;
                    self.set_value(&section.key, &field.key, serde_json::json!(rounded));
                }
            }
            FieldType::Opacity => {
                let mut value = current.as_f64().unwrap_or(0.0) as f32;
                let response = ui.scope(|ui| {
                    ui.set_max_width(420.0);
                    ui.add(OpacitySlider::new(&mut value))
                });
                if response.inner.changed() {
                    let stored = f64::from((value * 100.0).round()) / 100.0;
                    self.set_value(&section.key, &field.key, serde_json::json!(stored));
                }
            }
            FieldType::Color => self.colour_control(ui, field, &full, &current),
            FieldType::Choice => {
                let labels: Vec<String> = field.choices.iter().map(|(_, l)| l.clone()).collect();
                let mut index = field.choices.iter().position(|(v, _)| *v == current).unwrap_or(0);
                let id = super::base_id().with(("choice", &full));
                // wide enough for the longest choice, as the Qt combo box is
                let tokens = Tokens::of(ui.ctx());
                let font = tokens.font(13.0, crate::fonts::Weight::Medium, false);
                let widest = labels
                    .iter()
                    .map(|l| {
                        ui.painter().layout_no_wrap(l.clone(), font.clone(), tokens.p.text_primary).size().x
                    })
                    .fold(0.0, f32::max);
                let width = (widest + 46.0).max(220.0).min(ui.available_width().max(120.0));
                if ui.add(Combo::new(id, &mut index, &labels).width(width)).changed()
                    && let Some((value, _)) = field.choices.get(index)
                {
                    self.set_value(&section.key, &field.key, value.clone());
                }
            }
            FieldType::Custom | FieldType::Widget => self.custom_control(ui, section, field, env),
            FieldType::Bool | FieldType::Hidden => {}
        }
    }

    fn hotkey_control(&mut self, ui: &mut Ui, section: &Section, field: &Field, full: &str) {
        let combo = self.hotkey_text(&section.key, &field.key);
        let recording = self.recording.as_deref() == Some(full);
        let warning = self.warnings.get(full).cloned();
        let response = HotkeyField::new(&combo).recording(recording).warning(warning.as_deref()).show(ui);
        if response.field.clicked() || response.record.clicked() {
            if recording {
                self.recording = None;
                self.out.suspend_keys = Some(false);
            } else {
                // moving straight from one field to another keeps the keys suspended
                self.recording = Some(full.to_owned());
                self.out.suspend_keys = Some(true);
            }
        }
        if response.clear.clicked() {
            self.set_value(&section.key, &field.key, Value::String(String::new()));
        }
        let waiting = recording && !response.field.clicked() && !response.record.clicked();
        if waiting && let Some(recorded) = recorded_key(ui) {
            self.recording = None;
            self.out.suspend_keys = Some(false);
            if let Some(new) = recorded {
                self.set_value(&section.key, &field.key, Value::String(new));
            }
        }
    }

    fn path_control(&mut self, ui: &mut Ui, field: &Field, full: &str, current: &Value) {
        let mut text = current.as_str().unwrap_or_default().to_owned();
        let (section, key) = full.split_once('.').unwrap_or((full, ""));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let width = (ui.available_width().min(560.0) - 90.0).max(100.0);
            if ui.add(TextInput::new(&mut text).mono().width(width).name(&field.label)).changed() {
                self.set_value(section, key, Value::String(text.clone()));
            }
            if ui.add(Button::new("Browse…")).clicked() {
                let folder = field.kind == FieldType::Dir;
                self.out.files.push(FileAsk {
                    job: FileJob::Browse(full.to_owned()),
                    save: false,
                    folder,
                    request: browse_request(field, &text),
                });
            }
        });
    }

    fn colour_control(&mut self, ui: &mut Ui, field: &Field, full: &str, current: &Value) {
        let hex = current.as_str().filter(|s| !s.is_empty()).unwrap_or("#000000");
        let rgb = lum_theme::Rgb::parse(hex).unwrap_or(lum_theme::Rgb::BLACK);
        let swatch = ColourButton::new(crate::theme::colour(rgb));
        if ui.add(swatch).on_hover_text(&field.label).clicked() {
            let target = format!("settings:{full}");
            self.picker.open(&target, "Pick a colour", crate::picker::Picked::Colour(rgb), false, self.near);
        }
    }
}

/// The dialog for a path box: starts in the folder of what is typed, with the field's file types.
fn browse_request(field: &Field, typed: &str) -> Request {
    let title = if field.kind == FieldType::Dir { "Choose a folder" } else { "Choose a file" };
    let start = std::path::Path::new(typed);
    let folder = if field.kind == FieldType::Dir { Some(start) } else { start.parent() };
    let mut request =
        Request::new(title).folder(folder.filter(|p| p.is_dir()).map(std::path::Path::to_path_buf));
    for (name, extensions) in filters(&field.file_filter) {
        let list: Vec<&str> = extensions.iter().map(String::as_str).collect();
        request = request.filter(&name, &list);
    }
    request
}

/// Qt's `"Audio (*.wav *.mp3);;All files (*)"` as (name, extensions); "everything" needs no filter.
pub(super) fn filters(spec: &str) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    for part in spec.split(";;").map(str::trim).filter(|p| !p.is_empty()) {
        let (name, patterns) = match part.split_once('(') {
            Some((name, rest)) => (name.trim(), rest.trim_end_matches(')')),
            None => (part, ""),
        };
        let extensions: Vec<String> = patterns
            .split_whitespace()
            .filter_map(|p| p.strip_prefix("*."))
            .map(|e| e.trim_end_matches('*').to_owned())
            .filter(|e| !e.is_empty())
            .collect();
        if !extensions.is_empty() {
            out.push((name.to_owned(), extensions));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qt_filters_become_extension_lists() {
        let parsed = filters("Audio (*.wav *.mp3 *.ogg);;All files (*)");
        assert_eq!(
            parsed,
            vec![("Audio".to_owned(), vec!["wav".to_owned(), "mp3".to_owned(), "ogg".to_owned()])]
        );
        assert!(filters("").is_empty());
        let python = filters("Python (python python3*);;All files (*)");
        assert!(python.is_empty());
    }
}
