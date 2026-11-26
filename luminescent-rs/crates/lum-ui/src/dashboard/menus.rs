//! The page's menus: the toolbar's, and the right-click menus of the open space, a box and a row.

use egui::{Pos2, Ui, vec2};
use lum_core::dashboard::{DashboardStore, FULL_PAGES, ItemView, TOOLS_TAB, TabView};
use serde_json::json;

use super::dialogs::{Ask, Focus};
use super::{Act, Cx, DashboardUi};
use crate::overlay::pages;
use crate::widgets::{Button, menu};

const TIDY_STYLES: [(&str, &str); 3] = [
    ("columns", "Columns (every box keeps its width)"),
    ("grid", "Grid (tabs the same width)"),
    ("rows", "Rows (left to right, then the next row)"),
];
const TIDY_ORDERS: [(&str, &str); 3] =
    [("position", "Keep the order they are in"), ("name", "By name, A to Z"), ("size", "Biggest first")];
const FRAMES: [(&str, &str); 3] = [
    ("panel", "Panel (a background and a border)"),
    ("border", "Just a border"),
    ("none", "Nothing (only the picture)"),
];
const FITS: [(&str, &str); 3] =
    [("fit", "Fit inside"), ("fill", "Fill the box (crop the edges)"), ("stretch", "Stretch to the box")];
const CORNERS: [(i64, &str); 5] =
    [(0, "Square"), (6, "A little round"), (12, "Round"), (24, "Very round"), (999, "As round as it goes")];
const SOLID: [f64; 5] = [1.0, 0.85, 0.7, 0.5, 0.3];
const SNAPS: [(i64, &str); 5] =
    [(0, "Off (smooth, pixel by pixel)"), (4, "4 px"), (8, "8 px"), (16, "16 px"), (32, "32 px")];

fn mark(on: bool) -> &'static str {
    if on { "●  " } else { "     " }
}

/// The pages that can sit on the dashboard as a box: not the ones that fill the screen.
pub(super) fn boxable_pages() -> Vec<(&'static str, &'static str)> {
    pages::all_pages().filter(|(key, _)| !FULL_PAGES.contains(key)).map(|(k, n)| (*k, *n)).collect()
}

impl DashboardUi {
    // ── the toolbar ───────────────────────────────────────────────────────

    pub(super) fn boards_bar(
        &mut self,
        ui: &mut Ui,
        store: &DashboardStore,
        cx: &Cx<'_>,
        acts: &mut Vec<Act>,
    ) {
        let count = store.board_count();
        let active = store.active_board();
        let pills: f32 = store.boards().map(|b| b.name().chars().count() as f32 * 7.5 + 30.0).sum();
        let compact = count > 1 && pills + 560.0 > cx.slot.canvas.width() - 120.0;
        if count > 1 && !compact {
            for board in store.boards() {
                let pill = ui.add(Button::new(board.name()).checked(board.id() == active.id()));
                if pill.clicked() {
                    acts.push(Act::SwitchBoard(board.id().to_owned()));
                }
                let id = board.id().to_owned();
                pill.context_menu(|ui| self.board_menu(ui, store, cx, &id, acts));
            }
        }
        let label = if compact {
            format!("{}  ▾", active.name())
        } else if count > 1 {
            "▾".to_owned()
        } else {
            "Dashboards ▾".to_owned()
        };
        let more = ui.add(if compact { Button::new(label).checked(true) } else { Button::ghost(label) });
        crate::widgets::tip(more.clone(), "Switch dashboard, new dashboard, rename, delete…");
        let id = active.id().to_owned();
        egui::Popup::menu(&more).show(|ui| self.board_menu(ui, store, cx, &id, acts));
        crate::widgets::vrule(ui, 18.0);
    }

