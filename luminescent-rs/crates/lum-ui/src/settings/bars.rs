//! The profile bar, the sidebar (with search and pinned sections) and the bar at the bottom.

use egui::{
    Align, Event, Id, Layout, Rect, ScrollArea, Sense, Stroke, Ui, UiBuilder, WidgetInfo, pos2, vec2,
};
use lum_core::settings::Settings;

use super::fields::marked_job;
use super::model::{self, field_count, section_hits};
use super::{Ask, Env, Next, Saved, SettingsUi, dialog_id, short_path};
use crate::dialog::AskDialog;
use crate::fonts::Weight;
use crate::paint;
use crate::text::{Text, caps};
use crate::theme::{Role, Tokens, Tone};
use crate::widgets::{Button, Checkbox, Combo, TextInput, Variant, key_cap};

fn search_id() -> Id {
    super::base_id().with("search")
}

impl SettingsUi {
    pub(super) fn profile_bar(&mut self, ui: &mut Ui, rect: Rect, settings: &mut Settings, tokens: &Tokens) {
        paint::fill(ui.painter(), rect, 0.0, tokens, Role::BgHeader, 1.0);
        ui.painter().hline(rect.x_range(), rect.max.y - 0.5, Stroke::new(1.0, tokens.p.border));
        let inner = rect.shrink2(vec2(if rect.width() < 640.0 { 12.0 } else { 20.0 }, 0.0));
        let mut child =
            ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)));
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        // on a narrow page the bar scrolls sideways instead of squeezing its buttons
        ScrollArea::horizontal()
            .id_salt("settings-profile-bar")
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    ui.add(caps("PROFILE").px(10.0).spacing(0.12).tone(Tone::Muted));
                    self.profile_controls(ui, settings);
                });
            });
    }

    fn profile_controls(&mut self, ui: &mut Ui, settings: &mut Settings) {
        let profiles = settings.profiles();
        let active = settings.active_profile();
        let names: Vec<String> = profiles.iter().map(|p| p.name.clone()).collect();
        let mut index = profiles.iter().position(|p| p.id == active).unwrap_or(0);
        let combo = Combo::new(super::base_id().with("profile"), &mut index, &names).width(150.0);
        let tip = "Themes can pick a profile (Themes → right-click → Settings…). General, Overlay and Dashboard settings are shared by every profile.";
        if ui.add(combo).on_hover_text(tip).changed()
            && let Some(picked) = profiles.get(index)
        {
            self.ask_before(Next::Switch(picked.id.clone()), settings);
        }
        if ui.add(Button::ghost("New profile")).clicked() {
            self.ask_before(Next::NewProfile, settings);
        }
        if ui.add(Button::ghost("Rename")).clicked() {
            let name = settings.profile_name(&active);
            let dialog = AskDialog::ask_text(dialog_id(), "Rename profile", "New name:", &name);
            self.ask(Ask::Rename(active.clone()), dialog);
        }
        let can_delete = active != self.schema.default_profile;
        if ui.add_enabled(can_delete, Button::ghost("Delete")).clicked() {
            let text = format!(
                "Delete the settings profile \"{}\"?\nYou go back to the Default profile.",
                settings.profile_name(&active)
            );
            let dialog = AskDialog::confirm(dialog_id(), "Delete profile", &text, "Delete", "Cancel", true);
            self.ask(Ask::Delete(active), dialog);
        }
        if ui.available_width() > 160.0 {
            let full = short_path(settings.settings_path());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let path = Text::new(full.clone()).px(10.5).mono().tone(Tone::Muted).elide();
                ui.add(path).on_hover_text(full);
            });
        }
    }

    /// Switching profile or making one throws away unsaved edits, so that is asked first.
    fn ask_before(&mut self, next: Next, settings: &mut Settings) {
        if self.draft.as_ref().is_some_and(|d| d.is_dirty()) {
            let text = "Discard the changes you have not saved yet?";
            let dialog = AskDialog::confirm(dialog_id(), "Unsaved changes", text, "Discard", "Cancel", false);
            self.ask(Ask::Discard(next), dialog);
            return;
        }
        match next {
            Next::Switch(id) => self.switch_profile(settings, &id),
            Next::NewProfile => self.new_profile_dialog(),
        }
    }

    pub(super) fn new_profile_dialog(&mut self) {
        let text = "Name (it starts as a copy of the current settings):";
        self.ask(Ask::NewProfile, AskDialog::ask_text(dialog_id(), "New settings profile", text, ""));
    }

    pub(super) fn switch_profile(&mut self, settings: &mut Settings, id: &str) {
        match settings.switch_profile(id, false) {
            Ok(true) => self.profile_changed(settings),
            Ok(false) => {}
            Err(e) => self.message("Profile", &format!("Could not switch the profile:\n{e}")),
        }
    }

    pub(super) fn create_profile(&mut self, settings: &mut Settings, name: &str) {
        let made = settings.create_profile(name).and_then(|id| settings.switch_profile(&id, false));
        match made {
            Ok(_) => self.profile_changed(settings),
            Err(e) => self.message("Profile", &format!("Could not make the profile:\n{e}")),
        }
    }

    pub(super) fn delete_profile(&mut self, settings: &mut Settings, id: &str) {
        let was_active = settings.active_profile() == id;
        if let Err(e) = settings.delete_profile(id) {
            self.message("Profile", &format!("Could not delete the profile:\n{e}"));
        } else if was_active {
            self.profile_changed(settings);
        }
    }

    /// Another profile's values are in the settings now: the page shows them and the app applies them all.
    fn profile_changed(&mut self, settings: &Settings) {
        self.reload(settings);
        self.note.clear();
        let all = self.schema.sections.iter().map(|s| s.key.clone()).collect();
        self.out.saved = Some(Saved { changed: all });
    }

    pub(super) fn sidebar(&mut self, ui: &mut Ui, rect: Rect, settings: &mut Settings, tokens: &Tokens) {
        paint::fill(ui.painter(), rect, 0.0, tokens, Role::BgFrame, 1.0);
        ui.painter().vline(rect.max.x - 0.5, rect.y_range(), Stroke::new(1.0, tokens.p.border));
        let inner = Rect::from_min_max(rect.min + vec2(10.0, 14.0), rect.max - vec2(11.0, 14.0));
        let mut child = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Min)));
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        let ui = &mut child;
        self.search_box(ui, settings, tokens);
        ui.add_space(10.0);
        ui.spacing_mut().item_spacing.y = 2.0;
        let height = ui.available_height();
        ScrollArea::vertical()
            .id_salt("settings-sidebar")
            .max_height(height)
            .auto_shrink([false, false])
            .show(ui, |ui| self.section_list(ui, settings, tokens));
    }

    fn search_box(&mut self, ui: &mut Ui, settings: &mut Settings, tokens: &Tokens) {
        let wants_slash = !ui.memory(|m| m.focused().is_some())
            && ui.input(|i| i.events.iter().any(|e| matches!(e, Event::Text(t) if t == "/")));
        if wants_slash {
            ui.input_mut(|i| i.events.retain(|e| !matches!(e, Event::Text(t) if t == "/")));
            ui.memory_mut(|m| m.request_focus(search_id()));
        }
        let width = ui.available_width();
        let input = TextInput::new(&mut self.query).hint("Search settings").id(search_id()).width(width);
        let response = ui.add(input.name("Search settings"));
        if self.query.is_empty() {
            let cap = key_cap(ui, pos2(response.rect.max.x - 28.0, response.rect.center().y - 10.0), "/");
            let _ = cap;
        }
        ui.add_space(6.0);
        let mut only = Self::only_matching(settings);
        let tip = "Searching hides every setting whose name or description does not contain what you typed (instead of only hiding whole sections). Matches are highlighted either way.";
        let changed = ui.add(Checkbox::new(&mut only, "Only matching settings")).on_hover_text(tip).changed();
        if changed {
            self.set_only_matching(settings, only);
        }
        let _ = tokens;
    }

    fn section_list(&mut self, ui: &mut Ui, settings: &mut Settings, tokens: &Tokens) {
        let q = self.query.trim().to_lowercase();
        let only = Self::only_matching(settings);
        let pins = self.pinned(settings);
        let schema = self.schema;
        let index_of = |key: &str| schema.sections.iter().position(|s| s.key == key);
        let mut order: Vec<usize> = pins.iter().filter_map(|k| index_of(k)).collect();
        let pinned_count = order.len();
        let rest: Vec<usize> = (0..schema.sections.len()).filter(|i| !order.contains(i)).collect();
        order.extend(rest);
        let mut chose = None;
        let mut pin_change: Option<(String, bool)> = None;
        for (place, &i) in order.iter().enumerate() {
            if place == 0 && pinned_count > 0 {
                heading(ui, "★  PINNED");
            }
            if place == pinned_count && pinned_count > 0 {
                ui.add_space(6.0);
                heading(ui, "ALL SETTINGS");
            }
            let section = &schema.sections[i];
            if model::section_hidden(section, &q, only) {
                continue;
            }
            let hits = (!q.is_empty()).then(|| section_hits(section, &q));
            let entry = Entry {
                title: &section.title,
                total: field_count(section),
                hits,
                selected: i == self.current,
                pinned: place < pinned_count,
                query: &q,
            };
            let (clicked, pin) = side_entry(ui, tokens, &entry);
            if clicked {
                chose = Some(i);
            }
            if let Some(on) = pin {
                pin_change = Some((section.key.clone(), on));
            }
        }
        if let Some(i) = chose {
            self.select(i);
        }
        if let Some((key, on)) = pin_change {
            self.pin(settings, &key, on);
        }
    }

    /// The narrow page: search and the section list as one row above the page.
    pub(super) fn sidebar_strip(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        settings: &mut Settings,
        tokens: &Tokens,
    ) {
        paint::fill(ui.painter(), rect, 0.0, tokens, Role::BgFrame, 1.0);
        ui.painter().hline(rect.x_range(), rect.max.y - 0.5, Stroke::new(1.0, tokens.p.border));
        let inner = rect.shrink2(vec2(12.0, 8.0));
        let mut child = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Min)));
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        let ui = &mut child;
        let q = self.query.trim().to_lowercase();
        let only = Self::only_matching(settings);
        let schema = self.schema;
        let visible: Vec<usize> = (0..schema.sections.len())
            .filter(|i| !model::section_hidden(&schema.sections[*i], &q, only))
            .collect();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let half = ((ui.available_width() - 8.0) * 0.45).max(100.0);
            let input = TextInput::new(&mut self.query).hint("Search settings").id(search_id()).width(half);
            ui.add(input.name("Search settings"));
            let titles: Vec<String> = visible.iter().map(|i| schema.sections[*i].title.clone()).collect();
            let mut at = visible.iter().position(|i| *i == self.current).unwrap_or(0);
            let combo = Combo::new(super::base_id().with("section-pick"), &mut at, &titles)
                .width(ui.available_width());
            if ui.add(combo).changed()
                && let Some(i) = visible.get(at)
            {
                self.select(*i);
            }
        });
        if inner.height() > 62.0 {
            ui.add_space(6.0);
            let mut only_now = only;
            if ui.add(Checkbox::new(&mut only_now, "Only matching settings")).changed() {
                self.set_only_matching(settings, only_now);
            }
        }
    }

    pub(super) fn footer(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        settings: &mut Settings,
        env: &Env<'_>,
        tokens: &Tokens,
        compact: bool,
    ) {
        paint::fill(ui.painter(), rect, 0.0, tokens, Role::BgHeader, 1.0);
        ui.painter().hline(rect.x_range(), rect.min.y + 0.5, Stroke::new(1.0, tokens.p.border));
        let inner = rect.shrink2(vec2(if compact { 12.0 } else { 20.0 }, 0.0));
        let (mut save, mut undo, mut reset) = (false, false, false);
        let mut right =
            ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::right_to_left(Align::Center)));
        right.spacing_mut().item_spacing.x = 10.0;
        save |= right.add(Button::primary("Save & Apply")).clicked();
        let undo_response = right.add(Button::new("Undo changes").variant(Variant::Outline));
        undo |= undo_response.clicked();
        let mut left =
            ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)));
        let reset_response = left.add(Button::danger("Reset page"));
        reset |= reset_response
            .on_hover_text("Put this page back to its defaults (not saved until Save & Apply)")
            .clicked();
        self.status(ui, tokens, reset_response_end(&left, inner), undo_response.rect.min.x, rect.center().y);
        if save {
            self.save(settings, env);
        }
        if undo {
            self.undo_changes(settings);
        }
        if reset {
            self.reset_section(settings);
        }
    }

    /// "3 UNSAVED CHANGES", or what just happened, in the space between the buttons.
    fn status(&mut self, ui: &mut Ui, tokens: &Tokens, from: f32, to: f32, y: f32) {
        let n = self.dirty_count();
        let text = if n > 0 {
            self.note.clear();
            format!("{n} UNSAVED CHANGE{}", if n == 1 { "" } else { "S" })
        } else if self.note.is_empty() {
            "ALL SAVED".to_owned()
        } else {
            self.note.clone()
        };
        let font = tokens.font(11.0, Weight::Regular, true);
        let galley = ui.painter().layout_no_wrap(text.clone(), font, tokens.p.text_muted);
        let width = galley.size().x + 6.0 + if n > 0 { 12.0 } else { 0.0 };
        let room = to - from - 20.0;
        let centre = (from + to) / 2.0;
        if width > room {
            // no room for words: the dot still says there is something unsaved
            if n > 0 {
                ui.painter().circle_filled(pos2(from + 14.0, y), 3.0, tokens.p.primary);
            }
            return;
        }
        let mut x = centre - width / 2.0;
        if n > 0 {
            ui.painter().circle_filled(pos2(x + 3.0, y), 3.0, tokens.p.primary);
            x += 12.0;
        }
        ui.painter().galley(pos2(x, y - galley.size().y / 2.0), galley, tokens.p.text_muted);
        let rect = Rect::from_min_size(pos2(x, y - 8.0), vec2(width, 16.0));
        let response = ui.interact(rect, super::base_id().with("status"), Sense::hover());
        response.widget_info(|| WidgetInfo::labeled(egui::Role::Label, true, &text));
    }
}

