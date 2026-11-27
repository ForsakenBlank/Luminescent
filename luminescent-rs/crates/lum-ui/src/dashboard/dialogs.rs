//! The page's questions and the item dialog, and the pass that carries out what the page decided.

use std::sync::{Arc, Mutex};

use egui::{Context, Id, Key, Pos2, Ui, vec2};
use lum_core::dashboard::{DashboardStore, ItemView, snap_to};
use lum_platform::dialogs::{self, Request};
use lum_platform::hotkeys::{Os, keys};
use serde_json::{Map, Value, json};

use super::{Act, DashboardUi, Env, UNDO_FOR, Undo, base_id, tidy};
use crate::dialog::{AskDialog, Icon, Outcome, rounded};
use crate::overlay::PageSlot;
use crate::text::{Text, hint};
use crate::widgets::{Button, Checkbox, HotkeyField, TextInput, recorded_key};

/// What a question on the page is about.
#[derive(Debug, Clone)]
pub enum Ask {
    NewTab {
        at: Pos2,
        screen: String,
        board: String,
    },
    RenameTab(String),
    RenameItem(String),
    RenameBoard(String),
    Heading {
        tab: String,
        index: i64,
    },
    NewBoardHow,
    NewBoardName {
        copy: bool,
        empty: bool,
    },
    DeleteBoard(String),
    DeleteItem(String),
    RemoveBoxes(Vec<String>),
    RemoveLocked(Vec<String>),
    /// "Run with arguments…": the arguments, then it runs.
    WithArguments(String),
    /// An item that asks for its arguments each time it runs.
    Arguments(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Name,
    File,
    Hotkey,
}

/// A file or folder chosen in the system's dialog, on its way back to the page.
#[derive(Debug, Clone)]
pub enum Pick {
    NewPicture { at: Pos2, screen: String, board: String },
    Picture(String),
    AddFile(String),
}

fn dialog_id() -> Id {
    base_id().with("ask")
}

impl DashboardUi {
    /// Asks for the arguments of an item that wants them each time (`ask_args`), then runs it.
    pub fn ask_arguments(&mut self, id: &str, name: &str, default: &str) {
        let label = format!("Arguments for {name}:");
        let title = if name.is_empty() { "Arguments" } else { name };
        self.ask =
            Some((Ask::Arguments(id.to_owned()), AskDialog::ask_text(dialog_id(), title, &label, default)));
    }

    fn open_ask(&mut self, ask: Ask, dialog: AskDialog) {
        self.ask = Some((ask, dialog));
    }

    /// Shows the question and the item dialog on the screen last used, and takes what the system's file dialogs
    /// brought back.
    pub(super) fn dialogs(
        &mut self,
        ctx: &Context,
        slot: &PageSlot<'_>,
        store: &mut DashboardStore,
        env: &Env<'_>,
    ) {
        let target = if slot.screens.iter().any(|s| s.name == self.ctx_screen) {
            self.ctx_screen.clone()
        } else {
            self.host.clone()
        };
        if slot.screen == self.host {
            self.take_picks(store, env);
        }
        if slot.screen != target {
            return;
        }
        if let Some((ask, mut dialog)) = self.ask.take() {
            match dialog.show(ctx) {
                Outcome::Open => self.ask = Some((ask, dialog)),
                Outcome::Cancelled => {}
                Outcome::Accepted(label) => self.answered(ask, &label, dialog.text().to_owned(), store, env),
            }
        }
        if let Some(mut item) = self.item_dialog.take() {
            let own_id = item.id.clone().unwrap_or_default();
            let known = |combo: &str| -> String {
                let normal = keys::normalize(combo);
                if let Some(owner) = (env.key_owner)(&normal).filter(|o| o != "dashboard") {
                    return format!("Already used by {owner}");
                }
                let users = store.hotkey_users(&own_id, &|c| keys::normalize(c));
                users.get(&normal).map_or_else(String::new, |other| format!("Already used by {other}"))
            };
            let outcome = item.show(ctx, &known);
            match outcome {
                ItemOutcome::Open => self.item_dialog = Some(item),
                ItemOutcome::Cancelled => self.out.suspend_keys = Some(false),
                ItemOutcome::Saved(map) => {
                    self.out.suspend_keys = Some(false);
                    let (tab, index, id) = (item.tab.clone(), item.index, item.id.clone());
                    match id {
                        Some(id) => {
                            let changes: Vec<(&str, Value)> = [
                                "name", "path", "args", "hotkey", "cwd", "admin", "ask_args", "terminal",
                                "group",
                            ]
                            .into_iter()
                            .filter_map(|k| map.get(k).map(|v| (k, v.clone())))
                            .collect();
                            store.update_item(&id, changes);
                        }
                        None => store.add_item(&tab, map, index),
                    }
                }
            }
            if self.item_dialog.is_some() && item_recording(&self.item_dialog) {
                self.out.suspend_keys = Some(true);
            }
        }
    }