    fn board_menu(
        &mut self,
        ui: &mut Ui,
        store: &DashboardStore,
        cx: &Cx<'_>,
        bid: &str,
        acts: &mut Vec<Act>,
    ) {
        let Some(board) = store.board(bid) else { return };
        let active = store.active_id().to_owned();
        let count = store.board_count();
        if count > 1 {
            for other in store.boards() {
                if menu::item(ui, &format!("{}{}", mark(other.id() == active), other.name())).clicked() {
                    acts.push(Act::SwitchBoard(other.id().to_owned()));
                }
            }
            menu::separator(ui);
        }
        if menu::item(ui, "New dashboard…").clicked() {
            acts.push(Act::Ask(Ask::NewBoardHow));
        }
        if menu::item(ui, &format!("Rename “{}”…", board.name())).clicked() {
            acts.push(Act::Ask(Ask::RenameBoard(bid.to_owned())));
        }
        if count > 1 {
            let at = store.boards().position(|b| b.id() == bid).unwrap_or(0);
            if at > 0 && menu::item(ui, &format!("Move “{}” left", board.name())).clicked() {
                acts.push(Act::MoveBoard(bid.to_owned(), -1));
            }
            if at + 1 < count && menu::item(ui, &format!("Move “{}” right", board.name())).clicked() {
                acts.push(Act::MoveBoard(bid.to_owned(), 1));
            }
        }
        let others: Vec<&str> =
            cx.slot.screens.iter().map(|s| s.name.as_str()).filter(|n| *n != self.host).collect();
        if !others.is_empty() {
            ui.menu_button(format!("Show “{}” on screen", board.name()), |ui| {
                let pinned = store.pinned_screens(bid);
                for name in &others {
                    let on = pinned.iter().any(|p| p == name);
                    if menu::item(ui, &format!("{}{name}", mark(on))).clicked() {
                        acts.push(Act::SetPin(
                            (*name).to_owned(),
                            if on { String::new() } else { bid.to_owned() },
                        ));
                    }
                }
            });
        }
        ui.add_enabled_ui(count > 1, |ui| {
            if menu::item(ui, &format!("Delete “{}”…", board.name())).clicked() {
                acts.push(Act::Ask(Ask::DeleteBoard(bid.to_owned())));
            }
        });
    }

    /// What can be added: a tab of items, a picture, a page.
    pub(super) fn add_menu(&mut self, ui: &mut Ui, cx: &Cx<'_>, at: Option<Pos2>, acts: &mut Vec<Act>) {
        let screen = cx.slot.screen.to_owned();
        let board = self.board_for(&screen);
        let at_or_free = |this: &DashboardUi| at.unwrap_or_else(|| this.free_spot(cx.slot));
        if menu::item(ui, "Tab of items…").clicked() {
            acts.push(Act::Ask(Ask::NewTab {
                at: at_or_free(self),
                screen: screen.clone(),
                board: board.clone(),
            }));
        }
        if menu::item(ui, "Picture or GIF…").clicked() {
            acts.push(Act::ChoosePicture {
                at: at_or_free(self),
                screen: screen.clone(),
                board: board.clone(),
            });
        }
        let pick = |ui: &mut Ui, acts: &mut Vec<Act>, this: &DashboardUi| {
            for (key, name) in boxable_pages() {
                if menu::item(ui, name).clicked() {
                    let at = at_or_free(this);
                    acts.push(Act::AddPage {
                        key: key.to_owned(),
                        at,
                        screen: screen.clone(),
                        board: board.clone(),
                        size: Self::page_size(cx.slot, at),
                    });
                }
            }
        };
        if at.is_some() {
            ui.menu_button("Page", |ui| pick(ui, acts, self));
        } else {
            menu::separator(ui);
            menu::heading(ui, "PAGES  (resizable boxes)");
            pick(ui, acts, self);
        }
    }

    /// The board a box added on `screen` goes to: the one pinned there, else the active one.
    pub(super) fn board_for(&self, screen: &str) -> String {
        self.pins.get(screen).cloned().unwrap_or_default()
    }

