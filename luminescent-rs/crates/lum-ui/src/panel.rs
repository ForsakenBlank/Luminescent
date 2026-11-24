//! `FloatingPanel`: the frameless, always-on-top, see-through panel Notes, Timers, the Calculator and the other
//! tools live in. A header with the accent tick, a mono caps title, optional buttons and ✕; dragged by the
//! header, resized from the edges, an "EDIT" look, an opacity, and a geometry the module remembers.
//!
//! Each open panel is a window of its own. Where there are no separate windows (tests, the widget gallery) it
//! is an area inside the current window, drawn the same way.

use egui::viewport::ResizeDirection;
use egui::{
    Align, Area, Context, CornerRadius, CursorIcon, Id, Key, Layout, Order, Pos2, Rect, Sense, Stroke,
    StrokeKind, Ui, UiBuilder, Vec2, ViewportBuilder, ViewportCommand, ViewportId, pos2, vec2,
};
use lum_platform::desktop::{APP_ID, marker};

use crate::anim::{self, Ease, Kind, Motion, Style};
use crate::paint;
use crate::text::{Text, caps};
use crate::theme::{Role, Tokens, Tone};
use crate::widgets::button::IconButton;

/// Where a panel is and how big, in desktop coordinates. Modules save it in their settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Geometry {
    pub fn rect(&self) -> Rect {
        Rect::from_min_size(pos2(self.x, self.y), vec2(self.w, self.h))
    }

    pub fn from_rect(rect: Rect) -> Geometry {
        Geometry { x: rect.min.x, y: rect.min.y, w: rect.width(), h: rect.height() }
    }
}

/// What happened while the panel was drawn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PanelResponse {
    /// ✕ was clicked (the panel fades out and closes by itself).
    pub closed: bool,
    /// Esc was pressed while it had the keyboard.
    pub escape: bool,
    /// It was moved or resized; save its geometry.
    pub geometry_changed: bool,
}

/// The edges a panel can be resized from, and the strip each one listens on.
const EDGES: [ResizeDirection; 8] = [
    ResizeDirection::West,
    ResizeDirection::East,
    ResizeDirection::North,
    ResizeDirection::South,
    ResizeDirection::NorthWest,
    ResizeDirection::NorthEast,
    ResizeDirection::SouthWest,
    ResizeDirection::SouthEast,
];

const EDGE: f32 = 7.0;

fn edge_rect(rect: Rect, dir: ResizeDirection) -> Rect {
    let (l, r, t, b) = (rect.min.x, rect.max.x, rect.min.y, rect.max.y);
    let span = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_min_max(pos2(x0, y0), pos2(x1, y1));
    match dir {
        ResizeDirection::West => span(l, t + EDGE, l + EDGE, b - EDGE),
        ResizeDirection::East => span(r - EDGE, t + EDGE, r, b - EDGE),
        ResizeDirection::North => span(l + EDGE, t, r - EDGE, t + EDGE),
        ResizeDirection::South => span(l + EDGE, b - EDGE, r - EDGE, b),
        ResizeDirection::NorthWest => span(l, t, l + EDGE, t + EDGE),
        ResizeDirection::NorthEast => span(r - EDGE, t, r, t + EDGE),
        ResizeDirection::SouthWest => span(l, b - EDGE, l + EDGE, b),
        ResizeDirection::SouthEast => span(r - EDGE, b - EDGE, r, b),
    }
}

fn edge_cursor(dir: ResizeDirection) -> CursorIcon {
    match dir {
        ResizeDirection::West | ResizeDirection::East => CursorIcon::ResizeHorizontal,
        ResizeDirection::North | ResizeDirection::South => CursorIcon::ResizeVertical,
        ResizeDirection::NorthWest | ResizeDirection::SouthEast => CursorIcon::ResizeNwSe,
        ResizeDirection::NorthEast | ResizeDirection::SouthWest => CursorIcon::ResizeNeSw,
    }
}

