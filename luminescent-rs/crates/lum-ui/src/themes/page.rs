//! The Themes page: a grid of theme cards grouped in colour-coded folders (`ui/themes_page.py` `ThemesPage`).
//!
//! Click a card to apply it. Right-click it for Settings…, Rename, Duplicate, Export, Move to folder and Delete.
//! Drag a card onto a folder to put it there; drag a folder's header to move the whole folder. The toolbar
//! imports a `.lumtheme` file or makes a whole theme from one colour. Themes outside a folder are listed under
//! BUILT-IN (Dark, Light) and MY THEMES.

use std::path::PathBuf;

use egui::{
    Align2, Color32, CursorIcon, Id, Layout, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2,
};
use lum_platform::dialogs::Request;
use lum_theme::store::{Folder, Group};
use lum_theme::{Rgb, Theme, ThemeStore};

use crate::dialog::{AskDialog, Outcome};
use crate::fonts::Weight;
use crate::paint;
use crate::picker::{ColourDialog, Picked, PickerEvent, PickerKind};
use crate::text::Text;
use crate::theme::{Palette, Role, Tokens, Tone, colour};
use crate::themes::pictures::Pictures;
use crate::themes::previews::{Canvas, Reg, paint_overlay};
use crate::widgets::{self, Button, menu};

const GAP: f32 = 16.0;
const MAX_COLUMNS: usize = 5;
const PREVIEW_HEIGHT: f32 = 112.0;
const CARD_HEIGHT: f32 = 8.0 + PREVIEW_HEIGHT + 10.0 + 20.0 + 8.0 + 2.0;

/// What a file dialog is for; it comes back with the answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileJob {
    Import,
    Export(String),
    /// A picture or wallpaper for one of the editor's path boxes.
    Browse(String),
}

/// A file dialog the page wants the app to open.
#[derive(Debug, Clone)]
pub struct FileAsk {
    pub job: FileJob,
    pub save: bool,
    pub request: Request,
}

/// What the page hands back to the app after a frame.
#[derive(Debug, Default)]
pub struct PageOut {
    pub files: Vec<FileAsk>,
    /// Desktop notifications: (title, text).
    pub notify: Vec<(String, String)>,
    /// The theme editor should open on this theme.
    pub edit: Option<String>,
}

/// What the page needs from the app.
pub struct PageCx<'a> {
    pub store: &'a mut ThemeStore,
    pub picker: &'a mut ColourDialog,
    /// The window the page is in, so dialogs open in front of it.
    pub near: Rect,
    /// The settings profile's name for its id (for a card's tooltip).
    pub profile_name: &'a dyn Fn(&str) -> String,
}

#[derive(Debug)]
enum Ask {
    NewTheme(String),
    Rename(String),
    Delete(String),
    NewFolder,
    RenameFolder(String),
    FromColour(Rgb),
    UseImported(String),
    Message,
}

enum Act {
    Apply(String),
    Edit(String),
    Rename(String),
    Duplicate(String),
    Export(String),
    Swap(String, String),
    MoveTo(String, String),
    Reset(String),
    Delete(String),
    Toggle(Group),
    RenameFolder(Folder),
    FolderColour(Folder),
    MoveGroup(usize, usize),
    DeleteFolder(String),
    NewTheme(String),
    Import,
    FromColour,
    NewFolder,
}

#[derive(Debug, Clone)]
struct GroupDrag {
    key: String,
}

#[derive(Debug, Default)]
pub struct ThemesPage {
    dialog: Option<(Ask, AskDialog)>,
    group_drag: Option<GroupDrag>,
    pub pictures: Pictures,
    /// The name typed for a folder, until its colour is picked.
    folder_name: String,
}

fn dialog_id() -> Id {
    Id::unique("themes-page-dialog")
}

impl ThemesPage {
    pub fn new() -> ThemesPage {
        ThemesPage::default()
    }