    pub(super) fn tidy_menu(
        &mut self,
        ui: &mut Ui,
        store: &DashboardStore,
        cx: &Cx<'_>,
        acts: &mut Vec<Act>,
    ) {
        menu::heading(ui, "ARRANGE");
        for (key, label) in TIDY_STYLES {
            let mut on = cx.env.tidy_style == key;
            if menu::check_item(ui, label, &mut on).clicked() {
                acts.push(Act::SetSetting("dashboard.tidy_style", json!(key)));
                acts.push(Act::Tidy(None));
            }
        }
        menu::separator(ui);
        menu::heading(ui, "ORDER");
        for (key, label) in TIDY_ORDERS {
            let mut on = cx.env.tidy_order == key;
            if menu::check_item(ui, label, &mut on).clicked() {
                acts.push(Act::SetSetting("dashboard.tidy_order", json!(key)));
                acts.push(Act::Tidy(None));
            }
        }
        menu::separator(ui);
        if menu::item(ui, "Tidy up now").clicked() {
            acts.push(Act::Tidy(None));
        }
        let shown: Vec<String> = self.drawn.iter().map(|d| d.id.clone()).collect();
        if menu::item(ui, "Fold every box").clicked() {
            acts.push(Act::Fold(shown.clone(), true));
        }
        if menu::item(ui, "Unfold every box").clicked() {
            acts.push(Act::Fold(shown, false));
        }
        if self.undo.is_some() && menu::item(ui, "Undo the last tidy up   (Ctrl+Z)").clicked() {
            acts.push(Act::UndoTidy);
        }
        let _ = store;
    }

    // ── the open space ────────────────────────────────────────────────────

    pub(super) fn canvas_menu(
        &mut self,
        ui: &mut Ui,
        store: &DashboardStore,
        cx: &Cx<'_>,
        acts: &mut Vec<Act>,
    ) {
        let slot = cx.slot;
        self.ctx_screen = slot.screen.to_owned();
        let at = self.menu_at.map(|p| p - slot.canvas.min.to_vec2()).unwrap_or(Pos2::ZERO);
        ui.menu_button("+  Add here", |ui| self.add_menu(ui, cx, Some(at), acts));
        // another screen: which dashboard it shows
        if slot.extra_screen.is_some() && slot.screens.len() > 1 {
            ui.menu_button("This screen shows", |ui| {
                let current = store.screen_board(slot.screen).map(|b| b.id().to_owned());
                if menu::item(
                    ui,
                    &format!("{}The active dashboard (follows the toolbar)", mark(current.is_none())),
                )
                .clicked()
                {
                    acts.push(Act::SetPin(slot.screen.to_owned(), String::new()));
                }
                menu::separator(ui);
                for board in store.boards() {
                    let on = current.as_deref() == Some(board.id());
                    if menu::item(ui, &format!("{}Always “{}”", mark(on), board.name())).clicked() {
                        acts.push(Act::SetPin(slot.screen.to_owned(), board.id().to_owned()));
                    }
                }
            });
        }
        if !self.selected.is_empty() {
            menu::separator(ui);
            menu::heading(ui, &format!("{} SELECTED", self.selected.len()));
            let sel = self.selected.clone();
            if menu::item(ui, "Fold them").clicked() {
                acts.push(Act::Fold(sel.clone(), true));
            }
            if menu::item(ui, "Unfold them").clicked() {
                acts.push(Act::Fold(sel.clone(), false));
            }
            self.move_to_board_menu(ui, store, &sel, acts);
            self.screen_menu(ui, cx, &sel, acts);
            if menu::item(ui, "Remove them…").clicked() {
                acts.push(Act::Remove(sel));
            }
        }
        menu::separator(ui);
        if menu::item_with_hint(ui, "Select all", "Ctrl+A").clicked() {
            acts.push(Act::SelectAll);
        }
        if menu::item(ui, "Tidy up this screen").clicked() {
            acts.push(Act::Tidy(Some(slot.screen.to_owned())));
        }
        if self.undo.is_some() && menu::item_with_hint(ui, "Undo tidy up", "Ctrl+Z").clicked() {
            acts.push(Act::UndoTidy);
        }
        ui.menu_button("Snap to a grid", |ui| {
            for (v, text) in SNAPS {
                let mut on = cx.env.snap == v || (v == 0 && cx.env.snap <= 1);
                if menu::check_item(ui, text, &mut on).clicked() {
                    acts.push(Act::SetSetting("dashboard.snap", json!(v)));
                }
            }
        });
        let board = store.screen_board(slot.screen).or_else(|| Some(store.active_board())).filter(|_| true);
        if let Some(board) = board
            && !board.tabs().any(|t| t.id() == TOOLS_TAB)
            && menu::item(ui, "Bring back the Tools tab").clicked()
        {
            let on_main = slot.extra_screen.is_none();
            acts.push(Act::RestoreTools {
                x: at.x as i64,
                y: at.y as i64,
                board: board.id().to_owned(),
                screen: if on_main { String::new() } else { slot.screen.to_owned() },
            });
        }
        menu::separator(ui);
        let locked = cx.locked;
        if menu::item(ui, if locked { "Unlock layout" } else { "Lock layout" }).clicked() {
            acts.push(Act::SetLocked(!locked));
        }
    }