    fn answered(&mut self, ask: Ask, label: &str, text: String, store: &mut DashboardStore, env: &Env<'_>) {
        let name = text.trim().to_owned();
        match ask {
            Ask::NewTab { at, screen, board } => {
                if !name.is_empty() {
                    let g = env.snap;
                    let on = if screen == self.host { String::new() } else { screen };
                    store.add_tab(
                        &name,
                        snap_to(f64::from(at.x), g),
                        snap_to(f64::from(at.y), g),
                        &on,
                        &board,
                    );
                }
            }
            Ask::RenameTab(id) => store.rename_tab(&id, &name),
            Ask::RenameItem(id) => {
                if !name.is_empty() {
                    store.update_item(&id, vec![("name", json!(name))]);
                }
            }
            Ask::RenameBoard(id) => store.rename_board(&id, &name),
            Ask::Heading { tab, index } => {
                if !name.is_empty() {
                    store.add_item(&tab, DashboardStore::blank_item("heading", &name), index);
                }
            }
            Ask::NewBoardHow => {
                let copy = label.starts_with("A copy");
                let empty = label == "Empty";
                self.open_ask(
                    Ask::NewBoardName { copy, empty },
                    AskDialog::ask_text(dialog_id(), "New dashboard", "Name (e.g. Work, Gaming):", ""),
                );
            }
            Ask::NewBoardName { copy, empty } => {
                if !name.is_empty() {
                    let from = if copy { store.active_id().to_owned() } else { String::new() };
                    store.add_board(&name, &from, empty, env.tools);
                    self.selected.clear();
                    self.out.switched = Some(name);
                }
            }
            Ask::DeleteBoard(id) => {
                store.delete_board(&id);
                self.selected.clear();
            }
            Ask::DeleteItem(id) => store.delete_item(&id),
            Ask::RemoveLocked(ids) => self.confirm_remove(&ids, store),
            Ask::RemoveBoxes(ids) => {
                store.delete_tabs(&ids);
                self.selected.retain(|s| !ids.contains(s));
            }
            Ask::WithArguments(id) => {
                self.out.run.push((id, Some(text)));
                self.out.hide_overlay = true;
            }
            Ask::Arguments(id) => self.out.run_now.push((id, Some(text))),
        }
    }

    fn confirm_remove(&mut self, ids: &[String], store: &DashboardStore) {
        let tabs: Vec<_> = ids.iter().filter_map(|id| store.tab(id)).collect();
        if tabs.is_empty() {
            return;
        }
        let items: usize = tabs.iter().map(|t| t.item_count()).sum();
        let text = match tabs.as_slice() {
            [t] if t.kind() == "picture" => {
                "Take this picture off the dashboard? (The picture file stays where it is.)".to_owned()
            }
            [t] if t.kind() == "page" => {
                format!(
                    "Take the {} box off the dashboard?",
                    if t.title().is_empty() { "page" } else { t.title() }
                )
            }
            _ => {
                let more = if items > 0 { format!(" with {items} item(s)") } else { String::new() };
                format!("Delete {} box(es){more}?", tabs.len())
            }
        };
        let dialog = AskDialog::confirm(dialog_id(), "Remove", &text, "Remove", "Cancel", true);
        self.open_ask(Ask::RemoveBoxes(ids.to_vec()), dialog);
    }

    fn take_picks(&mut self, store: &mut DashboardStore, env: &Env<'_>) {
        let picked: Vec<(Pick, String)> =
            self.picked.lock().map(|mut p| std::mem::take(&mut *p)).unwrap_or_default();
        for (pick, path) in picked {
            match pick {
                Pick::NewPicture { at, screen, board } => {
                    let (w, h) = image::image_dimensions(&path).map_or((260, 200), |(pw, ph)| {
                        (260, ((260.0 * f64::from(ph) / f64::from(pw.max(1))) as i64).clamp(80, 520))
                    });
                    let g = env.snap;
                    let on = if screen == self.host { String::new() } else { screen };
                    store.add_picture_tab(
                        &path,
                        [snap_to(f64::from(at.x), g), snap_to(f64::from(at.y), g), w, h],
                        &on,
                        &board,
                    );
                }
                Pick::Picture(id) => {
                    store.set_geometry(&id, &[("path", json!(path))]);
                    self.pictures().forget(&path);
                    self.gifs.forget(&path);
                }
                Pick::AddFile(tab) => store.add_paths(&tab, &[path], -1, &desktop_name),
            }
        }
    }

