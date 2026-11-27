//! What Notes draws, into whatever room it is given: the list of notes, the text of one, and a to-do list. The
//! panel, the overlay page, a dashboard box and the sticky notes all use these. Edits go straight into the note
//! that is passed in, so every view of a note shows the same text; things that need a dialog or the app come
//! back as [`Event`]s.

use egui::text::{LayoutJob, TextFormat};
use egui::{
    Align2, Color32, Id, Key, Rect, Response, Role, Sense, Stroke, StrokeKind, TextEdit, Ui, UiBuilder,
    WidgetInfo, pos2, vec2,
};
use jiff::civil::{Date, DateTime};
use lum_core::notes::{self, COLOURS, Note, Notes};

use super::markdown;
use crate::fonts::Weight;
use crate::text::Text;
use crate::theme::{Tokens, Tone, mix};
use crate::widgets::{Button, IconButton, IconShape, TextInput, drag_rows, menu};

const DANGER_RED: Color32 = Color32::from_rgb(0xE5, 0x48, 0x4D);

/// What the views need to know about the world and the settings.
pub struct Env {
    pub today: Date,
    pub now: DateTime,
    pub font_px: f32,
    pub todo_px: f32,
    pub markdown: bool,
}

/// Things a view can not do by itself.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Open(String),
    Back,
    New,
    Pin(String, bool),
    Rename(String),
    Colour(String, String),
    Tags(String),
    Stick(String),
    Unstick(String),
    Export(String),
    Delete(String),
    Move(String, isize),
    /// Dragged in the list: positions in the list's order.
    Reorder(usize, usize),
    /// The text or the tasks of this note changed.
    Changed(String),
    /// Ask for the due date of this task (note id, item id).
    AskDue(String, String),
    Undo,
    Preview(bool),
}

#[derive(Debug)]
struct Editing {
    item: String,
    text: String,
    fresh: bool,
}

/// What one place that shows notes (the panel, the page, one sticky) remembers while it is open.
#[derive(Debug)]
pub struct State {
    salt: Id,
    pub query: String,
    /// The "Add a task" box.
    pub input: String,
    /// The preview of the page or a sticky; the panel keeps its own in the file.
    pub preview: bool,
    editing: Option<Editing>,
}

impl State {
    pub fn new(name: &str) -> State {
        State {
            salt: Id::unique(("lum-notes", name)),
            query: String::new(),
            input: String::new(),
            preview: false,
            editing: None,
        }
    }

