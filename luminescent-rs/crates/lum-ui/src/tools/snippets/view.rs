//! What Snippets draws into whatever room it is given: the breadcrumb and search row, then favourites, folders and
//! clips as cards. The Ctrl+B pop-up, the overlay page and a dashboard box all use it. Cards are laid out by hand
//! and only the rows in view are drawn, so a folder with hundreds of clips costs what the screen shows. Anything
//! that needs a dialog, the disk or the clipboard goes back as an [`Event`].

use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::epaint::{CornerRadius, RectShape};
use egui::text::{LayoutJob, TextFormat};
use egui::{
    Align2, Color32, CursorIcon, Galley, Id, Key, Modifiers, Rect, Response, Role, Sense, Stroke, StrokeKind,
    Ui, UiBuilder, WidgetInfo, pos2, vec2,
};
use lum_core::snippets::{self, Clip, FolderEntry, Kind, Listing};

use super::thumbs::Thumbs;
use crate::fonts::Weight;
use crate::theme::{Tokens, luminance};
use crate::widgets::{Button, TextInput, menu};

const PREVIEW_H: f32 = 128.0;
const FOOT_H: f32 = 34.0;
const CARD_H: f32 = PREVIEW_H + FOOT_H + 2.0;
const FOLDER_H: f32 = 52.0;
const GAP: f32 = 12.0;
const SECTION_GAP: f32 = 20.0;
const HEAD_H: f32 = 52.0;
const HOVER_AFTER: f64 = 0.65;
const SEARCH_AFTER: f64 = 0.22;
const FOLDER_ACCENTS: [Color32; 3] = [
    Color32::from_rgb(0x3D, 0xBE, 0x96),
    Color32::from_rgb(0xE0, 0x89, 0x4E),
    Color32::from_rgb(0x5B, 0x9B, 0xE6),
];

/// Things the grid can not do by itself.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    OpenFolder(PathBuf),
    Up,
    Use {
        path: PathBuf,
        ctrl: bool,
    },
    Copy(PathBuf),
    OpenFile(PathBuf),
    Favourite(PathBuf, bool),
    FolderColour(PathBuf),
    Rename(PathBuf),
    Reveal(PathBuf),
    Delete(PathBuf),
    NewFolder,
    OpenCurrent,
    SaveClipboard,
    /// The search text settled (or was cleared): list again.
    Search,
    /// A card was dropped on a folder card or a folder in the breadcrumb.
    DropOnFolder {
        src: PathBuf,
        folder: PathBuf,
    },
    /// A card was dropped on a clip: it goes before it (and into its folder).
    DropOnClip {
        src: PathBuf,
        target: PathBuf,
        favourite: bool,
    },
    /// Pictures and videos on view that have no thumbnail yet: (path, is a video, modified).
    Thumbnails(Vec<(PathBuf, bool, Option<std::time::SystemTime>)>),
}

#[derive(Debug, Clone)]
enum Target {
    Folder(PathBuf),
    Clip { path: PathBuf, favourite: bool },
}

#[derive(Debug)]
struct Hover {
    path: PathBuf,
    rect: Rect,
    since: f64,
}

/// What one place that shows the grid (the pop-up, the page) remembers while it is open.
#[derive(Debug)]
pub struct Browser {
    salt: Id,
    pub root: PathBuf,
    pub path: PathBuf,
    pub query: String,
    /// `None` while a listing is on its way.
    pub listing: Option<Listing>,
    /// Which listing is wanted; an answer for an older one is dropped.
    pub generation: u64,
    searching_at: Option<f64>,
    hover: Option<Hover>,
    preview_text: Option<(PathBuf, String)>,
    drag: Option<PathBuf>,
    targets: Vec<(Rect, Target)>,
    focus_search: bool,
    /// A fresh listing is wanted: the module starts one and clears this.
    pub wanted: bool,
}

impl Browser {
    pub fn new(name: &str) -> Browser {
        Browser {
            salt: Id::unique(("lum-snippets", name)),
            root: PathBuf::new(),
            path: PathBuf::new(),
            query: String::new(),
            listing: None,
            generation: 0,
            searching_at: None,
            hover: None,
            preview_text: None,
            drag: None,
            targets: Vec::new(),
            focus_search: false,
            wanted: false,
        }
    }

    /// Starts over at the top of `root`.
    pub fn reset(&mut self, root: &Path, at: &Path) {
        self.root = snippets::normpath(root);
        self.path = snippets::normpath(at);
        self.query.clear();
        self.listing = None;
        self.searching_at = None;
        self.hover = None;
        self.preview_text = None;
        self.drag = None;
        self.targets.clear();
        self.wanted = true;
    }

    pub fn at_root(&self) -> bool {
        snippets::same(&self.path, &self.root)
    }

    pub fn go_up(&mut self) -> bool {
        if self.at_root() {
            return false;
        }
        if let Some(parent) = self.path.parent() {
            self.path = parent.to_owned();
        }
        self.listing = None;
        self.wanted = true;
        true
    }

    pub fn open_folder(&mut self, path: &Path) {
        self.path = snippets::normpath(path);
        self.query.clear();
        self.searching_at = None;
        self.listing = None;
        self.wanted = true;
    }

    pub fn searching(&self) -> bool {
        !self.query.trim().is_empty()
    }