    fn pick_file(&self, ctx: &Context, title: &str, pick: Pick, images: bool) {
        let picked = Arc::clone(&self.picked);
        let ctx = ctx.clone();
        let mut request = Request::new(title).folder(std::env::home_dir());
        if images {
            request = request.filter("Pictures and GIFs", &["png", "jpg", "jpeg", "webp", "gif", "bmp"]);
        }
        dialogs::open_file(request, move |path| {
            if let Some(path) = path
                && let Ok(mut list) = picked.lock()
            {
                list.push((pick, path.to_string_lossy().into_owned()));
            }
            ctx.request_repaint();
        });
    }

    // ── carrying out the acts ────────────────────────────────────────────

    pub(super) fn apply(
        &mut self,
        ctx: &Context,
        store: &mut DashboardStore,
        env: &Env<'_>,
        slot: &PageSlot<'_>,
    ) {
        let acts = std::mem::take(&mut self.acts);
        for act in acts {
            self.apply_one(ctx, store, env, slot, act);
        }
    }

    fn apply_one(
        &mut self,
        ctx: &Context,
        store: &mut DashboardStore,
        env: &Env<'_>,
        slot: &PageSlot<'_>,
        act: Act,
    ) {
        match act {
            Act::Geometry(id, changes) => store.set_geometry(&id, &changes),
            Act::Fold(ids, on) => {
                let boxes: Vec<_> = ids.into_iter().map(|id| (id, vec![("collapsed", json!(on))])).collect();
                store.set_geometry_many(&boxes);
            }
            Act::Run(id, args) => self.out.run.push((id, args)),
            Act::DeleteItem(id) => store.delete_item(&id),
            Act::MoveItem { id, to_tab, delta, index } => store.move_item(&id, &to_tab, delta, index),
            Act::SortTab(id) => store.sort_tab(&id),
            Act::MoveToBoard(ids, board) => store.move_tabs_to_board(&ids, &board),
            Act::PlaceBoxes { boxes, screen } => self.place_boxes(store, env, slot, boxes, &screen),
            Act::SwitchBoard(id) => {
                if store.switch_board(&id) {
                    self.selected.clear();
                    self.out.switched = store.board(&id).map(|b| b.name().to_owned());
                }
            }
            Act::MoveBoard(id, delta) => store.move_board(&id, delta),
            Act::SetPin(screen, board) => store.set_screen_board(&screen, &board),
            Act::SetLocked(on) => store.set_locked(on),
            Act::SetSetting(key, value) => self.out.settings.push((key.to_owned(), value)),
            Act::RestoreTools { x, y, board, screen } => {
                store.restore_tools(env.tools, x, y, &board, &screen)
            }
            Act::AddPaths { tab, paths, index } => store.add_paths(&tab, &paths, index, &desktop_name),
            Act::AddPage { key, at, screen, board, size } => {
                let g = env.snap;
                let on = if screen == self.host { String::new() } else { screen };
                let title = crate::overlay::pages::title(&key).unwrap_or(&key).to_owned();
                let rect =
                    [snap_to(f64::from(at.x), g), snap_to(f64::from(at.y), g), size.x as i64, size.y as i64];
                store.add_page_tab(&key, &title, rect, &on, &board);
            }
            Act::Ask(ask) => self.ask_for(ask, store),
            Act::EditItem { id, focus } => {
                if let Some((_, item)) = store.find_item(&id) {
                    if item.kind() == "heading" {
                        let name = item.name().to_owned();
                        self.open_ask(
                            Ask::RenameItem(id),
                            AskDialog::ask_text(dialog_id(), "Rename", "New name:", &name),
                        );
                    } else {
                        self.item_dialog = Some(ItemDialog::edit(&item, store, focus));
                    }
                }
            }
            Act::NewItem { tab, kind, index } => {
                self.item_dialog = Some(ItemDialog::new(kind, &tab, index, store))
            }
            Act::NewHeading { tab, index } => {
                self.open_ask(
                    Ask::Heading { tab, index },
                    AskDialog::ask_text(dialog_id(), "Heading", "Heading text:", ""),
                );
            }
            Act::AddSeparator { tab, index } => {
                store.add_item(&tab, DashboardStore::blank_item("separator", ""), index)
            }
            Act::AddFiles(tab) => self.pick_file(ctx, "Add files", Pick::AddFile(tab), false),
            Act::ChoosePicture { at, screen, board } => {
                self.pick_file(ctx, "Choose a picture or GIF", Pick::NewPicture { at, screen, board }, true);
            }
            Act::ChangePicture(id) => self.pick_file(ctx, "Choose a picture or GIF", Pick::Picture(id), true),
            Act::Tidy(only) => self.tidy(ctx, store, env, slot, only.as_deref()),
            Act::UndoTidy => self.undo_tidy(ctx, store),
            Act::SelectAll => self.selected = self.drawn.iter().map(|d| d.id.clone()).collect(),
            Act::Remove(ids) => {
                if ids.is_empty() {
                    return;
                }
                if store.locked() {
                    let dialog = AskDialog::confirm(
                        dialog_id(),
                        "Layout locked",
                        "The layout is locked. Remove them anyway?",
                        "Remove",
                        "Cancel",
                        false,
                    );
                    self.open_ask(Ask::RemoveLocked(ids), dialog);
                } else {
                    self.confirm_remove(&ids, store);
                }
            }
        }
    }

