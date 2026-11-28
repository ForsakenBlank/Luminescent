//! The endless canvas: where the view looks and how close, the dot grid, wires as bezier curves, and what is
//! under the pointer.

use std::collections::HashMap;

use egui::emath::TSTransform;
use egui::epaint::CubicBezierShape;
use egui::{Color32, Context, Painter, Pos2, Rect, Shape, Stroke, Vec2, pos2, vec2};
use lum_automation::doc;
use lum_automation::spec::{self, NodeSpec};
use serde_json::{Value, json};

use super::node::{self, Layout, VALUE_COLOUR};
use crate::anim::{Kind, Motion};
use crate::theme::Tokens;

const MIN_ZOOM: f32 = 0.2;
const MAX_ZOOM: f32 = 2.5;
/// How long a smooth zoom glides after a wheel notch.
const GLIDE_SECS: f64 = 0.14;

/// A zoom on its way: from, to, and the point that stays under the pointer.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Glide {
    from: f32,
    to: f32,
    screen: Pos2,
    scene: Pos2,
    started: f64,
}

/// Where the view looks (the canvas point in the middle of the page) and how close.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub center: Pos2,
    pub zoom: f32,
    glide: Option<Glide>,
}

impl Default for View {
    fn default() -> View {
        View { center: Pos2::ZERO, zoom: 1.0, glide: None }
    }
}

impl View {
    pub fn new(center: Pos2, zoom: f32) -> View {
        View { center, zoom: zoom.clamp(MIN_ZOOM, MAX_ZOOM), glide: None }
    }

    /// A workspace's saved `view`.
    pub fn of(v: &Value) -> View {
        let n = |k: &str, d: f64| v.get(k).and_then(Value::as_f64).unwrap_or(d) as f32;
        let zoom = n("zoom", 1.0);
        View::new(pos2(n("x", 0.0), n("y", 0.0)), if zoom > 0.0 { zoom } else { 1.0 })
    }

    pub fn to_json(&self) -> Value {
        let zoom = (f64::from(self.zoom) * 1000.0).round() / 1000.0;
        json!({"x": self.center.x.round() as i64, "y": self.center.y.round() as i64, "zoom": zoom})
    }

    pub fn transform(&self, rect: Rect) -> TSTransform {
        TSTransform::new(rect.center().to_vec2() - self.center.to_vec2() * self.zoom, self.zoom)
    }

    pub fn to_scene(&self, rect: Rect, p: Pos2) -> Pos2 {
        self.center + (p - rect.center()) / self.zoom
    }

    pub fn to_screen(&self, rect: Rect, p: Pos2) -> Pos2 {
        rect.center() + (p - self.center) * self.zoom
    }

    /// The canvas follows the pointer by `delta` screen points.
    pub fn pan(&mut self, delta: Vec2) {
        self.center -= delta / self.zoom;
    }

    pub fn set_zoom(&mut self, zoom: f32) {
        self.glide = None;
        self.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    }

    fn zoom_at(&mut self, rect: Rect, screen: Pos2, scene: Pos2, zoom: f32) {
        self.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        self.center = scene - (screen - rect.center()) / self.zoom;
    }