    /// The panel's title part: the folder's name, or empty at the top.
    pub fn title(&self) -> String {
        if self.at_root() {
            String::new()
        } else {
            self.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
        }
    }

    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }
}

/// How the grid sits in its room.
#[derive(Debug, Clone, Copy)]
pub struct Look {
    pub cols: usize,
    /// Space at the left and right.
    pub pad: f32,
    /// A small box: fewer buttons in the header.
    pub narrow: bool,
}

/// How a block of text looks: size, weight, colour, and whether a word longer than the line may be broken.
#[derive(Clone, Copy)]
struct Face {
    px: f32,
    weight: Weight,
    mono: bool,
    colour: Color32,
    line: Option<f32>,
    break_words: bool,
}

impl Face {
    fn new(px: f32, weight: Weight, colour: Color32) -> Face {
        Face { px, weight, mono: false, colour, line: None, break_words: false }
    }

    fn mono(mut self) -> Face {
        self.mono = true;
        self
    }

    fn line(mut self, height: f32) -> Face {
        self.line = Some(height);
        self
    }

    fn break_words(mut self) -> Face {
        self.break_words = true;
        self
    }
}

fn layout(ui: &Ui, text: &str, face: Face, width: f32, rows: usize) -> Arc<Galley> {
    let tokens = Tokens::of(ui.ctx());
    let mut job = LayoutJob::single_section(
        text.to_owned(),
        TextFormat {
            font_id: tokens.font(face.px, face.weight, face.mono),
            color: face.colour,
            line_height: face.line,
            ..Default::default()
        },
    );
    job.wrap.max_width = width.max(8.0);
    job.wrap.max_rows = rows;
    job.wrap.break_anywhere = face.break_words;
    ui.painter().layout_job(job)
}

fn mono_caps(ui: &Ui, text: &str, px: f32, colour: Color32) -> Arc<Galley> {
    let tokens = Tokens::of(ui.ctx());
    let mut job = LayoutJob::single_section(
        text.to_owned(),
        TextFormat {
            font_id: tokens.font(px, Weight::Medium, true),
            color: colour,
            extra_letter_spacing: px * 0.08,
            ..Default::default()
        },
    );
    job.wrap.max_rows = 1;
    ui.painter().layout_job(job)
}

fn elide_to(
    ui: &Ui,
    text: &str,
    px: f32,
    weight: Weight,
    mono: bool,
    colour: Color32,
    width: f32,
) -> Arc<Galley> {
    let face = Face { mono, break_words: true, ..Face::new(px, weight, colour) };
    layout(ui, text, face, width, 1)
}

// ── the whole grid ─────────────────────────────────────────────────────────

/// Draws the header and the cards into `rect`.
pub fn show(
    ui: &mut Ui,
    rect: Rect,
    browser: &mut Browser,
    thumbs: &Thumbs,
    look: Look,
    events: &mut Vec<Event>,
) {
    let tokens = Tokens::of(ui.ctx());
    let mut child =
        ui.new_child(UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min)));
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    let ui = &mut child;
    browser.targets_next();
    let head =
        Rect::from_min_size(rect.min, vec2(rect.width(), if look.narrow { HEAD_H + 40.0 } else { HEAD_H }));
    header(ui, head, browser, look, events, &tokens);
    let body = Rect::from_min_max(pos2(rect.min.x, head.max.y), rect.max);
    grid(ui, body, browser, thumbs, look, events, &tokens);
    keys(ui, browser, events);
    finish_drag(ui, browser, events, &tokens);
    settle_search(ui, browser, events);
}

impl Browser {
    /// Each pass records where the drop targets are; the ones from the pass before are not wanted any more.
    fn targets_next(&mut self) {
        self.targets.clear();
    }
}

fn keys(ui: &mut Ui, browser: &mut Browser, events: &mut Vec<Event>) {
    if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::F)) {
        browser.focus_search = true;
    }
    let typing = ui.ctx().egui_wants_keyboard_input();
    if !typing && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Backspace)) && browser.go_up() {
        events.push(Event::Up);
    }
}

fn settle_search(ui: &mut Ui, browser: &mut Browser, events: &mut Vec<Event>) {
    let Some(at) = browser.searching_at else { return };
    let now = ui.input(|i| i.time);
    if now >= at {
        browser.searching_at = None;
        events.push(Event::Search);
    } else {
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(at - now));
    }
}

// ── header ─────────────────────────────────────────────────────────────────

