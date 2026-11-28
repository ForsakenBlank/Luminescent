//! Notes: plain notes and to-do lists in a floating panel, any number of them stuck on the screen as sticky notes,
//! an overlay page and a dashboard box, all showing the same `notes.json`. The data and its rules live in
//! `lum_core::notes`; `view.rs` draws a note and `markdown.rs` draws the 👁 preview.

mod markdown;
mod view;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::time::{Duration, Instant};

use egui::{Rect, Ui, pos2, vec2};
use jiff::ZonedDateTime;
use lum_core::actions::Action;
use lum_core::module::{Module, ModuleCx, StartError};
use lum_core::notes::{self, Item, Kind, Notes};
use lum_core::settings::Settings;
use lum_platform::dialogs;

use crate::dialog::{AskDialog, DialogWindow, Outcome};
use crate::overlay::PageFrame;
use crate::palette::Entry;
use crate::panel::{FloatingPanel, Geometry, centred_on};
use crate::theme::Tokens;
use crate::tools::edit::{self, keep_on_screen};
use crate::tools::{dialog_id, dialog_window};
use crate::widgets::{Anchor, DimOverlay, IconButton};
use crate::window::Screen;
use crate::{Bind, Inbox, Request, Services, UiCx, UiModule};
use view::{Env, Event, ListStyle, Look, MenuKind, Meta, State};

const SAVE_AFTER: Duration = Duration::from_millis(400);
const STICKY_SIZE: (f32, f32) = (260.0, 240.0);
const SIDE: f32 = 36.0;

#[derive(Debug)]
enum Cmd {
    Toggle,
    ToggleEdit,
    NewNote,
}

#[derive(Debug)]
enum Ask {
    NewName,
    NewType(String),
    Rename(String),
    Tags(String),
    Due(String, String),
}

/// One note stuck on the screen as a panel of its own.
#[derive(Debug)]
struct Sticky {
    id: String,
    panel: FloatingPanel,
    state: State,
}

#[derive(Debug)]
struct PageState {
    list: State,
    note: State,
    current: Option<String>,
    /// The pass the page was last drawn in, to notice it coming back. `None` before the first time.
    pass: Option<u64>,
}

pub struct NotesModule {
    services: Services,
    inbox: Inbox<Cmd>,
    notes: Option<Notes>,
    /// The tasks of each to-do note as reminders last heard of them, to tell what changed.
    known: HashMap<String, Vec<Item>>,
    panel: FloatingPanel,
    panel_state: State,
    /// The note the panel shows, or the list when none.
    current: Option<String>,
    stickies: Vec<Sticky>,
    page: PageState,
    dialog: Option<(DialogWindow, Ask)>,
    edit: bool,
    open_pinned: bool,
    screens: Vec<Screen>,
    save_due: Option<Instant>,
}

impl std::fmt::Debug for NotesModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotesModule").field("running", &self.notes.is_some()).finish_non_exhaustive()
    }
}

fn number(settings: &Settings, key: &str, default: f32) -> f32 {
    settings.get_f64(&format!("notes.{key}")).map_or(default, |v| v as f32)
}

fn env(settings: &Settings) -> Env {
    let now = ZonedDateTime::now();
    Env {
        today: now.date(),
        now: now.datetime(),
        font_px: number(settings, "font_size", 16.0),
        todo_px: number(settings, "todo_font_size", 12.0),
        markdown: settings.get_bool("notes.markdown", true),
    }
}

impl NotesModule {
    pub fn new(services: &Services) -> NotesModule {
        let geometry = Geometry { x: 100.0, y: 100.0, w: 360.0, h: 460.0 };
        NotesModule {
            services: services.clone(),
            inbox: Inbox::new(services),
            notes: None,
            known: HashMap::new(),
            panel: FloatingPanel::new("notes", "Notes", geometry).drag_needs_edit_mode(false),
            panel_state: State::new("panel"),
            current: None,
            stickies: Vec::new(),
            page: PageState {
                list: State::new("page-list"),
                note: State::new("page-note"),
                current: None,
                pass: None,
            },
            dialog: None,
            edit: false,
            open_pinned: true,
            screens: Vec::new(),
            save_due: None,
        }
    }

    fn path(&self) -> std::path::PathBuf {
        self.services.data_file("notes.json")
    }

    fn open_dialog(&mut self, dialog: AskDialog, ask: Ask) {
        self.dialog = Some((dialog_window(dialog), ask));
    }