    /// Where an inline edit or the box left off is forgotten when another note is shown.
    pub fn reset_note(&mut self) {
        self.input.clear();
        self.editing = None;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListStyle {
    Panel,
    Page,
}

fn name_job(text: &str, tokens: &Tokens, px: f32, weight: Weight, colour: Color32, width: f32) -> LayoutJob {
    let mut job = LayoutJob::single_section(
        text.to_owned(),
        TextFormat { font_id: tokens.font(px, weight, false), color: colour, ..Default::default() },
    );
    job.wrap.max_width = width.max(8.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job
}

fn parse_colour(hex: &str) -> Option<Color32> {
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(hex, 16).ok()?;
    Some(Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

pub fn note_colour(note: &Note) -> Option<Color32> {
    parse_colour(&note.colour)
}

/// What the menu of a note needs to know about it, so a header can build it without holding the note.
#[derive(Debug, Clone)]
pub struct Meta {
    pub id: String,
    pub pinned: bool,
    pub colour: String,
    pub stuck: bool,
}

impl Meta {
    pub fn of(note: &Note, notes: &Notes) -> Meta {
        Meta {
            id: note.id.clone(),
            pinned: note.pinned,
            colour: note.colour.clone(),
            stuck: notes.is_stuck(&note.id),
        }
    }
}

/// The menu a note has in the list, in the header and on the page. `list` adds Open and the moves.
pub struct MenuKind {
    pub list: bool,
    pub can_up: bool,
    pub can_down: bool,
}

pub fn note_menu(ui: &mut Ui, note: &Meta, kind: &MenuKind, events: &mut Vec<Event>) {
    let id = note.id.clone();
    if kind.list && menu::item(ui, "Open").clicked() {
        events.push(Event::Open(id.clone()));
    }
    let pin = if note.pinned { "Unpin" } else { "📌  Pin (shown first, opened first)" };
    if menu::item(ui, pin).clicked() {
        events.push(Event::Pin(id.clone(), !note.pinned));
    }
    if menu::item(ui, "Rename…").clicked() {
        events.push(Event::Rename(id.clone()));
    }
    ui.menu_button("Colour", |ui| {
        for (name, hex) in COLOURS {
            if swatch_entry(ui, name, hex, note.colour.eq_ignore_ascii_case(hex)).clicked() {
                events.push(Event::Colour(id.clone(), hex.to_owned()));
                ui.close();
            }
        }
        menu::separator(ui);
        if menu::item(ui, "No colour").clicked() {
            events.push(Event::Colour(id.clone(), String::new()));
        }
    });
    if menu::item(ui, "Tags…").clicked() {
        events.push(Event::Tags(id.clone()));
    }
    menu::separator(ui);
    let stick =
        if note.stuck { "Unstick from the screen" } else { "📌  Stick on the screen (own small panel)" };
    if menu::item(ui, stick).clicked() {
        events.push(if note.stuck { Event::Unstick(id.clone()) } else { Event::Stick(id.clone()) });
    }
    if menu::item(ui, "Export to a file…").clicked() {
        events.push(Event::Export(id.clone()));
    }
    if kind.list {
        menu::separator(ui);
        if kind.can_up && menu::item(ui, "Move up").clicked() {
            events.push(Event::Move(id.clone(), -1));
        }
        if kind.can_down && menu::item(ui, "Move down").clicked() {
            events.push(Event::Move(id.clone(), 1));
        }
    }
    menu::separator(ui);
    if menu::item(ui, "Delete").clicked() {
        events.push(Event::Delete(id));
    }
}

/// A colour in the Colour menu: a dot and its name, ticked when it is the note's.
fn swatch_entry(ui: &mut Ui, name: &str, hex: &str, on: bool) -> Response {
    let tokens = Tokens::of(ui.ctx());
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width().max(150.0), 28.0), Sense::click());
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, on, name));
    if response.hovered() {
        ui.painter().rect_filled(rect, tokens.rs(), tokens.p.bg_card_hover);
    }
    let dot = pos2(rect.min.x + 18.0, rect.center().y);
    ui.painter().circle_filled(dot, 5.5, parse_colour(hex).unwrap_or(tokens.p.primary));
    if on {
        ui.painter().circle_stroke(dot, 8.0, Stroke::new(1.5, tokens.p.text_primary));
    }
    ui.painter().text(
        pos2(rect.min.x + 34.0, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        tokens.font(13.0, Weight::Regular, false),
        tokens.p.text_primary,
    );
    response
}

fn badge(ui: &mut Ui, at_right: f32, y: f32, text: &str, tokens: &Tokens) -> f32 {
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        tokens.font(10.0, Weight::Medium, true),
        tokens.p.primary_text,
    );
    let size = galley.size() + vec2(14.0, 6.0);
    let rect = Rect::from_min_size(pos2(at_right - size.x, y - size.y / 2.0), size);
    ui.painter().rect_filled(rect, 4.0, tokens.p.primary_muted);
    ui.painter().galley(rect.min + vec2(7.0, 3.0), galley, tokens.p.primary_text);
    size.x
}

/// The search box, with the small new note button beside it in the panel.
fn search_row(ui: &mut Ui, state: &mut State, plus: bool, events: &mut Vec<Event>) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let button_w = if plus { 40.0 } else { 0.0 };
        let width = ui.available_width() - button_w;
        let response = ui.add(
            TextInput::new(&mut state.query)
                .id(state.salt.with("search"))
                .hint("Search notes (#tag for a tag)")
                .width(width),
        );
        if !state.query.is_empty() {
            // a cross over the right end of the box, as Qt's clear button was
            let rect =
                Rect::from_center_size(response.rect.right_center() - vec2(18.0, 0.0), vec2(22.0, 22.0));
            if ui.put(rect, IconButton::new("✕").size(22.0).font_px(10.0).ghost().tooltip("Clear")).clicked()
            {
                state.query.clear();
            }
        }
        if plus && ui.add(IconButton::new("+").size(32.0).font_px(15.0).tooltip("New note")).clicked() {
            events.push(Event::New);
        }
    });
}