fn header(ui: &mut Ui, head: Rect, b: &mut Browser, look: Look, events: &mut Vec<Event>, tokens: &Tokens) {
    let inner =
        Rect::from_min_max(pos2(head.min.x + look.pad, head.min.y), pos2(head.max.x - look.pad, head.max.y));
    let crumb_row = Rect::from_min_size(inner.min, vec2(inner.width(), HEAD_H));
    crumbs(ui, crumb_row, b, look, events, tokens);
    let controls = if look.narrow {
        Rect::from_min_size(pos2(inner.min.x, inner.min.y + HEAD_H - 10.0), vec2(inner.width(), 30.0))
    } else {
        Rect::from_min_size(
            pos2(inner.max.x - controls_width(look), inner.min.y + 11.0),
            vec2(controls_width(look), 30.0),
        )
    };
    let mut row = ui.new_child(
        UiBuilder::new().max_rect(controls).layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    row.spacing_mut().item_spacing.x = 8.0;
    let search_w = if look.narrow { (controls.width() - 96.0).clamp(90.0, 220.0) } else { 220.0 };
    search_box(&mut row, b, search_w, tokens);
    if !look.narrow {
        if row.add(Button::ghost("New folder")).clicked() {
            events.push(Event::NewFolder);
        }
        if row.add(Button::ghost("Open folder")).clicked() {
            events.push(Event::OpenCurrent);
        }
    }
    let paste = row.add(Button::primary("Paste here"));
    let paste = paste.on_hover_text("Saves the text, image or files on the clipboard into this folder");
    if paste.clicked() {
        events.push(Event::SaveClipboard);
    }
    if look.narrow {
        let more =
            row.add(crate::widgets::IconButton::new("⋯").size(28.0).font_px(12.0).ghost().tooltip("More"));
        egui::Popup::menu(&more).show(|ui| {
            if menu::item(ui, "New folder").clicked() {
                events.push(Event::NewFolder);
            }
            if menu::item(ui, "Open folder").clicked() {
                events.push(Event::OpenCurrent);
            }
        });
    }
}

fn controls_width(_look: Look) -> f32 {
    220.0 + 8.0 + 98.0 + 8.0 + 98.0 + 8.0 + 92.0
}

fn search_box(ui: &mut Ui, b: &mut Browser, width: f32, tokens: &Tokens) {
    let id = b.salt.with("search");
    if std::mem::take(&mut b.focus_search) {
        ui.memory_mut(|m| m.request_focus(id));
    }
    let before = b.query.clone();
    let response = ui.add(TextInput::new(&mut b.query).id(id).hint("Search every folder").width(width));
    let hint = mono_caps(ui, "CTRL+F", 10.0, tokens.p.text_muted);
    if width > 150.0 {
        let at =
            pos2(response.rect.max.x - hint.size().x - 10.0, response.rect.center().y - hint.size().y / 2.0);
        ui.painter().galley(at, hint, tokens.p.text_muted);
    }
    if response.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) && !b.query.is_empty() {
        b.query.clear();
    }
    if b.query != before {
        let now = ui.input(|i| i.time);
        b.searching_at = Some(now + if b.query.trim().is_empty() { 0.0 } else { SEARCH_AFTER });
        ui.ctx().request_repaint();
    }
    response.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, "Search every folder"));
}

fn crumbs(ui: &mut Ui, row: Rect, b: &mut Browser, look: Look, events: &mut Vec<Event>, tokens: &Tokens) {
    let rel = if b.at_root() { String::new() } else { snippets::relative(&b.path, &b.root) };
    let mut parts = vec![("Clipboard".to_owned(), b.root.clone())];
    let mut at = b.root.clone();
    for part in rel.split(std::path::MAIN_SEPARATOR).filter(|p| !p.is_empty() && *p != ".") {
        at = at.join(part);
        parts.push((part.to_owned(), at.clone()));
    }
    let max_right = if look.narrow { row.max.x } else { row.max.x - controls_width(look) - 16.0 };
    let mut x = row.min.x;
    let y = row.center().y;
    let last = parts.len() - 1;
    for (i, (name, target)) in parts.iter().enumerate() {
        if i > 0 {
            let slash = ui.painter().layout_no_wrap(
                "/".into(),
                tokens.font(11.0, Weight::Regular, false),
                tokens.p.text_muted,
            );
            ui.painter().galley(pos2(x, y - slash.size().y / 2.0), slash.clone(), tokens.p.text_muted);
            x += slash.size().x + 8.0;
        }
        let is_last = i == last;
        let (weight, colour) = if is_last {
            (Weight::SemiBold, tokens.p.text_primary)
        } else {
            (Weight::Medium, tokens.p.text_muted)
        };
        let galley = elide_to(ui, name, 13.0, weight, false, colour, (max_right - x).max(40.0));
        let rect = Rect::from_min_size(pos2(x, y - galley.size().y / 2.0), galley.size());
        let width = galley.size().x;
        if !is_last {
            let response =
                ui.interact(rect.expand2(vec2(2.0, 6.0)), b.salt.with(("crumb", i)), Sense::click());
            response.widget_info(|| WidgetInfo::labeled(Role::Button, true, format!("Folder {name}")));
            if response.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            if response.clicked() {
                events.push(Event::OpenFolder(target.clone()));
            }
            let over = b.drag.is_some()
                && ui.input(|i| i.pointer.hover_pos()).is_some_and(|p| rect.expand(6.0).contains(p));
            let colour = if over { tokens.p.primary } else { colour };
            ui.painter().galley_with_override_text_color(rect.min, galley, colour);
            b.targets.push((rect.expand(6.0), Target::Folder(target.clone())));
        } else {
            ui.painter().galley(rect.min, galley, colour);
        }
        x += width + 8.0;
        if x > max_right {
            break;
        }
    }
}

// ── the cards ──────────────────────────────────────────────────────────────

/// Where each section starts, so only the rows in view are drawn.
struct Plan {
    favourites: Option<(f32, usize)>,
    folders: Option<(f32, usize)>,
    clips: Option<(f32, usize)>,
    message: Option<f32>,
    height: f32,
}