    fn screen_rects(&self) -> Vec<Rect> {
        self.screens.iter().map(|s| s.rect).collect()
    }

    fn place_panel(&mut self, settings: &Settings) {
        let Some(notes) = &self.notes else { return };
        let [x, y, w, h] = notes.geometry();
        self.screens = crate::window::screens();
        let g = keep_on_screen(Geometry { x, y, w, h }, &self.screen_rects());
        self.panel.geometry = g;
        self.panel.opacity = number(settings, "bg_opacity", 0.85);
        self.panel
            .set_min_size(vec2(number(settings, "min_width", 250.0), number(settings, "min_height", 180.0)));
    }

    // ── what the views ask for ─────────────────────────────────────────────

    fn changed(&mut self, id: &str) {
        let Some(notes) = self.notes.as_mut() else { return };
        notes.mark_dirty();
        let Some(note) = notes.note(id) else { return };
        if note.is_todo() {
            let before = self.known.remove(id).unwrap_or_default();
            for sync in notes::todo_changes(note, &before, ZonedDateTime::now().datetime()) {
                self.services.send(Request::TodoReminder(sync));
            }
            self.known.insert(id.to_owned(), note.items.clone());
        }
    }

    fn open_in_panel(&mut self, id: &str) {
        if self.notes.as_ref().is_some_and(|n| n.index_of(id).is_some()) {
            self.current = Some(id.to_owned());
            self.panel_state.reset_note();
        }
    }

    fn handle(&mut self, events: Vec<Event>, from_page: bool) {
        for event in events {
            self.handle_one(event, from_page);
        }
    }

    fn handle_one(&mut self, event: Event, from_page: bool) {
        let Some(notes) = self.notes.as_mut() else { return };
        match event {
            Event::Open(id) if from_page => {
                self.page.current = Some(id);
                self.page.note.reset_note();
            }
            Event::Open(id) => self.open_in_panel(&id),
            Event::Back if from_page => self.page.current = None,
            Event::Back => {
                self.current = None;
                self.panel_state.reset_note();
            }
            Event::New => self.open_dialog(
                AskDialog::ask_text(dialog_id("note-name"), "New Note", "Name:", ""),
                Ask::NewName,
            ),
            Event::Pin(id, on) => {
                if let Some(note) = notes.note_mut(&id) {
                    note.pinned = on;
                    notes.mark_dirty();
                }
            }
            Event::Rename(id) => {
                let name = notes.note(&id).map(|n| n.name.clone()).unwrap_or_default();
                self.open_dialog(
                    AskDialog::ask_text(dialog_id("note-rename"), "Rename", "New name:", &name),
                    Ask::Rename(id),
                );
            }
            Event::Colour(id, colour) => {
                if let Some(note) = notes.note_mut(&id) {
                    note.colour = colour;
                    notes.mark_dirty();
                }
            }
            Event::Tags(id) => {
                let tags = notes.note(&id).map(|n| n.tags.join(", ")).unwrap_or_default();
                let label = "Tags, separated by commas (search them with #tag):";
                self.open_dialog(
                    AskDialog::ask_text(dialog_id("note-tags"), "Tags", label, &tags),
                    Ask::Tags(id),
                );
            }
            Event::Stick(id) => self.stick(&id),
            Event::Unstick(id) => self.unstick(&id),
            Event::Export(id) => {
                let named = notes.note(&id).map(|n| (n.export_name(), n.markdown()));
                if let Some((name, text)) = named {
                    self.export(name, text);
                }
            }
            Event::Delete(id) => self.delete(&id),
            Event::Move(id, delta) => {
                if let Some(at) = notes.index_of(&id) {
                    notes.move_note(at, delta);
                }
            }
            Event::Reorder(from, to) => notes.reorder(from, to),
            Event::Changed(id) => self.changed(&id),
            Event::AskDue(note_id, item_id) => self.ask_due(&note_id, &item_id),
            Event::Undo => self.restore(),
            Event::Preview(on) if from_page => self.page.note.preview = on,
            Event::Preview(on) => notes.set_preview(on),
        }
    }

    fn ask_due(&mut self, note_id: &str, item_id: &str) {
        let Some(item) = self
            .notes
            .as_ref()
            .and_then(|n| n.note(note_id))
            .and_then(|n| n.items.iter().find(|i| i.id == item_id))
        else {
            return;
        };
        let now = ZonedDateTime::now().datetime();
        let at = notes::parse_due(&item.due)
            .or_else(|| now.checked_add(jiff::Span::new().hours(1)).ok())
            .unwrap_or(now);
        let text =
            format!("When is \u{201c}{}\u{201d} due?\nLuminescent reminds you at that time.", item.text);
        let dialog = AskDialog::ask_datetime(dialog_id("note-due"), "Due date", &text, at);
        self.open_dialog(dialog, Ask::Due(note_id.to_owned(), item_id.to_owned()));
    }

