//! The theme editor (`ThemeDialog` in `ui/themes_page.py`): everything about one theme in three columns.
//! Left: the parts of it (General, Colours, every module, Notifications…); middle: that part's settings;
//! right: a live preview. Drag the lines between the columns to resize them. Changes show in the preview at
//! once but only reach the theme when Save is pressed; Undo and Redo walk through the steps meanwhile.

use std::path::PathBuf;

use egui::{Align, Context, Id, Key, Layout, Modifiers, Rect, Sense, Stroke, Ui, UiBuilder, pos2, vec2};
use lum_platform::hotkeys::keys;
use lum_theme::history::{History, Recent, note_change};
use lum_theme::{Role, Theme, ThemeStore};
use serde_json::{Map, Value};

use crate::dialog::{AskDialog, Outcome};
use crate::panel::{FloatingPanel, Geometry};
use crate::picker::{ColourDialog, Picked, PickerEvent, PickerKind};
use crate::text::caps;
use crate::theme::{Palette, Tokens, Tone};
use crate::themes::page::{FileAsk, FileJob};
use crate::themes::previews::Reg;
use crate::themes::{Env, MODULES};
use crate::widgets::{Button, IconButton, tip};
use tools::{BulkDialog, Tool};
use values::{Fields, picked_from_value, value_of};

mod pages;
mod preview;
pub mod tools;
mod values;