    fn ask_for(&mut self, ask: Ask, store: &DashboardStore) {
        let id = dialog_id();
        let dialog = match &ask {
            Ask::NewTab { .. } => {
                AskDialog::ask_text(id, "New tab", "Name of the tab (e.g. Scripts, Games, Work):", "")
            }
            Ask::RenameTab(tid) => {
                let title = store.tab(tid).map(|t| t.title().to_owned()).unwrap_or_default();
                AskDialog::ask_text(id, "Rename", "New name:", &title)
            }
            Ask::RenameItem(iid) => {
                let name = store.find_item(iid).map(|(_, i)| i.name().to_owned()).unwrap_or_default();
                AskDialog::ask_text(id, "Rename", "New name:", &name)
            }
            Ask::RenameBoard(bid) => {
                let name = store.board(bid).map(|b| b.name().to_owned()).unwrap_or_default();
                AskDialog::ask_text(id, "Rename dashboard", "New name:", &name)
            }
            Ask::NewBoardHow => {
                let current = store.active_board().name().to_owned();
                let copy = format!("A copy of “{current}”");
                AskDialog::choose(
                    id,
                    "New dashboard",
                    "How should the new dashboard start?",
                    &["Empty", "With the Tools tab", &copy],
                    "Cancel",
                )
            }
            Ask::DeleteBoard(bid) => {
                let Some(board) = store.board(bid) else { return };
                let items: usize = board.tabs().map(|t| t.item_count()).sum();
                let text = format!(
                    "Delete the dashboard \"{}\" with its {} box(es) and {items} item(s)?",
                    board.name(),
                    board.tabs().count()
                );
                AskDialog::confirm(id, "Delete dashboard", &text, "Delete", "Cancel", true)
            }
            Ask::DeleteItem(iid) => {
                let name = store.find_item(iid).map(|(_, i)| i.name().to_owned()).unwrap_or_default();
                let text =
                    format!("Delete \"{name}\" from the dashboard?\n(The file itself is not touched.)");
                AskDialog::confirm(id, "Delete item", &text, "Delete", "Cancel", true)
            }
            Ask::WithArguments(iid) => {
                let (name, args) = store
                    .find_item(iid)
                    .map(|(_, i)| (i.name().to_owned(), i.args().to_owned()))
                    .unwrap_or_default();
                AskDialog::ask_text(id, if name.is_empty() { "Run" } else { &name }, "Arguments:", &args)
            }
            Ask::Heading { .. }
            | Ask::NewBoardName { .. }
            | Ask::RemoveLocked(_)
            | Ask::RemoveBoxes(_)
            | Ask::Arguments(_) => {
                return;
            }
        };
        self.open_ask(ask, dialog);
    }