/// A drag of `delta` on an edge, applied to a rectangle that may not get smaller than `min`.
fn resized(rect: Rect, dir: ResizeDirection, delta: Vec2, min: Vec2) -> Rect {
    let mut r = rect;
    let (west, east, north, south) = match dir {
        ResizeDirection::West => (true, false, false, false),
        ResizeDirection::East => (false, true, false, false),
        ResizeDirection::North => (false, false, true, false),
        ResizeDirection::South => (false, false, false, true),
        ResizeDirection::NorthWest => (true, false, true, false),
        ResizeDirection::NorthEast => (false, true, true, false),
        ResizeDirection::SouthWest => (true, false, false, true),
        ResizeDirection::SouthEast => (false, true, false, true),
    };
    if west {
        r.min.x = (r.min.x + delta.x).min(r.max.x - min.x);
    }
    if east {
        r.max.x = (r.max.x + delta.x).max(r.min.x + min.x);
    }
    if north {
        r.min.y = (r.min.y + delta.y).min(r.max.y - min.y);
    }
    if south {
        r.max.y = (r.max.y + delta.y).max(r.min.y + min.y);
    }
    r
}

#[derive(Debug, Clone)]
pub struct FloatingPanel {
    id: Id,
    title: String,
    subtitle: String,
    pub geometry: Geometry,
    min_size: Vec2,
    /// How opaque the background is (Settings has a slider per tool).
    pub opacity: f32,
    resizable: bool,
    /// Only movable and resizable in edit mode (sticky notes).
    drag_needs_edit: bool,
    pub edit_mode: bool,
    /// Take the keyboard when it opens. Panels opened from the overlay do; ones that pop up by themselves
    /// (a timer finishing) do not.
    activate_on_show: bool,
    header_height: f32,
    open: bool,
    closing: bool,
    /// Opened since the last frame: its fades start over.
    fresh: bool,
}

/// What a frame of the panel needs from outside its own state.
struct Drawn {
    close: bool,
    escape: bool,
    drag: Option<Vec2>,
    drag_started: bool,
    resize: Option<(ResizeDirection, Vec2, bool)>,
}

impl FloatingPanel {
    pub fn new(key: &str, title: &str, geometry: Geometry) -> FloatingPanel {
        FloatingPanel {
            id: Id::unique(("lum-panel", key)),
            title: title.to_owned(),
            subtitle: String::new(),
            geometry,
            min_size: vec2(200.0, 120.0),
            opacity: 0.85,
            resizable: true,
            drag_needs_edit: false,
            edit_mode: false,
            activate_on_show: true,
            header_height: 38.0,
            open: false,
            closing: false,
            fresh: false,
        }
    }

    pub fn min_size(mut self, size: Vec2) -> FloatingPanel {
        self.min_size = size;
        self
    }

    pub fn resizable(mut self, resizable: bool) -> FloatingPanel {
        self.resizable = resizable;
        self
    }

    pub fn drag_needs_edit_mode(mut self, needs: bool) -> FloatingPanel {
        self.drag_needs_edit = needs;
        self
    }

    pub fn activate_on_show(mut self, activate: bool) -> FloatingPanel {
        self.activate_on_show = activate;
        self
    }

    pub fn id(&self) -> Id {
        self.id
    }