/// How long a run of small edits (dragging in the colour picker) counts as one step to undo.
const BURST: f64 = 0.7;
const FOOTER: f32 = 54.0;
const SIDE_MIN: f32 = 170.0;
const PREVIEW_MIN: f32 = 260.0;
const MIDDLE_MIN: f32 = 380.0;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Page {
    General,
    Colours,
    Applied,
    /// A page of the desktop integration (`caelestia_colours`, `caelestia_look`, `glass`, `borders`,
    /// `terminal`), which comes in a later build.
    Desktop(&'static str),
    /// The look of one module, or of `toasts` or `alerts`.
    Section(String),
}

pub(super) const DESKTOP: [(&str, &str, &str); 5] = [
    ("caelestia_colours", "◐", "Caelestia colours"),
    ("caelestia_look", "◐", "Caelestia look"),
    ("glass", "▢", "Glass"),
    ("borders", "▭", "Window borders"),
    ("terminal", "▣", "Terminal"),
];

/// Parts of a theme that can have their own look besides the modules: key, title, icon, what it covers.
pub(super) const EXTRA: [(&str, &str, &str, &str); 2] = [
    ("toasts", "Notifications", "◔", "The notification cards in the corner of the screen."),
    ("alerts", "Warnings on screen", "⚠", "The big warnings in the middle of the screen (Automation)."),
];

/// What a question the editor asked is for.
#[derive(Debug)]
enum Ask {
    Reset(String),
    Discard,
    Message,
}

/// A colour being picked in the shared dialog, and what it was before, for Cancel.
#[derive(Debug)]
struct Picking {
    target: String,
    before: Option<Picked>,
}

/// What the app has to do after a frame of the editor.
#[derive(Debug, Default)]
pub struct EditorOut {
    pub files: Vec<FileAsk>,
    /// Hotkeys must not fire while a key is being recorded (`Some(true)`), and may again afterwards.
    pub suspend_keys: Option<bool>,
}

#[derive(Debug)]
pub struct ThemeEditor {
    panel: FloatingPanel,
    id: String,
    t: Theme,
    fields: Fields,
    /// The theme as it was when the editor opened.
    saved: Value,
    history: History,
    recents: Vec<Recent>,
    page: Page,
    /// The colour the pointer is on (lit up in the preview), and one the preview was clicked on (its row
    /// scrolls into view and flashes).
    hover: Option<Role>,
    jump: Option<Role>,
    flash: Option<(Role, f64)>,
    reg: Reg,
    status: String,
    /// Something changed this frame.
    changed: bool,
    /// When the last edit was made, until it is a step of its own.
    edited_at: Option<f64>,
    restoring: bool,
    picking: Option<Picking>,
    ask: Option<(Ask, AskDialog)>,
    tool: Option<BulkDialog>,
    recording: bool,
    key_warning: String,
    wallpaper_missing: bool,
    checked_wallpaper: bool,
    left: f32,
    right: f32,
    /// Closed by Save or by giving up.
    pub finished: bool,
}

impl ThemeEditor {
    pub fn new(theme: &Theme, recents: Vec<Recent>, near: Rect) -> ThemeEditor {
        let mut t = theme.clone();
        t.colours_mut();
        let gradients = t.gradients();
        t.set_gradients(&gradients);
        let fields = Fields::of(&t);
        let size = vec2(
            1240.0f32.min(near.width() - 40.0).max(820.0),
            820.0f32.min(near.height() - 40.0).max(540.0),
        );
        let mut panel = FloatingPanel::new(
            "theme-editor",
            "Theme settings",
            Geometry::from_rect(Rect::from_center_size(near.center(), size)),
        )
        .min_size(vec2(820.0, 540.0));
        panel.opacity = 1.0;
        let mut editor = ThemeEditor {
            panel,
            id: theme.id().to_owned(),
            t,
            fields,
            saved: Value::Null,
            history: History::new(&Value::Null),
            recents,
            page: Page::General,
            hover: None,
            jump: None,
            flash: None,
            reg: Reg::default(),
            status: String::new(),
            changed: false,
            edited_at: None,
            restoring: false,
            picking: None,
            ask: None,
            tool: None,
            recording: false,
            key_warning: String::new(),
            wallpaper_missing: false,
            checked_wallpaper: false,
            left: 210.0,
            right: 380.0,
            finished: false,
        };
        let first = editor.snapshot();
        editor.saved = first.clone();
        editor.history = History::new(&first);
        editor.panel.open();
        editor
    }

    fn snapshot(&self) -> Value {
        Value::Object(self.result().0)
    }

    pub fn theme_id(&self) -> &str {
        &self.id
    }

    /// Every page by key, in the order of the left column.
    pub fn page_keys() -> Vec<String> {
        let mut keys = vec!["general".to_owned(), "colours".to_owned(), "applied".to_owned()];
        keys.extend(DESKTOP.iter().map(|d| d.0.to_owned()));
        keys.extend(EXTRA.iter().map(|e| e.0.to_owned()));
        keys.extend(MODULES.iter().map(|m| m.0.to_owned()));
        keys
    }

    /// Shows a page by key (for `--smoke` and the tests, which have no mouse).
    pub fn show_page(&mut self, key: &str) {
        self.page = match key {
            "general" => Page::General,
            "colours" => Page::Colours,
            "applied" => Page::Applied,
            other => match DESKTOP.iter().find(|d| d.0 == other) {
                Some(d) => Page::Desktop(d.0),
                None => Page::Section(other.to_owned()),
            },
        };
    }

    /// Opens quick tool number `index` of the Colours page (for `--smoke`); the picture tool reads `picture`.
    pub fn show_tool(&mut self, index: usize, picture: Option<PathBuf>) {
        if let Some((tool, _, _)) = tools::TOOLS.get(index) {
            let near = self.panel.geometry.rect();
            self.open_tool_with(*tool, picture, near);
        }
    }

    fn change(&mut self) {
        self.changed = true;
    }

    fn dirty(&self) -> bool {
        self.snapshot() != self.saved
    }

    // ── undo, redo and recent changes ──────────────────────────────────────

    fn commit(&mut self, label: &str) {
        self.edited_at = None;
        let now = self.snapshot();
        self.history.record(&now, label);
    }

    fn undo(&mut self) {
        self.commit("Change");
        if let Some((label, value)) = self.history.undo() {
            self.restore(value);
            self.status = format!("UNDID: {}", label.to_uppercase());
        }
    }

    fn redo(&mut self) {
        self.commit("Change");
        if let Some((label, value)) = self.history.redo() {
            self.restore(value);
            self.status = format!("REDID: {}", label.to_uppercase());
        }
    }

    /// Puts a snapshot back: the theme and the fields.
    fn restore(&mut self, value: Value) {
        let Value::Object(map) = value else { return };
        self.restoring = true;
        self.t = Theme(map);
        self.t.colours_mut();
        self.fields = Fields::of(&self.t);
        self.wallpaper_missing = false;
        self.checked_wallpaper = false;
        self.key_warning.clear();
        self.change();
    }

    /// Many colours at once (a quick tool, putting one back): one step to undo and one entry in recent changes.
    fn apply_bulk(&mut self, changes: Map<String, Value>, label: &str) {
        if changes.is_empty() {
            return;
        }
        self.commit("Change");
        let old: Map<String, Value> = changes
            .keys()
            .filter_map(|k| Some((k.clone(), value_of(&self.global(Role::from_key(k)?)))))
            .collect();
        for (key, value) in &changes {
            if let (Some(role), Some(picked)) = (Role::from_key(key), picked_from_value(value)) {
                self.set_global(role, &picked);
            }
        }
        if let Some(top) = self.recents.first_mut() {
            top.live = false;
        }
        note_change(&mut self.recents, "lum", old, changes, label, Map::new());
        if let Some(top) = self.recents.first_mut() {
            top.live = false;
        }
        self.change();
        self.commit(label);
    }

    fn reset_recent(&mut self, index: usize) {
        let Some(r) = self.recents.get(index).cloned() else { return };
        self.apply_bulk(r.old, &format!("Back: {}", r.label));
    }

    // ── the colour dialog ──────────────────────────────────────────────────

    /// Opens the shared colour dialog for `target`.
    fn pick(
        &mut self,
        picker: &mut ColourDialog,
        target: String,
        title: &str,
        start: Picked,
        gradient: bool,
    ) {
        picker.open(&target, title, start.clone(), gradient, self.panel.geometry.rect());
        self.picking = Some(Picking { target, before: Some(start) });
    }

    /// What the colour dialog said; false when the colour was not asked for by the editor.
    pub fn picked(&mut self, event: &PickerEvent) -> bool {
        let Some(target) = event.target.strip_prefix("editor:") else { return false };
        match &event.kind {
            PickerKind::Live(p) | PickerKind::Done(p) => self.set_by_target(target, Some(p.clone())),
            PickerKind::Cancelled => {
                let before = self.picking.take().filter(|p| p.target == event.target).and_then(|p| p.before);
                match (target.strip_prefix("tool:"), &mut self.tool) {
                    (Some(key), Some(tool)) => tool.cancel_pick(key),
                    _ => self.set_by_target(target, before),
                }
            }
        }
        if matches!(event.kind, PickerKind::Done(_)) {
            self.picking = None;
        }
        true
    }

    fn set_by_target(&mut self, target: &str, value: Option<Picked>) {
        let mut parts = target.splitn(3, ':');
        match (parts.next(), parts.next(), parts.next()) {
            (Some("role"), Some(role), _) => {
                if let (Some(role), Some(value)) = (Role::from_key(role), value) {
                    self.set_global_tracked(role, &value);
                }
            }
            (Some("section"), Some(key), Some(part)) => {
                if part == "accent" {
                    self.set_section_accent(key, value.map(|v| v.first()));
                } else if let Some(role) = Role::from_key(part) {
                    self.set_section_role(key, role, value.as_ref());
                }
            }
            (Some("tool"), Some(key), _) => {
                if let (Some(tool), Some(value)) = (&mut self.tool, value) {
                    tool.picked(key, value.first());
                }
            }
            _ => {}
        }
    }

    /// A file dialog the editor asked for came back.
    pub fn file_chosen(&mut self, field: &str, path: Option<PathBuf>, near: Rect) {
        let Some(path) = path else { return };
        let text = path.to_string_lossy().into_owned();
        match field {
            "background_image" => self.fields.background_image = text,
            "icon" => self.fields.icon = text,
            "wallpaper" => {
                self.fields.wallpaper = text;
                self.wallpaper_missing = false;
            }
            "tool:picture" => {
                self.open_tool_with(Tool::Picture, Some(path), near);
                return;
            }
            _ => return,
        }
        self.change();
    }

    // ── the quick tools ────────────────────────────────────────────────────

    fn open_tool(&mut self, tool: Tool, out: &mut EditorOut) {
        if tool == Tool::Picture {
            let request = lum_platform::dialogs::Request::new("Choose a picture")
                .folder(Some(lum_core::paths::user_home()))
                .filter("Pictures", &["png", "jpg", "jpeg", "webp", "bmp", "gif"]);
            out.files.push(FileAsk { job: FileJob::Browse("tool:picture".into()), save: false, request });
        } else {
            self.open_tool_with(tool, None, self.panel.geometry.rect());
        }
    }

    fn open_tool_with(&mut self, tool: Tool, picture: Option<PathBuf>, near: Rect) {
        let before = lum_theme::tones::Colours::from_iter(
            Role::ALL.iter().map(|role| (role.key().to_owned(), self.t.role_value(*role).solid().hex())),
        );
        let mut dialog = BulkDialog::new(tool, before, self.t.palette(), picture);
        dialog.open(near);
        self.tool = Some(dialog);
    }

    // ── closing ────────────────────────────────────────────────────────────

    /// Save: the theme goes back into the store, and the recent changes stay with it for next time.
    fn save(&mut self, store: &mut ThemeStore) {
        let t = self.result();
        store.update_theme(t);
        store.set_recent_changes(&self.id, &self.recents);
        self.panel.close();
        self.finished = true;
    }

    /// Cancel, the ✕ and Esc: asks first when there are changes.
    fn cancel(&mut self, ctx: &Context) {
        if self.dirty() {
            let dialog = AskDialog::confirm(
                Id::unique("theme-editor-ask"),
                "Unsaved changes",
                "Close without saving your changes to this theme?",
                "Discard",
                "Keep editing",
                true,
            );
            self.ask = Some((Ask::Discard, dialog));
            ctx.request_repaint();
        } else {
            self.panel.close();
            self.finished = true;
        }
    }

    fn message(&mut self, title: &str, text: &str) {
        self.ask = Some((Ask::Message, AskDialog::error(Id::unique("theme-editor-ask"), title, text)));
    }

    // ── a frame ────────────────────────────────────────────────────────────

    pub fn show(
        &mut self,
        ctx: &Context,
        store: &mut ThemeStore,
        picker: &mut ColourDialog,
        env: &Env<'_>,
    ) -> EditorOut {
        let mut out = EditorOut::default();
        let mut panel = self.panel.clone();
        panel.set_title(&format!("Theme settings · {}", self.fields.name), "");
        let response = panel.show(ctx, |ui| self.body(ui, store, picker, env, &mut out));
        let was_open = self.panel.is_open();
        self.panel = panel;
        if response.escape && self.ask.is_none() && !picker.is_open() && self.tool.is_none() {
            self.cancel(ctx);
        }
        if response.closed && was_open && self.ask.is_none() {
            // the ✕ was pressed: ask first when there is something to lose
            if self.dirty() {
                self.panel.open();
                self.cancel(ctx);
            } else {
                self.finished = true;
            }
        }
        self.tool_frame(ctx, picker);
        self.ask_frame(ctx);
        self.after_frame(ctx, picker);
        out
    }

    fn tool_frame(&mut self, ctx: &Context, picker: &mut ColourDialog) {
        let Some(tool) = &mut self.tool else { return };
        tool.show(ctx, picker);
        let applied = tool.applied.take();
        let finished = tool.finished;
        if let Some((changes, label)) = applied {
            self.apply_bulk(changes, &label);
        }
        if finished {
            self.tool = None;
        }
    }

    fn ask_frame(&mut self, ctx: &Context) {
        let Some((kind, mut dialog)) = self.ask.take() else { return };
        match dialog.show(ctx) {
            Outcome::Open => self.ask = Some((kind, dialog)),
            Outcome::Cancelled => {}
            Outcome::Accepted(_) => match kind {
                Ask::Reset(key) => self.clear_section(&key),
                Ask::Discard => {
                    self.panel.close();
                    self.finished = true;
                }
                Ask::Message => {}
            },
        }
    }

    /// What follows from the changes of this frame: the unsaved note, the step to undo after a pause.
    fn after_frame(&mut self, ctx: &Context, _picker: &mut ColourDialog) {
        let now = ctx.input(|i| i.time);
        if std::mem::take(&mut self.changed) {
            self.status = if self.dirty() { "UNSAVED CHANGES".to_owned() } else { String::new() };
            if !std::mem::take(&mut self.restoring) {
                self.edited_at = Some(now);
            }
            ctx.request_repaint();
        }
        if let Some(at) = self.edited_at {
            if now - at >= BURST {
                self.commit("Change");
                ctx.request_repaint();
            } else {
                ctx.request_repaint_after_secs((BURST - (now - at)) as f32);
            }
        }
        if let Some((_, until)) = self.flash {
            if now >= until {
                self.flash = None;
                ctx.request_repaint();
            } else {
                ctx.request_repaint_after_secs((until - now) as f32);
            }
        }
    }

    fn body(
        &mut self,
        ui: &mut Ui,
        store: &mut ThemeStore,
        picker: &mut ColourDialog,
        env: &Env<'_>,
        out: &mut EditorOut,
    ) {
        let tokens = Tokens::of(ui.ctx());
        self.shortcuts(ui);
        let full = ui.available_rect_before_wrap();
        let footer = Rect::from_min_max(pos2(full.min.x, full.max.y - FOOTER), full.max);
        let main = Rect::from_min_max(full.min, pos2(full.max.x, footer.min.y));

        let total = main.width();
        self.left = self.left.clamp(SIDE_MIN, (total - MIDDLE_MIN - PREVIEW_MIN).max(SIDE_MIN));
        self.right = self.right.clamp(PREVIEW_MIN, (total - self.left - MIDDLE_MIN).max(PREVIEW_MIN));
        let side = Rect::from_min_size(main.min, vec2(self.left, main.height()));
        let preview = Rect::from_min_max(pos2(main.max.x - self.right, main.min.y), main.max);
        let middle = Rect::from_min_max(pos2(side.max.x, main.min.y), pos2(preview.min.x, main.max.y));

        self.hover = None;
        ui.scope_builder(UiBuilder::new().max_rect(side.shrink2(vec2(10.0, 0.0))), |ui| {
            self.sidebar(ui, env)
        });
        ui.scope_builder(UiBuilder::new().max_rect(middle), |ui| {
            self.middle(ui, store, picker, env, out);
        });
        ui.scope_builder(UiBuilder::new().max_rect(preview.shrink2(vec2(14.0, 14.0))), |ui| {
            self.preview(ui, env);
        });
        self.splitters(ui, main, side.max.x, preview.min.x, &tokens);
        self.footer(ui, footer, store, &tokens);
    }

    fn splitters(&mut self, ui: &mut Ui, main: Rect, left_x: f32, right_x: f32, tokens: &Tokens) {
        for (i, x) in [left_x, right_x].into_iter().enumerate() {
            let rect = Rect::from_min_max(pos2(x - 4.5, main.min.y), pos2(x + 4.5, main.max.y));
            let response = ui.interact(rect, Id::unique(("theme-editor-split", i)), Sense::drag());
            if response.hovered() || response.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            let colour =
                if response.hovered() || response.dragged() { tokens.p.primary } else { tokens.p.border };
            ui.painter().vline(x, main.y_range(), Stroke::new(1.0, colour));
            if response.dragged() {
                let dx = response.drag_delta().x;
                if i == 0 {
                    self.left += dx;
                } else {
                    self.right -= dx;
                }
            }
        }
    }

    fn shortcuts(&mut self, ui: &mut Ui) {
        let redo = ui.input_mut(|i| {
            i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
                || i.consume_key(Modifiers::COMMAND, Key::Y)
        });
        let undo = ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Z));
        if redo {
            self.redo();
        } else if undo {
            self.undo();
        }
    }

    fn footer(&mut self, ui: &mut Ui, rect: Rect, store: &mut ThemeStore, tokens: &Tokens) {
        ui.painter().rect_filled(rect, 0.0, tokens.p.bg_header);
        ui.painter().hline(rect.x_range(), rect.min.y + 0.5, Stroke::new(1.0, tokens.p.border));
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(rect.shrink2(vec2(16.0, 0.0)))
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if let Page::Section(key) = self.page.clone() {
                    let reset = ui.add(Button::danger("Reset this part"));
                    if tip(reset, "Everything on this page back to the theme's own look").clicked() {
                        let text = format!("Put {} back to the theme's own look?", self.section_title(&key));
                        let dialog = AskDialog::confirm(
                            Id::unique("theme-editor-ask"),
                            "Reset",
                            &text,
                            "Reset",
                            "Cancel",
                            true,
                        );
                        self.ask = Some((Ask::Reset(key), dialog));
                    }
                }
                let undo = IconButton::new("↶").size(30.0).font_px(14.0).ghost();
                let label_undo = self.history.undo_label().map_or_else(
                    || "Nothing to undo".to_owned(),
                    |l| format!("Undo {} (Ctrl+Z)", l.to_lowercase()),
                );
                ui.add_enabled_ui(self.history.can_undo(), |ui| {
                    if tip(ui.add(undo), label_undo).clicked() {
                        self.undo();
                    }
                });
                let redo = IconButton::new("↷").size(30.0).font_px(14.0).ghost();
                let label_redo = self.history.redo_label().map_or_else(
                    || "Nothing to redo".to_owned(),
                    |l| format!("Redo {} (Ctrl+Shift+Z)", l.to_lowercase()),
                );
                ui.add_enabled_ui(self.history.can_redo(), |ui| {
                    if tip(ui.add(redo), label_redo).clicked() {
                        self.redo();
                    }
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if ui.add(Button::primary("Save")).clicked() {
                        self.save(store);
                    }
                    if ui.add(Button::ghost("Cancel")).clicked() {
                        let ctx = ui.ctx().clone();
                        self.cancel(&ctx);
                    }
                    ui.add_space(8.0);
                    ui.add(crate::text::mono(self.status.clone()).px(10.5).spacing(0.04).tone(Tone::Muted));
                });
            },
        );
    }

    // ── the left column ────────────────────────────────────────────────────

    fn sidebar(&mut self, ui: &mut Ui, env: &Env<'_>) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("theme-editor-side").show(
            ui,
            |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.add_space(12.0);
                self.heading(ui, "THEME");
                self.side_button(ui, Page::General, "General");
                self.side_button(ui, Page::Colours, "Colours");
                self.side_button(ui, Page::Applied, "When it is applied");
                self.heading(ui, "DESKTOP");
                for (key, icon, title) in DESKTOP {
                    let shown = match key {
                        "caelestia_colours" | "caelestia_look" | "glass" => env.caelestia && !cfg!(windows),
                        "borders" => env.hyprland,
                        _ => true,
                    };
                    if shown {
                        self.side_button(ui, Page::Desktop(key), &format!("{icon}  {title}"));
                    }
                }
                self.heading(ui, "PARTS");
                for (key, title, icon, _) in EXTRA {
                    self.side_button(ui, Page::Section(key.to_owned()), &format!("{icon}  {title}"));
                }
                self.heading(ui, "MODULES");
                for (key, icon, title) in MODULES {
                    self.side_button(ui, Page::Section((*key).to_owned()), &format!("{icon}  {title}"));
                }
                ui.add_space(12.0);
            },
        );
    }

    fn heading(&self, ui: &mut Ui, text: &str) {
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.add(caps(text).px(9.5).semibold().spacing(0.14).tone(Tone::Muted));
        });
        ui.add_space(4.0);
    }

    fn side_button(&mut self, ui: &mut Ui, page: Page, text: &str) {
        let tokens = Tokens::of(ui.ctx());
        let selected = self.page == page;
        let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
        let role = if selected {
            Some(Role::PrimaryMuted)
        } else if response.hovered() {
            Some(Role::BgCardHover)
        } else {
            None
        };
        if let Some(role) = role {
            crate::paint::fill(ui.painter(), rect, tokens.rs(), &tokens, role, 1.0);
        }
        let colour = if selected { tokens.p.primary_text } else { tokens.p.text_secondary };
        ui.painter().text(
            pos2(rect.min.x + 10.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            text,
            tokens.font(12.5, crate::fonts::Weight::Regular, false),
            colour,
        );
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let name = text.to_owned();
        response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, &name));
        if response.clicked() {
            self.page = page;
        }
    }

    pub(super) fn section_title(&self, key: &str) -> String {
        if let Some((_, _, title)) = DESKTOP.iter().find(|d| d.0 == key) {
            return (*title).to_owned();
        }
        if let Some((_, title, _, _)) = EXTRA.iter().find(|e| e.0 == key) {
            return (*title).to_owned();
        }
        MODULES.iter().find(|m| m.0 == key).map_or_else(|| key.to_owned(), |m| m.2.to_owned())
    }

    /// The palette of the theme as it is being edited.
    pub(super) fn palette(&self) -> lum_theme::Palette {
        self.result().palette()
    }

    pub(super) fn ui_palette(&self) -> Palette {
        Palette::from(&self.palette())
    }
}

