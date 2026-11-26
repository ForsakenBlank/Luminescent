//! The Dashboard page (`ui/dashboard.py`): boxes of items, page boxes and picture boxes on every screen,
//! dragged, resized, folded, selected and tidied like icons on a desktop.
//!
//! Each screen is a window of its own, so the page is drawn once per screen with that screen's slot (see
//! `overlay::PageSlot`). The state that spans screens (the selection, a drag in progress, the undo of a tidy up)
//! lives in [`DashboardUi`]; the boxes' data lives in the `DashboardStore`, which the page changes only through
//! its acts at the end of a pass, never while it is still drawing from it.
//!
//! A drag that leaves its screen carries on: while the mouse button is held the origin window keeps getting
//! the mouse, so its position on the desktop says which screen a box or row is over, and the drop goes there.

mod boxes;
mod dialogs;
mod gif;
mod icons;
mod menus;
pub mod tidy;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{
    Align2, Context, Id, Key, Modifiers, Order, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
};
use lum_core::dashboard::{DashboardStore, GAP, HEADER_H, ORIGIN, PAGE_H, PAGE_MIN, PAGE_W, ToolSource};
use serde_json::Value;

use crate::overlay::{PageFrame, PageSlot};
use crate::theme::{Tokens, Tone, alpha};
use crate::themes::pictures::Pictures;
use crate::window::Screen;

use dialogs::Ask;
pub use dialogs::ItemDialog;

/// How long Undo stays on the toolbar after a tidy up.
const UNDO_FOR: Duration = Duration::from_secs(12);

/// Draws a page that is a box on the dashboard: its key and the room it has.
pub type SubPage<'a> = dyn FnMut(&mut Ui, &str, Rect) -> PageFrame + 'a;

/// What the page needs from the settings and the rest of the app each frame.
pub struct Env<'a> {
    /// `dashboard.snap`: the grid boxes snap to while moving, 0 for pixel by pixel.
    pub snap: i64,
    pub tidy_style: &'a str,
    pub tidy_order: &'a str,
    pub show_icons: bool,
    pub safe_mode: bool,
    pub is_running: &'a dyn Fn(&str) -> bool,
    /// What already uses a hotkey (the hotkey manager's owner), for the item dialog's warning.
    pub key_owner: &'a dyn Fn(&str) -> Option<String>,
    /// The modules' actions, for the Tools tab of a new dashboard.
    pub tools: &'a [ToolSource],
}

/// What the app has to do after a frame of the page.
#[derive(Debug, Default)]
pub struct Out {
    /// Items to run, with arguments that replace their own ("Run with arguments…").
    pub run: Vec<(String, Option<String>)>,
    /// Items to run at once, without hiding the overlay: the arguments an item asked for.
    pub run_now: Vec<(String, Option<String>)>,
    pub stop: Vec<String>,
    /// Folders to open a terminal in.
    pub terminal_here: Vec<String>,
    pub reveal: Vec<String>,
    /// Desktop notices: title and text.
    pub notify: Vec<(String, String)>,
    /// Settings the page changed (the snap grid, how to tidy up).
    pub settings: Vec<(String, Value)>,
    pub hide_overlay: bool,
    pub open_page: Option<String>,
    /// Hotkeys must not fire while a key is being recorded.
    pub suspend_keys: Option<bool>,
    /// The name of the board that was switched to, for the `dashboard.switched` event.
    pub switched: Option<String>,
}