/// The list of notes: a search box, the "Undo" line after a delete, and a card for each note. `selected` marks
/// the one the page has open. `undo` is the deleted note's name and how long Undo has left.
pub fn list(
    ui: &mut Ui,
    notes: &Notes,
    state: &mut State,
    style: ListStyle,
    selected: Option<&str>,
    undo: Option<&str>,
    events: &mut Vec<Event>,
) {
    let tokens = Tokens::of(ui.ctx());
    let page = style == ListStyle::Page;
    ui.spacing_mut().item_spacing.y = 6.0;
    if page {
        ui.horizontal(|ui| {
            ui.add(Text::new("Notes").px(15.0).semibold());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(Button::primary("+ New note")).clicked() {
                    events.push(Event::New);
                }
            });
        });
    }
    search_row(ui, state, !page, events);
    if let Some(name) = undo {
        egui::Frame::new()
            .fill(tokens.p.bg_card)
            .stroke(Stroke::new(1.0, tokens.p.border))
            .corner_radius(tokens.r())
            .inner_margin(egui::Margin::symmetric(12, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.add(Text::new(format!("Deleted \"{name}\"")).px(12.5).tone(Tone::Muted));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(Button::new("Undo")).clicked() {
                            events.push(Event::Undo);
                        }
                    });
                });
            });
    }
    let order = notes.order();
    let shown: Vec<usize> = order.iter().copied().filter(|&i| notes.notes[i].matches(&state.query)).collect();
    if shown.is_empty() {
        let text = if notes.notes.is_empty() {
            if page {
                "No notes yet.".to_owned()
            } else {
                "No notes yet.\nPress the edit hotkey and click + to add one.".to_owned()
            }
        } else if page {
            "No notes match.".to_owned()
        } else {
            format!("Nothing matches \"{}\".", state.query.trim())
        };
        ui.add_space(16.0);
        ui.vertical_centered(|ui| ui.add(Text::new(text).px(12.0).tone(Tone::Muted)));
        return;
    }
    let reorder = !page && state.query.trim().is_empty();
    let mut rows: Vec<(Rect, Response)> = Vec::new();
    egui::ScrollArea::vertical().id_salt(state.salt.with("list")).auto_shrink([false, false]).show(
        ui,
        |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.add_space(4.0);
            for &i in &shown {
                let note = &notes.notes[i];
                let on = selected == Some(note.id.as_str());
                let (rect, response) = card(ui, note, style, on, &tokens);
                let kind = MenuKind { list: true, can_up: i > 0, can_down: i + 1 < notes.notes.len() };
                let meta = Meta::of(note, notes);
                if response.clicked() {
                    events.push(Event::Open(note.id.clone()));
                }
                response.context_menu(|ui| note_menu(ui, &meta, &kind, events));
                if reorder {
                    rows.push((rect, response));
                }
            }
        },
    );
    if reorder && let Some((from, to)) = drag_rows(ui, state.salt.with("drag"), &rows) {
        events.push(Event::Reorder(from, to));
    }
}