/// The combo a key press makes while a hotkey is being recorded, in the stored form (`ctrl+shift+space`).
pub(super) fn combo_of(modifiers: Modifiers, key: Key) -> Option<String> {
    let name = match key {
        Key::Escape => return None,
        Key::Space => "space".to_owned(),
        Key::Enter => "enter".to_owned(),
        Key::Tab => "tab".to_owned(),
        Key::Backspace => "backspace".to_owned(),
        Key::Delete => "delete".to_owned(),
        Key::Insert => "insert".to_owned(),
        Key::Home => "home".to_owned(),
        Key::End => "end".to_owned(),
        Key::PageUp => "page up".to_owned(),
        Key::PageDown => "page down".to_owned(),
        Key::ArrowUp => "up".to_owned(),
        Key::ArrowDown => "down".to_owned(),
        Key::ArrowLeft => "left".to_owned(),
        Key::ArrowRight => "right".to_owned(),
        Key::Minus => "-".to_owned(),
        Key::Equals => "=".to_owned(),
        Key::Comma => ",".to_owned(),
        Key::Period => ".".to_owned(),
        Key::Slash => "/".to_owned(),
        Key::Semicolon => ";".to_owned(),
        Key::Backslash => "\\".to_owned(),
        Key::OpenBracket => "[".to_owned(),
        Key::CloseBracket => "]".to_owned(),
        Key::Backtick => "`".to_owned(),
        Key::Quote => "'".to_owned(),
        other => other.name().trim_start_matches("Num").to_lowercase(),
    };
    let mut parts = Vec::new();
    if modifiers.ctrl {
        parts.push("ctrl");
    }
    if modifiers.alt {
        parts.push("alt");
    }
    if modifiers.shift {
        parts.push("shift");
    }
    parts.push(&name);
    Some(keys::normalize(&parts.join("+")))
}