    fn delete(&mut self, id: &str) {
        let Some(notes) = self.notes.as_mut() else { return };
        let Some(at) = notes.index_of(id) else { return };
        let Some(note) = notes.delete(at, Instant::now()) else { return };
        self.stickies.retain(|s| s.id != id);
        for item in note.items.iter().filter(|i| !i.due.is_empty()) {
            self.services
                .send(Request::TodoReminder(notes::TodoSync::Drop { id: notes::reminder_id(&item.id) }));
        }
        self.known.remove(id);
        if self.current.as_deref() == Some(id) {
            self.current = None;
        }
        if self.page.current.as_deref() == Some(id) {
            self.page.current = None;
        }
    }

    fn restore(&mut self) {
        let Some(notes) = self.notes.as_mut() else { return };
        let Some(note) = notes.restore() else { return };
        let now = ZonedDateTime::now().datetime();
        for item in note.items.iter().filter(|i| !i.due.is_empty()) {
            self.services.send(Request::TodoReminder(notes::sync_for(&note, item, now)));
        }
        self.known.insert(note.id.clone(), note.items);
    }

    fn export(&self, name: String, text: String) {
        let services = self.services.clone();
        let documents = lum_core::paths::user_home().join("Documents");
        let request = dialogs::Request::new("Export note")
            .folder(documents.is_dir().then_some(documents))
            .file_name(&format!("{name}.md"))
            .filter("Markdown", &["md"])
            .filter("Text", &["txt"]);
        dialogs::save_file(request, move |path| {
            let Some(mut path) = path else { return };
            if path.extension().is_none() {
                path.set_extension("md");
            }
            match std::fs::write(&path, text) {
                Ok(()) => services.send(Request::Notify {
                    title: "Notes".into(),
                    text: format!("Exported to {}", path.display()),
                    ms: 4000,
                }),
                Err(e) => services.send(Request::Toast {
                    title: "Export note".into(),
                    text: format!("Could not write the file:\n{e}"),
                    ms: 6000,
                }),
            }
        });
    }

    // ── sticky notes ───────────────────────────────────────────────────────