    /// The title, with an optional part after a slash ("NOTES / SHOPPING").
    pub fn set_title(&mut self, title: &str, subtitle: &str) {
        self.title = title.to_owned();
        self.subtitle = subtitle.to_owned();
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn open(&mut self) {
        if !self.open || self.closing {
            self.open = true;
            self.closing = false;
            self.fresh = true;
        }
    }

    /// Fades out (when panel animations are on) and closes, like `fade_hide`.
    pub fn close(&mut self) {
        if self.open {
            self.closing = true;
        }
    }

    pub fn toggle(&mut self) {
        if self.open && !self.closing { self.close() } else { self.open() }
    }

    fn forget_fades(&self, ctx: &Context) {
        anim::forget(ctx, self.id.with("fade-in"));
        anim::forget(ctx, self.id.with("rise"));
        anim::forget(ctx, self.id.with("fade-out"));
    }

    pub fn can_resize(&self) -> bool {
        self.resizable && (self.edit_mode || !self.drag_needs_edit)
    }

    fn can_move(&self) -> bool {
        self.edit_mode || !self.drag_needs_edit
    }

    /// Draws the panel for this frame (nothing while it is closed).
    pub fn show(&mut self, ctx: &Context, body: impl FnOnce(&mut Ui)) -> PanelResponse {
        self.show_with(ctx, |_| {}, body)
    }

    /// Like [`FloatingPanel::show`], with extra buttons in the header, left of ✕.
    pub fn show_with(
        &mut self,
        ctx: &Context,
        header: impl FnOnce(&mut Ui),
        body: impl FnOnce(&mut Ui),
    ) -> PanelResponse {
        let mut response = PanelResponse::default();
        if !self.open {
            return response;
        }
        if std::mem::take(&mut self.fresh) {
            self.forget_fades(ctx);
        }
        let fade_out = if self.closing {
            anim::value_from(ctx, self.id.with("fade-out"), 1.0, 0.0, Kind::Panels, 140.0, Ease::In)
        } else {
            1.0
        };
        if self.closing && fade_out <= 0.0 {
            self.open = false;
            self.closing = false;
            self.forget_fades(ctx);
            return response;
        }
        let drawn = if ctx.embed_viewports() {
            self.show_embedded(ctx, fade_out, header, body)
        } else {
            self.show_window(ctx, fade_out, header, body)
        };
        let Some(drawn) = drawn else { return response };
        response.escape = drawn.escape;
        if drawn.close {
            response.closed = true;
            self.close();
        }
        if ctx.embed_viewports() {
            let mut rect = self.geometry.rect();
            if let Some(delta) = drawn.drag {
                rect = rect.translate(delta);
            }
            if let Some((dir, delta, _)) = drawn.resize {
                rect = resized(rect, dir, delta, self.min_size);
            }
            if rect != self.geometry.rect() {
                self.geometry = Geometry::from_rect(rect);
                response.geometry_changed = true;
            }
        } else {
            let viewport = ViewportId::from_hash_of(self.id);
            if drawn.drag_started {
                ctx.send_viewport_cmd_to(viewport, ViewportCommand::StartDrag);
            }
            if let Some((dir, _, true)) = drawn.resize {
                ctx.send_viewport_cmd_to(viewport, ViewportCommand::BeginResize(dir));
            }
            // where the system really put it (Wayland does not say; there the placer in phase 3 knows)
            if let Some(outer) = ctx.input_for(viewport, |i| i.viewport().outer_rect)
                && Geometry::from_rect(outer) != self.geometry
                && outer.is_positive()
            {
                self.geometry = Geometry::from_rect(outer);
                response.geometry_changed = true;
            }
        }
        response
    }

    fn show_embedded(
        &mut self,
        ctx: &Context,
        fade_out: f32,
        header: impl FnOnce(&mut Ui),
        body: impl FnOnce(&mut Ui),
    ) -> Option<Drawn> {
        let geometry = self.geometry;
        let area = Area::new(self.id).order(Order::Foreground).fixed_pos(pos2(geometry.x, geometry.y)).show(
            ctx,
            |ui| {
                let rect = Rect::from_min_size(ui.cursor().min, vec2(geometry.w, geometry.h));
                ui.allocate_rect(rect, Sense::hover());
                self.draw(ui, rect, fade_out, header, body)
            },
        );
        Some(area.inner)
    }

    fn show_window(
        &mut self,
        ctx: &Context,
        fade_out: f32,
        header: impl FnOnce(&mut Ui),
        body: impl FnOnce(&mut Ui),
    ) -> Option<Drawn> {
        let title = if self.activate_on_show {
            self.title.clone()
        } else {
            format!("{}{}", self.title, marker::NO_ACTIVATE)
        };
        let g = self.geometry;
        let builder = ViewportBuilder::default()
            .with_title(format!("Luminescent {title}"))
            .with_app_id(APP_ID)
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_taskbar(false)
            .with_active(self.activate_on_show)
            .with_position(pos2(g.x, g.y))
            .with_inner_size(vec2(g.w, g.h))
            .with_min_inner_size(self.min_size)
            .with_resizable(self.resizable);
        let mut drawn = None;
        let mut header = Some(header);
        let mut body = Some(body);
        ctx.show_viewport_immediate(ViewportId::from_hash_of(self.id), builder, |ui, _class| {
            let rect = ui.max_rect();
            if let (Some(header), Some(body)) = (header.take(), body.take()) {
                drawn = Some(self.draw(ui, rect, fade_out, header, body));
            }
        });
        crate::window::shown(
            ctx,
            &format!("Luminescent {title}"),
            Some(pos2(g.x, g.y)),
            self.activate_on_show,
        );
        drawn
    }

    fn draw(
        &self,
        ui: &mut Ui,
        rect: Rect,
        fade_out: f32,
        header: impl FnOnce(&mut Ui),
        body: impl FnOnce(&mut Ui),
    ) -> Drawn {
        let ctx = ui.ctx().clone();
        let tokens = Tokens::of(&ctx);
        let motion = Motion::of(&ctx);
        let fade_in =
            anim::value_from(&ctx, self.id.with("fade-in"), 0.0, 1.0, Kind::Panels, 160.0, Ease::Out);
        // with the Caelestia style it rises a few pixels into place while it fades in (a window can not move
        // smoothly on Wayland, so the panel moves inside its window)
        let rise = if motion.style == Style::Caelestia {
            anim::value_from(&ctx, self.id.with("rise"), 14.0, 0.0, Kind::Panels, 260.0, Ease::Move)
        } else {
            0.0
        };
        ui.multiply_opacity(fade_in * fade_out);
        let panel = Rect::from_min_max(rect.min + vec2(0.0, rise.max(0.0)), rect.max);
        let p = &tokens.p;
        let radius = tokens.r();
        let painter = ui.painter().clone();
        paint::fill(&painter, panel, radius, &tokens, Role::BgBase, self.opacity);
        let border = if self.edit_mode { p.primary } else { p.border };

        let header_rect = Rect::from_min_size(panel.min, vec2(panel.width(), self.header_height));
        let top = CornerRadius {
            nw: (radius - 1.0).max(0.0) as u8,
            ne: (radius - 1.0).max(0.0) as u8,
            sw: 0,
            se: 0,
        };
        painter.rect_filled(header_rect.shrink(0.5), top, p.bg_header);
        painter.hline(header_rect.x_range(), header_rect.max.y - 0.5, Stroke::new(1.0, p.border));
        painter.rect_filled(
            Rect::from_min_size(pos2(header_rect.min.x, header_rect.max.y - 1.0), vec2(40.0, 1.0)),
            0.0,
            p.primary,
        );
        painter.rect_stroke(panel, radius, Stroke::new(1.0, border), StrokeKind::Inside);

        let mut drawn = Drawn { close: false, escape: false, drag: None, drag_started: false, resize: None };
        // dragging by the header comes first, so the header's buttons (added after) sit on top of it
        if self.can_move() {
            let drag = ui.interact(header_rect, self.id.with("drag"), Sense::click_and_drag());
            if drag.dragged() {
                drawn.drag = Some(drag.drag_delta());
            }
            drawn.drag_started = drag.drag_started();
        }
        let mut header_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(Rect::from_min_max(
                    header_rect.min + vec2(14.0, 0.0),
                    header_rect.max - vec2(6.0, 0.0),
                ))
                .layout(Layout::left_to_right(Align::Center)),
        );
        header_ui.spacing_mut().item_spacing.x = 8.0;
        header_ui.add(caps(&self.title));
        if !self.subtitle.is_empty() {
            header_ui.add(
                Text::new(format!("/ {}", self.subtitle.to_uppercase())).px(10.0).mono().tone(Tone::Muted),
            );
        }
        header_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if ui.add(IconButton::close()).clicked() {
                drawn.close = true;
            }
            if self.edit_mode {
                ui.add(caps("● EDIT").px(9.5).spacing(0.12).tone(Tone::Accent));
            }
            header(ui);
        });

        let body_rect = Rect::from_min_max(pos2(panel.min.x, header_rect.max.y), panel.max);
        let mut body_ui =
            ui.new_child(UiBuilder::new().max_rect(body_rect).layout(Layout::top_down(Align::Min)));
        body_ui.set_clip_rect(body_rect.intersect(ui.clip_rect()));
        body(&mut body_ui);

        if self.can_resize() {
            for dir in EDGES {
                let r = edge_rect(panel, dir);
                let edge = ui.interact(r, self.id.with(("edge", dir as u8)), Sense::drag());
                if edge.hovered() || edge.dragged() {
                    ctx.set_cursor_icon(edge_cursor(dir));
                }
                if edge.drag_started() || edge.dragged() {
                    drawn.resize = Some((dir, edge.drag_delta(), edge.drag_started()));
                }
            }
        }
        drawn.escape = ui.input(|i| i.key_pressed(Key::Escape)) && ui.ctx().memory(|m| m.focused().is_none());
        drawn
    }
}