/// A change to the store, made after the pass.
#[derive(Debug)]
enum Act {
    Geometry(String, Vec<(&'static str, Value)>),
    Fold(Vec<String>, bool),
    Run(String, Option<String>),
    DeleteItem(String),
    MoveItem { id: String, to_tab: String, delta: i64, index: i64 },
    SortTab(String),
    MoveToBoard(Vec<String>, String),
    PlaceBoxes { boxes: Vec<(String, i64, i64)>, screen: String },
    SwitchBoard(String),
    MoveBoard(String, i64),
    SetPin(String, String),
    SetLocked(bool),
    SetSetting(&'static str, Value),
    RestoreTools { x: i64, y: i64, board: String, screen: String },
    AddPaths { tab: String, paths: Vec<String>, index: i64 },
    AddPage { key: String, at: Pos2, screen: String, board: String, size: Vec2 },
    Ask(Ask),
    EditItem { id: String, focus: dialogs::Focus },
    NewItem { tab: String, kind: &'static str, index: i64 },
    NewHeading { tab: String, index: i64 },
    AddSeparator { tab: String, index: i64 },
    AddFiles(String),
    ChoosePicture { at: Pos2, screen: String, board: String },
    ChangePicture(String),
    Tidy(Option<String>),
    UndoTidy,
    SelectAll,
    Remove(Vec<String>),
}

/// Where a box was when a drag of it began.
#[derive(Debug, Clone)]
struct Origin {
    id: String,
    x: i64,
    y: i64,
    w: i64,
}

#[derive(Debug, Clone)]
struct BoxDrag {
    lead: String,
    boxes: Vec<Origin>,
    /// The mouse on the desktop when it started, and where it grabbed the lead box.
    start: Pos2,
    grab: Vec2,
    /// The screen it began on, which keeps getting the mouse until the button is let go.
    screen: String,
    moved: bool,
}

#[derive(Debug, Clone)]
struct RowDrag {
    id: String,
    name: String,
    screen: String,
}

/// Something drawn on a screen last frame: where drags, drops, selection and tidy up look for boxes.
#[derive(Debug, Clone)]
struct Drawn {
    id: String,
    screen: String,
    rect: Rect,
    desktop: Rect,
    /// A tab of items, which takes rows and files.
    items: bool,
    collapsed: bool,
    /// The middle of each row on the desktop, for where a drop lands.
    rows: Vec<f32>,
}

#[derive(Debug, Clone)]
struct Undo {
    boxes: Vec<(String, i64, i64, i64)>,
    until: Instant,
}

#[derive(Debug, Default)]
pub struct DashboardUi {
    selected: Vec<String>,
    drag: Option<BoxDrag>,
    row_drag: Option<RowDrag>,
    /// Where each box being dragged is now, and where it glides from after a tidy up.
    drag_pos: HashMap<String, (i64, i64)>,
    band: Option<(String, Pos2, Vec<String>)>,
    undo: Option<Undo>,
    search: String,
    pointer: Option<Pos2>,
    pointer_frame: u64,
    frame: u64,
    drawn: Vec<Drawn>,
    drawn_now: Vec<Drawn>,
    sizes: HashMap<String, (f32, f32)>,
    dock: Option<Rect>,
    chrome_bottom: f32,
    host: String,
    /// The box last pressed, drawn above the rest.
    top: Option<String>,
    ask: Option<(Ask, crate::dialog::AskDialog)>,
    item_dialog: Option<ItemDialog>,
    /// The screen dialogs and menus open on: the last one clicked.
    ctx_screen: String,
    pictures: Option<Pictures>,
    gifs: gif::Gifs,
    icons: icons::Icons,
    glides: HashMap<String, (Vec2, f64)>,
    resize: Option<boxes::Resize>,
    /// Screen name to the board pinned to it, for boxes added there.
    pins: HashMap<String, String>,
    /// A page box has a dialog of its own open.
    page_busy: bool,
    /// Where the canvas menu was asked for.
    menu_at: Option<Pos2>,
    picked: Arc<std::sync::Mutex<Vec<(dialogs::Pick, String)>>>,
    out: Out,
    acts: Vec<Act>,
}

fn base_id() -> Id {
    Id::unique("lum-dashboard")
}

impl DashboardUi {
    /// What the app has to do now; take it after every frame of the page.
    pub fn take_out(&mut self) -> Out {
        std::mem::take(&mut self.out)
    }

    /// A dialog of the page has the keyboard.
    pub fn busy(&self) -> bool {
        self.ask.is_some() || self.item_dialog.is_some()
    }

    /// The overlay hid: the search clears, the selection and any half-made drag go.
    pub fn hidden(&mut self) {
        self.search.clear();
        self.drag = None;
        self.row_drag = None;
        self.band = None;
        self.drag_pos.clear();
    }

    /// Forgets everything (the overlay was rebuilt after a failure).
    pub fn reset(&mut self) {
        *self = DashboardUi::default();
    }

    /// Draws the page on one screen. Call it for every screen the overlay is on; the first call of a pass for
    /// the switcher's screen sets up the pass.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        slot: &PageSlot<'_>,
        store: &mut DashboardStore,
        env: &Env<'_>,
        sub: &mut SubPage<'_>,
    ) -> PageFrame {
        let ctx = ui.ctx().clone();
        let host = slot.extra_screen.is_none();
        self.begin_pass(&ctx, slot, host);
        let tokens = Tokens::of(&ctx);
        let mut cx =
            Cx { env, slot, tokens, host, q: self.search.trim().to_lowercase(), locked: store.locked() };

        self.pins = slot
            .screens
            .iter()
            .filter(|s| s.name != self.host)
            .filter_map(|s| store.screen_board(&s.name).map(|b| (s.name.clone(), b.id().to_owned())))
            .collect();
        self.track_pointer(ui, slot);
        let ids = self.tabs_here(store, slot);
        let bg = ui.interact(slot.canvas, base_id().with(("canvas", slot.screen)), Sense::click_and_drag());
        self.select_band(ui, &bg, slot);
        self.drag_boxes(ui, store, &cx);
        for id in &ids {
            self.draw_box(ui, store, &mut cx, sub, id);
        }
        if ids.is_empty() && host {
            self.empty_notice(ui, store, slot);
        }
        self.drop_files(ui, &cx);
        self.row_ghost(ui, &cx);
        if bg.secondary_clicked() {
            self.menu_at = bg.interact_pointer_pos();
        }
        bg.context_menu(|ui| {
            let mut acts = Vec::new();
            self.canvas_menu(ui, store, &cx, &mut acts);
            self.acts.extend(acts);
        });
        if host {
            self.dock(ui, store, &cx);
        }
        self.keys(ui, store);
        self.dialogs(ui.ctx(), slot, store, env);
        self.apply(ui.ctx(), store, env, slot);
        PageFrame { busy: self.busy() || self.page_busy }
    }

    fn begin_pass(&mut self, ctx: &Context, slot: &PageSlot<'_>, host: bool) {
        let frame = ctx.cumulative_frame_nr();
        if frame != self.frame {
            self.frame = frame;
            self.drawn = std::mem::take(&mut self.drawn_now);
        }
        if host {
            self.page_busy = false;
            self.chrome_bottom = slot.chrome_bottom;
            self.host = slot.screen.to_owned();
        }
        if self.ctx_screen.is_empty() {
            self.ctx_screen = slot.screen.to_owned();
        }
        if self.undo.as_ref().is_some_and(|u| u.until <= Instant::now()) {
            self.undo = None;
        }
        if let Some(u) = &self.undo {
            ctx.request_repaint_after(u.until.saturating_duration_since(Instant::now()));
        }
    }

    /// The mouse on the desktop while something is being dragged, once per frame.
    fn track_pointer(&mut self, ui: &Ui, slot: &PageSlot<'_>) {
        if self.drag.is_none() && self.row_drag.is_none() {
            self.pointer = None;
            return;
        }
        let at = ui.input(|i| i.pointer.interact_pos()).map(|p| p + slot.to_desktop);
        if self.pointer_frame != self.frame {
            self.pointer_frame = self.frame;
            self.pointer = lum_platform::desktop::cursor().map(|(x, y)| pos2(x as f32, y as f32)).or(at);
        } else if self.pointer.is_none() {
            self.pointer = at;
        }
        ui.ctx().request_repaint();
    }

    fn screen_at<'s>(&self, slot: &'s PageSlot<'_>, desktop: Pos2) -> Option<&'s Screen> {
        slot.screens.iter().find(|s| s.rect.contains(desktop))
    }

    /// The boxes this screen shows. The switcher's screen always shows the active board, with the boxes put on
    /// other screens left out; another screen shows the board pinned to it, else the active board's boxes that
    /// are on it.
    fn tabs_here(&self, store: &DashboardStore, slot: &PageSlot<'_>) -> Vec<String> {
        let host = slot.extra_screen.is_none();
        let all = |board: lum_core::dashboard::BoardView<'_>| {
            board.tabs().map(|t| t.id().to_owned()).collect::<Vec<_>>()
        };
        if !host && let Some(board) = store.screen_board(slot.screen) {
            return all(board);
        }
        let names: Vec<&str> = slot.screens.iter().map(|s| s.name.as_str()).collect();
        let active = store.active_board();
        // the active board pinned to another screen lives there entirely
        if store.pinned_screens(active.id()).iter().any(|s| names.contains(&s.as_str()) && *s != self.host) {
            return Vec::new();
        }
        let mut ids: Vec<String> = active
            .tabs()
            .filter(|t| {
                let on = t.screen();
                if host {
                    let pinned_away = on != self.host && store.screen_board(on).is_some();
                    on.is_empty() || on == slot.screen || !names.contains(&on) || pinned_away
                } else {
                    on == slot.screen
                }
            })
            .map(|t| t.id().to_owned())
            .collect();
        // the box last pressed goes on top, and boxes being dragged above that
        if let Some(top) = &self.top
            && let Some(i) = ids.iter().position(|id| id == top)
        {
            let id = ids.remove(i);
            ids.push(id);
        }
        ids.sort_by_key(|id| self.drag.as_ref().is_some_and(|d| d.boxes.iter().any(|b| &b.id == id)));
        ids
    }

    fn empty_notice(&self, ui: &mut Ui, store: &DashboardStore, slot: &PageSlot<'_>) {
        let tokens = Tokens::of(ui.ctx());
        let active = store.active_board();
        let away = store.pinned_screens(active.id());
        let text = if away.is_empty() {
            "This dashboard is empty. Click + Add for a tab of items or a page, or right-click anywhere."
                .to_owned()
        } else {
            format!(
                "“{}” is shown on {}. Switch to another dashboard for this screen.",
                active.name(),
                away.join(", ")
            )
        };
        let galley = ui.painter().layout_no_wrap(
            text,
            tokens.font(13.0, crate::fonts::Weight::Regular, false),
            tokens.p.text_muted,
        );
        let at = pos2(
            slot.canvas.center().x - galley.size().x / 2.0,
            slot.canvas.min.y + slot.canvas.height() / 3.0,
        );
        ui.painter().galley(at, galley, tokens.p.text_muted);
    }

    // ── selection ───────────────────────────────────────────────────────

    fn is_selected(&self, id: &str) -> bool {
        self.selected.iter().any(|s| s == id)
    }

    fn toggle_selected(&mut self, id: &str) {
        if let Some(i) = self.selected.iter().position(|s| s == id) {
            self.selected.remove(i);
        } else {
            self.selected.push(id.to_owned());
        }
    }

    /// A drag on empty space draws a band and selects what it touches, like a desktop.
    fn select_band(&mut self, ui: &mut Ui, bg: &egui::Response, slot: &PageSlot<'_>) {
        let add = ui.input(|i| i.modifiers.ctrl || i.modifiers.shift);
        if bg.clicked() && !add {
            self.selected.clear();
        }
        if bg.drag_started()
            && let Some(from) = ui.input(|i| i.pointer.press_origin())
        {
            if !add {
                self.selected.clear();
            }
            self.band = Some((slot.screen.to_owned(), from, self.selected.clone()));
        }
        let Some((screen, from, base)) = self.band.clone() else { return };
        if screen != slot.screen {
            return;
        }
        if !ui.input(|i| i.pointer.any_down()) {
            self.band = None;
            return;
        }
        let Some(to) = ui.input(|i| i.pointer.interact_pos()) else { return };
        let band = Rect::from_two_pos(from, to);
        let tokens = Tokens::of(ui.ctx());
        ui.painter().rect_filled(band, 4.0, alpha(tokens.p.primary, 0.14));
        ui.painter().rect_stroke(band, 4.0, Stroke::new(1.0, tokens.p.primary), StrokeKind::Inside);
        let mut picked = base;
        for d in self.drawn.iter().filter(|d| d.screen == slot.screen && d.rect.intersects(band)) {
            if !picked.contains(&d.id) {
                picked.push(d.id.clone());
            }
        }
        self.selected = picked;
    }

    fn keys(&mut self, ui: &mut Ui, store: &DashboardStore) {
        if self.busy() || ui.ctx().memory(|m| m.focused().is_some()) {
            return;
        }
        let (delete, all, undo, escape) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::Delete),
                i.consume_key(Modifiers::CTRL, Key::A),
                i.consume_key(Modifiers::CTRL, Key::Z),
                !self.selected.is_empty() && i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        if delete && !self.selected.is_empty() {
            self.acts.push(Act::Remove(self.selected.clone()));
        }
        if all {
            self.acts.push(Act::SelectAll);
        }
        if undo && self.undo.is_some() {
            self.acts.push(Act::UndoTidy);
        }
        if escape {
            self.selected.clear();
        }
        let _ = store;
    }

    // ── the toolbar ────────────────────────────────────────────────────

    /// The pill at the bottom centre of the switcher's screen: dashboards, add, tidy up, lock, search.
    fn dock(&mut self, ui: &mut Ui, store: &DashboardStore, cx: &Cx<'_>) {
        let tokens = cx.tokens.clone();
        let canvas = cx.slot.canvas;
        let at = pos2(canvas.center().x, canvas.max.y - 28.0);
        let mut acts: Vec<Act> = Vec::new();
        let frame = egui::Frame::new()
            .fill(alpha(tokens.p.bg_card, 0.96))
            .stroke(Stroke::new(1.0, tokens.p.border))
            .corner_radius(tokens.rb() + 4.0)
            .inner_margin(egui::Margin::same(5));
        let shown = egui::Area::new(base_id().with("dock"))
            .order(Order::Foreground)
            .pivot(Align2::CENTER_BOTTOM)
            .fixed_pos(at)
            .show(ui.ctx(), |ui| {
                frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        self.dock_items(ui, store, cx, &mut acts);
                    });
                });
            });
        self.dock = Some(shown.response.rect.translate(-canvas.min.to_vec2()));
        self.acts.extend(acts);
    }

    fn dock_items(&mut self, ui: &mut Ui, store: &DashboardStore, cx: &Cx<'_>, acts: &mut Vec<Act>) {
        use crate::widgets::{Button, TextInput, menu, tip};
        let tokens = cx.tokens.clone();
        let locked = cx.locked;
        self.boards_bar(ui, store, cx, acts);
        let add = ui.add(Button::new("+ Add"));
        tip(add.clone(), "A new tab of items, or a page (Themes, Settings, Clipboard…) as a box");
        egui::Popup::menu(&add).show(|ui| self.add_menu(ui, cx, None, acts));
        let tidy = ui.add(Button::ghost("Tidy up"));
        tip(
            tidy.clone(),
            "Line the boxes up (only the selected ones if any are selected). The arrow next to it picks how; Ctrl+Z puts them back.",
        );
        if tidy.clicked() {
            acts.push(Act::Tidy(None));
        }
        let more = ui.add(Button::ghost("▾").min_width(28.0));
        tip(more.clone(), "How Tidy up arranges the boxes");
        egui::Popup::menu(&more).show(|ui| self.tidy_menu(ui, store, cx, acts));
        if self.undo.is_some() {
            let undo = ui.add(Button::ghost("↶ Undo"));
            tip(undo.clone(), "Put the boxes back where they were before Tidy up (Ctrl+Z)");
            if undo.clicked() {
                acts.push(Act::UndoTidy);
            }
        }
        let lock = ui.add(Button::ghost(if locked { "🔒 Locked" } else { "🔓 Lock" }).checked(locked));
        tip(
            lock.clone(),
            if locked {
                "Unlock to move boxes and items again"
            } else {
                "Lock the layout: no moving boxes or dragging items"
            },
        );
        if lock.clicked() {
            acts.push(Act::SetLocked(!locked));
        }
        crate::widgets::vrule(ui, 18.0);
        let search =
            ui.add(TextInput::new(&mut self.search).hint("Find an item…").width(190.0).name("Find an item"));
        if search.changed() {
            self.search = self.search.trim_start().to_owned();
        }
        if search.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            self.run_first_match(store, acts);
        }
        let q = self.search.trim().to_lowercase();
        let hint = if !q.is_empty() {
            let n = self.count_matches(store, &q);
            format!("{n} match{} · Enter runs the first", if n == 1 { "" } else { "es" })
        } else if locked {
            "Layout locked".to_owned()
        } else {
            "Drag headers and items · drop files on a box".to_owned()
        };
        ui.add(crate::text::Text::new(hint).px(12.0).tone(Tone::Muted));
        let _ = (menu::item, tokens);
    }

    fn count_matches(&self, store: &DashboardStore, q: &str) -> usize {
        self.shown_tabs(store)
            .iter()
            .filter_map(|t| store.tab(t))
            .map(|t| t.items().filter(|i| i.runnable() && i.search_text().contains(q)).count())
            .sum()
    }

    /// Every box on screen now (what the last frame drew).
    fn shown_tabs(&self, _store: &DashboardStore) -> Vec<String> {
        self.drawn.iter().map(|d| d.id.clone()).collect()
    }

    fn run_first_match(&mut self, store: &DashboardStore, acts: &mut Vec<Act>) {
        let q = self.search.trim().to_lowercase();
        if q.is_empty() {
            return;
        }
        for id in self.shown_tabs(store) {
            if let Some(tab) = store.tab(&id)
                && let Some(item) = tab.items().find(|i| i.runnable() && i.search_text().contains(&q))
            {
                acts.push(Act::Run(item.id().to_owned(), None));
                return;
            }
        }
    }

    // ── placing boxes ─────────────────────────────────────────────────────

    /// Keeps a box on its screen: the header always stays reachable.
    fn clamp(canvas: Rect, x: i64, y: i64, w: i64) -> (i64, i64) {
        let max_x = (canvas.width() as i64 - w).max(0);
        let max_y = (canvas.height() as i64 - HEADER_H - 2).max(0);
        (x.min(max_x).max(0), y.min(max_y).max(0))
    }

    /// The next free spot for a new box on a screen.
    fn free_spot(&self, slot: &PageSlot<'_>) -> Pos2 {
        let cols = (((slot.canvas.width() as i64 - ORIGIN.0 * 2 + GAP) / (lum_core::dashboard::BOX_W + GAP))
            .max(1)) as usize;
        let n = self.drawn.iter().filter(|d| d.screen == slot.screen).count();
        let (x0, y0) = tidy::start_of(slot.screen == self.host, self.chrome_bottom);
        pos2(
            (x0 + (n % cols) as i64 * (lum_core::dashboard::BOX_W + GAP)) as f32,
            (y0 + (n / cols) as i64 * 56) as f32,
        )
    }

    fn pictures(&mut self) -> &mut Pictures {
        self.pictures.get_or_insert_with(|| Pictures::with_largest(2048))
    }

    fn page_size(slot: &PageSlot<'_>, at: Pos2) -> Vec2 {
        let w = (PAGE_W as f32).min((slot.canvas.width() - at.x - 24.0).max(PAGE_MIN.0 as f32));
        let h = (PAGE_H as f32)
            .min((slot.canvas.height() - at.y - HEADER_H as f32 - 96.0).max(PAGE_MIN.1 as f32));
        vec2(w, h)
    }
}

/// What every box of one pass shares.
struct Cx<'a> {
    env: &'a Env<'a>,
    slot: &'a PageSlot<'a>,
    tokens: Arc<Tokens>,
    host: bool,
    /// What the search box holds, lower case.
    q: String,
    locked: bool,
}