    fn make_sticky(&mut self, id: &str, place: Option<[f32; 4]>, opacity: f32) {
        let geometry = match place {
            Some([x, y, w, h]) => keep_on_screen(Geometry { x, y, w, h }, &self.screen_rects()),
            None => {
                let screen = edit::cursor_screen_rect()
                    .unwrap_or(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0)));
                let at = centred_on(vec2(STICKY_SIZE.0, STICKY_SIZE.1), screen);
                Geometry { x: at.x, y: at.y, w: STICKY_SIZE.0, h: STICKY_SIZE.1 }
            }
        };
        let mut panel = FloatingPanel::new(&format!("sticky-{id}"), "Sticky", geometry)
            .min_size(vec2(180.0, 120.0))
            .header_height(32.0)
            .drag_needs_edit_mode(false);
        panel.opacity = opacity;
        panel.open();
        self.stickies.push(Sticky { id: id.to_owned(), panel, state: State::new(&format!("sticky-{id}")) });
    }

    fn stick(&mut self, id: &str) {
        if self.notes.as_ref().is_none_or(|n| n.index_of(id).is_none()) {
            return;
        }
        if let Some(sticky) = self.stickies.iter_mut().find(|s| s.id == id) {
            sticky.panel.open();
            return;
        }
        self.make_sticky(id, None, self.panel.opacity);
        let place = self.stickies.last().map(|s| s.panel.geometry);
        if let (Some(g), Some(notes)) = (place, self.notes.as_mut()) {
            notes.set_sticky(id, [g.x, g.y, g.w, g.h]);
        }
    }

    fn unstick(&mut self, id: &str) {
        self.stickies.retain(|s| s.id != id);
        if let Some(notes) = self.notes.as_mut() {
            notes.remove_sticky(id);
        }
    }

    // ── dialogs, commands and saving ───────────────────────────────────────

    fn show_dialog(&mut self, ctx: &egui::Context) {
        let Some((window, _)) = self.dialog.as_mut() else { return };
        let outcome = window.show(ctx);
        if outcome == Outcome::Open {
            return;
        }
        let text = window.dialog.text().trim().to_owned();
        let chosen = window.dialog.chosen().to_owned();
        let when = window.dialog.when();
        let Some((_, ask)) = self.dialog.take() else { return };
        if !matches!(outcome, Outcome::Accepted(_)) {
            return;
        }
        match ask {
            Ask::NewName if !text.is_empty() => {
                let dialog =
                    AskDialog::ask_choice(dialog_id("note-type"), "Type", "Choose type:", &["note", "todo"]);
                self.open_dialog(dialog, Ask::NewType(text));
            }
            Ask::NewName => {}
            Ask::NewType(name) => {
                let kind = if chosen == "todo" { Kind::Todo } else { Kind::Note };
                let Some(notes) = self.notes.as_mut() else { return };
                let at = notes.add(&name, kind);
                let id = notes.notes[at].id.clone();
                self.page.current = Some(id.clone());
                self.page.note.reset_note();
                self.open_in_panel(&id);
            }
            Ask::Rename(id) => {
                if let Some(notes) = self.notes.as_mut()
                    && !text.is_empty()
                    && let Some(note) = notes.note_mut(&id)
                {
                    note.name = text;
                    notes.mark_dirty();
                }
            }
            Ask::Tags(id) => {
                if let Some(notes) = self.notes.as_mut()
                    && let Some(note) = notes.note_mut(&id)
                {
                    note.tags = notes::parse_tags(&text);
                    notes.mark_dirty();
                }
            }
            Ask::Due(note_id, item_id) => {
                let Some(when) = when else { return };
                let due = when.strftime("%Y-%m-%dT%H:%M").to_string();
                if let Some(notes) = self.notes.as_mut()
                    && let Some(item) =
                        notes.note_mut(&note_id).and_then(|n| n.items.iter_mut().find(|i| i.id == item_id))
                {
                    item.due = due;
                }
                self.changed(&note_id);
            }
        }
    }

    fn take_commands(&mut self, cx: &UiCx<'_>) {
        for cmd in self.inbox.take() {
            match cmd {
                Cmd::Toggle => self.toggle(),
                Cmd::ToggleEdit => {
                    if !self.panel.is_open() {
                        self.panel.open();
                    }
                    self.set_edit(!self.edit, cx.ctx);
                }
                Cmd::NewNote => {
                    self.panel.open();
                    self.current = None;
                    self.handle_one(Event::New, false);
                }
            }
        }
    }

    fn toggle(&mut self) {
        let Some(notes) = self.notes.as_ref() else { return };
        let pinned = notes.first_pinned().map(|pin| notes.notes[pin].id.clone());
        if !self.panel.is_open()
            && self.open_pinned
            && let Some(id) = pinned
            && self.current.as_deref() != Some(id.as_str())
        {
            self.open_in_panel(&id);
        }
        self.panel.toggle();
    }

    fn set_edit(&mut self, on: bool, ctx: &egui::Context) {
        self.edit = on;
        self.panel.edit_mode = on;
        if on {
            self.screens = crate::window::screens();
            if self.screens.is_empty() {
                self.screens = vec![crate::window::fallback_screen(ctx)];
            }
        } else {
            self.save_now();
        }
    }

    fn save_now(&mut self) {
        self.save_due = None;
        let Some(notes) = self.notes.as_mut() else { return };
        let g = self.panel.geometry;
        notes.set_geometry([g.x, g.y, g.w, g.h]);
        let open = self.current.as_deref().and_then(|id| notes.index_of(id));
        notes.set_open(open);
        if notes.is_dirty() {
            notes.save();
        }
    }

    fn after(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let Some(notes) = self.notes.as_mut() else { return };
        if let Some((_, left)) = notes.undo(now) {
            ctx.request_repaint_after(left + Duration::from_millis(50));
        }
        if notes.is_dirty() && self.save_due.is_none() {
            self.save_due = Some(now + SAVE_AFTER);
        }
        if let Some(due) = self.save_due {
            if now >= due {
                self.save_now();
            } else {
                ctx.request_repaint_after(due - now);
            }
        }
    }
}