fn card(ui: &mut Ui, note: &Note, style: ListStyle, selected: bool, tokens: &Tokens) -> (Rect, Response) {
    let page = style == ListStyle::Page;
    let p = &tokens.p;
    let height = if page { 46.0 } else { 48.0 };
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click_and_drag());
    let name = format!("Note {}", note.name);
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, selected, &name));
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let colour = note_colour(note);
    let fill = if page {
        if selected { p.primary_muted } else { p.bg_card }
    } else if let Some(c) = colour {
        mix(p.bg_card, c, 0.12)
    } else {
        p.bg_card
    };
    let fill = if response.hovered() && !page { p.bg_card_hover } else { fill };
    let border = if page && (selected || response.hovered()) { p.primary } else { p.border };
    ui.painter().rect_filled(rect, tokens.r(), fill);
    ui.painter().rect_stroke(rect, tokens.r(), Stroke::new(1.0, border), StrokeKind::Inside);
    let (left_pad, right_pad) = if page { (12.0, 12.0) } else { (14.0, 12.0) };
    let mut left = rect.min.x + left_pad;
    let mut right = rect.max.x - right_pad;
    let y = rect.center().y;
    if note.pinned || colour.is_some() {
        let h = if colour.is_some() { if page { 16.0 } else { 18.0 } } else { 12.0 };
        let tick = Rect::from_center_size(pos2(left + 1.5, y), vec2(3.0, h));
        ui.painter().rect_filled(tick, 1.0, colour.unwrap_or(p.primary));
        left += 13.0;
    }
    if note.is_todo() {
        let (done, total) = note.progress();
        let text = format!("{done}/{total}");
        if page {
            let galley =
                ui.painter().layout_no_wrap(text, tokens.font(10.0, Weight::Regular, true), p.text_muted);
            right -= galley.size().x;
            ui.painter().galley(pos2(right, y - galley.size().y / 2.0), galley, p.text_muted);
        } else {
            right -= badge(ui, right, y, &text, tokens) + 10.0;
        }
    } else if !page {
        let galley = ui.painter().layout_no_wrap(
            "NOTE".to_owned(),
            tokens.font(9.0, Weight::Medium, true),
            p.text_muted,
        );
        right -= galley.size().x;
        ui.painter().galley(pos2(right, y - galley.size().y / 2.0), galley, p.text_muted);
        right -= 10.0;
    }
    if note.pinned && !page {
        let galley = ui.painter().layout_no_wrap(
            "PINNED".to_owned(),
            tokens.font(9.0, Weight::Medium, true),
            p.primary_text,
        );
        right -= galley.size().x;
        ui.painter().galley(pos2(right, y - galley.size().y / 2.0), galley, p.primary_text);
        right -= 10.0;
    }
    if !page {
        for tag in note.tags.iter().take(3).rev() {
            let chip = format!("#{tag}");
            let galley = ui.painter().layout_no_wrap(
                chip,
                tokens.font(9.5, Weight::Medium, true),
                colour.unwrap_or(p.primary_text),
            );
            right -= galley.size().x;
            let c = colour.unwrap_or(p.primary_text);
            ui.painter().galley(pos2(right, y - galley.size().y / 2.0), galley, c);
            right -= 8.0;
        }
    }
    let label = if page && note.pinned { format!("📌 {}", note.name) } else { note.name.clone() };
    let job = name_job(&label, tokens, 13.0, Weight::Medium, p.text_primary, right - left - 6.0);
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(pos2(left, y - galley.size().y / 2.0), galley, p.text_primary);
    (rect, response)
}

/// How a note's body sits in its room.
#[derive(Debug, Clone, Copy)]
pub struct Look {
    pub pad: egui::Vec2,
    /// A sticky: smaller input row and no "Clear done".
    pub compact: bool,
    /// The panel shows the note's name, its count and a progress line above a to-do list.
    pub title: bool,
    /// Show a text note formatted instead of for writing.
    pub preview: bool,
}

