//! Drawing the boxes: a box of items, a page, a picture. Their headers, rows, resize edges and drags.

use egui::{
    Align, Color32, CursorIcon, Image, Layout, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2,
    pos2, vec2,
};
use lum_core::dashboard::{
    DashboardStore, GRID, HEAD_ROW_H, HEADER_H, ItemView, PAGE_MIN, ROW_H, SEP_ROW_H, TabView, badge_for,
    snap_to,
};
use lum_platform::hotkeys::{self, Os};
use lum_platform::runner::{self, Kind};
use lum_theme::Role;
use serde_json::json;

use super::{Act, BoxDrag, Cx, DashboardUi, Drawn, Origin, RowDrag, SubPage, base_id};
use crate::anim::{self, Ease, Kind as Anim};
use crate::fonts::Weight;
use crate::overlay::PageFrame;
use crate::paint;
use crate::text::caps;
use crate::theme::{Tokens, Tone, alpha, mix};
use crate::widgets::{IconButton, tip};

const RUNNING: Color32 = Color32::from_rgb(0x3F, 0xBF, 0x74);
/// Rows sit this far inside a box, away from its rounded corners.
const PAD: f32 = 4.0;
const GRIP: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edge {
    Right,
    Bottom,
    Corner,
}

#[derive(Debug, Clone)]
pub(super) struct Resize {
    id: String,
    w0: i64,
    h0: i64,
    total: Vec2,
}

fn target_kind(kind: Kind) -> &'static str {
    match kind {
        Kind::Url => "url",
        Kind::Folder => "folder",
        Kind::Python => "python",
        Kind::Program => "program",
        Kind::File => "file",
        Kind::Command => "command",
        Kind::None => "none",
    }
}

fn row_height(item: &ItemView<'_>) -> f32 {
    match item.kind() {
        "separator" => SEP_ROW_H as f32,
        "heading" => HEAD_ROW_H as f32,
        _ => ROW_H as f32,
    }
}

impl DashboardUi {
    pub(super) fn draw_box(
        &mut self,
        ui: &mut Ui,
        store: &DashboardStore,
        cx: &mut Cx<'_>,
        sub: &mut SubPage<'_>,
        id: &str,
    ) {
        let Some(tab) = store.tab(id) else { return };
        match tab.kind() {
            "page" => self.page_box(ui, store, cx, sub, tab),
            "picture" => self.picture_box(ui, store, cx, tab),
            _ => self.tab_box(ui, store, cx, tab),
        }
    }

    /// Where a box is now on this screen: its saved place, or where a drag has it, plus the glide of a tidy up.
    fn box_origin(&self, ui: &Ui, cx: &Cx<'_>, tab: &TabView<'_>, w: i64) -> Pos2 {
        let (x, y) = self.drag_pos.get(tab.id()).copied().unwrap_or((tab.x(), tab.y()));
        let (x, y) = Self::clamp(cx.slot.canvas, x, y, w);
        let glide = self.glide_offset(ui.ctx(), tab.id());
        cx.slot.canvas.min + vec2(x as f32, y as f32) + glide
    }

    /// A box that moved (a tidy up) arrives from where it was.
    fn glide_offset(&self, ctx: &egui::Context, id: &str) -> Vec2 {
        let Some((from, started)) = self.glides.get(id) else { return Vec2::ZERO };
        let motion = anim::Motion::of(ctx);
        if !motion.on(Anim::Boxes) {
            return Vec2::ZERO;
        }
        let t = ((ctx.input(|i| i.time) - started) as f32 / motion.secs(260.0)).clamp(0.0, 1.0);
        if t < 1.0 {
            ctx.request_repaint();
        }
        let eased = 1.0 - (1.0 - t).powi(3);
        *from * (1.0 - eased)
    }

    /// The width a box has now (a resize in progress shows at once).
    fn width_of(&self, tab: &TabView<'_>, cx: &Cx<'_>, x: i64) -> i64 {
        match &self.resize {
            Some(r) if r.id == tab.id() => self.resized(tab, r, cx, x).0,
            _ => tab.w(),
        }
    }

    fn resized(&self, tab: &TabView<'_>, r: &Resize, cx: &Cx<'_>, x: i64) -> (i64, i64) {
        let canvas = cx.slot.canvas;
        let grid = cx.env.snap;
        let (w, h) = (
            snap_to(r.w0 as f64 + f64::from(r.total.x), grid),
            snap_to(r.h0 as f64 + f64::from(r.total.y), grid),
        );
        let room_w = canvas.width() as i64 - x;
        let y = tab.y();
        let w = match tab.kind() {
            "page" => w.min(room_w).max(PAGE_MIN.0),
            "picture" => w.min(room_w).max(80),
            _ => w.clamp(184, 720).min(room_w),
        };
        let bar =
            if tab.kind() == "page" || (tab.kind() == "picture" && tab.title_bar()) { HEADER_H } else { 0 };
        let room_h = canvas.height() as i64 - y - bar;
        let h = match tab.kind() {
            "page" => h.min(room_h).max(PAGE_MIN.1),
            "picture" => h.min(room_h).max(60),
            _ => tab.h(),
        };
        (w, h)
    }