fn reset_response_end(left: &Ui, inner: Rect) -> f32 {
    left.min_rect().max.x.max(inner.min.x)
}

fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        ui.add(caps(text).px(9.5).spacing(0.14).tone(Tone::Muted));
    });
    ui.add_space(4.0);
}

struct Entry<'a> {
    title: &'a str,
    total: usize,
    hits: Option<usize>,
    selected: bool,
    pinned: bool,
    query: &'a str,
}

/// One section in the sidebar: its name, how many settings it has (or how many the search found) and a star
/// that shows on hover, or always once the section is pinned. Gives back whether it was clicked and whether the
/// star was.
fn side_entry(ui: &mut Ui, tokens: &Tokens, entry: &Entry<'_>) -> (bool, Option<bool>) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
    let hovered = ui.rect_contains_pointer(rect);
    let star_rect = Rect::from_center_size(pos2(rect.max.x - 15.0, rect.center().y), vec2(22.0, 22.0));
    let show_star = hovered || entry.pinned;
    let star = ui.interact(
        star_rect,
        response.id.with("star"),
        if show_star { Sense::click() } else { Sense::hover() },
    );
    let p = &tokens.p;
    if ui.is_rect_visible(rect) {
        let radius = tokens.rs();
        if entry.selected {
            paint::fill(ui.painter(), rect, radius, tokens, Role::PrimaryMuted, 1.0);
        } else if hovered {
            paint::fill(ui.painter(), rect, radius, tokens, Role::BgCardHover, 1.0);
        }
        let count = match entry.hits {
            None => entry.total.to_string(),
            Some(0) => String::new(),
            Some(n) => format!("{n} found"),
        };
        let right_edge = if show_star { star_rect.min.x - 2.0 } else { rect.max.x - 12.0 };
        let mut name_right = right_edge;
        if !count.is_empty() {
            let font = tokens.font(10.0, Weight::Regular, true);
            let galley = ui.painter().layout_no_wrap(count, font, p.text_muted);
            name_right = right_edge - galley.size().x - 8.0;
            ui.painter().galley(
                pos2(right_edge - galley.size().x, rect.center().y - galley.size().y / 2.0),
                galley,
                p.text_muted,
            );
        }
        let tone = if entry.selected { Tone::Primary } else { Tone::Secondary };
        let format = Text::new(entry.title).medium().tone(tone).format(tokens);
        let mut job = marked_job(format, entry.title, entry.query, tokens);
        job.wrap.max_width = (name_right - rect.min.x - 12.0).max(20.0);
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        let galley = ui.painter().layout_job(job);
        ui.painter().galley(
            pos2(rect.min.x + 12.0, rect.center().y - galley.size().y / 2.0),
            galley,
            p.text_secondary,
        );
        if show_star {
            let glyph = if entry.pinned { "★" } else { "☆" };
            let colour = if entry.pinned || star.hovered() { p.primary } else { p.text_muted };
            let font = tokens.font(14.0, Weight::Regular, false);
            ui.painter().text(star_rect.center(), egui::Align2::CENTER_CENTER, glyph, font, colour);
        }
    }
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let (title, selected) = (entry.title.to_owned(), entry.selected);
    response.widget_info(|| WidgetInfo::selected(egui::Role::Button, true, selected, &title));
    let pin_label = format!("{} {}", if entry.pinned { "Unpin" } else { "Pin" }, entry.title);
    star.widget_info(|| WidgetInfo::labeled(egui::Role::Button, show_star, &pin_label));
    let star_clicked = show_star && star.clicked();
    if star_clicked {
        return (false, Some(!entry.pinned));
    }
    (response.clicked() && !star.hovered(), None)
}