/// The text or the tasks of one note.
pub fn body(
    ui: &mut Ui,
    rect: Rect,
    note: &mut Note,
    state: &mut State,
    look: Look,
    env: &Env,
    events: &mut Vec<Event>,
) {
    let mut child =
        ui.new_child(UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min)));
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    if note.is_todo() {
        todo_body(&mut child, note, state, look, env, events);
    } else if look.preview && env.markdown {
        preview_body(&mut child, note, state, look, env);
    } else {
        text_body(&mut child, note, state, look, env, events);
    }
}

fn text_body(
    ui: &mut Ui,
    note: &mut Note,
    state: &mut State,
    look: Look,
    env: &Env,
    events: &mut Vec<Event>,
) {
    let tokens = Tokens::of(ui.ctx());
    let hint = if env.markdown {
        "Start typing…  (**bold**, # heading, - list; 👁 shows it formatted)"
    } else {
        "Start typing…"
    };
    let full = ui.available_size();
    egui::ScrollArea::vertical()
        .id_salt(state.salt.with(("text", &note.id)))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let size = vec2(full.x - look.pad.x * 2.0, (full.y - look.pad.y * 2.0).max(40.0));
            ui.add_space(look.pad.y);
            ui.horizontal(|ui| {
                ui.add_space(look.pad.x);
                let edit = TextEdit::multiline(&mut note.content)
                    .id(state.salt.with(("edit", &note.id)))
                    .frame(egui::Frame::NONE)
                    .font(tokens.font(env.font_px, Weight::Regular, false))
                    .text_color(tokens.p.text_primary)
                    .hint_text(egui::RichText::new(hint).color(tokens.p.text_muted))
                    .desired_width(size.x)
                    .min_size(size)
                    .lock_focus(true);
                if ui.add(edit).changed() {
                    events.push(Event::Changed(note.id.clone()));
                }
            });
        });
}

fn preview_body(ui: &mut Ui, note: &Note, state: &mut State, look: Look, env: &Env) {
    let full = ui.available_size();
    egui::ScrollArea::vertical()
        .id_salt(state.salt.with(("preview", &note.id)))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_width(full.x - look.pad.x * 2.0);
            ui.add_space(look.pad.y);
            ui.horizontal(|ui| {
                ui.add_space(look.pad.x);
                ui.vertical(|ui| {
                    ui.set_width(full.x - look.pad.x * 2.0);
                    if note.content.trim().is_empty() {
                        ui.add(Text::new("Empty note. Click 👁 to write.").px(env.font_px).tone(Tone::Muted));
                    } else if let Some(url) = markdown::show(ui, &note.content, env.font_px)
                        && let Err(e) = lum_platform::runner::open_path(&url)
                    {
                        tracing::warn!("could not open {url}: {e}");
                    }
                });
            });
        });
}

enum Act {
    Toggle(usize),
    Remove(usize),
    StartEdit(usize),
    Commit(usize, String),
    CancelEdit,
    AskDue(usize),
    ClearDue(usize),
}