    /// Draws the page into `ui`.
    pub fn ui(&mut self, ui: &mut Ui, cx: &mut PageCx<'_>) -> PageOut {
        let tokens = Tokens::of(ui.ctx());
        let mut out = PageOut::default();
        let mut acts: Vec<Act> = Vec::new();
        self.toolbar(ui, cx, &mut acts);
        egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("themes-page-scroll").show(
            ui,
            |ui| {
                egui::Frame::NONE
                    .inner_margin(egui::Margin { left: 24, right: 24, top: 4, bottom: 24 })
                    .show(ui, |ui| {
                        self.groups(ui, cx, &tokens, &mut acts);
                    });
            },
        );
        self.run(acts, cx, &mut out, ui.ctx());
        self.dialogs(ui.ctx(), cx, &mut out);
        out
    }

    fn toolbar(&mut self, ui: &mut Ui, cx: &PageCx<'_>, acts: &mut Vec<Act>) {
        ui.allocate_ui_with_layout(
            vec2(ui.available_width(), 52.0),
            Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.add_space(24.0);
                ui.spacing_mut().item_spacing.x = 12.0;
                ui.add(Text::new("Themes").px(15.0).semibold());
                let active = cx.store.active().name().to_uppercase();
                ui.add(
                    Text::new(format!("ACTIVE: {active}"))
                        .px(10.0)
                        .mono()
                        .medium()
                        .spacing(0.06)
                        .tone(Tone::Muted),
                );
                ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(24.0);
                    ui.spacing_mut().item_spacing.x = 12.0;
                    let add = ui.add(Button::primary("+ New theme"));
                    widgets::tip(
                        add.clone(),
                        "A copy of the active theme. Right-click any card → Settings… to change it.",
                    );
                    if add.clicked() {
                        acts.push(Act::NewTheme(String::new()));
                    }
                    if ui.add(Button::ghost("New folder")).clicked() {
                        acts.push(Act::NewFolder);
                    }
                    let from = ui.add(Button::ghost("From a colour…"));
                    if widgets::tip(from, "Pick one colour; every other colour is chosen to match it")
                        .clicked()
                    {
                        acts.push(Act::FromColour);
                    }
                    let import = ui.add(Button::ghost("Import…"));
                    if widgets::tip(import, "Add a theme someone shared (.lumtheme)").clicked() {
                        acts.push(Act::Import);
                    }
                });
            },
        );
    }

    fn groups(&mut self, ui: &mut Ui, cx: &mut PageCx<'_>, tokens: &Tokens, acts: &mut Vec<Act>) {
        let groups = cx.store.groups();
        let width = ui.available_width() - 10.0;
        let columns = (((width + GAP) / (170.0 + GAP)).floor() as usize).clamp(1, MAX_COLUMNS);
        let card_w = (((width - GAP * (columns - 1) as f32) / columns as f32).floor()).max(150.0);
        let active = cx.store.active_id().to_owned();
        ui.spacing_mut().item_spacing.y = 22.0;
        let mut header_centres: Vec<(String, f32)> = Vec::new();
        let dragging_theme = egui::DragAndDrop::payload::<String>(ui.ctx());
        for (index, group) in groups.iter().enumerate() {
            let collapsed = cx.store.group_collapsed(group);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 14.0;
                let header = self.header(
                    ui,
                    group,
                    collapsed,
                    index,
                    groups.len(),
                    tokens,
                    acts,
                    dragging_theme.is_some(),
                );
                header_centres.push((group.key.clone(), header.rect.center().y));
                if let Some(id) = header.dropped {
                    let folder = group.folder.as_ref().map(|f| f.id.clone()).unwrap_or_default();
                    acts.push(Act::MoveTo(id, folder));
                }
                if collapsed {
                    return;
                }
                let themes: Vec<&Theme> = group.themes.iter().filter_map(|id| cx.store.get(id)).collect();
                let folder_id = group.folder.as_ref().map(|f| f.id.clone()).unwrap_or_default();
                let cells = themes.len() + 1;
                let rows = cells.div_ceil(columns);
                let size = vec2(width, rows as f32 * CARD_HEIGHT + (rows.saturating_sub(1)) as f32 * GAP);
                let (area, _) = ui.allocate_exact_size(size, Sense::hover());
                for (i, t) in themes.iter().enumerate() {
                    let rect = cell(area, i, columns, card_w);
                    let order: Vec<&str> = group.themes.iter().map(String::as_str).collect();
                    let at = order.iter().position(|id| *id == t.id());
                    self.card(
                        ui,
                        rect,
                        t,
                        t.id() == active,
                        cx,
                        tokens,
                        acts,
                        &group.themes,
                        at,
                        &cx.store.folders(),
                    );
                }
                let rect = cell(area, themes.len(), columns, card_w);
                if add_card(ui, rect, tokens).clicked() {
                    acts.push(Act::NewTheme(folder_id));
                }
            });
        }
        self.finish_group_drag(ui, &groups, &header_centres, tokens, acts);
        if let Some(id) = egui::DragAndDrop::payload::<String>(ui.ctx())
            && let Some(p) = ui.ctx().pointer_latest_pos()
            && let Some(t) = cx.store.get(&id)
        {
            let layer = egui::LayerId::new(egui::Order::Tooltip, Id::unique("themes-card-drag"));
            let painter = ui.ctx().layer_painter(layer);
            let galley = painter.layout_no_wrap(
                t.name().to_owned(),
                tokens.font(12.0, Weight::Medium, false),
                tokens.p.text_primary,
            );
            let rect = Rect::from_min_size(p + vec2(14.0, 10.0), galley.size() + vec2(20.0, 12.0));
            painter.rect_filled(rect, tokens.rs(), tokens.p.bg_card_hover);
            painter.rect_stroke(rect, tokens.rs(), Stroke::new(1.0, tokens.p.primary), StrokeKind::Inside);
            painter.galley(rect.min + vec2(10.0, 6.0), galley, tokens.p.text_primary);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn header(
        &mut self,
        ui: &mut Ui,
        group: &Group,
        collapsed: bool,
        index: usize,
        total: usize,
        tokens: &Tokens,
        acts: &mut Vec<Act>,
        card_in_hand: bool,
    ) -> HeaderOut {
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(vec2(width, 36.0), Sense::click_and_drag());
        let p = &tokens.p;
        let painter = ui.painter();
        let line_y = rect.max.y - 0.5;
        let dropping = card_in_hand && response.contains_pointer();
        painter.hline(rect.x_range(), line_y, Stroke::new(1.0, if dropping { p.primary } else { p.border }));
        if dropping {
            painter.rect_filled(rect, tokens.rs(), alpha_of(p.primary, 0.1));
        }
        let mid = rect.min.y + 14.0;
        painter.text(
            pos2(rect.min.x + 5.0, mid),
            Align2::CENTER_CENTER,
            if collapsed { "▸" } else { "▾" },
            tokens.font(9.0, Weight::Regular, false),
            p.text_muted,
        );
        let chip = Rect::from_center_size(pos2(rect.min.x + 10.0 + 10.0 + 4.0, mid), vec2(8.0, 8.0));
        let chip_colour = Rgb::parse(&group.colour).map_or(p.primary, colour);
        painter.rect_filled(chip, 2.0, chip_colour);
        let title = painter.layout_job(
            Text::new(group.name.to_uppercase()).px(11.0).mono().semibold().spacing(0.14).job(tokens),
        );
        let title_pos = pos2(chip.max.x + 10.0, mid - title.size().y / 2.0);
        let count_at = pos2(title_pos.x + title.size().x + 10.0, mid);
        painter.galley(title_pos, title, p.text_primary);
        painter.text(
            count_at,
            Align2::LEFT_CENTER,
            format!("{:02}", group.themes.len()),
            tokens.font(10.0, Weight::Medium, true),
            p.text_muted,
        );
        if response.hovered() {
            ui.ctx().set_cursor_icon(if self.group_drag.is_some() {
                CursorIcon::Grabbing
            } else {
                CursorIcon::PointingHand
            });
        }
        widgets::tip(response.clone(), "Click to fold · drag to move the folder · right-click for options");
        if response.clicked() {
            acts.push(Act::Toggle(group.clone()));
        }
        if response.drag_started() {
            self.group_drag = Some(GroupDrag { key: group.key.clone() });
        }
        response.context_menu(|ui| {
            if let Some(f) = &group.folder {
                if menu::item(ui, "Rename…").clicked() {
                    acts.push(Act::RenameFolder(f.clone()));
                }
                if menu::item(ui, "Colour…").clicked() {
                    acts.push(Act::FolderColour(f.clone()));
                }
                menu::separator(ui);
            }
            let (can_up, can_down) = (index > 0, index + 1 < total);
            ui.add_enabled_ui(can_up, |ui| {
                if menu::item(ui, "Move up").clicked() {
                    acts.push(Act::MoveGroup(index, index - 1));
                }
            });
            ui.add_enabled_ui(can_down, |ui| {
                if menu::item(ui, "Move down").clicked() {
                    acts.push(Act::MoveGroup(index, index + 1));
                }
            });
            ui.add_enabled_ui(can_up, |ui| {
                if menu::item(ui, "Move to the top").clicked() {
                    acts.push(Act::MoveGroup(index, 0));
                }
            });
            ui.add_enabled_ui(can_down, |ui| {
                if menu::item(ui, "Move to the bottom").clicked() {
                    acts.push(Act::MoveGroup(index, total - 1));
                }
            });
            if let Some(f) = &group.folder {
                menu::separator(ui);
                if menu::item(ui, "Delete folder (keeps its themes)").clicked() {
                    acts.push(Act::DeleteFolder(f.id.clone()));
                }
            }
        });
        let dropped = response.dnd_release_payload::<String>().map(|id| (*id).clone());
        HeaderOut { rect, dropped }
    }

    /// While a folder is dragged by its header: a line where it would land, and the move when it is let go.
    fn finish_group_drag(
        &mut self,
        ui: &mut Ui,
        groups: &[Group],
        centres: &[(String, f32)],
        tokens: &Tokens,
        acts: &mut Vec<Act>,
    ) {
        let Some(drag) = self.group_drag.clone() else { return };
        let Some(from) = groups.iter().position(|g| g.key == drag.key) else {
            self.group_drag = None;
            return;
        };
        let pointer = ui.ctx().pointer_latest_pos();
        let down = ui.input(|i| i.pointer.primary_down());
        let Some(p) = pointer else {
            self.group_drag = None;
            return;
        };
        // the slot among the groups: before the first header whose middle is below the pointer
        let slot = centres.iter().position(|(_, y)| p.y < *y).unwrap_or(centres.len());
        let to = if slot > from { slot - 1 } else { slot };
        if down {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
            let y =
                centres.get(slot).map_or_else(|| centres.last().map_or(p.y, |c| c.1 + 60.0), |c| c.1 - 20.0);
            let rect = ui.max_rect();
            ui.painter().hline(rect.x_range(), y, Stroke::new(2.0, tokens.p.primary));
        } else {
            self.group_drag = None;
            if to != from {
                acts.push(Act::MoveGroup(from, to));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn card(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        t: &Theme,
        active: bool,
        cx: &PageCx<'_>,
        tokens: &Tokens,
        acts: &mut Vec<Act>,
        group: &[String],
        place: Option<usize>,
        folders: &[Folder],
    ) {
        let id = Id::unique(("themes-card", t.id()));
        let response = ui.interact(rect, id, Sense::click_and_drag());
        response.dnd_set_drag_payload(t.id().to_owned());
        if !ui.is_rect_visible(rect) {
            return;
        }
        let p = &tokens.p;
        let radius = tokens.r();
        let painter = ui.painter();
        let role = if response.hovered() { Role::BgCardHover } else { Role::BgCard };
        paint::fill(painter, rect, radius, tokens, role, 1.0);
        let border = if active { Stroke::new(2.0, p.primary) } else { Stroke::new(1.0, p.border) };
        painter.rect_stroke(rect, radius, border, StrokeKind::Inside);

        let inner = rect.shrink(9.0);
        let picture = Rect::from_min_size(inner.min, vec2(inner.width(), PREVIEW_HEIGHT));
        self.picture(ui, picture, t);

        let row = Rect::from_min_size(
            pos2(inner.min.x + 4.0, picture.max.y + 10.0),
            vec2(inner.width() - 8.0, 20.0),
        );
        let painter = ui.painter();
        let badge_w = if active { 26.0 } else { 0.0 };
        let mut job = Text::new(t.name()).px(13.0).medium().job(tokens);
        job.wrap.max_width = (row.width() - badge_w).max(10.0);
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        job.wrap.overflow_character = Some('…');
        let galley = painter.layout_job(job);
        painter.galley(pos2(row.min.x, row.center().y - galley.size().y / 2.0), galley, p.text_primary);
        if active {
            let badge = Rect::from_center_size(pos2(row.max.x - 9.0, row.center().y), vec2(18.0, 18.0));
            paint::fill(painter, badge, 9.0, tokens, Role::Primary, 1.0);
            painter.text(
                badge.center(),
                Align2::CENTER_CENTER,
                "✓",
                tokens.font(10.0, Weight::Regular, false),
                p.on_primary,
            );
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        let mut tip = t.name().to_owned();
        if t.builtin_flag() {
            tip.push_str("\nBuilt-in theme");
        }
        if !t.profile().is_empty() {
            tip.push_str(&format!("\nSettings profile: {}", (cx.profile_name)(t.profile())));
        }
        tip.push_str("\nClick to use · right-click for settings");
        let name = t.name().to_owned();
        response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, active, &name));
        widgets::tip(response.clone(), tip);
        if response.clicked() && !active {
            acts.push(Act::Apply(t.id().to_owned()));
        }
        response.context_menu(|ui| {
            let id = t.id().to_owned();
            if menu::item(ui, "Use this theme").clicked() {
                acts.push(Act::Apply(id.clone()));
            }
            if menu::item(ui, "Settings…").clicked() {
                acts.push(Act::Edit(id.clone()));
            }
            if menu::item(ui, "Rename…").clicked() {
                acts.push(Act::Rename(id.clone()));
            }
            if menu::item(ui, "Duplicate").clicked() {
                acts.push(Act::Duplicate(id.clone()));
            }
            if menu::item(ui, "Export to a file…").clicked() {
                acts.push(Act::Export(id.clone()));
            }
            let earlier = place.filter(|i| *i > 0).map(|i| group[i - 1].clone());
            let later = place.filter(|i| i + 1 < group.len()).map(|i| group[i + 1].clone());
            ui.add_enabled_ui(earlier.is_some(), |ui| {
                if menu::item(ui, "Move earlier").clicked()
                    && let Some(other) = &earlier
                {
                    acts.push(Act::Swap(id.clone(), other.clone()));
                }
            });
            ui.add_enabled_ui(later.is_some(), |ui| {
                if menu::item(ui, "Move later").clicked()
                    && let Some(other) = &later
                {
                    acts.push(Act::Swap(id.clone(), other.clone()));
                }
            });
            ui.menu_button("Move to folder", |ui| {
                if menu::item(ui, "(no folder)").clicked() {
                    acts.push(Act::MoveTo(id.clone(), String::new()));
                }
                for f in folders {
                    if menu::item(ui, &f.name).clicked() {
                        acts.push(Act::MoveTo(id.clone(), f.id.clone()));
                    }
                }
            });
            if t.builtin_flag() && menu::item(ui, "Reset to the original colours").clicked() {
                acts.push(Act::Reset(id.clone()));
            }
            menu::separator(ui);
            let can_delete = cx.store.can_delete(&id);
            let text = if t.builtin_flag() { "Delete… (built-in themes stay)" } else { "Delete…" };
            ui.add_enabled_ui(can_delete, |ui| {
                if menu::item(ui, text).clicked() {
                    acts.push(Act::Delete(id.clone()));
                }
            });
        });
    }

    /// The card picture: the theme's icon, else its background image, else a little mock-up in its colours.
    fn picture(&mut self, ui: &mut Ui, area: Rect, t: &Theme) {
        let palette = Palette::from(&t.palette());
        let radius = crate::themes::previews::card_radius(palette.radius);
        for path in [t.icon(), t.background_image()] {
            if path.is_empty() || !std::path::Path::new(path).is_file() {
                continue;
            }
            let ctx = ui.ctx().clone();
            if let Some(texture) = self.pictures.get(&ctx, path) {
                let painter = ui.painter().with_clip_rect(area);
                painter.rect_filled(area, radius, palette.bg_base);
                let size = texture.size_vec2();
                let scale = (area.width() / size.x).max(area.height() / size.y);
                let shown = size * scale;
                let at = Rect::from_center_size(area.center(), shown);
                painter.image(
                    texture.id(),
                    at,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
                return;
            }
        }
        let canvas = Canvas::new(ui.painter(), &palette);
        let mut reg = Reg::default();
        paint_overlay(&canvas, area, PREVIEW_HEIGHT / 112.0, &mut reg);
    }

    // ── doing things ───────────────────────────────────────────────────────

    fn run(&mut self, acts: Vec<Act>, cx: &mut PageCx<'_>, out: &mut PageOut, ctx: &egui::Context) {
        let acted = !acts.is_empty();
        for act in acts {
            match act {
                Act::Apply(id) => {
                    cx.store.apply(&id);
                }
                Act::Edit(id) => out.edit = Some(id),
                Act::Rename(id) => {
                    let name = cx.store.get(&id).map(|t| t.name().to_owned()).unwrap_or_default();
                    self.ask(
                        Ask::Rename(id),
                        AskDialog::ask_text(dialog_id(), "Rename theme", "New name:", &name),
                    );
                }
                Act::Duplicate(id) => {
                    cx.store.duplicate(&id);
                }
                Act::Export(id) => {
                    let name = cx.store.get(&id).map(|t| t.name().to_owned()).unwrap_or_default();
                    let safe: String = name.chars().filter(|c| !"\\/:*?\"<>|".contains(*c)).collect();
                    let safe =
                        if safe.trim().is_empty() { "theme".to_owned() } else { safe.trim().to_owned() };
                    let request = Request::new("Export theme")
                        .folder(Some(lum_core::paths::user_home()))
                        .file_name(&format!("{safe}.lumtheme"))
                        .filter("Luminescent themes", &["lumtheme"]);
                    out.files.push(FileAsk { job: FileJob::Export(id), save: true, request });
                }
                Act::Swap(a, b) => cx.store.swap_themes(&a, &b),
                Act::MoveTo(id, folder) => cx.store.move_to_folder(&id, &folder),
                Act::Reset(id) => cx.store.reset_builtin(&id),
                Act::Delete(id) => {
                    let name = cx.store.get(&id).map(|t| t.name().to_owned()).unwrap_or_default();
                    self.ask(
                        Ask::Delete(id),
                        AskDialog::confirm(
                            dialog_id(),
                            "Delete theme",
                            &format!("Delete the theme \"{name}\"?"),
                            "Delete",
                            "Cancel",
                            true,
                        ),
                    );
                }
                Act::Toggle(group) => cx.store.toggle_group(&group),
                Act::RenameFolder(f) => {
                    self.ask(
                        Ask::RenameFolder(f.id.clone()),
                        AskDialog::ask_text(dialog_id(), "Rename folder", "New name:", &f.name),
                    );
                }
                Act::FolderColour(f) => {
                    let start = Rgb::parse(&f.colour).unwrap_or(Rgb::from_u32(0x7B6CF6));
                    cx.picker.open(
                        &format!("folder:{}", f.id),
                        "Folder colour",
                        Picked::Colour(start),
                        false,
                        cx.near,
                    );
                }
                Act::MoveGroup(from, to) => self.move_group(cx.store, from, to),
                Act::DeleteFolder(id) => cx.store.delete_folder(&id),
                Act::NewTheme(folder) => {
                    self.ask(
                        Ask::NewTheme(folder),
                        AskDialog::ask_text(dialog_id(), "New theme", "Name:", ""),
                    );
                }
                Act::Import => {
                    let request = Request::new("Import a theme")
                        .folder(Some(lum_core::paths::user_home()))
                        .filter("Luminescent themes", &["lumtheme"]);
                    out.files.push(FileAsk { job: FileJob::Import, save: false, request });
                }
                Act::FromColour => {
                    let start = Picked::Colour(Rgb::new(
                        Tokens::of(ctx).p.primary.r(),
                        Tokens::of(ctx).p.primary.g(),
                        Tokens::of(ctx).p.primary.b(),
                    ));
                    cx.picker.open("from-colour", "The theme's accent colour", start, false, cx.near);
                }
                Act::NewFolder => {
                    self.ask(
                        Ask::NewFolder,
                        AskDialog::ask_text(dialog_id(), "New folder", "Folder name:", ""),
                    );
                }
            }
        }
        if acted {
            // a change of the store has to reach the app's poll, which restyles everything
            ctx.request_repaint();
        }
    }

    fn move_group(&mut self, store: &mut ThemeStore, from: usize, to: usize) {
        let mut order: Vec<String> = store.groups().into_iter().map(|g| g.key).collect();
        if from >= order.len() || to >= order.len() || from == to {
            return;
        }
        let moved = order.remove(from);
        order.insert(to, moved);
        store.set_group_order(&order);
    }

    fn ask(&mut self, kind: Ask, dialog: AskDialog) {
        self.dialog = Some((kind, dialog));
    }

    fn message(&mut self, title: &str, text: &str) {
        self.ask(Ask::Message, AskDialog::error(dialog_id(), title, text));
    }

    fn dialogs(&mut self, ctx: &egui::Context, cx: &mut PageCx<'_>, out: &mut PageOut) {
        let Some((kind, mut dialog)) = self.dialog.take() else { return };
        match dialog.show(ctx) {
            Outcome::Open => self.dialog = Some((kind, dialog)),
            Outcome::Cancelled => {}
            Outcome::Accepted(answer) => self.answered(kind, &answer, dialog.text().to_owned(), cx, out),
        }
    }

    fn answered(&mut self, kind: Ask, answer: &str, text: String, cx: &mut PageCx<'_>, out: &mut PageOut) {
        let text = text.trim().to_owned();
        match kind {
            Ask::NewTheme(folder) if !text.is_empty() => {
                let t = cx.store.new_theme(&text, None, &folder);
                out.edit = Some(t.id().to_owned());
            }
            Ask::Rename(id) => cx.store.rename(&id, &text),
            Ask::Delete(id) => cx.store.delete(&id),
            Ask::NewFolder if !text.is_empty() => {
                self.folder_name = text;
                let start = Rgb::from_u32(0x7B6CF6);
                cx.picker.open("new-folder", "Folder colour", Picked::Colour(start), false, cx.near);
            }
            Ask::RenameFolder(id) if !text.is_empty() => {
                cx.store.update_folder(&id, "name", serde_json::json!(text));
            }
            Ask::FromColour(accent) => {
                let t = cx.store.from_accent(accent, answer == "Light", "");
                cx.store.apply(t.id());
            }
            Ask::UseImported(id) if answer == "Use it" => {
                cx.store.apply(&id);
            }
            _ => {}
        }
    }

    /// What the colour picker said, for the colours the page asked for.
    pub fn picked(&mut self, event: &PickerEvent, cx: &mut PageCx<'_>) {
        let colour = match &event.kind {
            PickerKind::Done(Picked::Colour(c)) => Some(*c),
            _ => None,
        };
        if event.target == "from-colour"
            && let Some(accent) = colour
        {
            self.ask(
                Ask::FromColour(accent),
                AskDialog::choose(
                    dialog_id(),
                    "Theme from a colour",
                    &format!("A dark or a light theme around {}?", accent.hex()),
                    &["Dark", "Light"],
                    "Cancel",
                ),
            );
        } else if event.target == "new-folder" {
            // cancelling the colour still makes the folder, in the accent
            let chosen = colour.unwrap_or(Rgb::from_u32(0x7B6CF6));
            let name = std::mem::take(&mut self.folder_name);
            cx.store.add_folder(&name, &chosen.hex());
        } else if let Some(id) = event.target.strip_prefix("folder:")
            && let Some(c) = colour
        {
            cx.store.update_folder(id, "colour", serde_json::json!(c.hex()));
        }
    }

    /// A file dialog the page asked for came back.
    pub fn file_chosen(
        &mut self,
        job: &FileJob,
        path: Option<PathBuf>,
        cx: &mut PageCx<'_>,
        out: &mut PageOut,
    ) {
        let Some(path) = path else { return };
        match job {
            FileJob::Import => match cx.store.import_file(&path) {
                Ok(t) => {
                    let text = format!("Added \"{}\". Use it now?", t.name());
                    self.ask(
                        Ask::UseImported(t.id().to_owned()),
                        AskDialog::choose(dialog_id(), "Import theme", &text, &["Use it"], "Not now"),
                    );
                }
                Err(e) => self.message("Import theme", &e.to_string()),
            },
            FileJob::Export(id) => {
                let path = if path.extension().is_some_and(|e| e == "lumtheme") {
                    path
                } else {
                    path.with_extension("lumtheme")
                };
                match cx.store.export(id, &path) {
                    Ok(()) => out.notify.push(("Themes".to_owned(), format!("Saved {}", path.display()))),
                    Err(e) => self.message("Export theme", &e.to_string()),
                }
            }
            FileJob::Browse(_) => {}
        }
    }

    /// Whether a dialog of the page is open (Esc then belongs to it).
    pub fn has_dialog(&self) -> bool {
        self.dialog.is_some()
    }
}

struct HeaderOut {
    rect: Rect,
    dropped: Option<String>,
}

fn cell(area: Rect, i: usize, columns: usize, card_w: f32) -> Rect {
    let (col, row) = (i % columns, i / columns);
    Rect::from_min_size(
        pos2(area.min.x + col as f32 * (card_w + GAP), area.min.y + row as f32 * (CARD_HEIGHT + GAP)),
        vec2(card_w, CARD_HEIGHT),
    )
}

fn alpha_of(c: Color32, a: f32) -> Color32 {
    crate::theme::alpha(c, a)
}

/// The dashed "+ New theme" tile at the end of each group.
fn add_card(ui: &mut Ui, rect: Rect, tokens: &Tokens) -> Response {
    let response = ui.interact(
        rect,
        Id::unique(("themes-add-card", rect.min.x as i32, rect.min.y as i32)),
        Sense::click(),
    );
    let p = &tokens.p;
    let stroke = Stroke::new(1.0, if response.hovered() { p.text_muted } else { p.border });
    let r = rect.shrink(0.5);
    let radius = tokens.r().min(r.height() / 2.0);
    // the dashes follow the rounded outline
    let outline = paint::rounded_outline(r, radius);
    let mut closed = outline.clone();
    if let Some(first) = outline.first() {
        closed.push(*first);
    }
    ui.painter().add(Shape::dashed_line(&closed, stroke, 4.0, 3.0));
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        "+ New theme",
        tokens.font(13.0, Weight::Medium, false),
        p.text_muted,
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "New theme"));
    response
}