    fn move_to_board_menu(&self, ui: &mut Ui, store: &DashboardStore, ids: &[String], acts: &mut Vec<Act>) {
        let here = ids
            .first()
            .and_then(|id| store.board_of_tab(id))
            .map_or_else(|| store.active_id().to_owned(), |b| b.id().to_owned());
        let others: Vec<(String, String)> = store
            .boards()
            .filter(|b| b.id() != here)
            .map(|b| (b.id().to_owned(), b.name().to_owned()))
            .collect();
        if others.is_empty() {
            return;
        }
        let title = if ids.len() > 1 {
            format!("Move {} boxes to dashboard", ids.len())
        } else {
            "Move to dashboard".to_owned()
        };
        ui.menu_button(title, |ui| {
            for (bid, name) in &others {
                if menu::item(ui, name).clicked() {
                    acts.push(Act::MoveToBoard(ids.to_vec(), bid.clone()));
                }
            }
        });
    }

    /// "Move to screen", when there is more than one.
    fn screen_menu(&self, ui: &mut Ui, cx: &Cx<'_>, ids: &[String], acts: &mut Vec<Act>) {
        if cx.slot.screens.len() < 2 {
            return;
        }
        ui.menu_button("Move to screen", |ui| {
            for screen in cx.slot.screens {
                let all_here =
                    ids.iter().all(|id| self.drawn.iter().any(|d| &d.id == id && d.screen == screen.name));
                let label = if screen.name == self.host {
                    format!("{}  (this screen)", screen.name)
                } else {
                    screen.name.clone()
                };
                ui.add_enabled_ui(!all_here, |ui| {
                    if menu::item(ui, &format!("{}{label}", mark(all_here))).clicked() {
                        let boxes =
                            ids.iter()
                                .map(|id| {
                                    let at =
                                        self.drawn.iter().find(|d| &d.id == id).map_or(Pos2::ZERO, |d| {
                                            d.rect.min - cx.slot.canvas.min.to_vec2()
                                        });
                                    (id.clone(), at.x as i64, at.y as i64)
                                })
                                .collect();
                        acts.push(Act::PlaceBoxes { boxes, screen: screen.name.clone() });
                    }
                });
            }
        });
    }

    // ── a box ─────────────────────────────────────────────────────────────

    pub(super) fn tab_menu(&mut self, ui: &mut Ui, store: &DashboardStore, cx: &Cx<'_>, tab: &TabView<'_>) {
        let mut acts = Vec::new();
        let id = tab.id().to_owned();
        let several = self.is_selected(&id) && self.selected.len() > 1;
        let group = if several { self.selected.clone() } else { vec![id.clone()] };
        self.ctx_screen = cx.slot.screen.to_owned();
        match tab.kind() {
            "picture" => self.picture_entries(ui, cx, tab, &mut acts),
            "page" => {
                let name = pages::title(tab.page()).unwrap_or(tab.page());
                if menu::item(ui, &format!("Open {name} as the full page")).clicked() {
                    self.out.open_page = Some(tab.page().to_owned());
                }
                if menu::item(ui, "Rename box…").clicked() {
                    acts.push(Act::Ask(Ask::RenameTab(id.clone())));
                }
            }
            _ => {
                if menu::item(ui, "Add item…").clicked() {
                    acts.push(Act::NewItem { tab: id.clone(), kind: "item", index: -1 });
                }
                if menu::item(ui, "Add files…").clicked() {
                    acts.push(Act::AddFiles(id.clone()));
                }
                if menu::item(ui, "Add group (runs several items)…").clicked() {
                    acts.push(Act::NewItem { tab: id.clone(), kind: "group", index: -1 });
                }
                if menu::item(ui, "Add heading…").clicked() {
                    acts.push(Act::NewHeading { tab: id.clone(), index: -1 });
                }
                if menu::item(ui, "Add separator").clicked() {
                    acts.push(Act::AddSeparator { tab: id.clone(), index: -1 });
                }
                menu::separator(ui);
                if menu::item(ui, "Rename tab…").clicked() {
                    acts.push(Act::Ask(Ask::RenameTab(id.clone())));
                }
                if menu::item(ui, "Sort items A–Z").clicked() {
                    acts.push(Act::SortTab(id.clone()));
                }
            }
        }
        menu::separator(ui);
        self.move_to_board_menu(ui, store, &group, &mut acts);
        self.screen_menu(ui, cx, &group, &mut acts);
        menu::separator(ui);
        let text = if several {
            format!("Remove the {} selected boxes…", group.len())
        } else if tab.kind() == "items" {
            "Delete tab…".to_owned()
        } else {
            "Take it off the dashboard".to_owned()
        };
        if menu::item(ui, &text).clicked() {
            acts.push(Act::Remove(group));
        }
        self.acts.extend(acts);
    }

