//! `DashboardStore`: loading and migrating `dashboard.json`, and every edit to boards, boxes and items.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use super::view::{BoardView, ItemView, TabView, int_of, str_of};
use super::{BOX_W, LAYOUT, MODULE_ICONS, ORIGIN, PAGE_H, TOOLS_TAB, new_id};
use crate::pyjson::{self, Style};
use crate::store::{self, Writer};

/// A module's actions, for the Tools tab: what the Python launcher reads from `launcher.modules`.
#[derive(Debug, Clone, Default)]
pub struct ToolSource {
    pub key: String,
    pub icon: String,
    /// Action id and label.
    pub actions: Vec<(String, String)>,
}

type IdMaker = Box<dyn FnMut() -> String + Send>;

pub struct DashboardStore {
    path: PathBuf,
    data: Map<String, Value>,
    writer: Writer,
    next_id: IdMaker,
    /// Counts structural edits (boards, boxes or items added, removed, renamed, moved), so whoever caches
    /// something made from the data knows when to make it again.
    revision: u64,
    hotkeys_stale: bool,
}

impl std::fmt::Debug for DashboardStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DashboardStore").field("path", &self.path).field("revision", &self.revision).finish()
    }
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

fn empty_item(kind: &str) -> Map<String, Value> {
    let mut item = Map::new();
    item.insert("kind".into(), json!(kind));
    item
}

/// Fills in what an item has to have (`_fill_item`).
fn fill_item(item: &mut Map<String, Value>, ids: &mut IdMaker) {
    if !item.contains_key("id") {
        item.insert("id".into(), json!(ids()));
    }
    for key in ["kind", "name", "path", "args", "hotkey", "action", "cwd"] {
        item.entry(key).or_insert_with(|| json!(""));
    }
    let kind_missing = match item.get("kind") {
        None | Some(Value::Null) => true,
        Some(Value::String(s)) => s.is_empty(),
        Some(Value::Bool(false)) => true,
        _ => false,
    };
    if kind_missing {
        item.insert("kind".into(), json!("item"));
    }
    for key in ["admin", "ask_args", "terminal"] {
        item.entry(key).or_insert_with(|| json!(false));
    }
    if !item.get("group").is_some_and(Value::is_array) {
        item.insert("group".into(), json!([]));
    }
}

fn tools_tab(sources: &[ToolSource], x: i64, y: i64, ids: &mut IdMaker) -> Map<String, Value> {
    let mut items = Vec::new();
    for source in sources {
        for (id, label) in &source.actions {
            let mut item = Map::new();
            item.insert("id".into(), json!(ids()));
            item.insert("kind".into(), json!("item"));
            item.insert("name".into(), json!(label));
            item.insert("path".into(), json!(""));
            item.insert("args".into(), json!(""));
            item.insert("hotkey".into(), json!(""));
            item.insert("action".into(), json!(format!("{}:{id}", source.key)));
            fill_item(&mut item, ids);
            items.push(Value::Object(item));
        }
    }
    object(json!({
        "id": TOOLS_TAB, "kind": "items", "title": "Tools", "x": x, "y": y, "w": BOX_W, "h": PAGE_H,
        "collapsed": false, "screen": "", "items": items,
    }))
}

fn base_name(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    let name = if cfg!(windows) { trimmed.rsplit(['/', '\\']).next() } else { trimmed.rsplit('/').next() };
    name.unwrap_or("").to_owned()
}