    /// A wheel notch: zooms around `at`, gliding there when Settings → Animations says so.
    pub fn zoom_by(&mut self, ctx: &Context, rect: Rect, at: Pos2, factor: f32) {
        let scene = self.to_scene(rect, at);
        if !Motion::of(ctx).on(Kind::Zoom) {
            self.zoom_at(rect, at, scene, self.zoom * factor);
            return;
        }
        // every notch moves the target, and the view glides after it
        let to = (self.glide.map_or(self.zoom, |g| g.to) * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let started = ctx.input(|i| i.time);
        self.glide = Some(Glide { from: self.zoom, to, screen: at, scene, started });
        ctx.request_repaint();
    }

    /// Moves a smooth zoom on. Gives back true on the frame it arrives.
    pub fn glide(&mut self, ctx: &Context, rect: Rect) -> bool {
        let Some(g) = self.glide else { return false };
        let t = ((ctx.input(|i| i.time) - g.started) / GLIDE_SECS).clamp(0.0, 1.0) as f32;
        let eased = 1.0 - (1.0 - t).powi(3);
        self.zoom_at(rect, g.screen, g.scene, g.from + (g.to - g.from) * eased);
        if t >= 1.0 {
            self.glide = None;
            return true;
        }
        ctx.request_repaint();
        false
    }
}

/// `round(v / g) * g`: a position on the snap grid (none below 2).
pub fn snap(v: f32, grid: f32) -> f32 {
    // Python's round() goes to the even neighbour on a tie
    if grid > 1.0 { (v / grid).round_ties_even() * grid } else { v }
}

/// A new node's position as Python stored it: a whole number on a grid, a decimal without one.
pub fn number(v: f32, grid: f32) -> Value {
    if grid > 1.0 { json!(v.round() as i64) } else { json!(f64::from(v)) }
}

pub struct NodeLayout {
    pub spec: &'static NodeSpec,
    pub layout: Layout,
}

impl NodeLayout {
    pub fn port(&self, port: &str) -> Option<Pos2> {
        self.layout.port(port)
    }
}

/// Every node's top-left corner and layout, by id.
pub type Layouts = HashMap<String, (Pos2, NodeLayout)>;

pub fn layouts(painter: &Painter, tokens: &Tokens, nodes: &[Value]) -> Layouts {
    let mut out = Layouts::new();
    for n in nodes {
        let Some(spec) = spec::spec(doc::str_of(n, "type")) else { continue };
        let at = pos2(n["x"].as_f64().unwrap_or(0.0) as f32, n["y"].as_f64().unwrap_or(0.0) as f32);
        out.insert(
            doc::str_of(n, "id").to_owned(),
            (at, NodeLayout { spec, layout: node::layout(painter, tokens, spec) }),
        );
    }
    out
}

/// Where a port sits on a node of this type, from its top-left corner.
pub fn port_offset(painter: &Painter, tokens: &Tokens, spec: &'static NodeSpec, port: &str) -> Option<Pos2> {
    node::layout(painter, tokens, spec).port(port)
}

/// The view's see-through backdrop with its dot grid, which thins out as you zoom out.
pub fn background(painter: &Painter, tokens: &Tokens, rect: Rect, view: &View) {
    let p = &tokens.p;
    painter.rect_filled(rect, 0.0, p.bg_frame.gamma_multiply(0.72));
    let mut step = 24.0;
    while step * view.zoom < 12.0 {
        step *= 2.0;
    }
    let light = p.bg_base.r() as u32 + p.bg_base.g() as u32 + p.bg_base.b() as u32 > 3 * 140;
    let dot = p.text_primary.gamma_multiply(if light { 0.16 } else { 0.10 });
    let top_left = view.to_scene(rect, rect.min);
    let bottom_right = view.to_scene(rect, rect.max);
    let x0 = (top_left.x / step).floor() * step;
    let y0 = (top_left.y / step).floor() * step;
    let mut shapes = Vec::new();
    let mut y = y0;
    while y <= bottom_right.y {
        let mut x = x0;
        while x <= bottom_right.x {
            let at = view.to_screen(rect, pos2(x, y));
            shapes.push(Shape::rect_filled(Rect::from_center_size(at, vec2(2.0, 2.0)), 0.0, dot));
            x += step;
        }
        y += step;
    }
    painter.extend(shapes);
}

fn bezier(a: Pos2, b: Pos2) -> [Pos2; 4] {
    let dx = ((b.x - a.x).abs() * 0.5).max(60.0);
    [a, a + vec2(dx, 0.0), b - vec2(dx, 0.0), b]
}

/// A point along the wire, `t` from 0 (its start) to 1.
pub fn point_on(a: Pos2, b: Pos2, t: f32) -> Pos2 {
    let [p0, p1, p2, p3] = bezier(a, b);
    let u = 1.0 - t;
    let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
    pos2(
        w[0] * p0.x + w[1] * p1.x + w[2] * p2.x + w[3] * p3.x,
        w[0] * p0.y + w[1] * p1.y + w[2] * p2.y + w[3] * p3.y,
    )
}

fn points(a: Pos2, b: Pos2) -> Vec<Pos2> {
    (0..=32).map(|i| point_on(a, b, i as f32 / 32.0)).collect()
}

/// Both ends of a wire on the canvas.
pub fn ends(w: &Value, layouts: &Layouts) -> Option<(Pos2, Pos2)> {
    let end = |key: &str| {
        let (nid, port) = (w[key][0].as_str()?, w[key][1].as_str()?);
        let (origin, l) = layouts.get(nid)?;
        Some(*origin + l.port(port)?.to_vec2())
    };
    Some((end("src")?, end("dst")?))
}

pub fn paint_wire(
    painter: &Painter,
    tokens: &Tokens,
    w: &Value,
    layouts: &Layouts,
    selected: bool,
    heat: f32,
) {
    let Some((a, b)) = ends(w, layouts) else { return };
    let value = w["src"][1].as_str().is_some_and(|p| p.starts_with("val:"));
    let src_colour = w["src"][0]
        .as_str()
        .and_then(|nid| layouts.get(nid))
        .map_or(tokens.p.primary, |(_, l)| node::category_colour(&l.spec.category));
    let mut colour = if value { VALUE_COLOUR } else { src_colour };
    if selected {
        colour = tokens.p.text_primary;
    }
    if heat > 0.0 {
        painter.add(Shape::line(points(a, b), Stroke::new(7.0, colour.gamma_multiply(0.35 * heat))));
    }
    let alpha = if heat > 0.0 || selected { 1.0 } else { 0.75 };
    if value {
        painter.extend(Shape::dashed_line(
            &points(a, b),
            Stroke::new(1.8, colour.gamma_multiply(alpha)),
            6.0,
            4.0,
        ));
    } else {
        let shape = CubicBezierShape::from_points_stroke(
            bezier(a, b),
            false,
            Color32::TRANSPARENT,
            Stroke::new(2.2, colour.gamma_multiply(alpha)),
        );
        painter.add(shape);
    }
}

/// The wire being dragged out of a port.
pub fn dashed_wire(painter: &Painter, a: Pos2, b: Pos2, stroke: Stroke) {
    painter.extend(Shape::dashed_line(&points(a, b), stroke, 6.0, 4.0));
}

fn distance_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = if ab.length_sq() > 0.0 { ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) } else { 0.0 };
    (a + ab * t - p).length()
}