fn todo_body(
    ui: &mut Ui,
    note: &mut Note,
    state: &mut State,
    look: Look,
    env: &Env,
    events: &mut Vec<Event>,
) {
    let tokens = Tokens::of(ui.ctx());
    let p = &tokens.p;
    let width = ui.available_width();
    if look.title {
        ui.add_space(14.0);
        let (done, total) = note.progress();
        let row = ui.allocate_exact_size(vec2(width, 22.0), Sense::hover()).0;
        let badge_w = badge(ui, row.max.x - 14.0, row.center().y, &format!("{done}/{total}"), &tokens);
        let job = name_job(
            &note.name,
            &tokens,
            15.0,
            Weight::SemiBold,
            p.text_primary,
            width - 28.0 - badge_w - 10.0,
        );
        let galley = ui.painter().layout_job(job);
        ui.painter().galley(
            pos2(row.min.x + 14.0, row.center().y - galley.size().y / 2.0),
            galley,
            p.text_primary,
        );
        ui.add_space(6.0);
        let track = ui.allocate_exact_size(vec2(width - 28.0, 2.0), Sense::hover()).0;
        let track = track.translate(vec2(14.0, 0.0));
        ui.painter().rect_filled(track, 1.0, p.bg_frame);
        let fraction = if total == 0 { 0.0 } else { done as f32 / total as f32 };
        let fill = Rect::from_min_size(track.min, vec2(track.width() * fraction, 2.0));
        ui.painter().rect_filled(fill, 1.0, note_colour(note).unwrap_or(p.primary));
        ui.add_space(10.0);
    }
    let input_h = if look.compact { 40.0 } else { 56.0 };
    let list_h = (ui.available_height() - input_h).max(40.0);
    let list_rect = Rect::from_min_size(ui.cursor().min + vec2(6.0, 0.0), vec2(width - 12.0, list_h));
    let mut list_ui =
        ui.new_child(UiBuilder::new().max_rect(list_rect).layout(egui::Layout::top_down(egui::Align::Min)));
    list_ui.set_clip_rect(list_rect.intersect(ui.clip_rect()));
    ui.advance_cursor_after_rect(Rect::from_min_size(list_rect.min - vec2(6.0, 0.0), vec2(width, list_h)));

    let row_salt = state.salt.with(("rows", &note.id));
    let mut acts: Vec<Act> = Vec::new();
    let mut rows: Vec<(Rect, Response)> = Vec::new();
    let note_id = note.id.clone();
    egui::ScrollArea::vertical()
        .id_salt(state.salt.with(("todos", &note_id)))
        .auto_shrink([false, false])
        .show(&mut list_ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(0.0);
            for (i, item) in note.items.iter().enumerate() {
                let editing = state.editing.as_mut().filter(|e| e.item == item.id);
                let row = todo_row(ui, row_salt, item, i, editing, env, &mut acts);
                rows.push(row);
            }
        });
    if let Some((from, to)) = drag_rows(ui, state.salt.with(("move", &note_id)), &rows) {
        note.move_item(from, to);
        events.push(Event::Changed(note_id.clone()));
    }
    for act in acts {
        match act {
            Act::Toggle(i) => {
                if let Some(item) = note.items.get_mut(i) {
                    item.checked = !item.checked;
                }
                events.push(Event::Changed(note_id.clone()));
            }
            Act::Remove(i) => {
                if i < note.items.len() {
                    note.items.remove(i);
                    events.push(Event::Changed(note_id.clone()));
                }
            }
            Act::StartEdit(i) => {
                if let Some(item) = note.items.get(i) {
                    state.editing =
                        Some(Editing { item: item.id.clone(), text: item.text.clone(), fresh: true });
                }
            }
            Act::Commit(i, text) => {
                state.editing = None;
                let text = text.trim().to_owned();
                if let Some(item) = note.items.get_mut(i)
                    && !text.is_empty()
                    && text != item.text
                {
                    item.text = text;
                    events.push(Event::Changed(note_id.clone()));
                }
            }
            Act::CancelEdit => state.editing = None,
            Act::AskDue(i) => {
                if let Some(item) = note.items.get(i) {
                    events.push(Event::AskDue(note_id.clone(), item.id.clone()));
                }
            }
            Act::ClearDue(i) => {
                if let Some(item) = note.items.get_mut(i) {
                    item.due.clear();
                }
                events.push(Event::Changed(note_id.clone()));
            }
        }
    }

    // the "Add a task" box and Clear done
    let (px, py) = if look.compact { (10.0, 6.0) } else { (14.0, 10.0) };
    ui.add_space(py);
    ui.horizontal(|ui| {
        ui.add_space(px);
        ui.spacing_mut().item_spacing.x = 8.0;
        let clear_w = if look.compact { 0.0 } else { 96.0 };
        let response = ui.add(
            TextInput::new(&mut state.input)
                .id(state.salt.with(("add", &note_id)))
                .hint("Add a task…")
                .width(ui.available_width() - px - clear_w - 8.0),
        );
        let enter = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        if enter {
            if note.add_item(&state.input) {
                events.push(Event::Changed(note_id.clone()));
            }
            state.input.clear();
            response.request_focus();
        }
        if !look.compact
            && ui.add(Button::ghost("Clear done")).on_hover_text("Remove completed items").clicked()
        {
            note.clear_done();
            events.push(Event::Changed(note_id.clone()));
        }
    });
}