impl NotesModule {
    fn draw_panel(&mut self, cx: &mut UiCx<'_>, env: &Env) {
        let Some(notes) = self.notes.as_mut() else { return };
        if self.current.as_deref().is_some_and(|id| notes.index_of(id).is_none()) {
            self.current = None;
        }
        let meta = self.current.as_deref().and_then(|id| notes.note(id)).map(|n| Meta::of(n, notes));
        let text_note = self.current.as_deref().and_then(|id| notes.note(id)).is_some_and(|n| !n.is_todo());
        let subtitle =
            self.current.as_deref().and_then(|id| notes.note(id)).map(|n| n.name.clone()).unwrap_or_default();
        self.panel.set_title("Notes", &subtitle);
        self.panel.set_back(meta.is_some().then_some("Back to all notes"));
        let preview = notes.preview();
        let edit = self.edit;
        let events = RefCell::new(Vec::new());
        let toggle_edit = Cell::new(false);
        let undo = notes.undo(Instant::now()).map(|(name, _)| name.to_owned());
        let current = self.current.clone();
        let state = &mut self.panel_state;
        let response = self.panel.show_with(
            cx.ctx,
            |ui| {
                let more =
                    ui.add(IconButton::new("⋯").size(26.0).font_px(12.0).ghost().tooltip("Note options"));
                egui::Popup::menu(&more).show(|ui| {
                    let mut out = events.borrow_mut();
                    match &meta {
                        None => {
                            if crate::widgets::menu::item(ui, "New note…").clicked() {
                                out.push(Event::New);
                            }
                        }
                        Some(meta) => {
                            let kind = MenuKind { list: false, can_up: false, can_down: false };
                            view::note_menu(ui, meta, &kind, &mut out);
                        }
                    }
                    crate::widgets::menu::separator(ui);
                    let label = if edit { "Exit edit mode" } else { "Edit mode (move, resize, anchor)" };
                    if crate::widgets::menu::item(ui, label).clicked() {
                        toggle_edit.set(true);
                    }
                });
                if text_note && env.markdown && view::eye_button(ui, preview, 26.0) {
                    events.borrow_mut().push(Event::Preview(!preview));
                }
            },
            |ui| {
                let full = ui.max_rect();
                let (content, side) = if edit {
                    let (c, s) = full.split_left_right_at_x(full.max.x - SIDE);
                    (c, Some(s))
                } else {
                    (full, None)
                };
                let mut out = events.borrow_mut();
                match current.as_deref() {
                    None => {
                        let inner = content.shrink2(vec2(14.0, 12.0));
                        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
                        view::list(
                            &mut child,
                            notes,
                            state,
                            ListStyle::Panel,
                            None,
                            undo.as_deref(),
                            &mut out,
                        );
                    }
                    Some(id) => {
                        if let Some(note) = notes.note_mut(id) {
                            let look = Look { pad: vec2(14.0, 12.0), compact: false, title: true, preview };
                            view::body(ui, content, note, state, look, env, &mut out);
                        }
                    }
                }
                if let Some(side) = side {
                    let mut col = ui.new_child(egui::UiBuilder::new().max_rect(side.shrink2(vec2(4.0, 8.0))));
                    col.spacing_mut().item_spacing.y = 6.0;
                    match current.as_deref() {
                        None => {
                            if col
                                .add(IconButton::new("+").size(26.0).font_px(14.0).tooltip("Add a new note"))
                                .clicked()
                            {
                                out.push(Event::New);
                            }
                        }
                        Some(id) => {
                            if col
                                .add(
                                    IconButton::new("✎").size(26.0).font_px(13.0).tooltip("Rename this note"),
                                )
                                .clicked()
                            {
                                out.push(Event::Rename(id.to_owned()));
                            }
                            if col
                                .add(
                                    IconButton::new("🗑").size(26.0).font_px(12.0).tooltip("Delete this note"),
                                )
                                .clicked()
                            {
                                out.push(Event::Delete(id.to_owned()));
                            }
                        }
                    }
                }
            },
        );
        let mut events = events.into_inner();
        if response.back {
            events.push(Event::Back);
        }
        if response.geometry_changed
            && let Some(notes) = self.notes.as_mut()
        {
            let g = self.panel.geometry;
            notes.set_geometry([g.x, g.y, g.w, g.h]);
        }
        if response.escape && self.edit {
            self.set_edit(false, cx.ctx);
        }
        if toggle_edit.get() {
            self.set_edit(!self.edit, cx.ctx);
        }
        self.handle(events, false);
    }