/// The wire under `p` (within `tolerance` canvas units), topmost first.
pub fn wire_at(p: Pos2, wires: &[Value], layouts: &Layouts, tolerance: f32) -> Option<String> {
    wires.iter().rev().find_map(|w| {
        let (a, b) = ends(w, layouts)?;
        let pts = points(a, b);
        let near = pts.windows(2).any(|s| distance_to_segment(p, s[0], s[1]) <= tolerance);
        near.then(|| doc::str_of(w, "id").to_owned())
    })
}

/// The port nearest `p` within `radius` (measured as Python's `manhattanLength`).
pub fn port_at(p: Pos2, layouts: &Layouts, radius: f32) -> Option<(String, String)> {
    let mut best: Option<(String, String)> = None;
    let mut dist = radius;
    for (nid, (origin, l)) in layouts {
        for (port, at) in &l.layout.ports {
            let d = (*origin + at.to_vec2()) - p;
            let d = d.x.abs() + d.y.abs();
            if d < dist {
                dist = d;
                best = Some((nid.clone(), port.clone()));
            }
        }
    }
    best
}

/// A view that shows every node, never closer than 120 %.
pub fn fit(rect: Rect, layouts: &Layouts) -> View {
    let mut all: Option<Rect> = None;
    for (origin, l) in layouts.values() {
        let r = Rect::from_min_size(*origin, l.layout.size);
        all = Some(all.map_or(r, |a| a.union(r)));
    }
    let Some(all) = all else { return View::default() };
    let padded = Rect::from_min_max(all.min - vec2(80.0, 120.0), all.max + vec2(80.0, 160.0));
    let zoom = (rect.width() / padded.width()).min(rect.height() / padded.height()).min(1.2);
    View::new(padded.center(), zoom)
}