fn todo_row(
    ui: &mut Ui,
    salt: Id,
    item: &notes::Item,
    index: usize,
    editing: Option<&mut Editing>,
    env: &Env,
    acts: &mut Vec<Act>,
) -> (Rect, Response) {
    let tokens = &Tokens::of(ui.ctx());
    let p = &tokens.p;
    let width = ui.available_width();
    let px = env.todo_px + 1.0;
    let left = 2.0 + 12.0 + 8.0 + 16.0 + 8.0;
    let due_galley = (!item.due.is_empty()).then(|| {
        let late = !item.checked && notes::overdue(&item.due, env.now);
        let colour = if late { DANGER_RED } else { p.text_muted };
        let text = format!("⏰ {}", notes::due_text(&item.due, env.today));
        ui.painter().layout_no_wrap(text, tokens.font(10.0, Weight::Regular, true), colour)
    });
    let due_w = due_galley.as_ref().map_or(0.0, |g| g.size().x + 8.0);
    let right = 22.0 + 4.0;
    let text_w = (width - left - due_w - right - 4.0).max(40.0);
    let colour = if item.checked { p.text_muted } else { p.text_primary };
    let mut job = LayoutJob::single_section(
        item.text.clone(),
        TextFormat {
            font_id: tokens.font(px, Weight::Regular, false),
            color: colour,
            strikethrough: if item.checked { Stroke::new(1.0, colour) } else { Stroke::NONE },
            ..Default::default()
        },
    );
    job.wrap.max_width = text_w;
    let galley = ui.painter().layout_job(job);
    let height = (galley.size().y + 12.0).max(34.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let hovered = ui.rect_contains_pointer(rect);
    if hovered {
        ui.painter().rect_filled(rect, 6.0, p.bg_card_hover);
    }
    let y = rect.center().y;
    // the whole row answers a right click with its menu and a double click with editing
    let whole = ui.interact(rect, salt.with(("todo", &item.id)), Sense::click());
    let label_rect = Rect::from_min_size(pos2(rect.min.x + left, rect.min.y), vec2(text_w, height));
    let text_pos = pos2(label_rect.min.x, y - galley.size().y / 2.0);
    match editing {
        Some(edit) => {
            let edit_rect = Rect::from_min_size(pos2(label_rect.min.x, y - 13.0), vec2(text_w, 26.0));
            let id = salt.with(("todo-edit", &item.id));
            let response = ui.put(
                edit_rect,
                TextEdit::singleline(&mut edit.text)
                    .id(id)
                    .font(tokens.font(px, Weight::Regular, false))
                    .text_color(p.text_primary)
                    .desired_width(text_w),
            );
            if std::mem::take(&mut edit.fresh) {
                response.request_focus();
            }
            let escape = response.has_focus() && ui.input(|i| i.key_pressed(Key::Escape));
            if escape {
                acts.push(Act::CancelEdit);
            } else if response.lost_focus() {
                acts.push(Act::Commit(index, edit.text.clone()));
            }
        }
        None => {
            ui.painter().galley(text_pos, galley, colour);
            if whole.double_clicked() {
                acts.push(Act::StartEdit(index));
            }
        }
    }
    // the handle
    let handle_rect = Rect::from_min_size(pos2(rect.min.x, rect.min.y), vec2(18.0, height));
    let handle = ui.interact(handle_rect, salt.with(("todo-handle", &item.id)), Sense::drag());
    ui.painter().text(
        pos2(rect.min.x + 8.0, y),
        Align2::CENTER_CENTER,
        "⋮⋮",
        tokens.font(11.0, Weight::Regular, false),
        p.text_muted,
    );
    // the tick box
    let tick = Rect::from_center_size(pos2(rect.min.x + 2.0 + 12.0 + 8.0 + 8.0, y), vec2(16.0, 16.0));
    let tick_response = ui.interact(tick.expand(4.0), salt.with(("todo-tick", &item.id)), Sense::click());
    tick_response.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, item.checked, &item.text));
    if tick_response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let edge = if item.checked || tick_response.hovered() { p.primary } else { p.text_muted };
    if item.checked {
        ui.painter().rect_filled(tick, 4.0, p.primary);
        let at = |x: f32, y: f32| tick.min + vec2(x, y) * 16.0;
        ui.painter().add(egui::Shape::line(
            vec![at(0.24, 0.52), at(0.43, 0.72), at(0.78, 0.3)],
            Stroke::new(2.0, p.on_primary),
        ));
    } else {
        ui.painter().rect_stroke(tick, 4.0, Stroke::new(2.0, edge), StrokeKind::Inside);
    }
    if tick_response.clicked() {
        acts.push(Act::Toggle(index));
    }
    // the due date and the remove button
    let delete_rect = Rect::from_center_size(pos2(rect.max.x - 4.0 - 11.0, y), vec2(22.0, 22.0));
    if ui.put(delete_rect, IconButton::new("✕").size(22.0).font_px(10.0).ghost().tooltip("Remove")).clicked()
    {
        acts.push(Act::Remove(index));
    }
    if let Some(galley) = due_galley {
        let at = pos2(delete_rect.min.x - 8.0 - galley.size().x, y - galley.size().y / 2.0);
        let due_rect = Rect::from_min_size(at, galley.size()).expand(3.0);
        let due = ui.interact(due_rect, salt.with(("todo-due", &item.id)), Sense::click());
        ui.painter().galley(at, galley, p.text_muted);
        if due.on_hover_text("Due date (right-click to change)").clicked() {
            acts.push(Act::AskDue(index));
        }
    }
    whole.context_menu(|ui| {
        if menu::item(ui, "Edit").clicked() {
            acts.push(Act::StartEdit(index));
        }
        let label = if item.due.is_empty() { "⏰  Set due date…" } else { "⏰  Change due date…" };
        if menu::item(ui, label).clicked() {
            acts.push(Act::AskDue(index));
        }
        if !item.due.is_empty() && menu::item(ui, "Clear due date").clicked() {
            acts.push(Act::ClearDue(index));
        }
        menu::separator(ui);
        if menu::item(ui, "Remove").clicked() {
            acts.push(Act::Remove(index));
        }
    });
    (rect, handle)
}

/// The eye that switches between writing and the formatted preview.
pub fn eye_button(ui: &mut Ui, on: bool, size: f32) -> bool {
    let tip = "Show formatted (Markdown) / edit";
    let mut button = IconButton::shape(IconShape::Eye).size(size).ghost().tooltip(tip);
    if on {
        button = button.font_px(12.0);
    }
    let response = ui.add(button);
    if on {
        let tokens = Tokens::of(ui.ctx());
        ui.painter().rect_stroke(
            response.rect,
            size / 2.0,
            Stroke::new(1.0, tokens.p.primary),
            StrokeKind::Inside,
        );
    }
    response.clicked()
}

/// A short line of muted text in the middle of a room: "Pick a note on the left, or make a new one."
pub fn centred_hint(ui: &mut Ui, rect: Rect, text: &str) {
    let tokens = Tokens::of(ui.ctx());
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        tokens.font(13.0, Weight::Regular, false),
        tokens.p.text_muted,
    );
}