/// Where a panel of `size` opens on `screen`: the middle, as Settings → Overlay & Interface → Panels like Notes
/// open says by default.
pub fn centred_on(size: Vec2, screen: Rect) -> Pos2 {
    (screen.center() - size / 2.0).round()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_resize_without_going_under_the_minimum() {
        let r = Rect::from_min_size(pos2(100.0, 100.0), vec2(300.0, 200.0));
        let min = vec2(200.0, 120.0);
        assert_eq!(resized(r, ResizeDirection::East, vec2(50.0, 9.0), min).width(), 350.0);
        let shrunk = resized(r, ResizeDirection::NorthWest, vec2(250.0, 250.0), min);
        assert_eq!((shrunk.width(), shrunk.height()), (200.0, 120.0));
        assert_eq!(shrunk.max, r.max, "the opposite corner stays put");
    }

    #[test]
    fn opens_fades_and_closes_in_a_window_without_viewports() {
        let ctx = Context::default();
        crate::setup(&ctx, Tokens::dark());
        let mut panel =
            FloatingPanel::new("notes", "Notes", Geometry { x: 40.0, y: 40.0, w: 360.0, h: 260.0 });
        let frame = |panel: &mut FloatingPanel, t: f64| {
            let input = egui::RawInput {
                time: Some(t),
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 700.0))),
                ..Default::default()
            };
            let mut seen = PanelResponse::default();
            let _ = crate::headless_pass(&ctx, input, |ui| {
                seen = panel.show(ui.ctx(), |ui| {
                    ui.label("hello");
                })
            });
            seen
        };
        assert_eq!(frame(&mut panel, 0.0), PanelResponse::default(), "closed: nothing drawn");
        panel.open();
        frame(&mut panel, 0.1);
        assert!(anim::is_moving(&ctx, panel.id().with("fade-in")));
        frame(&mut panel, 1.0);
        assert!(!anim::is_moving(&ctx, panel.id().with("fade-in")));
        panel.close();
        frame(&mut panel, 1.05);
        assert!(panel.is_open(), "still fading out");
        frame(&mut panel, 2.0);
        assert!(!panel.is_open());
    }
}