    fn draw_stickies(&mut self, cx: &mut UiCx<'_>, env: &Env) {
        let Some(notes) = self.notes.as_mut() else { return };
        let mut events = Vec::new();
        let mut gone = Vec::new();
        for sticky in &mut self.stickies {
            let Some(note) = notes.note(&sticky.id) else {
                gone.push(sticky.id.clone());
                continue;
            };
            sticky.panel.set_title("Sticky", &note.name);
            let accent = view::note_colour(note).map(|c| (c, 64.0));
            sticky.panel.set_accent(accent);
            let meta = Meta::of(note, notes);
            let text_note = !note.is_todo();
            let id = sticky.id.clone();
            let preview = sticky.state.preview;
            let out = RefCell::new(Vec::new());
            let state = &mut sticky.state;
            let response = sticky.panel.show_with(
                cx.ctx,
                |ui| {
                    let more = ui
                        .add(IconButton::new("⋯").size(24.0).font_px(12.0).ghost().tooltip("Sticky options"));
                    egui::Popup::menu(&more).show(|ui| {
                        if crate::widgets::menu::item(ui, "Open in the Notes panel").clicked() {
                            out.borrow_mut().push(Event::Open(meta.id.clone()));
                        }
                        if crate::widgets::menu::item(ui, "Unstick (close)").clicked() {
                            out.borrow_mut().push(Event::Unstick(meta.id.clone()));
                        }
                    });
                    if text_note && env.markdown && view::eye_button(ui, preview, 24.0) {
                        out.borrow_mut().push(Event::Preview(!preview));
                    }
                },
                |ui| {
                    if let Some(note) = notes.note_mut(&id) {
                        let look = Look { pad: vec2(12.0, 10.0), compact: true, title: false, preview };
                        view::body(ui, ui.max_rect(), note, state, look, env, &mut out.borrow_mut());
                    }
                },
            );
            for event in out.into_inner() {
                match event {
                    Event::Preview(on) => sticky.state.preview = on,
                    Event::Open(id) => {
                        self.current = Some(id);
                        self.panel.open();
                    }
                    other => events.push(other),
                }
            }
            if response.closed {
                events.push(Event::Unstick(sticky.id.clone()));
            }
            if response.geometry_changed {
                let g = sticky.panel.geometry;
                notes.set_sticky(&sticky.id, [g.x, g.y, g.w, g.h]);
            }
        }
        self.stickies.retain(|s| !gone.contains(&s.id));
        self.handle(events, false);
    }

    /// Edit mode: the screens dimmed and the bar of anchors that snaps the panel.
    fn edit_windows(&mut self, cx: &UiCx<'_>) {
        if !self.edit {
            return;
        }
        let dim = DimOverlay { opacity: number(cx.settings, "overlay_dim", 0.35), click_through: true };
        dim.show(cx.ctx, &self.screens);
        let screen = edit::cursor_screen_rect()
            .or_else(|| self.screens.first().map(|s| s.rect))
            .unwrap_or_else(|| crate::window::fallback_screen(cx.ctx).rect);
        let Some(anchor) = edit::anchor_window(cx.ctx, "notes", "Snap notes to:", screen) else { return };
        let padding = number(cx.settings, "anchor_padding", 10.0);
        self.snap(anchor, padding, screen);
    }

    fn snap(&mut self, anchor: Anchor, padding: f32, screen: Rect) {
        let size = vec2(self.panel.geometry.w, self.panel.geometry.h);
        let at = anchor.place(size, padding, screen);
        self.panel.geometry.x = at.x;
        self.panel.geometry.y = at.y;
        if let Some(notes) = self.notes.as_mut() {
            let g = self.panel.geometry;
            notes.set_geometry([g.x, g.y, g.w, g.h]);
        }
    }