    /// Boxes put on a screen by hand (dropped from another, or "Move to screen"): they stay on that screen.
    fn place_boxes(
        &mut self,
        store: &mut DashboardStore,
        env: &Env<'_>,
        slot: &PageSlot<'_>,
        boxes: Vec<(String, i64, i64)>,
        screen: &str,
    ) {
        let Some(on) = slot.screens.iter().find(|s| s.name == screen) else { return };
        let target = match store.screen_board(screen).filter(|_| screen != self.host) {
            Some(b) => b.id().to_owned(),
            None => store.active_id().to_owned(),
        };
        for (id, x, y) in boxes {
            let w = self.sizes.get(&id).map_or(264, |s| s.0 as i64);
            let g = env.snap;
            let (x, y) = Self::clamp(
                on.rect.translate(-on.rect.min.to_vec2()),
                snap_to(x as f64, g),
                snap_to(y as f64, g),
                w,
            );
            store.set_geometry(&id, &[("x", json!(x)), ("y", json!(y)), ("screen", json!(screen))]);
            if store.board_of_tab(&id).is_some_and(|b| b.id() != target) {
                store.move_tabs_to_board(&[id], &target);
            }
        }
    }

    // ── tidy up ───────────────────────────────────────────────────────────

    /// Lines the boxes up, screen by screen: only the selected ones if any are selected (they stay where the
    /// selection starts), else every box. The boxes glide there and Ctrl+Z puts them back.
    fn tidy(
        &mut self,
        ctx: &Context,
        store: &mut DashboardStore,
        env: &Env<'_>,
        slot: &PageSlot<'_>,
        only: Option<&str>,
    ) {
        if store.locked() {
            self.out
                .notify
                .push(("Dashboard".to_owned(), "The layout is locked. Unlock it to tidy up.".to_owned()));
            return;
        }
        let picked: Vec<String> =
            self.selected.iter().filter(|id| self.drawn.iter().any(|d| &d.id == *id)).cloned().collect();
        let mut undo = Vec::new();
        let mut moves: Vec<(String, Vec<(&str, Value)>)> = Vec::new();
        let now = ctx.input(|i| i.time);
        for screen in slot.screens {
            if only.is_some_and(|o| o != screen.name) {
                continue;
            }
            let local = |d: &super::Drawn| d.desktop.min - screen.rect.min;
            let here: Vec<&super::Drawn> = self
                .drawn
                .iter()
                .filter(|d| d.screen == screen.name && (picked.is_empty() || picked.contains(&d.id)))
                .collect();
            if here.is_empty() {
                continue;
            }
            let tiles: Vec<tidy::Tile> = here
                .iter()
                .filter_map(|d| {
                    let tab = store.tab(&d.id)?;
                    let at = local(d);
                    Some(tidy::Tile {
                        id: d.id.clone(),
                        title: tab.title().to_owned(),
                        x: at.x as i64,
                        y: at.y as i64,
                        w: d.rect.width() as i64,
                        h: d.rect.height() as i64,
                        is_tab: tab.kind() == "items",
                    })
                })
                .collect();
            undo.extend(tiles.iter().map(|t| (t.id.clone(), t.x, t.y, t.w)));
            let (x0, y0) = if picked.is_empty() {
                tidy::start_of(screen.name == self.host, self.chrome_bottom)
            } else {
                (tiles.iter().map(|t| t.x).min().unwrap_or(0), tiles.iter().map(|t| t.y).min().unwrap_or(0))
            };
            let right = screen.rect.width() as i64 - lum_core::dashboard::ORIGIN.0 / 2;
            let size = screen.rect.size();
            for spot in tidy::arrange(tiles.clone(), x0, y0, right, env.tidy_style, env.tidy_order) {
                let (x, y) = Self::clamp(egui::Rect::from_min_size(Pos2::ZERO, size), spot.x, spot.y, spot.w);
                if let Some(old) = tiles.iter().find(|t| t.id == spot.id) {
                    self.glides.insert(spot.id.clone(), (vec2((old.x - x) as f32, (old.y - y) as f32), now));
                }
                moves.push((spot.id, vec![("x", json!(x)), ("y", json!(y)), ("w", json!(spot.w))]));
            }
        }
        if !undo.is_empty() {
            store.set_geometry_many(&moves);
            self.undo = Some(Undo { boxes: undo, until: std::time::Instant::now() + UNDO_FOR });
            ctx.request_repaint();
        }
    }

    fn undo_tidy(&mut self, ctx: &Context, store: &mut DashboardStore) {
        let Some(undo) = self.undo.take() else { return };
        let now = ctx.input(|i| i.time);
        let mut moves = Vec::new();
        for (id, x, y, w) in undo.boxes {
            if let Some(tab) = store.tab(&id) {
                self.glides.insert(id.clone(), (vec2((tab.x() - x) as f32, (tab.y() - y) as f32), now));
            }
            moves.push((id, vec![("x", json!(x)), ("y", json!(y)), ("w", json!(w))]));
        }
        store.set_geometry_many(&moves);
        ctx.request_repaint();
    }
}

fn item_recording(dialog: &Option<ItemDialog>) -> bool {
    dialog.as_ref().is_some_and(|d| d.recording)
}

#[cfg(target_os = "linux")]
fn desktop_name(path: &str) -> Option<String> {
    lum_platform::apps::read_entry(std::path::Path::new(path)).get("Name").cloned()
}

#[cfg(not(target_os = "linux"))]
fn desktop_name(_path: &str) -> Option<String> {
    None
}

// ── the item dialog ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Path,
    Cwd,
}