    fn height_of(&self, tab: &TabView<'_>, cx: &Cx<'_>, x: i64) -> i64 {
        match &self.resize {
            Some(r) if r.id == tab.id() => self.resized(tab, r, cx, x).1,
            _ => tab.h(),
        }
    }

    /// How far down a box may reach: above the toolbar if it would run into it, else the screen's edge.
    fn bottom_limit(&self, cx: &Cx<'_>, x: f32, w: f32, y: f32) -> f32 {
        let mut bottom = cx.slot.canvas.height() - GRID as f32;
        if cx.host
            && let Some(d) = self.dock
            && x < d.right()
            && x + w > d.left()
            && y < d.top()
        {
            bottom = d.top() - 12.0;
        }
        bottom
    }

    // ── a box of items ────────────────────────────────────────────────────

    fn tab_box(&mut self, ui: &mut Ui, store: &DashboardStore, cx: &mut Cx<'_>, tab: TabView<'_>) {
        let tokens = cx.tokens.clone();
        let id = tab.id().to_owned();
        let w = self.width_of(&tab, cx, tab.x()) as f32;
        let origin = self.box_origin(ui, cx, &tab, w as i64);
        let rel = origin - cx.slot.canvas.min;
        let hits = !cx.q.is_empty() && tab.items().any(|i| i.runnable() && i.search_text().contains(&cx.q));
        let folded = tab.collapsed() && !hits;
        let body: f32 = if tab.item_count() == 0 {
            ROW_H as f32 + 26.0
        } else {
            tab.items().map(|i| row_height(&i)).sum::<f32>() + 8.0
        };
        let closed = HEADER_H as f32 + 2.0;
        let room = (self.bottom_limit(cx, rel.x, w, rel.y) - rel.y).max((HEADER_H + ROW_H + 12) as f32);
        let open = (closed + body).min(room);
        let fold = anim::value(
            ui.ctx(),
            base_id().with(("fold", &id)),
            if folded { 0.0 } else { 1.0 },
            Anim::Boxes,
            200.0,
            Ease::Out,
        );
        let h = closed + (open - closed) * fold;
        let rect = Rect::from_min_size(origin, vec2(w, h));
        self.record(ui, cx, &id, rect, true, tab.collapsed());
        self.box_frame(ui, &tokens, rect, &id);
        let header = Rect::from_min_size(rect.min, vec2(w, HEADER_H as f32));
        let count = format!("{:02}", tab.runnable_count());
        let head = self.header(ui, cx, &tab, header, tab.title(), Some(&count), true, folded, &[]);
        head.resp.context_menu(|ui| self.tab_menu(ui, store, cx, &tab));
        if fold > 0.02 {
            let body_rect =
                Rect::from_min_max(pos2(rect.min.x, rect.min.y + HEADER_H as f32), rect.max - vec2(0.0, 1.0));
            self.rows(ui, store, cx, &tab, body_rect);
        }
        self.grips(ui, cx, &tab, rect, &[Edge::Right]);
        self.file_hover(ui, cx, rect);
    }

    fn box_frame(&self, ui: &Ui, tokens: &Tokens, rect: Rect, id: &str) {
        let selected = self.is_selected(id);
        let active = self.drag.as_ref().is_some_and(|d| d.boxes.iter().any(|b| b.id == id));
        let radius = tokens.r() + 2.0;
        paint::fill(ui.painter(), rect, radius, tokens, Role::BgCard, 1.0);
        let border = if selected {
            tokens.p.primary
        } else if active {
            mix(tokens.p.border, tokens.p.primary, 0.6)
        } else {
            tokens.p.border
        };
        ui.painter().rect_stroke(rect, radius, Stroke::new(1.0, border), StrokeKind::Inside);
    }

    /// The header strip: accent bar, title in mono caps, a count, the fold chevron. Dragging it moves the box,
    /// a click folds it; Ctrl or Shift and a click select it.
    #[allow(clippy::too_many_arguments)]
    fn header(
        &mut self,
        ui: &mut Ui,
        cx: &mut Cx<'_>,
        tab: &TabView<'_>,
        rect: Rect,
        title: &str,
        count: Option<&str>,
        chevron: bool,
        folded: bool,
        buttons: &[(&str, &str)],
    ) -> HeaderOut {
        let tokens = cx.tokens.clone();
        let id = tab.id();
        let resp = ui.interact(rect, base_id().with(("head", id)), Sense::click_and_drag());
        let radius = tokens.r() + 2.0;
        let strip = egui::epaint::RectShape::filled(
            rect.shrink(1.0),
            egui::CornerRadius { nw: radius as u8, ne: radius as u8, sw: 0, se: 0 },
            alpha(tokens.p.bg_header, if folded { 0.0 } else { 0.6 }),
        );
        ui.painter().add(strip);
        let painter = ui.painter();
        painter.rect_filled(
            Rect::from_center_size(pos2(rect.min.x + 12.5, rect.center().y), vec2(3.0, 12.0)),
            1.5,
            tokens.p.primary,
        );
        // the right end: chevron, buttons, count
        let mut right = rect.max.x - 9.0;
        if chevron {
            let glyph = if folded { "▸" } else { "▾" };
            let galley = painter.layout_no_wrap(
                glyph.into(),
                tokens.font(10.0, Weight::Regular, false),
                tokens.p.text_muted,
            );
            right -= galley.size().x;
            painter.galley(pos2(right, rect.center().y - galley.size().y / 2.0), galley, tokens.p.text_muted);
            right -= 9.0;
        }
        let mut clicked = vec![false; buttons.len()];
        for (n, (glyph, hint)) in buttons.iter().enumerate().rev() {
            let button = Rect::from_center_size(pos2(right - 11.0, rect.center().y), vec2(22.0, 22.0));
            clicked[n] = ui
                .put(button, IconButton::new(*glyph).size(22.0).font_px(11.0).ghost().tooltip(*hint))
                .clicked();
            right -= 28.0;
        }
        if let Some(count) = count {
            let galley = ui.painter().layout_no_wrap(
                count.into(),
                tokens.font(10.0, Weight::Medium, true),
                tokens.p.text_muted,
            );
            right -= galley.size().x;
            ui.painter().galley(
                pos2(right, rect.center().y - galley.size().y / 2.0),
                galley,
                tokens.p.text_muted,
            );
            right -= 9.0;
        }
        let max = (right - (rect.min.x + 24.0)).max(10.0);
        let mut job = caps(title).px(11.0).spacing(0.14).job(&tokens);
        job.wrap.max_width = max;
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        job.wrap.overflow_character = Some('…');
        let galley = ui.painter().layout_job(job);
        ui.painter().galley(
            pos2(rect.min.x + 24.0, rect.center().y - galley.size().y / 2.0),
            galley,
            tokens.p.text_primary,
        );
        self.header_input(ui, cx, tab, &resp, rect);
        HeaderOut { resp, clicked }
    }