fn rows(count: usize, cols: usize) -> usize {
    count.div_ceil(cols.max(1))
}

fn plan(listing: &Listing, cols: usize) -> Plan {
    let mut y = 4.0;
    let mut plan = Plan { favourites: None, folders: None, clips: None, message: None, height: 0.0 };
    let section = |y: &mut f32, count: usize, row_h: f32, head: f32| -> Option<(f32, usize)> {
        if count == 0 {
            return None;
        }
        let top = *y + head;
        *y = top + rows(count, cols) as f32 * (row_h + GAP) - GAP + SECTION_GAP;
        Some((top, count))
    };
    plan.favourites = section(&mut y, listing.favourites.len(), CARD_H, 26.0);
    plan.folders = section(&mut y, listing.folders.len(), FOLDER_H, 0.0);
    plan.clips = section(&mut y, listing.clips.len(), CARD_H, 0.0);
    if !listing.message.is_empty() {
        plan.message = Some(y);
        y += listing.message.lines().count() as f32 * 20.0 + 40.0;
    }
    plan.height = y + 8.0;
    plan
}

/// What drawing the cards needs and gathers: the theme, the thumbnails, what the listing knows about folders and
/// favourites, the card the mouse is on, the pictures that still need a thumbnail and the events to hand back.
struct Draw<'a> {
    tokens: &'a Tokens,
    thumbs: &'a Thumbs,
    colours: &'a std::collections::HashMap<String, String>,
    favourites: &'a [PathBuf],
    searching: bool,
    hovered: Option<(PathBuf, Rect, bool)>,
    wanted: Vec<(PathBuf, bool, Option<std::time::SystemTime>)>,
    events: &'a mut Vec<Event>,
}

fn grid(
    ui: &mut Ui,
    body: Rect,
    b: &mut Browser,
    thumbs: &Thumbs,
    look: Look,
    events: &mut Vec<Event>,
    tokens: &Tokens,
) {
    let Some(listing) = b.listing.take() else {
        let text = if b.searching() { "Searching..." } else { "" };
        ui.painter().text(
            body.center(),
            Align2::CENTER_CENTER,
            text,
            tokens.font(13.0, Weight::Regular, false),
            tokens.p.text_muted,
        );
        return;
    };
    let cols = look.cols.max(1);
    let inner_w = (body.width() - look.pad * 2.0).max(60.0);
    let card_w = ((inner_w - GAP * (cols as f32 - 1.0)) / cols as f32).max(40.0);
    let plan = plan(&listing, cols);
    let mut draw = Draw {
        tokens,
        thumbs,
        colours: &listing.colours,
        favourites: &listing.favourite_paths,
        searching: b.searching(),
        hovered: None,
        wanted: Vec::new(),
        events,
    };
    let mut scroll_ui = ui.new_child(UiBuilder::new().max_rect(body));
    scroll_ui.set_clip_rect(body.intersect(ui.clip_rect()));
    egui::ScrollArea::vertical()
        .id_salt(b.salt.with("scroll"))
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
        .show_viewport(&mut scroll_ui, |ui, viewport| {
            let (area, _) = ui.allocate_exact_size(vec2(body.width(), plan.height), Sense::hover());
            let origin = area.min + vec2(look.pad, 0.0);
            // the card for item `i` of a section that starts at `top`, when its row is in view
            let cell = |i: usize, top: f32, row_h: f32| {
                let (row, col) = (i / cols, i % cols);
                let y = top + row as f32 * (row_h + GAP);
                let in_view = y + row_h >= viewport.min.y - 40.0 && y <= viewport.max.y + 40.0;
                in_view.then(|| {
                    Rect::from_min_size(origin + vec2(col as f32 * (card_w + GAP), y), vec2(card_w, row_h))
                })
            };
            if let Some((top, count)) = plan.favourites {
                let head = mono_caps(ui, "FAVOURITES", 10.0, tokens.p.text_muted);
                ui.painter().galley(origin + vec2(0.0, top - 22.0), head, tokens.p.text_muted);
                for (i, clip) in listing.favourites.iter().enumerate().take(count) {
                    if let Some(rect) = cell(i, top, CARD_H) {
                        clip_card(ui, rect, clip, true, b, &mut draw);
                    }
                }
            }
            if let Some((top, count)) = plan.folders {
                for (i, folder) in listing.folders.iter().enumerate().take(count) {
                    if let Some(rect) = cell(i, top, FOLDER_H) {
                        folder_card(ui, rect, folder, i, b, &mut draw);
                    }
                }
            }
            if let Some((top, count)) = plan.clips {
                for (i, clip) in listing.clips.iter().enumerate().take(count) {
                    if let Some(rect) = cell(i, top, CARD_H) {
                        clip_card(ui, rect, clip, false, b, &mut draw);
                    }
                }
            }
            if let Some(top) = plan.message {
                ui.painter().text(
                    area.min + vec2(body.width() / 2.0, top + 20.0),
                    Align2::CENTER_TOP,
                    &listing.message,
                    tokens.font(13.0, Weight::Regular, false),
                    tokens.p.text_muted,
                );
            }
        });
    let (hovered, wanted) = (draw.hovered, draw.wanted);
    b.listing = Some(listing);
    if !wanted.is_empty() {
        events.push(Event::Thumbnails(wanted));
    }
    hover_preview(ui, body, b, hovered, thumbs, tokens);
}