#[derive(Debug, Clone)]
struct Member {
    id: String,
    label: String,
    on: bool,
}

pub enum ItemOutcome {
    Open,
    Cancelled,
    Saved(Map<String, Value>),
}

/// Edit or create an item (`ItemDialog`): name, what it opens, arguments, hotkey, working folder and the
/// options; for a group, which items it runs and in what order.
pub struct ItemDialog {
    pub(super) id: Option<String>,
    pub(super) tab: String,
    pub(super) index: i64,
    kind: String,
    name: String,
    path: String,
    args: String,
    cwd: String,
    ask_args: bool,
    admin: bool,
    terminal: bool,
    hotkey: String,
    pub(super) recording: bool,
    warning: String,
    action: String,
    members: Vec<Member>,
    focus: Option<Focus>,
    error: Option<AskDialog>,
    cancelled: bool,
    picks: Arc<Mutex<Vec<(Field, String)>>>,
    base: Map<String, Value>,
}

impl std::fmt::Debug for ItemDialog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ItemDialog").field("id", &self.id).field("name", &self.name).finish()
    }
}

impl ItemDialog {
    pub fn new(kind: &str, tab: &str, index: i64, store: &DashboardStore) -> ItemDialog {
        let mut base = Map::new();
        base.insert("kind".into(), json!(kind));
        let mut dialog = ItemDialog::from_map(base, None, tab, index, store);
        dialog.focus = Some(Focus::Name);
        dialog
    }

    pub fn edit(item: &ItemView<'_>, store: &DashboardStore, focus: Focus) -> ItemDialog {
        let tab = store.find_item(item.id()).map(|(t, _)| t.id().to_owned()).unwrap_or_default();
        let mut dialog = ItemDialog::from_map(item.0.clone(), Some(item.id().to_owned()), &tab, -1, store);
        dialog.focus = Some(focus);
        dialog
    }

    fn from_map(
        base: Map<String, Value>,
        id: Option<String>,
        tab: &str,
        index: i64,
        store: &DashboardStore,
    ) -> ItemDialog {
        let item = ItemView(&base);
        let kind = item.kind().to_owned();
        let chosen: Vec<String> = item.group().iter().map(|s| (*s).to_owned()).collect();
        let mut members = Vec::new();
        if kind == "group" {
            let several = store.board_count() > 1;
            let mut pool: Vec<Member> = Vec::new();
            for (board, t) in store.all_tabs() {
                for i in
                    t.items().filter(|i| i.runnable() && i.kind() != "group" && Some(i.id()) != id.as_deref())
                {
                    let at = if several {
                        format!("{} › {}", board.name(), t.title())
                    } else {
                        t.title().to_owned()
                    };
                    pool.push(Member {
                        id: i.id().to_owned(),
                        label: format!("{}    ·  {at}", i.name()),
                        on: chosen.iter().any(|c| c == i.id()),
                    });
                }
            }
            for c in &chosen {
                if let Some(at) = pool.iter().position(|m| &m.id == c) {
                    members.push(pool.remove(at));
                }
            }
            members.extend(pool);
        }
        ItemDialog {
            id,
            tab: tab.to_owned(),
            index,
            name: item.name().to_owned(),
            path: item.path().to_owned(),
            args: item.args().to_owned(),
            cwd: item.cwd().to_owned(),
            ask_args: item.ask_args(),
            admin: item.admin(),
            terminal: item.terminal(),
            hotkey: item.hotkey().to_owned(),
            action: item.action().to_owned(),
            kind,
            recording: false,
            warning: String::new(),
            members,
            focus: None,
            error: None,
            cancelled: false,
            picks: Arc::default(),
            base,
        }
    }

    fn title(&self) -> String {
        let what = if self.kind == "group" { "group" } else { "item" };
        format!("{} {what}", if self.id.is_some() { "Edit" } else { "New" })
    }