    fn header_input(
        &mut self,
        ui: &mut Ui,
        cx: &Cx<'_>,
        tab: &TabView<'_>,
        resp: &egui::Response,
        rect: Rect,
    ) {
        let id = tab.id().to_owned();
        let mods = ui.input(|i| i.modifiers);
        let picture_bar_free = tab.kind() == "picture" && !tab.title_bar();
        let dragging = self.drag.as_ref().is_some_and(|d| d.lead == id);
        if resp.hovered() || dragging {
            let icon = if dragging {
                CursorIcon::Grabbing
            } else if cx.locked {
                CursorIcon::PointingHand
            } else {
                CursorIcon::Grab
            };
            ui.ctx().set_cursor_icon(icon);
        }
        if resp.clicked() {
            self.top = Some(id.clone());
            if mods.ctrl || mods.shift {
                self.toggle_selected(&id);
            } else {
                if !self.is_selected(&id) {
                    self.selected.clear();
                }
                if !picture_bar_free {
                    self.acts.push(Act::Fold(vec![id.clone()], !tab.collapsed()));
                }
            }
        }
        if resp.drag_started() && !cx.locked && !mods.ctrl && !mods.shift {
            self.top = Some(id.clone());
            if !self.is_selected(&id) {
                self.selected.clear();
            }
            self.start_drag(ui, cx, tab, rect);
        }
    }

    fn start_drag(&mut self, ui: &Ui, cx: &Cx<'_>, tab: &TabView<'_>, rect: Rect) {
        let Some(press) = ui.input(|i| i.pointer.press_origin()) else { return };
        let mut boxes =
            vec![Origin { id: tab.id().to_owned(), x: tab.x(), y: tab.y(), w: rect.width() as i64 }];
        for other in self
            .drawn
            .iter()
            .filter(|d| d.screen == cx.slot.screen && d.id != tab.id() && self.selected.contains(&d.id))
        {
            let rel = other.rect.min - cx.slot.canvas.min;
            boxes.push(Origin {
                id: other.id.clone(),
                x: rel.x as i64,
                y: rel.y as i64,
                w: other.rect.width() as i64,
            });
        }
        self.drag = Some(BoxDrag {
            lead: tab.id().to_owned(),
            boxes,
            start: press + cx.slot.to_desktop,
            grab: press - rect.min,
            screen: cx.slot.screen.to_owned(),
            moved: false,
        });
    }