fn card_frame(ui: &Ui, rect: Rect, response: &Response, target: bool, tokens: &Tokens) {
    let p = &tokens.p;
    let fill = if response.hovered() { p.bg_card_hover } else { p.bg_card };
    let border = if target {
        p.primary
    } else if response.hovered() {
        tokens.hover_border(0.5)
    } else {
        p.border
    };
    ui.painter().rect_filled(rect, tokens.r(), fill);
    ui.painter().rect_stroke(rect, tokens.r(), Stroke::new(1.0, border), StrokeKind::Inside);
}

fn pointer_in(ui: &Ui, rect: Rect) -> bool {
    ui.input(|i| i.pointer.hover_pos()).is_some_and(|p| rect.contains(p))
}

fn drop_target(b: &Browser, ui: &Ui, rect: Rect, path: &Path) -> bool {
    b.drag.as_ref().is_some_and(|d| !snippets::same(d, path)) && pointer_in(ui, rect)
}

fn folder_card(
    ui: &mut Ui,
    rect: Rect,
    folder: &FolderEntry,
    index: usize,
    b: &mut Browser,
    d: &mut Draw<'_>,
) {
    let tokens = d.tokens;
    let name = folder.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let response = ui.interact(rect, b.salt.with(("folder", &folder.path)), Sense::click_and_drag());
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, format!("Folder {name}")));
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    let target = drop_target(b, ui, rect, &folder.path);
    card_frame(ui, rect, &response, target, tokens);
    let own = d.colours.get(&snippets::norm(&folder.path)).and_then(|c| crate::tools::draw::hex(c));
    let colour = own.unwrap_or_else(|| folder_accent(index, d.searching, tokens));
    let icon = Rect::from_center_size(pos2(rect.min.x + 14.0 + 11.0, rect.center().y), vec2(22.0, 16.0));
    ui.painter().rect_stroke(icon, 3.0, Stroke::new(2.0, colour), StrokeKind::Inside);
    let count = ui.painter().layout_no_wrap(
        format!("{:02}", folder.count),
        tokens.font(10.0, Weight::Regular, true),
        tokens.p.text_muted,
    );
    let count_at = pos2(rect.max.x - 14.0 - count.size().x, rect.center().y - count.size().y / 2.0);
    ui.painter().galley(count_at, count, tokens.p.text_muted);
    let left = rect.min.x + 14.0 + 22.0 + 12.0;
    let galley =
        elide_to(ui, &name, 13.0, Weight::Medium, false, tokens.p.text_primary, count_at.x - left - 8.0);
    ui.painter().galley(pos2(left, rect.center().y - galley.size().y / 2.0), galley, tokens.p.text_primary);
    if response.drag_started() {
        b.drag = Some(folder.path.clone());
    }
    if response.clicked() {
        d.events.push(Event::OpenFolder(folder.path.clone()));
    }
    let path = folder.path.clone();
    response.context_menu(|ui| {
        if menu::item(ui, "Open").clicked() {
            d.events.push(Event::OpenFolder(path.clone()));
        }
        if menu::item(ui, "Colour…").clicked() {
            d.events.push(Event::FolderColour(path.clone()));
        }
        common_menu(ui, &path, d.events);
    });
    b.targets.push((rect, Target::Folder(folder.path.clone())));
}

/// A folder with no colour of its own takes the next accent in turn; in search results they are all the theme's.
fn folder_accent(index: usize, searching: bool, tokens: &Tokens) -> Color32 {
    if searching {
        return tokens.p.primary;
    }
    match index % 5 {
        0 => tokens.p.primary,
        4 => tokens.p.text_muted,
        n => FOLDER_ACCENTS[n - 1],
    }
}

fn common_menu(ui: &mut Ui, path: &Path, events: &mut Vec<Event>) {
    if menu::item(ui, "Rename…").clicked() {
        events.push(Event::Rename(path.to_owned()));
    }
    if menu::item(ui, "Show in the file manager").clicked() {
        events.push(Event::Reveal(path.to_owned()));
    }
    menu::separator(ui);
    if menu::item(ui, "Delete… (to the Trash)").clicked() {
        events.push(Event::Delete(path.to_owned()));
    }
}