    fn draw_page(&mut self, ui: &mut Ui, rect: Rect, cx: &mut UiCx<'_>, env: &Env) {
        let tokens = Tokens::of(cx.ctx);
        let Some(notes) = self.notes.as_mut() else { return };
        let pass = cx.ctx.cumulative_pass_nr();
        let returning = self.page.pass.is_none_or(|last| last + 1 < pass);
        self.page.pass = Some(pass);
        if self.page.current.as_deref().is_some_and(|id| notes.index_of(id).is_none()) {
            self.page.current = None;
        }
        if returning && self.page.current.is_none() && wide(rect) {
            // the pinned note, or else the first, is open when the page comes up
            let first = notes.first_pinned().or((!notes.notes.is_empty()).then_some(0));
            self.page.current = first.map(|i| notes.notes[i].id.clone());
        }
        let area = Rect::from_min_max(rect.min + vec2(24.0, 12.0), rect.max - vec2(24.0, 24.0));
        let mut events = Vec::new();
        let undo = notes.undo(Instant::now()).map(|(name, _)| name.to_owned());
        let (list_rect, note_rect) = if wide(rect) {
            let left = Rect::from_min_size(area.min, vec2(320.0, area.height()));
            let right = Rect::from_min_max(pos2(left.max.x + 20.0, area.min.y), area.max);
            (Some(left), Some(right))
        } else if self.page.current.is_some() {
            (None, Some(area))
        } else {
            (Some(area), None)
        };
        if let Some(list_rect) = list_rect {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(list_rect));
            child.set_clip_rect(list_rect.intersect(ui.clip_rect()));
            view::list(
                &mut child,
                notes,
                &mut self.page.list,
                ListStyle::Page,
                self.page.current.as_deref(),
                undo.as_deref(),
                &mut events,
            );
        }
        if let Some(note_rect) = note_rect {
            ui.painter().rect_filled(note_rect, tokens.r(), tokens.p.bg_card);
            ui.painter().rect_stroke(
                note_rect,
                tokens.r(),
                egui::Stroke::new(1.0, tokens.p.border),
                egui::StrokeKind::Inside,
            );
            self.draw_page_note(ui, note_rect, env, &mut events);
        }
        self.handle(events, true);
    }

    fn draw_page_note(&mut self, ui: &mut Ui, rect: Rect, env: &Env, events: &mut Vec<Event>) {
        let Some(notes) = self.notes.as_mut() else { return };
        let current = self.page.current.clone();
        let Some(id) = current.as_deref() else {
            view::centred_hint(ui, rect, "Pick a note on the left, or make a new one.");
            return;
        };
        let Some(note) = notes.note(id) else { return };
        let meta = Meta::of(note, notes);
        let name = note.name.clone();
        let text_note = !note.is_todo();
        let bar = Rect::from_min_size(rect.min, vec2(rect.width(), 48.0));
        let mut bar_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(bar.shrink2(vec2(12.0, 0.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        if !wide(ui.max_rect())
            && bar_ui
                .add(IconButton::new("←").size(26.0).font_px(13.0).ghost().tooltip("Back to all notes"))
                .clicked()
        {
            events.push(Event::Back);
        }
        bar_ui.add_space(6.0);
        bar_ui.add(crate::text::Text::new(name).px(16.0).semibold().elide());
        bar_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let more = ui.add(IconButton::new("⋯").size(28.0).font_px(12.0).ghost().tooltip("Note options"));
            egui::Popup::menu(&more).show(|ui| {
                let kind = MenuKind { list: false, can_up: false, can_down: false };
                view::note_menu(ui, &meta, &kind, events);
            });
            if text_note && env.markdown && view::eye_button(ui, self.page.note.preview, 28.0) {
                events.push(Event::Preview(!self.page.note.preview));
            }
        });
        let body = Rect::from_min_max(pos2(rect.min.x, bar.max.y), rect.max);
        let preview = self.page.note.preview;
        if let Some(note) = notes.note_mut(id) {
            let look = Look { pad: vec2(18.0, 8.0), compact: false, title: false, preview };
            view::body(ui, body, note, &mut self.page.note, look, env, events);
        }
    }
}

fn wide(rect: Rect) -> bool {
    rect.width() >= 640.0
}