    fn picture_entries(&mut self, ui: &mut Ui, cx: &Cx<'_>, tab: &TabView<'_>, acts: &mut Vec<Act>) {
        let id = tab.id().to_owned();
        let set = |acts: &mut Vec<Act>, key: &'static str, value: serde_json::Value| {
            acts.push(Act::Geometry(id.clone(), vec![(key, value)]));
        };
        if menu::item(ui, "Choose a picture…").clicked() {
            acts.push(Act::ChangePicture(tab.id().to_owned()));
        }
        ui.menu_button("Frame", |ui| {
            for (key, text) in FRAMES {
                let mut on = tab.frame() == key;
                if menu::check_item(ui, text, &mut on).clicked() {
                    set(acts, "frame", json!(key));
                }
            }
            if tab.frame() == "border" {
                let mut on = tab.border_accent();
                if menu::check_item(ui, "Border in the accent colour", &mut on).clicked() {
                    set(acts, "border_accent", json!(on));
                }
            }
        });
        ui.menu_button("Fill the box", |ui| {
            for (key, text) in FITS {
                let mut on = tab.fit() == key;
                if menu::check_item(ui, text, &mut on).clicked() {
                    set(acts, "fit", json!(key));
                }
            }
        });
        ui.menu_button("Corners", |ui| {
            for (v, text) in CORNERS {
                let mut on = tab.radius() == v;
                if menu::check_item(ui, text, &mut on).clicked() {
                    set(acts, "radius", json!(v));
                }
            }
        });
        ui.menu_button("How solid", |ui| {
            for v in SOLID {
                let mut on = (tab.opacity() - v).abs() < 0.01;
                if menu::check_item(ui, &format!("{} %", (v * 100.0) as i64), &mut on).clicked() {
                    set(acts, "opacity", json!(v));
                }
            }
        });
        let mut bar = tab.title_bar();
        if menu::check_item(ui, "Title bar", &mut bar).clicked() {
            set(acts, "title_bar", json!(bar));
        }
        if menu::item(ui, "Rename…").clicked() {
            acts.push(Act::Ask(Ask::RenameTab(tab.id().to_owned())));
        }
        if !cx.locked && menu::item(ui, "Size of the picture").clicked() {
            let path = lum_platform::runner::expand(tab.picture_path());
            let ctx = ui.ctx().clone();
            if let Some(size) = self.pictures().get(&ctx, &path).map(|t| t.size_vec2())
                && size.x > 0.0
            {
                let pad = if tab.frame() == "panel" { 16.0 } else { 0.0 };
                let w = tab.w() as f32;
                let h = ((w - pad) * size.y / size.x + pad) as i64;
                acts.push(Act::Geometry(tab.id().to_owned(), vec![("h", json!(h))]));
            }
        }
        let _ = vec2(0.0, 0.0);
    }

    // ── a row ─────────────────────────────────────────────────────────────

    pub(super) fn item_menu(
        &mut self,
        ui: &mut Ui,
        store: &DashboardStore,
        cx: &Cx<'_>,
        tab: &TabView<'_>,
        item: &ItemView<'_>,
        acts: &mut Vec<Act>,
    ) {
        self.ctx_screen = cx.slot.screen.to_owned();
        let kind = item.kind();
        let iid = item.id().to_owned();
        let plain = kind == "item" && item.action().is_empty();
        let index = tab.items().position(|i| i.id() == item.id()).unwrap_or(0);
        if item.runnable() {
            if menu::item(ui, "▶  Run").clicked() {
                acts.push(Act::Run(iid.clone(), None));
                self.out.hide_overlay = true;
            }
            if plain && menu::item(ui, "Run with arguments…").clicked() {
                acts.push(Act::Ask(Ask::WithArguments(iid.clone())));
            }
            if (cx.env.is_running)(item.id()) && menu::item(ui, "■  Stop").clicked() {
                self.out.stop.push(iid.clone());
            }
            menu::separator(ui);
            if menu::item(ui, "Edit…").clicked() {
                acts.push(Act::EditItem { id: iid.clone(), focus: Focus::Name });
            }
            if menu::item(ui, "Rename…").clicked() {
                acts.push(Act::Ask(Ask::RenameItem(iid.clone())));
            }
            if plain && menu::item(ui, "Change file…").clicked() {
                acts.push(Act::EditItem { id: iid.clone(), focus: Focus::File });
            }
            if menu::item(ui, "Set hotkey…").clicked() {
                acts.push(Act::EditItem { id: iid.clone(), focus: Focus::Hotkey });
            }
        } else if kind == "heading" && menu::item(ui, "Rename…").clicked() {
            acts.push(Act::Ask(Ask::RenameItem(iid.clone())));
        }
        let others: Vec<(String, String)> = store
            .tabs()
            .into_iter()
            .filter(|t| t.id() != tab.id() && t.kind() != "page")
            .map(|t| (t.id().to_owned(), t.title().to_owned()))
            .collect();
        if !others.is_empty() {
            ui.menu_button("Move to tab", |ui| {
                for (tid, title) in &others {
                    if menu::item(ui, title).clicked() {
                        acts.push(Act::MoveItem {
                            id: iid.clone(),
                            to_tab: tid.clone(),
                            delta: 0,
                            index: -1,
                        });
                    }
                }
            });
        }
        if index > 0 && menu::item(ui, "Move up").clicked() {
            acts.push(Act::MoveItem { id: iid.clone(), to_tab: String::new(), delta: -1, index: -1 });
        }
        if index + 1 < tab.item_count() && menu::item(ui, "Move down").clicked() {
            acts.push(Act::MoveItem { id: iid.clone(), to_tab: String::new(), delta: 1, index: -1 });
        }
        let path = lum_platform::runner::expand(item.path());
        if kind == "item" && !path.is_empty() && std::path::Path::new(&path).exists() {
            menu::separator(ui);
            let folder = if std::path::Path::new(&path).is_dir() {
                path.clone()
            } else {
                std::path::Path::new(&path)
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default()
            };
            if menu::item(ui, "Open terminal here").clicked() {
                self.out.terminal_here.push(if item.cwd().is_empty() {
                    folder
                } else {
                    item.cwd().to_owned()
                });
                self.out.hide_overlay = true;
            }
            if menu::item(ui, "Show in file manager").clicked() {
                self.out.reveal.push(path);
            }
        }
        menu::separator(ui);
        ui.menu_button("Insert below", |ui| {
            if menu::item(ui, "Item…").clicked() {
                acts.push(Act::NewItem { tab: tab.id().to_owned(), kind: "item", index: index as i64 + 1 });
            }
            if menu::item(ui, "Group…").clicked() {
                acts.push(Act::NewItem { tab: tab.id().to_owned(), kind: "group", index: index as i64 + 1 });
            }
            if menu::item(ui, "Heading…").clicked() {
                acts.push(Act::NewHeading { tab: tab.id().to_owned(), index: index as i64 + 1 });
            }
            if menu::item(ui, "Separator").clicked() {
                acts.push(Act::AddSeparator { tab: tab.id().to_owned(), index: index as i64 + 1 });
            }
        });
        menu::separator(ui);
        if menu::item(ui, if item.runnable() { "Delete…" } else { "Delete" }).clicked() {
            if item.runnable() {
                acts.push(Act::Ask(Ask::DeleteItem(iid)));
            } else {
                acts.push(Act::DeleteItem(iid));
            }
        }
    }
}