#[allow(clippy::too_many_arguments)]
fn clip_card(ui: &mut Ui, rect: Rect, clip: &Clip, favourite: bool, b: &mut Browser, d: &mut Draw<'_>) {
    let tokens = d.tokens;
    let id = b.salt.with(("clip", &clip.path, favourite));
    let response = ui.interact(rect, id, Sense::click_and_drag());
    let name = clip.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, format!("Clip {name}")));
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    let target = drop_target(b, ui, rect, &clip.path);
    card_frame(ui, rect, &response, target, tokens);
    let preview = Rect::from_min_size(rect.min, vec2(rect.width(), PREVIEW_H));
    let painter = ui.painter().with_clip_rect(preview.intersect(ui.clip_rect()));
    paint_preview(ui, &painter, preview, clip, d.thumbs, &mut d.wanted, tokens);
    let foot = Rect::from_min_max(pos2(rect.min.x, preview.max.y), rect.max);
    ui.painter().hline(foot.x_range(), preview.max.y, Stroke::new(1.0, tokens.p.border));
    paint_foot(ui, foot, clip, tokens);
    if matches!(clip.kind, Kind::File(_)) {
        response.clone().on_hover_text(&name);
    }
    if response.drag_started() {
        b.drag = Some(clip.path.clone());
    }
    if response.hovered() && b.drag.is_none() && clip.kind.has_preview() {
        d.hovered = Some((clip.path.clone(), rect, true));
    }
    if response.clicked() {
        let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
        d.events.push(Event::Use { path: clip.path.clone(), ctrl });
    }
    let path = clip.path.clone();
    let is_fav = favourite || d.favourites.iter().any(|f| snippets::same(f, &path));
    response.context_menu(|ui| {
        if menu::item(ui, "Copy (don't paste)").clicked() {
            d.events.push(Event::Copy(path.clone()));
        }
        if menu::item(ui, "Open file").clicked() {
            d.events.push(Event::OpenFile(path.clone()));
        }
        let label = if is_fav { "☆  Remove from favourites" } else { "★  Favourite (row at the top)" };
        if menu::item(ui, label).clicked() {
            d.events.push(Event::Favourite(path.clone(), !is_fav));
        }
        common_menu(ui, &path, d.events);
    });
    b.targets.push((rect, Target::Clip { path: clip.path.clone(), favourite }));
}

fn paint_preview(
    ui: &Ui,
    painter: &egui::Painter,
    area: Rect,
    clip: &Clip,
    thumbs: &Thumbs,
    wanted: &mut Vec<(PathBuf, bool, Option<std::time::SystemTime>)>,
    tokens: &Tokens,
) {
    let p = &tokens.p;
    let top_round = CornerRadius { nw: tokens.rs() as u8, ne: tokens.rs() as u8, sw: 0, se: 0 };
    match &clip.kind {
        Kind::Image | Kind::Video => {
            let video = clip.kind == Kind::Video;
            let thumb = thumbs.get(&clip.path).filter(|t| t.modified == clip.modified);
            if thumbs.wants(&clip.path, clip.modified) {
                wanted.push((clip.path.clone(), video, clip.modified));
            }
            match thumb {
                Some(t) => {
                    let (w, h) = (area.width(), area.height());
                    let scale = (w / t.size.x).max(h / t.size.y);
                    let (uw, uh) = (w / (t.size.x * scale), h / (t.size.y * scale));
                    let uv = Rect::from_min_max(
                        pos2((1.0 - uw) / 2.0, (1.0 - uh) / 2.0),
                        pos2((1.0 + uw) / 2.0, (1.0 + uh) / 2.0),
                    );
                    painter.add(
                        RectShape::filled(area.shrink(1.0), top_round, Color32::WHITE)
                            .with_texture(t.texture.id(), uv),
                    );
                }
                None => striped(painter, area, &clip.path, tokens, top_round),
            }
        }
        Kind::Colour(hex) => {
            let fill = crate::tools::draw::hex(hex).unwrap_or(p.bg_frame);
            painter.add(RectShape::filled(area.shrink(1.0), top_round, fill));
            let light = luminance(fill) > 165.0;
            let text = if light { Color32::from_rgb(0x18, 0x18, 0x0F) } else { Color32::WHITE };
            let galley =
                painter.layout_no_wrap(hex.to_uppercase(), tokens.font(11.0, Weight::Medium, true), text);
            painter.galley(pos2(area.min.x + 12.0, area.max.y - 12.0 - galley.size().y), galley, text);
        }
        Kind::File(short) => {
            let ext = clip
                .path
                .extension()
                .map(|e| e.to_string_lossy().to_uppercase())
                .unwrap_or_else(|| "FILE".into());
            let ext: String = ext.chars().take(4).collect();
            let doc = Rect::from_center_size(pos2(area.center().x, area.center().y - 18.0), vec2(34.0, 42.0));
            painter.rect_stroke(doc, 4.0, Stroke::new(2.0, p.text_muted), StrokeKind::Inside);
            let galley = painter.layout_no_wrap(ext, tokens.font(8.0, Weight::SemiBold, true), p.text_muted);
            painter.galley(
                pos2(doc.center().x - galley.size().x / 2.0, doc.max.y - 4.0 - galley.size().y),
                galley,
                p.text_muted,
            );
            let caption =
                if clip.size.is_empty() { short.clone() } else { format!("{short} · {}", clip.size) };
            let galley =
                elide_to(ui, &caption, 12.0, Weight::Regular, false, p.text_primary, area.width() - 16.0);
            painter.galley(
                pos2(area.center().x - galley.size().x / 2.0, doc.max.y + 10.0),
                galley,
                p.text_primary,
            );
        }
        Kind::Link { title, url } => {
            let inner = area.shrink2(vec2(14.0, 14.0));
            let t = layout(ui, title, Face::new(13.0, Weight::SemiBold, p.text_primary), inner.width(), 2);
            painter.galley(inner.min, t.clone(), p.text_primary);
            let u = layout(
                ui,
                url,
                Face::new(11.0, Weight::Regular, p.primary_text).mono().break_words(),
                inner.width(),
                3,
            );
            painter.galley(pos2(inner.min.x, inner.min.y + t.size().y + 8.0), u, p.primary_text);
        }
        Kind::Code(body) | Kind::Text(body) => {
            let code = matches!(clip.kind, Kind::Code(_));
            let inner = Rect::from_min_max(area.min + vec2(14.0, 12.0), area.max - vec2(14.0, 8.0));
            let (px, factor) = if code { (11.5, 1.55) } else { (13.0, 1.45) };
            let rows = ((inner.height() / (px * factor)).floor() as usize).max(1);
            let galley = {
                let face =
                    Face { mono: code, ..Face::new(px, Weight::Regular, p.text_secondary).line(px * factor) };
                layout(ui, body, face, inner.width(), rows)
            };
            painter.galley(inner.min, galley, p.text_secondary);
        }
    }
}