    pub fn show(&mut self, ctx: &Context, conflict: &dyn Fn(&str) -> String) -> ItemOutcome {
        if let Some(error) = &mut self.error {
            if !matches!(error.show(ctx), Outcome::Open) {
                self.error = None;
            }
            return ItemOutcome::Open;
        }
        self.take_picks();
        let mut saved = false;
        let id = base_id().with("item-dialog");
        let (_, closed) = rounded(ctx, id, &self.title(), 560.0, |ui| {
            saved = self.body(ui, ctx, conflict);
        });
        if closed || self.cancelled {
            return ItemOutcome::Cancelled;
        }
        if saved {
            return self.save(conflict);
        }
        ItemOutcome::Open
    }

    fn take_picks(&mut self) {
        let picked: Vec<(Field, String)> =
            self.picks.lock().map(|mut p| std::mem::take(&mut *p)).unwrap_or_default();
        for (field, path) in picked {
            let stem = std::path::Path::new(&path)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            match field {
                Field::Path => {
                    if self.name.trim().is_empty() {
                        let is_dir = std::path::Path::new(&path).is_dir();
                        self.name = if is_dir {
                            path.trim_end_matches('/').rsplit('/').next().unwrap_or(&path).to_owned()
                        } else {
                            stem
                        };
                    }
                    self.path = path;
                }
                Field::Cwd => self.cwd = path,
            }
        }
    }

    fn browse(&self, ctx: &Context, field: Field, folder: bool) {
        let picks = Arc::clone(&self.picks);
        let ctx = ctx.clone();
        let start = lum_platform::runner::expand(if field == Field::Cwd { &self.cwd } else { &self.path });
        let request =
            Request::new(if folder { "Choose a folder" } else { "Choose a script, program or file" }).folder(
                Some(std::path::PathBuf::from(start)).filter(|p| p.is_dir()).or_else(std::env::home_dir),
            );
        let done = move |path: Option<std::path::PathBuf>| {
            if let Some(path) = path
                && let Ok(mut list) = picks.lock()
            {
                list.push((field, path.to_string_lossy().into_owned()));
            }
            ctx.request_repaint();
        };
        if folder {
            dialogs::pick_folder(request, done);
        } else {
            dialogs::open_file(request, done);
        }
    }