/// The first start: the Tools tab, and the retired File Launcher's shortcuts as a Shortcuts tab.
fn first_run(
    sources: &[ToolSource],
    raw_settings: &Map<String, Value>,
    ids: &mut IdMaker,
) -> Map<String, Value> {
    let mut tabs = vec![Value::Object(tools_tab(sources, ORIGIN.0, ORIGIN.1, ids))];
    let old = raw_settings
        .get("file_launcher")
        .and_then(|f| f.get("shortcuts"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut items = Vec::new();
    for shortcut in old.iter().filter_map(Value::as_object) {
        let path = str_of(shortcut, "path").trim().to_owned();
        if path.is_empty() {
            continue;
        }
        let name = match base_name(&path) {
            n if n.is_empty() => path.clone(),
            n => n,
        };
        let hotkey = shortcut.get("hotkey").cloned().unwrap_or_else(|| json!(""));
        items.push(
            json!({"id": ids(), "name": name, "path": path, "args": "", "hotkey": hotkey, "action": ""}),
        );
    }
    if !items.is_empty() {
        tracing::info!("moved {} File Launcher shortcut(s) into the dashboard", items.len());
        tabs.push(json!({
            "id": ids(), "title": "Shortcuts", "x": ORIGIN.0 + BOX_W + super::GAP, "y": ORIGIN.1, "w": BOX_W,
            "collapsed": false, "items": items,
        }));
    }
    object(
        json!({"layout": LAYOUT, "active": "main", "boards": [{"id": "main", "name": "Main", "tabs": tabs}]}),
    )
}

/// Panel-relative boxes (layout 1) become screen positions; the Tools tab loses its emoji prefixes.
fn to_screen_layout(d: &mut Map<String, Value>) {
    let Some(tabs) = d.get_mut("tabs").and_then(Value::as_array_mut) else { return };
    for tab in tabs.iter_mut().filter_map(Value::as_object_mut) {
        let x = int_of(tab, "x", 16) - 16 + ORIGIN.0;
        let y = int_of(tab, "y", 16) - 8 + ORIGIN.1;
        tab.insert("x".into(), json!(x));
        tab.insert("y".into(), json!(y));
        if int_of(tab, "w", BOX_W) == 232 {
            tab.insert("w".into(), json!(BOX_W));
        }
        let Some(items) = tab.get_mut("items").and_then(Value::as_array_mut) else { continue };
        for item in items.iter_mut().filter_map(Value::as_object_mut) {
            let module = str_of(item, "action").split(':').next().unwrap_or("");
            let Some((_, icon)) = MODULE_ICONS.iter().find(|(key, _)| *key == module) else { continue };
            let name = str_of(item, "name");
            if let Some(rest) = name.strip_prefix(icon) {
                let rest = rest.trim().to_owned();
                item.insert("name".into(), json!(rest));
            }
        }
    }
    d.insert("layout".into(), json!(2));
}

/// Everything `_load` does after the file is read: layouts 1 and 2 are moved over, then every board, box and
/// item gets the keys it needs.
fn migrate(mut d: Map<String, Value>, ids: &mut IdMaker) -> Map<String, Value> {
    let has_tabs = d.get("tabs").is_some_and(Value::is_array);
    if has_tabs && d.get("layout").and_then(Value::as_f64).unwrap_or(1.0) < 2.0 {
        to_screen_layout(&mut d);
    }
    if !d.get("boards").is_some_and(Value::is_array) {
        let tabs = d.shift_remove("tabs").unwrap_or_else(|| json!([]));
        d.insert("boards".into(), json!([{"id": "main", "name": "Main", "tabs": tabs}]));
        d.insert("active".into(), json!("main"));
    }
    d.shift_remove("tabs");
    d.insert("layout".into(), json!(LAYOUT));
    let mut boards: Vec<Value> = d.get("boards").and_then(Value::as_array).cloned().unwrap_or_default();
    boards.retain(Value::is_object);
    if boards.is_empty() {
        boards.push(json!({"id": "main", "name": "Main", "tabs": []}));
    }
    for board in boards.iter_mut().filter_map(Value::as_object_mut) {
        if !board.contains_key("id") {
            board.insert("id".into(), json!(ids()));
        }
        board.entry("name").or_insert_with(|| json!("Dashboard"));
        let mut tabs: Vec<Value> = board.get("tabs").and_then(Value::as_array).cloned().unwrap_or_default();
        tabs.retain(Value::is_object);
        for tab in tabs.iter_mut().filter_map(Value::as_object_mut) {
            fill_tab(tab, ids);
        }
        board.insert("tabs".into(), Value::Array(tabs));
    }
    let board_ids: Vec<String> = boards.iter().filter_map(|b| b["id"].as_str().map(str::to_owned)).collect();
    let active_ok = d.get("active").and_then(Value::as_str).is_some_and(|a| board_ids.iter().any(|b| b == a));
    if !active_ok {
        d.insert("active".into(), json!(board_ids.first().cloned().unwrap_or_default()));
    }
    d.insert("boards".into(), Value::Array(boards));
    d.entry("locked").or_insert_with(|| json!(false));
    let pins = d.get("screens").and_then(Value::as_object).cloned().unwrap_or_default();
    let kept: Map<String, Value> = pins
        .into_iter()
        .filter(|(_, v)| v.as_str().is_some_and(|b| board_ids.iter().any(|id| id == b)))
        .collect();
    d.insert("screens".into(), Value::Object(kept));
    d
}

fn fill_tab(tab: &mut Map<String, Value>, ids: &mut IdMaker) {
    if !tab.contains_key("id") {
        tab.insert("id".into(), json!(ids()));
    }
    for (key, value) in [
        ("kind", json!("items")),
        ("title", json!("Tab")),
        ("x", json!(16)),
        ("y", json!(16)),
        ("w", json!(BOX_W)),
        ("h", json!(PAGE_H)),
        ("collapsed", json!(false)),
        ("screen", json!("")),
    ] {
        tab.entry(key).or_insert(value);
    }
    if str_of(tab, "kind") == "page" {
        tab.entry("page").or_insert_with(|| json!("settings"));
        if !tab.get("state").is_some_and(Value::is_object) {
            tab.insert("state".into(), json!({}));
        }
    }
    let mut items: Vec<Value> = tab.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
    items.retain(Value::is_object);
    for item in items.iter_mut().filter_map(Value::as_object_mut) {
        fill_item(item, ids);
    }
    tab.insert("items".into(), Value::Array(items));
}

impl DashboardStore {
    /// Reads `path` (a file that is missing or not a dashboard starts the dashboard afresh and writes it) and
    /// migrates it.
    pub fn load(path: &Path, tools: &[ToolSource], raw_settings: &Map<String, Value>) -> DashboardStore {
        DashboardStore::load_with(path, tools, raw_settings, Box::new(new_id))
    }

    pub fn load_with(
        path: &Path,
        tools: &[ToolSource],
        raw_settings: &Map<String, Value>,
        mut ids: IdMaker,
    ) -> DashboardStore {
        let found = match store::load_json(path) {
            Some(Value::Object(d))
                if d.get("tabs").is_some_and(Value::is_array)
                    || d.get("boards").is_some_and(Value::is_array) =>
            {
                Some(d)
            }
            _ => None,
        };
        let d = found.unwrap_or_else(|| {
            let fresh = first_run(tools, raw_settings, &mut ids);
            if let Err(e) = store::save_json(path, &Value::Object(fresh.clone()), Style::indented(2)) {
                tracing::error!("could not write dashboard.json: {e}");
            }
            fresh
        });
        let data = migrate(d, &mut ids);
        DashboardStore {
            path: path.to_owned(),
            data,
            writer: Writer::new("dashboard-saver"),
            next_id: ids,
            revision: 0,
            hotkeys_stale: true,
        }
    }

    pub fn save(&self) {
        let text = pyjson::to_string(&Value::Object(self.data.clone()), Style::indented(2));
        self.writer.save(&self.path, text);
    }

    /// Waits until everything is on disk (at quit, and in tests).
    pub fn flush(&self) {
        self.writer.flush();
    }

    pub fn document(&self) -> &Map<String, Value> {
        &self.data
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// True once after an edit that may have changed an item's hotkey, so the app registers them again.
    pub fn take_hotkeys_stale(&mut self) -> bool {
        std::mem::take(&mut self.hotkeys_stale)
    }

    fn structural(&mut self) {
        self.revision += 1;
        self.hotkeys_stale = true;
        self.save();
    }

    fn make_id(&mut self) -> String {
        (self.next_id)()
    }

    fn boards_vec(&self) -> &[Value] {
        self.data.get("boards").and_then(Value::as_array).map_or(&[], Vec::as_slice)
    }

    fn boards_mut(&mut self) -> &mut Vec<Value> {
        let boards = self.data.entry("boards").or_insert_with(|| json!([]));
        if !boards.is_array() {
            *boards = json!([]);
        }
        boards.as_array_mut().expect("just made it an array")
    }

    // ── boards ───────────────────────────────────────────────────────────
    pub fn boards(&self) -> impl Iterator<Item = BoardView<'_>> {
        self.boards_vec().iter().filter_map(|b| b.as_object().map(BoardView))
    }

    pub fn board_count(&self) -> usize {
        self.boards_vec().len()
    }

    pub fn board(&self, bid: &str) -> Option<BoardView<'_>> {
        self.boards().find(|b| b.id() == bid)
    }

    pub fn active_id(&self) -> &str {
        str_of(&self.data, "active")
    }

    pub fn active_board(&self) -> BoardView<'_> {
        self.board(self.active_id()).or_else(|| self.boards().next()).expect("a dashboard always has a board")
    }

    /// Makes `bid` the active board; false when it already was or does not exist.
    pub fn switch_board(&mut self, bid: &str) -> bool {
        if self.board(bid).is_none() || bid == self.active_id() {
            return false;
        }
        self.data.insert("active".into(), json!(bid));
        self.save();
        true
    }

    /// A new board: a copy of `copy_from`, completely empty, or with just the Tools tab. Becomes the active one.
    pub fn add_board(&mut self, name: &str, copy_from: &str, empty: bool, tools: &[ToolSource]) -> String {
        let name = if name.trim().is_empty() { "Dashboard" } else { name.trim() };
        let id = self.make_id();
        let mut tabs: Vec<Value> = Vec::new();
        if empty {
        } else if !copy_from.is_empty() {
            if let Some(src) = self.board(copy_from) {
                tabs = src.0.get("tabs").and_then(Value::as_array).cloned().unwrap_or_default();
                for tab in tabs.iter_mut().filter_map(Value::as_object_mut) {
                    if str_of(tab, "id") != TOOLS_TAB {
                        tab.insert("id".into(), json!(self.make_id()));
                    }
                    for item in tab.get_mut("items").and_then(Value::as_array_mut).into_iter().flatten() {
                        if let Some(item) = item.as_object_mut() {
                            // a hotkey can only run one item
                            item.insert("id".into(), json!((self.next_id)()));
                            item.insert("hotkey".into(), json!(""));
                        }
                    }
                }
            }
        } else {
            let mut ids = std::mem::replace(&mut self.next_id, Box::new(new_id));
            let mut tab = tools_tab(tools, ORIGIN.0, ORIGIN.1, &mut ids);
            self.next_id = ids;
            tab.insert("id".into(), json!(self.make_id()));
            tabs.push(Value::Object(tab));
        }
        self.boards_mut().push(json!({"id": id, "name": name, "tabs": tabs}));
        self.data.insert("active".into(), json!(id));
        self.structural();
        id
    }

    pub fn rename_board(&mut self, bid: &str, name: &str) {
        if name.trim().is_empty() {
            return;
        }
        if let Some(board) = self.board_mut(bid) {
            board.insert("name".into(), json!(name.trim()));
            self.structural();
        }
    }

    fn board_mut(&mut self, bid: &str) -> Option<&mut Map<String, Value>> {
        self.boards_mut().iter_mut().filter_map(Value::as_object_mut).find(|b| str_of(b, "id") == bid)
    }

    pub fn move_board(&mut self, bid: &str, delta: i64) {
        let boards = self.boards_mut();
        let Some(i) = boards.iter().position(|b| b["id"].as_str() == Some(bid)) else { return };
        let to = i as i64 + delta;
        if (0..boards.len() as i64).contains(&to) {
            let board = boards.remove(i);
            boards.insert(to as usize, board);
            self.structural();
        }
    }

    pub fn delete_board(&mut self, bid: &str) {
        if self.board_count() < 2 {
            return;
        }
        self.boards_mut().retain(|b| b["id"].as_str() != Some(bid));
        if let Some(pins) = self.data.get_mut("screens").and_then(Value::as_object_mut) {
            pins.retain(|_, v| v.as_str() != Some(bid));
        }
        if self.active_id() == bid {
            let first = self.boards().next().map(|b| b.id().to_owned()).unwrap_or_default();
            self.data.insert("active".into(), json!(first));
        }
        self.structural();
    }

    // ── screens ──────────────────────────────────────────────────────────
    /// The board pinned to `screen`, if any.
    pub fn screen_board(&self, screen: &str) -> Option<BoardView<'_>> {
        let bid = self.data.get("screens")?.get(screen)?.as_str()?;
        self.board(bid)
    }

    pub fn pinned_screens(&self, bid: &str) -> Vec<String> {
        self.data.get("screens").and_then(Value::as_object).map_or_else(Vec::new, |pins| {
            pins.iter().filter(|(_, v)| v.as_str() == Some(bid)).map(|(k, _)| k.clone()).collect()
        })
    }

    /// Pins a board to a screen (an empty `bid` makes the screen show the active board again).
    pub fn set_screen_board(&mut self, screen: &str, bid: &str) {
        let exists = !bid.is_empty() && self.board(bid).is_some();
        let pins = self.data.entry("screens").or_insert_with(|| json!({}));
        if let Some(pins) = pins.as_object_mut() {
            if exists {
                pins.insert(screen.to_owned(), json!(bid));
            } else {
                pins.shift_remove(screen);
            }
        }
        self.structural();
    }

    pub fn move_tabs_to_board(&mut self, tids: &[String], bid: &str) {
        let mut moved = false;
        for tid in tids {
            let Some(src) = self.board_of_tab(tid).map(|b| b.id().to_owned()) else { continue };
            if self.board(bid).is_none() || src == bid {
                continue;
            }
            let tab = self.take_tab(&src, tid);
            if let (Some(tab), Some(dst)) = (tab, self.board_mut(bid))
                && let Some(tabs) = dst.get_mut("tabs").and_then(Value::as_array_mut)
            {
                tabs.push(tab);
                moved = true;
            }
        }
        if moved {
            self.structural();
        }
    }

    fn take_tab(&mut self, bid: &str, tid: &str) -> Option<Value> {
        let tabs = self.board_mut(bid)?.get_mut("tabs")?.as_array_mut()?;
        let i = tabs.iter().position(|t| t["id"].as_str() == Some(tid))?;
        Some(tabs.remove(i))
    }

    // ── queries ──────────────────────────────────────────────────────────
    /// The active board's boxes.
    pub fn tabs(&self) -> Vec<TabView<'_>> {
        self.active_board().tabs().collect()
    }

    pub fn all_tabs(&self) -> impl Iterator<Item = (BoardView<'_>, TabView<'_>)> {
        self.boards().flat_map(|b| b.tabs().map(move |t| (b, t)))
    }

    pub fn tab(&self, tid: &str) -> Option<TabView<'_>> {
        self.all_tabs().map(|(_, t)| t).find(|t| t.id() == tid)
    }

    pub fn board_of_tab(&self, tid: &str) -> Option<BoardView<'_>> {
        self.all_tabs().find(|(_, t)| t.id() == tid).map(|(b, _)| b)
    }

    pub fn find_item(&self, iid: &str) -> Option<(TabView<'_>, ItemView<'_>)> {
        self.all_tabs().find_map(|(_, t)| t.items().find(|i| i.id() == iid).map(|i| (t, i)))
    }

    /// Which item uses which hotkey (`normalize` is the hotkey manager's), for clash warnings.
    pub fn hotkey_users(
        &self,
        exclude_item: &str,
        normalize: &dyn Fn(&str) -> String,
    ) -> HashMap<String, String> {
        let mut users = HashMap::new();
        for (_, tab) in self.all_tabs() {
            for item in tab.items().filter(|i| !i.hotkey().is_empty() && i.id() != exclude_item) {
                users.insert(
                    normalize(item.hotkey()),
                    format!("Dashboard → {} → {}", tab.title(), item.name()),
                );
            }
        }
        users
    }

    pub fn locked(&self) -> bool {
        self.data.get("locked").and_then(Value::as_bool).unwrap_or(false)
    }

    pub fn set_locked(&mut self, on: bool) {
        self.data.insert("locked".into(), json!(on));
        self.save();
    }

    // ── edits ────────────────────────────────────────────────────────────
    fn tabs_of(&mut self, bid: &str) -> Option<&mut Vec<Value>> {
        let active = self.active_id().to_owned();
        let bid = if bid.is_empty() || self.board(bid).is_none() { active } else { bid.to_owned() };
        self.board_mut(&bid)?.get_mut("tabs")?.as_array_mut()
    }

    fn push_tab(&mut self, bid: &str, tab: Value) -> String {
        let id = tab["id"].as_str().unwrap_or("").to_owned();
        if let Some(tabs) = self.tabs_of(bid) {
            tabs.push(tab);
        }
        self.structural();
        id
    }

    pub fn add_tab(&mut self, title: &str, x: i64, y: i64, screen: &str, board: &str) -> String {
        let tab = json!({
            "id": self.make_id(), "kind": "items", "title": title, "x": x, "y": y, "w": BOX_W, "h": PAGE_H,
            "collapsed": false, "screen": screen, "items": [],
        });
        self.push_tab(board, tab)
    }

    /// A whole overlay page (Themes, Settings...) as a box on a board.
    #[allow(clippy::too_many_arguments)]
    pub fn add_page_tab(
        &mut self,
        page: &str,
        title: &str,
        rect: [i64; 4],
        screen: &str,
        board: &str,
    ) -> String {
        let tab = json!({
            "id": self.make_id(), "kind": "page", "page": page, "title": title, "x": rect[0], "y": rect[1],
            "w": rect[2], "h": rect[3], "collapsed": false, "screen": screen, "state": {}, "items": [],
        });
        self.push_tab(board, tab)
    }

    /// A picture or GIF as a box on a board.
    pub fn add_picture_tab(&mut self, path: &str, rect: [i64; 4], screen: &str, board: &str) -> String {
        let stem = Path::new(path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let title = if stem.is_empty() { "Picture".to_owned() } else { stem };
        let tab = json!({
            "id": self.make_id(), "kind": "picture", "title": title, "path": path, "x": rect[0], "y": rect[1],
            "w": rect[2], "h": rect[3], "frame": "panel", "fit": "fit", "radius": 12, "opacity": 1.0,
            "title_bar": false, "collapsed": false, "screen": screen, "items": [],
        });
        self.push_tab(board, tab)
    }

    fn tab_mut(&mut self, tid: &str) -> Option<&mut Map<String, Value>> {
        self.boards_mut()
            .iter_mut()
            .filter_map(|b| b.get_mut("tabs")?.as_array_mut())
            .flatten()
            .filter_map(Value::as_object_mut)
            .find(|t| str_of(t, "id") == tid)
    }

    pub fn rename_tab(&mut self, tid: &str, title: &str) {
        if title.trim().is_empty() {
            return;
        }
        if let Some(tab) = self.tab_mut(tid) {
            tab.insert("title".into(), json!(title.trim()));
            self.structural();
        }
    }

    pub fn delete_tabs(&mut self, tids: &[String]) {
        for board in self.boards_mut().iter_mut() {
            if let Some(tabs) = board.get_mut("tabs").and_then(Value::as_array_mut) {
                tabs.retain(|t| !tids.iter().any(|id| t["id"].as_str() == Some(id)));
            }
        }
        self.structural();
    }

    /// Brings the Tools tab back on a board that lost it.
    pub fn restore_tools(&mut self, tools: &[ToolSource], x: i64, y: i64, board: &str, screen: &str) {
        let has_tools = {
            let bid = if board.is_empty() { self.active_id() } else { board };
            self.board(bid).is_some_and(|b| b.tabs().any(|t| t.id() == TOOLS_TAB))
        };
        if has_tools {
            return;
        }
        let mut ids = std::mem::replace(&mut self.next_id, Box::new(new_id));
        let mut tab = tools_tab(tools, x, y, &mut ids);
        self.next_id = ids;
        tab.insert("screen".into(), json!(screen));
        self.push_tab(board, Value::Object(tab));
    }

    /// Adds `item` to a box at `index` (the end when `index` is out of range, or negative).
    pub fn add_item(&mut self, tid: &str, item: Map<String, Value>, index: i64) {
        let mut item = item;
        let mut ids = std::mem::replace(&mut self.next_id, Box::new(new_id));
        fill_item(&mut item, &mut ids);
        self.next_id = ids;
        if let Some(items) = self.tab_mut(tid).and_then(|t| t.get_mut("items")?.as_array_mut()) {
            insert_at(items, Value::Object(item), index);
            self.structural();
        }
    }

    /// A new item of `kind` that has not been filled in (a heading or separator, or the start of a dialog).
    pub fn blank_item(kind: &str, name: &str) -> Map<String, Value> {
        let mut item = empty_item(kind);
        item.insert("name".into(), json!(name));
        item
    }

    /// Files or folders dropped from the file manager become items. `desktop_name` reads the name out of an
    /// app's `.desktop` file.
    pub fn add_paths(
        &mut self,
        tid: &str,
        paths: &[String],
        index: i64,
        desktop_name: &dyn Fn(&str) -> Option<String>,
    ) {
        if self.tab(tid).is_none() {
            return;
        }
        let mut index = index;
        for path in paths {
            let path = path.trim_end_matches('/');
            if path.is_empty() {
                continue;
            }
            let is_file = Path::new(path).is_file();
            let mut name = if is_file {
                Path::new(path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
            } else {
                match base_name(path) {
                    n if n.is_empty() => path.to_owned(),
                    n => n,
                }
            };
            if path.ends_with(".desktop")
                && is_file
                && let Some(n) = desktop_name(path).filter(|n| !n.is_empty())
            {
                name = n;
            }
            let mut item = empty_item("item");
            item.insert("id".into(), json!(self.make_id()));
            item.insert("name".into(), json!(name));
            item.insert("path".into(), json!(path));
            let mut ids = std::mem::replace(&mut self.next_id, Box::new(new_id));
            fill_item(&mut item, &mut ids);
            self.next_id = ids;
            if let Some(items) = self.tab_mut(tid).and_then(|t| t.get_mut("items")?.as_array_mut()) {
                let in_range = index >= 0 && index as usize <= items.len();
                insert_at(items, Value::Object(item), index);
                if in_range {
                    index += 1;
                }
            }
        }
        self.structural();
    }

    fn item_mut(&mut self, iid: &str) -> Option<&mut Map<String, Value>> {
        self.boards_mut()
            .iter_mut()
            .filter_map(|b| b.get_mut("tabs")?.as_array_mut())
            .flatten()
            .filter_map(|t| t.get_mut("items")?.as_array_mut())
            .flatten()
            .filter_map(Value::as_object_mut)
            .find(|i| str_of(i, "id") == iid)
    }

    pub fn update_item(&mut self, iid: &str, changes: Vec<(&str, Value)>) {
        if let Some(item) = self.item_mut(iid) {
            for (key, value) in changes {
                item.insert(key.to_owned(), value);
            }
            self.structural();
        }
    }

    pub fn delete_item(&mut self, iid: &str) {
        for board in self.boards_mut().iter_mut() {
            let tabs = board.get_mut("tabs").and_then(Value::as_array_mut).into_iter().flatten();
            for tab in tabs {
                let Some(items) = tab.get_mut("items").and_then(Value::as_array_mut) else { continue };
                items.retain(|i| i["id"].as_str() != Some(iid));
                for item in items.iter_mut() {
                    if let Some(group) = item.get_mut("group").and_then(Value::as_array_mut) {
                        group.retain(|g| g.as_str() != Some(iid));
                    }
                }
            }
        }
        self.structural();
    }

    /// Moves an item to another box (at `index`, else the end), or `delta` places up or down, or to `index`.
    pub fn move_item(&mut self, iid: &str, to_tab: &str, delta: i64, index: i64) {
        let Some((src, _)) = self.find_item(iid) else { return };
        let src_id = src.id().to_owned();
        let dst_id = if to_tab.is_empty() { src_id.clone() } else { to_tab.to_owned() };
        if self.tab(&dst_id).is_none() {
            return;
        }
        let Some(items) = self.tab_mut(&src_id).and_then(|t| t.get_mut("items")?.as_array_mut()) else {
            return;
        };
        let Some(old) = items.iter().position(|i| i["id"].as_str() == Some(iid)) else { return };
        let mut index = index;
        if delta != 0 && dst_id == src_id {
            index = old as i64 + delta;
            if !(0..items.len() as i64).contains(&index) {
                return;
            }
            let item = items.remove(old);
            items.insert(index as usize, item);
        } else {
            let item = items.remove(old);
            if dst_id == src_id && index >= 0 && (old as i64) < index {
                // the list just got shorter above the drop point
                index -= 1;
            }
            let Some(items) = self.tab_mut(&dst_id).and_then(|t| t.get_mut("items")?.as_array_mut()) else {
                return;
            };
            insert_at(items, item, index);
        }
        self.structural();
    }

    pub fn sort_tab(&mut self, tid: &str) {
        let Some(items) = self.tab_mut(tid).and_then(|t| t.get_mut("items")?.as_array_mut()) else { return };
        items.sort_by_key(|i| i["name"].as_str().unwrap_or("").to_lowercase());
        self.structural();
    }

    /// Position, size, fold state and a picture's look. Saved, but nothing is rebuilt: the box already shows it.
    pub fn set_geometry(&mut self, tid: &str, changes: &[(&str, Value)]) {
        self.set_geometry_many(&[(tid.to_owned(), changes.to_vec())]);
    }

    /// Several boxes at once, saved once (a tidy up moves them all).
    pub fn set_geometry_many(&mut self, boxes: &[(String, Vec<(&str, Value)>)]) {
        let mut any = false;
        for (tid, changes) in boxes {
            if let Some(tab) = self.tab_mut(tid) {
                for (key, value) in changes {
                    tab.insert((*key).to_owned(), value.clone());
                }
                any = true;
            }
        }
        if any {
            self.save();
        }
    }

    /// A page box's remembered state; saved only when it changed.
    pub fn set_page_state(&mut self, tid: &str, state: Map<String, Value>) {
        let state = Value::Object(state);
        if let Some(tab) = self.tab_mut(tid)
            && tab.get("state") != Some(&state)
        {
            tab.insert("state".into(), state);
            self.save();
        }
    }
}

fn insert_at(items: &mut Vec<Value>, item: Value, index: i64) {
    if index >= 0 && index as usize <= items.len() {
        items.insert(index as usize, item);
    } else {
        items.push(item);
    }
}