    /// Moves what is being dragged with the mouse, on the snap grid; hands the boxes over when the mouse is on
    /// another screen, and lets go when the button is up.
    pub(super) fn drag_boxes(&mut self, ui: &mut Ui, store: &DashboardStore, cx: &Cx<'_>) {
        let Some(drag) = self.drag.clone() else { return };
        if drag.screen != cx.slot.screen {
            return;
        }
        let Some(pointer) = self.pointer else { return };
        let total = pointer - drag.start;
        let target = self.screen_at(cx.slot, pointer).map(|s| s.name.clone());
        let carried = target.as_ref().is_some_and(|t| *t != drag.screen);
        let released = !ui.input(|i| i.pointer.any_down());
        if carried {
            self.drag_pos.clear();
        } else {
            let (dx, dy) = (total.x as f64, total.y as f64);
            let canvas = cx.slot.canvas;
            let lead = drag.boxes.first();
            for b in &drag.boxes {
                let (x, y) = if lead.is_some_and(|l| l.id == b.id) {
                    let g = cx.env.snap;
                    Self::clamp(canvas, snap_to(b.x as f64 + dx, g), snap_to(b.y as f64 + dy, g), b.w)
                } else {
                    let lead = lead.expect("the lead is first");
                    let g = cx.env.snap;
                    let (lx, ly) = Self::clamp(
                        canvas,
                        snap_to(lead.x as f64 + dx, g),
                        snap_to(lead.y as f64 + dy, g),
                        lead.w,
                    );
                    Self::clamp(canvas, b.x + (lx - lead.x), b.y + (ly - lead.y), b.w)
                };
                self.drag_pos.insert(b.id.clone(), (x, y));
            }
        }
        let moved = total.length() >= 4.0 || carried;
        if let Some(d) = &mut self.drag {
            d.moved |= moved;
        }
        if !released {
            return;
        }
        if carried && let Some(screen) = target {
            let to = pointer - drag.grab;
            let to_screen = cx
                .slot
                .screens
                .iter()
                .find(|s| s.name == screen)
                .map_or(Vec2::ZERO, |s| s.rect.min.to_vec2());
            let lead = drag.boxes.first().cloned();
            let lead_at = to - to_screen;
            let boxes = drag
                .boxes
                .iter()
                .map(|b| {
                    let (ox, oy) = lead.as_ref().map_or((0, 0), |l| (b.x - l.x, b.y - l.y));
                    (b.id.clone(), lead_at.x as i64 + ox, lead_at.y as i64 + oy)
                })
                .collect();
            self.acts.push(Act::PlaceBoxes { boxes, screen });
        } else if drag.moved {
            let changes = self
                .drag_pos
                .iter()
                .map(|(id, (x, y))| (id.clone(), vec![("x", json!(x)), ("y", json!(y))]))
                .collect::<Vec<_>>();
            for (id, ch) in changes {
                self.acts.push(Act::Geometry(id, ch));
            }
        }
        let _ = store;
        self.drag_pos.clear();
        self.drag = None;
    }

    fn record(&mut self, ui: &Ui, cx: &Cx<'_>, id: &str, rect: Rect, items: bool, collapsed: bool) {
        self.sizes.insert(id.to_owned(), (rect.width(), rect.height()));
        self.drawn_now.push(Drawn {
            id: id.to_owned(),
            screen: cx.slot.screen.to_owned(),
            rect,
            desktop: rect.translate(cx.slot.to_desktop),
            items,
            collapsed,
            rows: Vec::new(),
        });
        let _ = ui;
    }

    // ── rows ──────────────────────────────────────────────────────────────

    fn rows(&mut self, ui: &mut Ui, store: &DashboardStore, cx: &mut Cx<'_>, tab: &TabView<'_>, body: Rect) {
        let tokens = cx.tokens.clone();
        let mut child = ui.new_child(UiBuilder::new().max_rect(body).layout(Layout::top_down(Align::Min)));
        child.set_clip_rect(body.intersect(ui.clip_rect()));
        child.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let mut mids: Vec<f32> = Vec::new();
        let tab_id = tab.id().to_owned();
        egui::ScrollArea::vertical().id_salt(("dash-body", &tab_id)).auto_shrink([false, false]).show(
            &mut child,
            |ui| {
                ui.add_space(PAD);
                if tab.item_count() == 0 {
                    let (rect, _) =
                        ui.allocate_exact_size(vec2(body.width(), ROW_H as f32 + 18.0), Sense::hover());
                    let text = "Right-click the header → Add item,\nor drop files here";
                    let galley = ui.painter().layout(
                        text.into(),
                        tokens.font(12.0, Weight::Regular, false),
                        tokens.p.text_muted,
                        rect.width(),
                    );
                    ui.painter().galley(
                        pos2(
                            rect.center().x - galley.size().x / 2.0,
                            rect.center().y - galley.size().y / 2.0,
                        ),
                        galley,
                        tokens.p.text_muted,
                    );
                }
                for item in tab.items() {
                    let h = row_height(&item);
                    let (rect, _) = ui.allocate_exact_size(vec2(body.width() - 2.0 * PAD, h), Sense::hover());
                    let rect = rect.translate(vec2(PAD, 0.0));
                    mids.push(rect.center().y);
                    self.row(ui, store, cx, tab, &item, rect);
                }
            },
        );
        self.drop_line(ui, cx, tab, body, &mids);
        if let Some(d) = self.drawn_now.iter_mut().rev().find(|d| d.id == tab_id) {
            d.rows = mids.iter().map(|y| y + cx.slot.to_desktop.y).collect();
        }
    }