    fn body(&mut self, ui: &mut Ui, ctx: &Context, conflict: &dyn Fn(&str) -> String) -> bool {
        let group = self.kind == "group";
        let focus = self.focus.take();
        ui.add(Text::new("Name").px(12.5));
        let placeholder = if group { "Start my day" } else { "What it shows in the tab, e.g. Backup photos" };
        let name = ui.add(TextInput::new(&mut self.name).hint(placeholder).name("Name"));
        if focus == Some(Focus::Name) {
            name.request_focus();
        }
        if group {
            ui.add(hint("Tick the items it runs. Use the arrows to change the order.").px(12.0));
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                let last = self.members.len().saturating_sub(1);
                let mut swap = None;
                for (n, m) in self.members.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.add(Checkbox::new(&mut m.on, m.label.clone()));
                        if n > 0 && ui.add(Button::ghost("↑")).clicked() {
                            swap = Some((n, n - 1));
                        }
                        if n < last && ui.add(Button::ghost("↓")).clicked() {
                            swap = Some((n, n + 1));
                        }
                    });
                }
                if let Some((a, b)) = swap {
                    self.members.swap(a, b);
                }
            });
        } else if self.action.is_empty() {
            ui.add(Text::new("Opens").px(12.5));
            ui.horizontal(|ui| {
                let path = ui.add(
                    TextInput::new(&mut self.path)
                        .hint(
                            "~/scripts/backup.py  ·  ~/Games  ·  https://…  ·  firefox  ·  an app's .desktop",
                        )
                        .width(ui.available_width() - 150.0)
                        .name("Opens"),
                );
                if focus == Some(Focus::File) {
                    path.request_focus();
                    self.browse(ctx, Field::Path, false);
                }
                if ui.add(Button::new("File…")).clicked() {
                    self.browse(ctx, Field::Path, false);
                }
                if ui.add(Button::new("Folder…")).clicked() {
                    self.browse(ctx, Field::Path, true);
                }
            });
            ui.add(Text::new("Arguments").px(12.5));
            ui.add(
                TextInput::new(&mut self.args)
                    .hint("Optional arguments for scripts and programs")
                    .name("Arguments"),
            );
            ui.add(Checkbox::new(&mut self.ask_args, "Ask for arguments each time it runs"));
            ui.add(Text::new("Working folder").px(12.5));
            ui.horizontal(|ui| {
                ui.add(
                    TextInput::new(&mut self.cwd)
                        .hint("Default: the script's own folder (your home for commands)")
                        .width(ui.available_width() - 90.0)
                        .name("Working folder"),
                );
                if ui.add(Button::new("Folder…")).clicked() {
                    self.browse(ctx, Field::Cwd, true);
                }
            });
            let tip = if cfg!(windows) {
                "Windows asks first (the administrator prompt)."
            } else {
                "Scripts run with sudo in their terminal; programs and commands ask through pkexec."
            };
            crate::widgets::tip(ui.add(Checkbox::new(&mut self.admin, "Run as administrator")), tip);
            crate::widgets::tip(
                ui.add(Checkbox::new(
                    &mut self.terminal,
                    "Commands run in a terminal window (to see their output)",
                )),
                "For a command (not a file or folder): it runs in your terminal, which stays open.",
            );
        } else {
            ui.add(Text::new("Opens").px(12.5));
            ui.add(hint(format!("Runs a Luminescent action ({})", self.action)).px(12.0));
        }
        ui.add(Text::new("Hotkey").px(12.5));
        if focus == Some(Focus::Hotkey) {
            self.recording = true;
        }
        let response =
            HotkeyField::new(&self.hotkey).recording(self.recording).warning(Some(&self.warning)).show(ui);
        if response.field.clicked() || response.record.clicked() {
            self.recording = !self.recording;
        }
        if response.clear.clicked() {
            self.hotkey.clear();
            self.warning.clear();
        }
        if self.recording {
            self.record_key(ui, conflict);
        }
        let mut saved = false;
        ui.add_space(4.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(Button::primary("Save")).clicked() {
                saved = true;
            }
            if ui.add(Button::ghost("Cancel")).clicked() {
                self.cancelled = true;
            }
        });
        if !self.recording && ui.input(|i| i.key_pressed(Key::Enter)) {
            saved = true;
        }
        saved
    }

    fn record_key(&mut self, ui: &Ui, conflict: &dyn Fn(&str) -> String) {
        let Some(recorded) = recorded_key(ui) else { return };
        self.recording = false;
        if let Some(combo) = recorded {
            self.hotkey = combo;
            self.warning = self.problem(conflict);
        }
    }

    fn problem(&self, conflict: &dyn Fn(&str) -> String) -> String {
        if self.hotkey.is_empty() {
            return String::new();
        }
        let invalid = keys::validate(&self.hotkey, Os::current());
        if invalid.is_empty() { conflict(&self.hotkey) } else { invalid }
    }

    fn save(&mut self, conflict: &dyn Fn(&str) -> String) -> ItemOutcome {
        let combo = keys::normalize(&self.hotkey);
        if !combo.is_empty() {
            let problem = self.problem(conflict);
            if !problem.is_empty() {
                let text = format!("{}: {problem}", keys::pretty(&combo, Os::current()));
                self.error = Some(AskDialog::error(base_id().with("item-error"), "Hotkey", &text));
                return ItemOutcome::Open;
            }
        }
        let (name, path) = (self.name.trim().to_owned(), self.path.trim().to_owned());
        let mut map = self.base.clone();
        if self.kind == "group" {
            map.insert("name".into(), json!(if name.is_empty() { "Group" } else { &name }));
            map.insert("hotkey".into(), json!(combo));
            map.insert("kind".into(), json!("group"));
            let ids: Vec<&str> = self.members.iter().filter(|m| m.on).map(|m| m.id.as_str()).collect();
            map.insert("group".into(), json!(ids));
            return ItemOutcome::Saved(map);
        }
        if self.action.is_empty() && path.is_empty() {
            self.error = Some(AskDialog::message(
                base_id().with("item-error"),
                "Item",
                "Choose what this item opens.",
                Icon::Warning,
            ));
            return ItemOutcome::Open;
        }
        let fallback = path.trim_end_matches('/').rsplit('/').next().unwrap_or(&path).to_owned();
        let name =
            if name.is_empty() { if fallback.is_empty() { path.clone() } else { fallback } } else { name };
        map.insert("name".into(), json!(name));
        map.insert("hotkey".into(), json!(combo));
        if self.action.is_empty() {
            map.insert("path".into(), json!(path));
            map.insert("args".into(), json!(self.args.trim()));
            map.insert("ask_args".into(), json!(self.ask_args));
            map.insert("cwd".into(), json!(self.cwd.trim()));
            map.insert("admin".into(), json!(self.admin));
            map.insert("terminal".into(), json!(self.terminal));
        }
        ItemOutcome::Saved(map)
    }
}