impl Module for NotesModule {
    fn key(&self) -> &'static str {
        "notes"
    }

    fn title(&self) -> &'static str {
        "Notes"
    }

    fn start(&mut self, cx: &mut ModuleCx<'_>) -> Result<(), StartError> {
        let width = cx.settings.get_i64("notes.default_width").unwrap_or(360);
        let height = cx.settings.get_i64("notes.default_height").unwrap_or(460);
        let mut notes = Notes::load(&self.path(), width, height);
        notes.forget_lost_stickies();
        self.known =
            notes.notes.iter().filter(|n| n.is_todo()).map(|n| (n.id.clone(), n.items.clone())).collect();
        let places = notes.sticky_places();
        let open = notes.open_note().map(|i| notes.notes[i].id.clone());
        self.notes = Some(notes);
        self.open_pinned = cx.settings.get_bool("notes.open_pinned", true);
        self.place_panel(cx.settings);
        let opacity = self.panel.opacity;
        self.current = open;
        for (id, place) in places {
            self.make_sticky(&id, Some(place), opacity);
        }
        if cx.shows_on_start("notes") {
            self.panel.open();
        }
        Ok(())
    }

    fn stop(&mut self, _cx: &mut ModuleCx<'_>) {
        self.save_now();
        if let Some(notes) = self.notes.as_mut() {
            notes.flush();
        }
        self.notes = None;
        self.panel.close();
        self.stickies.clear();
        self.dialog = None;
        self.edit = false;
        self.panel.edit_mode = false;
        self.current = None;
        self.known.clear();
    }

    /// Opacity, sizes and the font change in place; the open note and the stickies stay where they are.
    fn apply_settings(&mut self, cx: &mut ModuleCx<'_>) -> Result<(), StartError> {
        self.open_pinned = cx.settings.get_bool("notes.open_pinned", true);
        let opacity = number(cx.settings, "bg_opacity", 0.85);
        self.panel.opacity = opacity;
        self.panel.set_min_size(vec2(
            number(cx.settings, "min_width", 250.0),
            number(cx.settings, "min_height", 180.0),
        ));
        for sticky in &mut self.stickies {
            sticky.panel.opacity = opacity;
        }
        Ok(())
    }

    fn actions(&self) -> Vec<Action> {
        vec![
            self.inbox.action("toggle", "Show / hide Notes", || Cmd::Toggle),
            self.inbox.action("edit", "Notes edit mode", || Cmd::ToggleEdit),
            self.inbox.action("new", "New note…", || Cmd::NewNote),
        ]
    }
}

impl UiModule for NotesModule {
    fn binds(&self) -> Vec<Bind> {
        vec![
            Bind { setting: "hotkey", id: "toggle", label: "show/hide notes" },
            Bind { setting: "edit_hotkey", id: "edit", label: "notes edit mode" },
        ]
    }

    fn bind_pressed(&mut self, id: &str) {
        match id {
            "toggle" => self.inbox.push(Cmd::Toggle),
            "edit" => self.inbox.push(Cmd::ToggleEdit),
            _ => {}
        }
    }

    fn page_key(&self) -> Option<&'static str> {
        Some("notes")
    }

    fn reset_page(&mut self) {
        self.page = PageState {
            list: State::new("page-list"),
            note: State::new("page-note"),
            current: None,
            pass: None,
        };
    }

    fn tray_actions(&self) -> Vec<(String, String)> {
        vec![("toggle".into(), "Show / hide Notes".into())]
    }

    fn show(&mut self, cx: &mut UiCx<'_>) {
        self.take_commands(cx);
        if self.notes.is_none() {
            return;
        }
        self.show_dialog(cx.ctx);
        let env = env(cx.settings);
        if self.edit && !self.panel.is_open() {
            self.set_edit(false, cx.ctx);
        }
        if self.panel.is_open() {
            self.draw_panel(cx, &env);
        }
        self.draw_stickies(cx, &env);
        self.edit_windows(cx);
        self.after(cx.ctx);
    }

    fn page(&mut self, ui: &mut Ui, rect: Rect, cx: &mut UiCx<'_>) -> PageFrame {
        self.take_commands(cx);
        if self.notes.is_none() {
            return PageFrame::default();
        }
        let env = env(cx.settings);
        self.show_dialog(cx.ctx);
        self.draw_page(ui, rect, cx, &env);
        self.after(cx.ctx);
        PageFrame { busy: self.dialog.is_some() }
    }

    fn palette_entries(&self) -> Vec<Entry> {
        let Some(notes) = &self.notes else { return Vec::new() };
        notes
            .notes
            .iter()
            .map(|n| {
                let pin = if n.pinned { "📌 " } else { "" };
                Entry::new(format!("note:{}", n.id), format!("{pin}{}", n.name), n.palette_hint(), "note")
            })
            .collect()
    }

    fn run_palette(&mut self, id: &str, _services: &Services) -> bool {
        let Some(note) = id.strip_prefix("note:") else { return false };
        self.open_in_panel(note);
        self.panel.open();
        true
    }

    fn show_note(&mut self, id: &str) -> bool {
        // Python's show_note_id: a note that is not there leaves the panel as it was
        if self.notes.as_ref().is_some_and(|n| n.index_of(id).is_some()) {
            self.open_in_panel(id);
            self.panel.open();
        }
        true
    }

    fn note_choices(&self) -> Vec<(String, String)> {
        let Some(notes) = &self.notes else { return Vec::new() };
        notes
            .notes
            .iter()
            .map(|n| (n.id.clone(), if n.name.is_empty() { "Note".to_owned() } else { n.name.clone() }))
            .collect()
    }
}