    fn row(
        &mut self,
        ui: &mut Ui,
        store: &DashboardStore,
        cx: &mut Cx<'_>,
        tab: &TabView<'_>,
        item: &ItemView<'_>,
        rect: Rect,
    ) {
        let tokens = cx.tokens.clone();
        let kind = item.kind();
        let resp = ui.interact(rect, base_id().with(("row", item.id())), Sense::click_and_drag());
        if kind != "separator" {
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, item.name()));
        }
        let painter = ui.painter_at(rect.expand2(vec2(PAD, 0.0)));
        if kind == "separator" {
            painter.hline(rect.x_range().shrink(8.0), rect.center().y, Stroke::new(1.0, tokens.p.border));
            tip(resp.clone(), "Separator · drag to move · right-click to delete");
            self.row_input(ui, store, cx, tab, item, &resp);
            return;
        }
        let hit = !cx.q.is_empty() && item.runnable() && item.search_text().contains(&cx.q);
        let dim = !cx.q.is_empty() && !hit;
        let fade = if dim { 0.35 } else { 1.0 };
        if kind == "heading" {
            let galley = painter.layout_no_wrap(
                item.name().to_uppercase(),
                tokens.font(9.5, Weight::SemiBold, true),
                alpha(tokens.p.text_secondary, fade),
            );
            painter.galley(pos2(rect.min.x + 8.0, rect.min.y + 6.0), galley, tokens.p.text_secondary);
            tip(resp.clone(), "Heading · drag to move · right-click to rename");
            self.row_input(ui, store, cx, tab, item, &resp);
            return;
        }
        if hit {
            painter.rect_filled(rect, tokens.rs(), alpha(tokens.p.primary, 0.22));
        } else if resp.hovered() && !dim {
            painter.rect_filled(rect, tokens.rs(), tokens.p.bg_card_hover);
        }
        // the icon, or the type badge, in a 30 px column
        let column = Rect::from_min_size(pos2(rect.min.x + 8.0, rect.center().y - 9.0), vec2(30.0, 18.0));
        let icon =
            (cx.env.show_icons && !item.path().is_empty() && item.action().is_empty() && kind != "group")
                .then(|| self.icons.get(ui.ctx(), item.path()).map(|t| (t.id(), t.size_vec2())))
                .flatten();
        if let Some((tex, _)) = icon {
            let at = Rect::from_center_size(column.center(), vec2(18.0, 18.0));
            Image::from_texture((tex, vec2(18.0, 18.0)))
                .tint(Color32::WHITE.gamma_multiply(fade))
                .paint_at(ui, at);
        } else {
            let badge =
                badge_for(kind, item.action(), target_kind(runner::kind_of(item.path())), item.path());
            let b = Rect::from_center_size(column.center(), vec2(30.0, 17.0));
            painter.rect_stroke(b, 4.0, Stroke::new(1.0, alpha(tokens.p.border, fade)), StrokeKind::Inside);
            let galley = painter.layout_no_wrap(
                badge.into(),
                tokens.font(8.5, Weight::Medium, true),
                alpha(tokens.p.text_muted, fade),
            );
            painter.galley(b.center() - galley.size() / 2.0, galley, tokens.p.text_muted);
        }
        // the right end: hotkey, running dot, ROOT
        let mut right = rect.max.x - 8.0;
        if !item.hotkey().is_empty() {
            let text = hotkeys::pretty(item.hotkey(), Os::current());
            let galley = painter.layout_no_wrap(
                text,
                tokens.font(11.0, Weight::Regular, true),
                alpha(tokens.p.text_muted, fade),
            );
            right -= galley.size().x;
            painter.galley(pos2(right, rect.center().y - galley.size().y / 2.0), galley, tokens.p.text_muted);
            right -= 10.0;
        }
        let running = (cx.env.is_running)(item.id());
        if running {
            painter.circle_filled(pos2(right - 3.5, rect.center().y), 3.5, RUNNING);
            right -= 17.0;
        }
        if item.admin() {
            let galley = painter.layout_no_wrap(
                "ROOT".into(),
                tokens.font(8.5, Weight::SemiBold, true),
                alpha(tokens.p.text_primary, fade),
            );
            right -= galley.size().x;
            painter.galley(
                pos2(right, rect.center().y - galley.size().y / 2.0),
                galley,
                tokens.tone(Tone::Danger),
            );
            right -= 10.0;
        }
        let name = if item.name().is_empty() { "(unnamed)" } else { item.name() };
        let mut job = egui::text::LayoutJob::single_section(
            name.to_owned(),
            egui::TextFormat {
                font_id: tokens.font(13.0, Weight::Regular, false),
                color: alpha(tokens.p.text_primary, fade),
                ..Default::default()
            },
        );
        job.wrap.max_width = (right - (column.max.x + 10.0)).max(10.0);
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        job.wrap.overflow_character = Some('…');
        let galley = painter.layout_job(job);
        painter.galley(
            pos2(column.max.x + 10.0, rect.center().y - galley.size().y / 2.0),
            galley,
            tokens.p.text_primary,
        );
        if resp.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        let tooltip = self.row_tip(store, item);
        tip(resp.clone(), tooltip);
        self.row_input(ui, store, cx, tab, item, &resp);
    }

    fn row_tip(&self, store: &DashboardStore, item: &ItemView<'_>) -> String {
        let mut text = if item.kind() == "group" {
            let members: Vec<String> = item
                .group()
                .iter()
                .filter_map(|id| store.find_item(id))
                .enumerate()
                .map(|(n, (_, m))| format!("  {}. {}", n + 1, m.name()))
                .collect();
            format!("Runs, in order:\n{}", members.join("\n"))
        } else {
            let mut tip =
                if item.action().is_empty() { item.path().to_owned() } else { item.action().to_owned() };
            if !item.args().is_empty() {
                tip.push_str("  ");
                tip.push_str(item.args());
            }
            if !item.cwd().is_empty() {
                tip.push_str(&format!("\nin {}", item.cwd()));
            }
            if item.ask_args() {
                tip.push_str("\nAsks for arguments");
            }
            tip
        };
        text.push_str("\nDouble-click to run · drag to move · right-click for options");
        text
    }

    fn row_input(
        &mut self,
        ui: &mut Ui,
        store: &DashboardStore,
        cx: &Cx<'_>,
        tab: &TabView<'_>,
        item: &ItemView<'_>,
        resp: &egui::Response,
    ) {
        if resp.double_clicked() && item.runnable() {
            self.acts.push(Act::Run(item.id().to_owned(), None));
            self.out.hide_overlay = true;
        }
        if resp.drag_started() && !cx.locked {
            self.row_drag = Some(RowDrag {
                id: item.id().to_owned(),
                name: item.name().to_owned(),
                screen: cx.slot.screen.to_owned(),
            });
        }
        if self.row_drag.as_ref().is_some_and(|d| d.id == item.id()) {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        }
        resp.context_menu(|ui| {
            let mut acts = Vec::new();
            self.item_menu(ui, store, cx, tab, item, &mut acts);
            self.acts.extend(acts);
        });
    }

    /// A line where a dragged row would land, in the box under the mouse.
    fn drop_line(&mut self, ui: &mut Ui, cx: &Cx<'_>, tab: &TabView<'_>, body: Rect, mids: &[f32]) {
        let Some(pointer) = self.pointer else { return };
        if self.row_drag.is_none() || !body.translate(cx.slot.to_desktop).contains(pointer) || tab.collapsed()
        {
            return;
        }
        let y = pointer.y - cx.slot.to_desktop.y;
        let index = mids.iter().filter(|m| **m <= y).count();
        let tokens = &cx.tokens;
        let heights: Vec<f32> = tab.items().map(|i| row_height(&i)).collect();
        let line_y = match index {
            0 => mids.first().map_or(body.min.y + 6.0, |m| m - heights[0] / 2.0),
            n => mids.get(n - 1).map_or(body.min.y + 6.0, |m| m + heights[n - 1] / 2.0),
        };
        ui.painter().rect_filled(
            Rect::from_min_size(pos2(body.min.x + 8.0, line_y - 1.0), vec2(body.width() - 16.0, 2.0)),
            1.0,
            tokens.p.primary,
        );
    }

    /// While files are dragged over a box of items it lights up.
    fn file_hover(&self, ui: &Ui, cx: &Cx<'_>, rect: Rect) {
        let hovering = ui.input(|i| !i.raw.hovered_files.is_empty());
        let over = ui.input(|i| i.pointer.latest_pos()).is_some_and(|p| rect.contains(p));
        if hovering && over && !cx.locked {
            ui.painter().rect_stroke(
                rect,
                cx.tokens.r() + 2.0,
                Stroke::new(2.0, cx.tokens.p.primary),
                StrokeKind::Inside,
            );
        }
    }

    /// Files dropped from the file manager become items of the box they are dropped on.
    pub(super) fn drop_files(&mut self, ui: &mut Ui, cx: &Cx<'_>) {
        let files: Vec<String> = ui
            .input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_string_lossy().into_owned()).collect());
        if files.is_empty() || cx.locked {
            return;
        }
        let Some(at) = ui.input(|i| i.pointer.latest_pos()) else { return };
        let Some(target) = self
            .drawn
            .iter()
            .rev()
            .find(|d| d.screen == cx.slot.screen && d.items && d.rect.contains(at))
            .cloned()
        else {
            return;
        };
        let index = if target.collapsed {
            -1
        } else {
            target.rows.iter().filter(|m| **m <= at.y + cx.slot.to_desktop.y).count() as i64
        };
        self.acts.push(Act::AddPaths { tab: target.id, paths: files, index });
    }

    /// The row being dragged follows the mouse on whichever screen it is over, and drops where the button goes up.
    pub(super) fn row_ghost(&mut self, ui: &mut Ui, cx: &Cx<'_>) {
        let (Some(drag), Some(pointer)) = (self.row_drag.clone(), self.pointer) else { return };
        let released = drag.screen == cx.slot.screen && !ui.input(|i| i.pointer.any_down());
        if cx.slot.screen_rect().contains(pointer) {
            let tokens = &cx.tokens;
            let layer = egui::LayerId::new(egui::Order::Tooltip, base_id().with("ghost"));
            let painter = ui.ctx().layer_painter(layer);
            let at = pointer - cx.slot.to_desktop + vec2(10.0, -14.0);
            let galley = painter.layout_no_wrap(
                drag.name.clone(),
                tokens.font(13.0, Weight::Regular, false),
                tokens.p.text_primary,
            );
            let rect = Rect::from_min_size(at, galley.size() + vec2(24.0, 12.0));
            painter.rect_filled(rect, tokens.rs(), alpha(tokens.p.bg_card_hover, 0.95));
            painter.rect_stroke(rect, tokens.rs(), Stroke::new(1.0, tokens.p.primary), StrokeKind::Inside);
            painter.galley(rect.min + vec2(12.0, 6.0), galley, tokens.p.text_primary);
        }
        if !released {
            return;
        }
        self.row_drag = None;
        let Some(target) = self.drawn.iter().rev().find(|d| d.items && d.desktop.contains(pointer)).cloned()
        else {
            return;
        };
        let index = if target.collapsed {
            -1
        } else {
            target.rows.iter().filter(|m| **m <= pointer.y).count() as i64
        };
        self.acts.push(Act::MoveItem { id: drag.id, to_tab: target.id, delta: 0, index });
    }

    // ── resize edges ──────────────────────────────────────────────────────

    fn grips(&mut self, ui: &mut Ui, cx: &Cx<'_>, tab: &TabView<'_>, rect: Rect, edges: &[Edge]) {
        let folded = tab.collapsed();
        for edge in edges {
            if *edge != Edge::Right && folded {
                continue;
            }
            let t = GRIP;
            let strip = match edge {
                Edge::Right => Rect::from_min_max(
                    pos2(rect.max.x - t, rect.min.y + HEADER_H as f32),
                    pos2(rect.max.x, (rect.max.y - 2.0 * t).max(rect.min.y + HEADER_H as f32)),
                ),
                Edge::Bottom => Rect::from_min_max(
                    pos2(rect.min.x, rect.max.y - t),
                    pos2((rect.max.x - 2.0 * t).max(rect.min.x), rect.max.y),
                ),
                Edge::Corner => Rect::from_min_max(rect.max - vec2(2.0 * t, 2.0 * t), rect.max),
            };
            let resp = ui.interact(strip, base_id().with(("grip", tab.id(), *edge as u8)), Sense::drag());
            if resp.hovered() || resp.dragged() {
                ui.ctx().set_cursor_icon(match edge {
                    Edge::Right => CursorIcon::ResizeHorizontal,
                    Edge::Bottom => CursorIcon::ResizeVertical,
                    Edge::Corner => CursorIcon::ResizeNwSe,
                });
            }
            if resp.drag_started() && !cx.locked {
                self.resize = Some(Resize {
                    id: tab.id().to_owned(),
                    w0: rect.width() as i64,
                    h0: tab.h(),
                    total: Vec2::ZERO,
                });
            }
            if resp.dragged()
                && let Some(r) = &mut self.resize
                && r.id == tab.id()
            {
                let d = resp.drag_delta();
                if *edge != Edge::Bottom {
                    r.total.x += d.x;
                }
                if *edge != Edge::Right {
                    r.total.y += d.y;
                }
            }
            if resp.drag_stopped()
                && let Some(r) = self.resize.take()
                && r.id == tab.id()
            {
                let (w, h) = self.resized(tab, &r, cx, tab.x());
                let mut changes = vec![("w", json!(w))];
                if tab.kind() != "items" {
                    changes.push(("h", json!(h)));
                }
                self.acts.push(Act::Geometry(r.id, changes));
            }
        }
    }

    // ── a page ──────────────────────────────────────────────────────────

    fn page_box(
        &mut self,
        ui: &mut Ui,
        store: &DashboardStore,
        cx: &mut Cx<'_>,
        sub: &mut SubPage<'_>,
        tab: TabView<'_>,
    ) {
        let tokens = cx.tokens.clone();
        let id = tab.id().to_owned();
        let w = self.width_of(&tab, cx, tab.x()) as f32;
        let origin = self.box_origin(ui, cx, &tab, w as i64);
        let rel = origin - cx.slot.canvas.min;
        let closed = HEADER_H as f32 + 2.0;
        let room = (self.bottom_limit(cx, rel.x, w, rel.y) - rel.y).max((HEADER_H + PAGE_MIN.1) as f32);
        let open = (closed + self.height_of(&tab, cx, tab.x()) as f32).min(room);
        let fold = anim::value(
            ui.ctx(),
            base_id().with(("fold", &id)),
            if tab.collapsed() { 0.0 } else { 1.0 },
            Anim::Boxes,
            200.0,
            Ease::Out,
        );
        let rect = Rect::from_min_size(origin, vec2(w, closed + (open - closed) * fold));
        self.record(ui, cx, &id, rect, false, tab.collapsed());
        self.box_frame(ui, &tokens, rect, &id);
        let header = Rect::from_min_size(rect.min, vec2(w, HEADER_H as f32));
        let name = crate::overlay::pages::title(tab.page()).unwrap_or(tab.page());
        let title = if tab.title().is_empty() { name } else { tab.title() };
        let head = self.header(
            ui,
            cx,
            &tab,
            header,
            title,
            None,
            true,
            tab.collapsed(),
            &[("↗", "Open it as the full page"), ("✕", "Take it off the dashboard")],
        );
        if head.clicked[0] {
            self.out.open_page = Some(tab.page().to_owned());
        }
        if head.clicked[1] {
            self.acts.push(Act::Remove(vec![id.clone()]));
        }
        head.resp.context_menu(|ui| self.tab_menu(ui, store, cx, &tab));
        if fold > 0.02 {
            let body =
                Rect::from_min_max(pos2(rect.min.x, rect.min.y + HEADER_H as f32), rect.max - vec2(0.0, 1.0));
            let mut child =
                ui.new_child(UiBuilder::new().max_rect(body).layout(Layout::top_down(Align::Min)));
            child.set_clip_rect(body.intersect(ui.clip_rect()));
            if cx.env.safe_mode {
                child.add(
                    crate::text::Text::new(
                        "Safe mode: page boxes are not loaded.\nRestart normally from the tray.",
                    )
                    .px(12.0)
                    .tone(Tone::Muted),
                );
            } else {
                let frame: PageFrame = sub(&mut child, tab.page(), body.translate(cx.slot.to_desktop));
                if frame.busy {
                    self.page_busy = true;
                }
            }
        }
        self.grips(ui, cx, &tab, rect, &[Edge::Right, Edge::Bottom, Edge::Corner]);
    }

    // ── a picture ───────────────────────────────────────────────────────

    fn picture_box(&mut self, ui: &mut Ui, store: &DashboardStore, cx: &mut Cx<'_>, tab: TabView<'_>) {
        let tokens = cx.tokens.clone();
        let id = tab.id().to_owned();
        let w = self.width_of(&tab, cx, tab.x()).max(80) as f32;
        let origin = self.box_origin(ui, cx, &tab, w as i64);
        let bar = if tab.title_bar() { HEADER_H as f32 } else { 0.0 };
        let h = (self.height_of(&tab, cx, tab.x()) as f32).max(60.0) + bar;
        let rect = Rect::from_min_size(origin, vec2(w, h));
        self.record(ui, cx, &id, rect, false, false);
        let frame = tab.frame();
        let radius = tab.radius() as f32 + if frame == "panel" { 8.0 } else { 0.0 };
        let radius = radius.min(rect.width().min(rect.height()) / 2.0);
        let selected = self.is_selected(&id);
        if frame == "panel" {
            paint::fill(ui.painter(), rect, radius, &tokens, Role::BgCard, 1.0);
        }
        if selected {
            ui.painter().rect_stroke(rect, radius, Stroke::new(1.0, tokens.p.primary), StrokeKind::Inside);
        } else if frame == "panel" {
            ui.painter().rect_stroke(rect, radius, Stroke::new(1.0, tokens.p.border), StrokeKind::Inside);
        } else if frame == "border" {
            let stroke = if tab.border_accent() {
                Stroke::new(2.0, tokens.p.primary)
            } else {
                Stroke::new(1.0, tokens.p.border)
            };
            ui.painter().rect_stroke(rect, radius, stroke, StrokeKind::Inside);
        }
        let area = Rect::from_min_max(rect.min + vec2(0.0, bar), rect.max);
        self.picture(ui, &tab, area, &tokens);
        // the whole picture is the handle when there is no title bar
        let handle =
            if tab.title_bar() { Rect::from_min_size(rect.min, vec2(w, HEADER_H as f32)) } else { rect };
        let head = if tab.title_bar() {
            let title = if tab.title().is_empty() { "Picture" } else { tab.title() };
            let head = self.header(
                ui,
                cx,
                &tab,
                handle,
                title,
                None,
                false,
                false,
                &[("✕", "Take it off the dashboard")],
            );
            if head.clicked[0] {
                self.acts.push(Act::Remove(vec![id.clone()]));
            }
            head.resp
        } else {
            let resp = ui.interact(handle, base_id().with(("head", &id)), Sense::click_and_drag());
            self.header_input(ui, cx, &tab, &resp, rect);
            resp
        };
        head.context_menu(|ui| self.tab_menu(ui, store, cx, &tab));
        self.grips(ui, cx, &tab, rect, &[Edge::Right, Edge::Bottom, Edge::Corner]);
    }

    fn picture(&mut self, ui: &mut Ui, tab: &TabView<'_>, area: Rect, tokens: &Tokens) {
        let pad = if tab.frame() == "panel" { 8.0 } else { 0.0 };
        let r = area.shrink(pad);
        let path = lum_platform::runner::expand(tab.picture_path());
        let ctx = ui.ctx().clone();
        let texture = self.pictures().get(&ctx, &path).map(|t| (t.id(), t.size_vec2()));
        let Some((tex, size)) = texture else {
            let text = if path.is_empty() || !std::path::Path::new(&path).is_file() {
                "Right-click → Choose a picture…"
            } else {
                ""
            };
            let galley = ui.painter().layout_no_wrap(
                text.into(),
                tokens.font(12.0, Weight::Regular, false),
                tokens.p.text_muted,
            );
            ui.painter().galley(r.center() - galley.size() / 2.0, galley, tokens.p.text_muted);
            return;
        };
        let fit = tab.fit();
        let (target, uv) = match fit {
            "stretch" => (r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))),
            "fill" => {
                let scale = (r.width() / size.x).max(r.height() / size.y);
                let (fw, fh) = (r.width() / (size.x * scale), r.height() / (size.y * scale));
                (
                    r,
                    Rect::from_min_max(
                        pos2(0.5 - fw / 2.0, 0.5 - fh / 2.0),
                        pos2(0.5 + fw / 2.0, 0.5 + fh / 2.0),
                    ),
                )
            }
            _ => {
                let scale = (r.width() / size.x).min(r.height() / size.y);
                (
                    Rect::from_center_size(r.center(), size * scale),
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                )
            }
        };
        let radius = (tab.radius() as f32).min(target.width().min(target.height()) / 2.0);
        let tint = Color32::WHITE.gamma_multiply((tab.opacity() as f32).clamp(0.05, 1.0));
        Image::from_texture((tex, target.size()))
            .uv(uv)
            .corner_radius(radius)
            .tint(tint)
            .paint_at(ui, target);
    }
}

pub(super) struct HeaderOut {
    pub resp: egui::Response,
    pub clicked: Vec<bool>,
}

trait ScreenRect {
    fn screen_rect(&self) -> Rect;
}

impl ScreenRect for crate::overlay::PageSlot<'_> {
    /// This screen on the desktop.
    fn screen_rect(&self) -> Rect {
        self.canvas.translate(self.to_desktop)
    }
}