fn striped(painter: &egui::Painter, area: Rect, path: &Path, tokens: &Tokens, round: CornerRadius) {
    let p = &tokens.p;
    painter.add(RectShape::filled(area.shrink(1.0), round, p.bg_frame));
    let clip = painter.with_clip_rect(area.shrink(1.0));
    let stroke = Stroke::new(8.0, p.bg_header);
    let mut x = area.min.x - area.height();
    while x < area.max.x + area.height() {
        clip.line_segment([pos2(x, area.max.y), pos2(x + area.height(), area.min.y)], stroke);
        x += 16.0;
    }
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let galley = painter.layout_no_wrap(
        middle_elided(&name, 28),
        tokens.font(10.0, Weight::Regular, true),
        p.text_muted,
    );
    painter.galley(area.center() - galley.size() / 2.0, galley, p.text_muted);
}

fn middle_elided(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        return text.to_owned();
    }
    let keep = max.saturating_sub(1);
    let (head, tail) = (keep.div_ceil(2), keep / 2);
    format!(
        "{}…{}",
        chars[..head].iter().collect::<String>(),
        chars[chars.len() - tail..].iter().collect::<String>()
    )
}

fn paint_foot(ui: &Ui, foot: Rect, clip: &Clip, tokens: &Tokens) {
    let p = &tokens.p;
    let kind = mono_caps(ui, &clip.kind.label().to_uppercase(), 9.0, p.text_muted);
    let y = foot.center().y;
    let mut x = foot.min.x + 12.0;
    ui.painter().galley(pos2(x, y - kind.size().y / 2.0), kind.clone(), p.text_muted);
    x += kind.size().x + 8.0;
    let age =
        ui.painter().layout_no_wrap(clip.age.clone(), tokens.font(10.0, Weight::Regular, true), p.text_muted);
    let age_x = foot.max.x - 12.0 - age.size().x;
    ui.painter().galley(pos2(age_x, y - age.size().y / 2.0), age, p.text_muted);
    if !clip.where_.is_empty() && age_x - x > 24.0 {
        let place = elide_to(ui, &clip.where_, 9.5, Weight::Regular, true, p.primary_text, age_x - x - 8.0);
        ui.painter().galley(pos2(x, y - place.size().y / 2.0), place, p.primary_text);
    }
}

// ── dragging ───────────────────────────────────────────────────────────────

fn finish_drag(ui: &mut Ui, b: &mut Browser, events: &mut Vec<Event>, tokens: &Tokens) {
    let Some(src) = b.drag.clone() else { return };
    ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    ui.ctx().request_repaint();
    let pointer = ui.input(|i| i.pointer.hover_pos().or_else(|| i.pointer.latest_pos()));
    if let Some(at) = pointer {
        let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let layer = egui::LayerId::new(egui::Order::Tooltip, b.salt.with("ghost"));
        let painter = ui.ctx().layer_painter(layer);
        let galley = elide_to(ui, &name, 12.0, Weight::Medium, false, tokens.p.text_primary, 160.0);
        let rect = Rect::from_min_size(at + vec2(16.0, 16.0), galley.size() + vec2(24.0, 14.0));
        painter.rect_filled(rect, tokens.rs(), tokens.p.bg_card);
        painter.rect_stroke(rect, tokens.rs(), Stroke::new(1.0, tokens.p.primary), StrokeKind::Inside);
        painter.galley(rect.min + vec2(12.0, 7.0), galley, tokens.p.text_primary);
    }
    let released = ui.input(|i| i.pointer.any_released());
    if !released {
        return;
    }
    b.drag = None;
    let Some(at) = pointer else { return };
    let hit = b.targets.iter().rev().find(|(rect, _)| rect.contains(at)).map(|(_, t)| t.clone());
    match hit {
        Some(Target::Folder(folder)) if !snippets::same(&folder, &src) => {
            events.push(Event::DropOnFolder { src, folder });
        }
        Some(Target::Clip { path, favourite }) if !snippets::same(&path, &src) => {
            events.push(Event::DropOnClip { src, target: path, favourite });
        }
        _ => {}
    }
}

// ── the bigger look ────────────────────────────────────────────────────────

fn hover_preview(
    ui: &mut Ui,
    bounds: Rect,
    b: &mut Browser,
    hovered: Option<(PathBuf, Rect, bool)>,
    thumbs: &Thumbs,
    tokens: &Tokens,
) {
    let now = ui.input(|i| i.time);
    let menu_open = egui::Popup::is_any_open(ui.ctx());
    match hovered {
        Some((path, rect, _)) if !menu_open => {
            if b.hover.as_ref().is_none_or(|h| h.path != path) {
                b.hover = Some(Hover { path, rect, since: now });
                b.preview_text = None;
            } else if let Some(h) = b.hover.as_mut() {
                h.rect = rect;
            }
        }
        _ => b.hover = None,
    }
    let Some(hover) = &b.hover else { return };
    let waited = now - hover.since;
    if waited < HOVER_AFTER {
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(HOVER_AFTER - waited));
        return;
    }
    let (path, card) = (hover.path.clone(), hover.rect);
    let kind = b
        .listing
        .as_ref()
        .and_then(|l| l.favourites.iter().chain(l.clips.iter()).find(|c| c.path == path))
        .map(|c| c.kind.clone());
    let Some(kind) = kind else { return };
    if matches!(kind, Kind::Text(_) | Kind::Code(_) | Kind::Link { .. })
        && b.preview_text.as_ref().is_none_or(|(p, _)| *p != path)
    {
        let text = snippets::read_chars(&path, 4000).unwrap_or_default();
        b.preview_text = Some((path.clone(), text));
    }
    let size = match &kind {
        Kind::Image | Kind::Video => {
            let Some(t) = thumbs.get(&path) else { return };
            let fit = (560.0 / t.size.x).min(420.0 / t.size.y).min(1.0);
            t.size * fit
        }
        Kind::Colour(_) => vec2(220.0, 140.0),
        Kind::Text(_) | Kind::Code(_) | Kind::Link { .. } => vec2(520.0, 0.0),
        Kind::File(_) => return,
    };
    let text = b.preview_text.as_ref().map(|(_, t)| t.clone()).unwrap_or_default();
    let code = matches!(kind, Kind::Code(_));
    let content = if size.y == 0.0 {
        let face = Face { mono: code, ..Face::new(12.0, Weight::Regular, tokens.p.text_primary) };
        let galley = layout(ui, text.trim_end(), face, 520.0, 28);
        galley.size().max(vec2(40.0, 16.0))
    } else {
        size
    };
    let outer = content + vec2(22.0, 22.0);
    let right = pos2(card.max.x + 10.0, card.min.y);
    let at =
        if right.x + outer.x <= bounds.max.x { right } else { pos2(card.min.x - 10.0 - outer.x, card.min.y) };
    let x = at.x.min(bounds.max.x - outer.x).max(bounds.min.x);
    let y = at.y.min(bounds.max.y - outer.y).max(bounds.min.y);
    let rect = Rect::from_min_size(pos2(x, y), outer);
    let layer = egui::LayerId::new(egui::Order::Tooltip, b.salt.with("preview"));
    let painter = ui.ctx().layer_painter(layer);
    painter.rect_filled(rect, tokens.r(), tokens.p.bg_base);
    painter.rect_stroke(rect, tokens.r(), Stroke::new(1.0, tokens.p.border), StrokeKind::Inside);
    let inner = rect.shrink(11.0);
    match &kind {
        Kind::Image | Kind::Video => {
            if let Some(t) = thumbs.get(&path) {
                painter.image(
                    t.texture.id(),
                    inner,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
        }
        Kind::Colour(hex) => {
            painter.rect_filled(inner, 6.0, crate::tools::draw::hex(hex).unwrap_or(tokens.p.bg_frame));
        }
        _ => {
            let more = if text.chars().count() >= 4000 { "…" } else { "" };
            let shown = format!("{}{more}", text.trim_end());
            let galley = layout(
                ui,
                &shown,
                Face { mono: code, ..Face::new(12.0, Weight::Regular, tokens.p.text_primary) },
                inner.width(),
                28,
            );
            painter.galley(inner.min, galley, tokens.p.text_primary);
        }
    }
}

/// What the page does while the snippets folder is missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoFolder {
    Make,
    Other,
}

/// "No snippets folder yet": make it, or pick another, right here.
pub fn no_folder(ui: &mut Ui, rect: Rect, base: &Path) -> Option<NoFolder> {
    let tokens = Tokens::of(ui.ctx());
    let mut chosen = None;
    let centre = rect.center();
    let title = ui.painter().layout_no_wrap(
        "No snippets folder yet".into(),
        tokens.font(15.0, Weight::SemiBold, false),
        tokens.p.text_primary,
    );
    ui.painter().galley(pos2(centre.x - title.size().x / 2.0, centre.y - 70.0), title, tokens.p.text_primary);
    let text = format!("Snippets keeps what you save from the clipboard in\n{}", base.display());
    ui.painter().text(
        pos2(centre.x, centre.y - 40.0),
        Align2::CENTER_TOP,
        text,
        tokens.font(12.0, Weight::Regular, false),
        tokens.p.text_muted,
    );
    let row = Rect::from_center_size(pos2(centre.x, centre.y + 30.0), vec2(300.0, 32.0));
    let mut child =
        ui.new_child(UiBuilder::new().max_rect(row).layout(egui::Layout::left_to_right(egui::Align::Center)));
    child.spacing_mut().item_spacing.x = 10.0;
    if child.add(Button::ghost("Use another folder…")).clicked() {
        chosen = Some(NoFolder::Other);
    }
    if child.add(Button::primary("Make this folder")).clicked() {
        chosen = Some(NoFolder::Make);
    }
    chosen
}
