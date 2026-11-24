//! Painting by hand: gradient fills, the accent glow, the dot grid and the sparkle mark.
//!
//! Gradients are meshes, not textures, so they stay sharp at any size and on any scale. Each one is cut into
//! cells along which the colour only changes linearly (bands between stops, rings, wedges), each cell is
//! clipped to the rounded rectangle, and the rim gets a one-pixel fringe so the corners are as smooth as egui's
//! own shapes. A finished mesh is cached by its rect and gradient, so a panel background costs nothing to
//! repaint. The glow and the dot grid are textures made once per colour and size, the same "paint it once"
//! rule the Python `_Backdrop` follows.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use egui::epaint::{CubicBezierShape, Mesh};
use egui::{
    Color32, ColorImage, Context, Id, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, TextureHandle,
    TextureOptions, TextureWrapMode, Vec2, pos2, vec2,
};

use crate::theme::{Gradient, GradientKind, Role, Tokens, alpha as with_alpha};

/// Fills `rect` with a colour role: its gradient when the theme has one, else its colour. Like
/// `theme.qbrush(role, rect, alpha)`.
pub fn fill(painter: &Painter, rect: Rect, radius: f32, tokens: &Tokens, role: Role, alpha: f32) {
    painter.add(fill_shape(painter.ctx(), rect, radius, tokens, role, alpha));
}

/// The shape [`fill`] paints, for putting behind content that is laid out first.
pub fn fill_shape(ctx: &Context, rect: Rect, radius: f32, tokens: &Tokens, role: Role, alpha: f32) -> Shape {
    match tokens.p.gradient(role) {
        Some(g) if rect.is_positive() => {
            let feather = 1.0 / ctx.pixels_per_point();
            Shape::Mesh(cached_mesh(ctx, rect, radius, g, alpha, feather))
        }
        Some(_) => Shape::Noop,
        None => Shape::rect_filled(rect, radius, tokens.p.colour(role).gamma_multiply(alpha.clamp(0.0, 1.0))),
    }
}

/// Fills a rounded rectangle with a gradient. `alpha` fades the whole fill.
pub fn gradient(painter: &Painter, rect: Rect, radius: f32, g: &Gradient, alpha: f32) {
    if !rect.is_positive() {
        return;
    }
    let feather = 1.0 / painter.ctx().pixels_per_point();
    let mesh = cached_mesh(painter.ctx(), rect, radius, g, alpha, feather);
    painter.add(Shape::Mesh(mesh));
}

fn cached_mesh(ctx: &Context, rect: Rect, radius: f32, g: &Gradient, alpha: f32, feather: f32) -> Arc<Mesh> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for f in [rect.min.x, rect.min.y, rect.max.x, rect.max.y, radius, alpha, feather, g.angle] {
        f.to_bits().hash(&mut hasher);
    }
    (g.kind as u8).hash(&mut hasher);
    for (pos, colour) in &g.stops {
        pos.to_bits().hash(&mut hasher);
        colour.to_array().hash(&mut hasher);
    }
    let key = hasher.finish();
    let id = Id::unique("lum-gradient-meshes");
    if let Some(mesh) = ctx.data_mut(|d| d.get_temp_mut_or_default::<MeshCache>(id).0.get(&key).cloned()) {
        return mesh;
    }
    let mesh = Arc::new(gradient_mesh(rect, radius, g, alpha, feather));
    ctx.data_mut(|d| {
        let cache = d.get_temp_mut_or_default::<MeshCache>(id);
        // a resize makes a new mesh every frame; keep the cache small rather than clever
        if cache.0.len() >= 64 {
            cache.0.clear();
        }
        cache.0.insert(key, mesh.clone());
    });
    mesh
}

#[derive(Clone, Default)]
struct MeshCache(HashMap<u64, Arc<Mesh>>);

/// The mesh for a gradient-filled rounded rectangle, with a fringe `feather` wide around it.
pub fn gradient_mesh(rect: Rect, radius: f32, g: &Gradient, alpha: f32, feather: f32) -> Mesh {
    let outline = rounded_outline(rect, radius);
    let colour = |t: f32| g.at(t).gamma_multiply(alpha.clamp(0.0, 1.0));
    let mut mesh = Mesh::default();
    match g.kind {
        GradientKind::Linear => {
            let (start, end) = linear_line(rect, g.angle);
            let dir = end - start;
            let len2 = dir.length_sq().max(1e-6);
            let t_of = |p: Pos2| (p - start).dot(dir) / len2;
            // one band between each pair of stops, plus the flat colour before the first and after the last
            let mut cuts: Vec<f32> = g.stops.iter().map(|s| s.0).collect();
            cuts.dedup();
            let mut bounds = vec![f32::NEG_INFINITY];
            bounds.extend(cuts);
            bounds.push(f32::INFINITY);
            for pair in bounds.windows(2) {
                let mut band = outline.clone();
                if pair[0].is_finite() {
                    band = clip_half_plane(&band, dir, pair[0] * len2 + start.to_vec2().dot(dir));
                }
                if pair[1].is_finite() {
                    band = clip_half_plane(&band, -dir, -(pair[1] * len2 + start.to_vec2().dot(dir)));
                }
                fan(&mut mesh, &band, |p| colour(t_of(p)));
            }
            fringe(&mut mesh, &outline, feather, |p| colour(t_of(p)));
        }
        GradientKind::Radial => {
            let centre = rect.center();
            let r = 0.75 * rect.width().max(rect.height());
            let t_of = |p: Pos2| (p - centre).length() / r;
            let mut rings: Vec<f32> = (0..=12).map(|i| i as f32 / 12.0).collect();
            rings.extend(g.stops.iter().map(|s| s.0));
            rings.push(2.0);
            rings.sort_by(f32::total_cmp);
            rings.dedup();
            const SLICES: usize = 64;
            for slice in 0..SLICES {
                let a0 = slice as f32 / SLICES as f32 * std::f32::consts::TAU;
                let a1 = (slice + 1) as f32 / SLICES as f32 * std::f32::consts::TAU;
                let (d0, d1) = (vec2(a0.cos(), a0.sin()), vec2(a1.cos(), a1.sin()));
                for pair in rings.windows(2) {
                    let (r0, r1) = (pair[0] * r, pair[1] * r);
                    let cell = if r0 <= 0.0 {
                        vec![centre, centre + d0 * r1, centre + d1 * r1]
                    } else {
                        vec![centre + d0 * r0, centre + d0 * r1, centre + d1 * r1, centre + d1 * r0]
                    };
                    fan(&mut mesh, &clip_convex(&cell, &outline), |p| colour(t_of(p)));
                }
            }
            fringe(&mut mesh, &outline, feather, |p| colour(t_of(p)));
        }
        GradientKind::Sweep => {
            let centre = rect.center();
            // Qt's QConicalGradient, as qbrush makes it: starts at (90 - angle) degrees and runs
            // anticlockwise on screen
            let start = (90.0 - g.angle).rem_euclid(360.0);
            let t_of = |p: Pos2| {
                let d = p - centre;
                let theta = (-d.y).atan2(d.x).to_degrees();
                (theta - start).rem_euclid(360.0) / 360.0
            };
            let mut cuts: Vec<f32> = (0..=96).map(|i| i as f32 / 96.0).collect();
            cuts.extend(g.stops.iter().map(|s| s.0));
            cuts.sort_by(f32::total_cmp);
            cuts.dedup();
            let far = rect.size().length() + 2.0;
            let dir = |t: f32| {
                let a = (start + t * 360.0).to_radians();
                vec2(a.cos(), -a.sin())
            };
            for pair in cuts.windows(2) {
                let (ta, tb) = (pair[0], pair[1]);
                if tb - ta < 1e-6 {
                    continue;
                }
                let wedge = [centre, centre + dir(ta) * far, centre + dir(tb) * far];
                // the colour jumps where the sweep starts again, so each wedge keeps to its own range
                let shade = |p: Pos2| {
                    if (p - centre).length_sq() < 1e-6 {
                        return colour((ta + tb) / 2.0);
                    }
                    let mut t = t_of(p);
                    if t > tb + 0.5 {
                        t -= 1.0;
                    } else if t < ta - 0.5 {
                        t += 1.0;
                    }
                    colour(t.clamp(ta, tb))
                };
                fan(&mut mesh, &clip_convex(&wedge, &outline), shade);
            }
            fringe(&mut mesh, &outline, feather, |p| colour(t_of(p)));
        }
    }
    mesh
}

/// Start and end of a linear gradient across `rect`, long enough that the first and last colours reach the
/// corners. `angle` works like CSS: the direction it runs in, 0 = upwards, 90 = to the right (`_line`).
fn linear_line(rect: Rect, angle: f32) -> (Pos2, Pos2) {
    let a = angle.to_radians();
    let (dx, dy) = (a.sin(), -a.cos());
    let span = (dx.abs() + dy.abs()) / 2.0;
    let at = |fx: f32, fy: f32| pos2(rect.min.x + fx * rect.width(), rect.min.y + fy * rect.height());
    (at(0.5 - dx * span, 0.5 - dy * span), at(0.5 + dx * span, 0.5 + dy * span))
}

/// The outline of a rounded rectangle, clockwise on screen, with enough points per corner to look round.
pub fn rounded_outline(rect: Rect, radius: f32) -> Vec<Pos2> {
    let r = radius.clamp(0.0, rect.width().min(rect.height()) / 2.0);
    if r < 0.5 {
        return vec![rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()];
    }
    let steps = ((r * 0.75).ceil() as usize).clamp(3, 16);
    let corners = [
        (pos2(rect.max.x - r, rect.min.y + r), -90.0f32),
        (pos2(rect.max.x - r, rect.max.y - r), 0.0),
        (pos2(rect.min.x + r, rect.max.y - r), 90.0),
        (pos2(rect.min.x + r, rect.min.y + r), 180.0),
    ];
    let mut points = Vec::with_capacity(4 * (steps + 1));
    for (centre, from) in corners {
        for i in 0..=steps {
            let a = (from + 90.0 * i as f32 / steps as f32).to_radians();
            points.push(centre + vec2(a.cos(), a.sin()) * r);
        }
    }
    points
}

/// Twice the signed area: positive for points that run clockwise on screen (y grows downwards).
fn area2(poly: &[Pos2]) -> f32 {
    poly.iter().zip(poly.iter().cycle().skip(1)).map(|(a, b)| a.x * b.y - b.x * a.y).sum()
}

/// The part of a convex polygon where `p · normal >= offset`.
fn clip_half_plane(poly: &[Pos2], normal: Vec2, offset: f32) -> Vec<Pos2> {
    let mut out = Vec::with_capacity(poly.len() + 2);
    let side = |p: Pos2| p.to_vec2().dot(normal) - offset;
    for (i, &a) in poly.iter().enumerate() {
        let b = poly[(i + 1) % poly.len()];
        let (sa, sb) = (side(a), side(b));
        if sa >= 0.0 {
            out.push(a);
        }
        if (sa >= 0.0) != (sb >= 0.0) {
            out.push(a + (b - a) * (sa / (sa - sb)));
        }
    }
    out
}

/// A convex polygon clipped to a convex outline (Sutherland and Hodgman).
fn clip_convex(poly: &[Pos2], outline: &[Pos2]) -> Vec<Pos2> {
    let sign = area2(outline).signum();
    let mut out = poly.to_vec();
    for (i, &a) in outline.iter().enumerate() {
        if out.len() < 3 {
            break;
        }
        let b = outline[(i + 1) % outline.len()];
        let edge = b - a;
        // inside is to the right of each edge for a clockwise outline
        let normal = vec2(-edge.y, edge.x) * sign;
        out = clip_half_plane(&out, normal, a.to_vec2().dot(normal));
    }
    out
}

/// A convex polygon as a fan of triangles, each corner coloured by `shade`.
fn fan(mesh: &mut Mesh, poly: &[Pos2], shade: impl Fn(Pos2) -> Color32) {
    if poly.len() < 3 {
        return;
    }
    let first = mesh.vertices.len() as u32;
    for &p in poly {
        mesh.colored_vertex(p, shade(p));
    }
    for i in 1..poly.len() as u32 - 1 {
        mesh.add_triangle(first, first + i, first + i + 1);
    }
}

/// A soft edge around a closed outline: the colour at the rim fading to nothing `width` further out.
fn fringe(mesh: &mut Mesh, outline: &[Pos2], width: f32, shade: impl Fn(Pos2) -> Color32) {
    let n = outline.len();
    if n < 3 {
        return;
    }
    let sign = area2(outline).signum();
    let first = mesh.vertices.len() as u32;
    for i in 0..n {
        let (prev, here, next) = (outline[(i + n - 1) % n], outline[i], outline[(i + 1) % n]);
        let out_of = |e: Vec2| vec2(e.y, -e.x).normalized() * sign;
        let normal = (out_of(here - prev) + out_of(next - here)).normalized();
        mesh.colored_vertex(here, shade(here));
        mesh.colored_vertex(here + normal * width, Color32::TRANSPARENT);
    }
    for i in 0..n as u32 {
        let j = (i + 1) % n as u32;
        let (a, a_out, b, b_out) = (first + 2 * i, first + 2 * i + 1, first + 2 * j, first + 2 * j + 1);
        mesh.add_triangle(a, a_out, b_out);
        mesh.add_triangle(a, b_out, b);
    }
}

/// The Luminescent mark: a four-pointed sparkle with curved sides, `size` across, centred on `centre`.
pub fn sparkle(painter: &Painter, centre: Pos2, size: f32, colour: Color32) {
    let origin = centre - vec2(size, size) / 2.0;
    let s = size / 100.0;
    let pt = |a: f32, b: f32| origin + vec2(a * s, b * s);
    let curves = [
        [pt(50.0, 0.0), pt(54.0, 34.0), pt(66.0, 46.0), pt(100.0, 50.0)],
        [pt(100.0, 50.0), pt(66.0, 54.0), pt(54.0, 66.0), pt(50.0, 100.0)],
        [pt(50.0, 100.0), pt(46.0, 66.0), pt(34.0, 54.0), pt(0.0, 50.0)],
        [pt(0.0, 50.0), pt(34.0, 46.0), pt(46.0, 34.0), pt(50.0, 0.0)],
    ];
    let mut outline: Vec<Pos2> = Vec::new();
    for points in curves {
        let curve = CubicBezierShape::from_points_stroke(points, false, Color32::TRANSPARENT, Stroke::NONE);
        let mut flat = curve.flatten(Some(0.1));
        flat.pop();
        outline.extend(flat);
    }
    // the shape is star-shaped around its centre, so a fan from there covers it exactly
    let mut mesh = Mesh::default();
    let hub = mesh.vertices.len() as u32;
    mesh.colored_vertex(centre, colour);
    for &p in &outline {
        mesh.colored_vertex(p, colour);
    }
    let n = outline.len() as u32;
    for i in 0..n {
        mesh.add_triangle(hub, hub + 1 + i, hub + 1 + (i + 1) % n);
    }
    fringe(&mut mesh, &outline, 1.0 / painter.ctx().pixels_per_point(), |_| colour);
    painter.add(Shape::mesh(mesh));
}

/// How far the panel glow reaches past the panel's edge, in points.
pub const GLOW_REACH: f32 = 160.0;

/// Points per texel of the glow textures: the glow is soft, so a quarter of the resolution looks the same and
/// costs a sixteenth of the memory.
const GLOW_STEP: f32 = 4.0;

/// The soft accent halo around a panel on the overlay, plus its thin ring, like `_render_panel`. The halo is a
/// texture made once per panel size and colour.
pub fn panel_glow(painter: &Painter, rect: Rect, tokens: &Tokens) {
    let radius = tokens.r();
    if tokens.glow() {
        let texture = glow_texture(
            painter.ctx(),
            GlowShape::Panel {
                w: rect.width().round() as u32,
                h: rect.height().round() as u32,
                radius: radius as u32,
            },
            tokens.p.primary,
            tokens.glow_alpha(),
        );
        let area = rect.expand(GLOW_REACH);
        painter.image(texture.id(), area, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
    }
    painter.rect_stroke(
        rect.expand(1.5),
        radius + 1.5,
        Stroke::new(1.0, with_alpha(tokens.p.primary, 0.16)),
        StrokeKind::Middle,
    );
}

/// The halo hanging from the top-centre edge of the primary screen, behind the page switcher.
pub fn top_halo(painter: &Painter, screen: Rect, tokens: &Tokens) {
    if !tokens.glow() {
        return;
    }
    let texture = glow_texture(painter.ctx(), GlowShape::Halo, tokens.p.primary, tokens.glow_alpha());
    let area = Rect::from_min_size(pos2(screen.center().x - 450.0, screen.min.y), vec2(900.0, 180.0));
    painter.image(texture.id(), area, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum GlowShape {
    Panel { w: u32, h: u32, radius: u32 },
    Halo,
}

/// A glow's shape, colour and strength (as bits).
type GlowKey = (GlowShape, [u8; 4], u32);

#[derive(Clone, Default)]
struct GlowCache(Vec<(GlowKey, TextureHandle)>);

fn glow_texture(ctx: &Context, shape: GlowShape, colour: Color32, strength: f32) -> TextureHandle {
    let key = (shape, colour.to_array(), strength.to_bits());
    let id = Id::unique("lum-glow-textures");
    let found = ctx.data_mut(|d| {
        let cache = d.get_temp_mut_or_default::<GlowCache>(id);
        cache.0.iter().find(|(k, _)| *k == key).map(|(_, t)| t.clone())
    });
    if let Some(found) = found {
        return found;
    }
    let image = match shape {
        GlowShape::Panel { w, h, radius } => {
            panel_glow_image(w as f32, h as f32, radius as f32, colour, strength)
        }
        GlowShape::Halo => halo_image(colour, strength),
    };
    let texture = ctx.load_texture("lum-glow", image, TextureOptions::LINEAR);
    ctx.data_mut(|d| {
        let cache = d.get_temp_mut_or_default::<GlowCache>(id);
        // old sizes from a resize go; a few stay for panels that come and go
        if cache.0.len() >= 6 {
            drop(cache.0.remove(0));
        }
        cache.0.push((key, texture.clone()));
    });
    texture
}

/// How far a point is outside a rounded rectangle of size `w` x `h` with its corner at the origin (negative
/// inside).
fn rounded_distance(x: f32, y: f32, w: f32, h: f32, radius: f32) -> f32 {
    let r = radius.min(w / 2.0).min(h / 2.0);
    let qx = (x - w / 2.0).abs() - (w / 2.0 - r);
    let qy = (y - h / 2.0).abs() - (h / 2.0 - r);
    let outside = vec2(qx.max(0.0), qy.max(0.0)).length();
    outside + qx.max(qy).min(0.0) - r
}

/// The glow as `_render_panel` paints it: 26 rings 6 px apart, each fainter, here as one smooth falloff.
pub(crate) fn glow_strength(distance: f32, strength: f32) -> f32 {
    if distance <= 0.5 {
        return 0.0;
    }
    let ring = ((distance + 2.0) / 6.0).max(1.0);
    if ring > 26.5 {
        return 0.0;
    }
    strength * 0.16 * (1.0 - ring / 27.0).powi(2)
}

fn panel_glow_image(w: f32, h: f32, radius: f32, colour: Color32, strength: f32) -> ColorImage {
    let full = vec2(w + 2.0 * GLOW_REACH, h + 2.0 * GLOW_REACH);
    let (tw, th) = ((full.x / GLOW_STEP).ceil() as usize, (full.y / GLOW_STEP).ceil() as usize);
    let [r, g, b, _] = colour.to_srgba_unmultiplied();
    let mut pixels = Vec::with_capacity(tw * th);
    for ty in 0..th {
        for tx in 0..tw {
            let x = (tx as f32 + 0.5) * full.x / tw as f32 - GLOW_REACH;
            let y = (ty as f32 + 0.5) * full.y / th as f32 - GLOW_REACH;
            let a = glow_strength(rounded_distance(x, y, w, h, radius), strength);
            pixels.push(Color32::from_rgba_unmultiplied(r, g, b, (a * 255.0).round() as u8));
        }
    }
    ColorImage::new([tw, th], pixels)
}

/// The top halo: a radial gradient squashed to 40 % of its height, strongest at the top edge.
fn halo_image(colour: Color32, strength: f32) -> ColorImage {
    let (tw, th) = ((900.0 / GLOW_STEP) as usize, (180.0 / GLOW_STEP) as usize);
    let [r, g, b, _] = colour.to_srgba_unmultiplied();
    let mut pixels = Vec::with_capacity(tw * th);
    for ty in 0..th {
        for tx in 0..tw {
            let x = ((tx as f32 + 0.5) / tw as f32 - 0.5) * 2.0;
            let y = (ty as f32 + 0.5) / th as f32;
            let d = (x * x + y * y).sqrt();
            let a = if d < 0.5 {
                strength + (strength * 0.45 - strength) * d / 0.5
            } else if d < 1.0 {
                strength * 0.45 * (1.0 - (d - 0.5) / 0.5)
            } else {
                0.0
            };
            pixels.push(Color32::from_rgba_unmultiplied(r, g, b, (a * 255.0).round() as u8));
        }
    }
    ColorImage::new([tw, th], pixels)
}

/// Spacing of the backdrop's dot grid, as `DOT_STEP`.
pub const DOT_STEP: f32 = 20.0;

/// The overlay's dot grid over `rect`: one repeating texture instead of thousands of little circles.
pub fn dot_grid(painter: &Painter, rect: Rect, tokens: &Tokens) {
    let dot = with_alpha(tokens.p.text_primary, if tokens.p.is_light() { 0.12 } else { 0.085 });
    let id = Id::unique("lum-dot-tile");
    let cached = painter.ctx().data(|d| d.get_temp::<(Color32, TextureHandle)>(id));
    let texture = match cached {
        Some((colour, texture)) if colour == dot => texture,
        _ => {
            let texture = painter.ctx().load_texture(
                "lum-dots",
                dot_tile(dot),
                TextureOptions { wrap_mode: TextureWrapMode::Repeat, ..TextureOptions::LINEAR },
            );
            painter.ctx().data_mut(|d| d.insert_temp(id, (dot, texture.clone())));
            texture
        }
    };
    let uv = Rect::from_min_max(
        (rect.min.to_vec2() / DOT_STEP).to_pos2(),
        (rect.max.to_vec2() / DOT_STEP).to_pos2(),
    );
    painter.image(texture.id(), rect, uv, Color32::WHITE);
}

/// One cell of the grid at four texels a point, with a dot of radius 1.1 in the middle.
fn dot_tile(colour: Color32) -> ColorImage {
    const SCALE: f32 = 4.0;
    let size = (DOT_STEP * SCALE) as usize;
    let [r, g, b, a] = colour.to_srgba_unmultiplied();
    let centre = DOT_STEP / 2.0;
    let pixels = (0..size * size)
        .map(|i| {
            let (x, y) = ((i % size) as f32 + 0.5, (i / size) as f32 + 0.5);
            let d = vec2(x / SCALE - centre, y / SCALE - centre).length();
            let cover = (1.1 - d + 0.5).clamp(0.0, 1.0);
            Color32::from_rgba_unmultiplied(r, g, b, (f32::from(a) * cover).round() as u8)
        })
        .collect();
    ColorImage::new([size, size], pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(rgb: u32) -> Color32 {
        Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
    }

    fn triangles_area(mesh: &Mesh) -> f32 {
        mesh.indices
            .chunks(3)
            .map(|t| {
                let [a, b, c] = [0, 1, 2].map(|i| mesh.vertices[t[i] as usize].pos);
                ((b - a).x * (c - a).y - (b - a).y * (c - a).x).abs() / 2.0
            })
            .sum()
    }

    /// The colour of the mesh at a point: the first triangle that holds it, interpolated.
    fn colour_at(mesh: &Mesh, p: Pos2) -> Option<Color32> {
        for t in mesh.indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| mesh.vertices[t[i] as usize]);
            let d = (b.pos - a.pos).x * (c.pos - a.pos).y - (b.pos - a.pos).y * (c.pos - a.pos).x;
            if d.abs() < 1e-6 {
                continue;
            }
            let u = ((p - a.pos).x * (c.pos - a.pos).y - (p - a.pos).y * (c.pos - a.pos).x) / d;
            let v = ((b.pos - a.pos).x * (p - a.pos).y - (b.pos - a.pos).y * (p - a.pos).x) / d;
            if u >= -1e-4 && v >= -1e-4 && u + v <= 1.0 + 1e-4 {
                let w = 1.0 - u - v;
                let mix = |f: fn(&Color32) -> u8| {
                    (w * f32::from(f(&a.color)) + u * f32::from(f(&b.color)) + v * f32::from(f(&c.color)))
                        .round() as u8
                };
                return Some(Color32::from_rgba_premultiplied(
                    mix(Color32::r),
                    mix(Color32::g),
                    mix(Color32::b),
                    mix(Color32::a),
                ));
            }
        }
        None
    }

    fn close(a: Color32, b: Color32) -> bool {
        a.to_array().iter().zip(b.to_array()).all(|(x, y)| x.abs_diff(y) <= 3)
    }

    #[test]
    fn a_linear_gradient_runs_the_way_its_angle_says() {
        let g = Gradient::new(GradientKind::Linear, 90.0, vec![(0.0, hex(0x000000)), (1.0, hex(0xFFFFFF))])
            .unwrap();
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(200.0, 100.0));
        let mesh = gradient_mesh(rect, 0.0, &g, 1.0, 0.0);
        // left to right at 90 degrees, the full rect covered
        assert!((triangles_area(&mesh) - 200.0 * 100.0).abs() < 1.0);
        assert!(close(colour_at(&mesh, pos2(1.0, 50.0)).unwrap(), hex(0x010101)));
        assert!(close(colour_at(&mesh, pos2(100.0, 20.0)).unwrap(), hex(0x808080)));
        assert!(close(colour_at(&mesh, pos2(199.0, 80.0)).unwrap(), hex(0xFEFEFE)));
    }

    #[test]
    fn stops_in_between_are_exact() {
        let g = Gradient::new(
            GradientKind::Linear,
            180.0,
            vec![(0.0, hex(0xFF0000)), (0.25, hex(0x00FF00)), (1.0, hex(0x0000FF))],
        )
        .unwrap();
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0));
        let mesh = gradient_mesh(rect, 12.0, &g, 1.0, 0.0);
        assert!(
            close(colour_at(&mesh, pos2(50.0, 25.0)).unwrap(), hex(0x00FF00)),
            "the middle stop, top to bottom"
        );
        let corner_cut = 4.0 * 12.0 * 12.0 * (1.0 - std::f32::consts::FRAC_PI_4);
        assert!(
            (triangles_area(&mesh) - (10_000.0 - corner_cut)).abs() < 8.0,
            "rounded corners are left out"
        );
        assert!(colour_at(&mesh, pos2(0.5, 0.5)).is_none());
    }

    #[test]
    fn radial_and_sweep_cover_the_shape_and_shade_like_qt() {
        let g = Gradient::new(GradientKind::Radial, 0.0, vec![(0.0, hex(0xFFFFFF)), (1.0, hex(0x000000))])
            .unwrap();
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0));
        let mesh = gradient_mesh(rect, 10.0, &g, 1.0, 0.0);
        assert!(close(colour_at(&mesh, pos2(50.0, 50.0)).unwrap(), hex(0xFFFFFF)));
        // the radius is 0.75 of the longer side, so the edge middle is a third of the way to black
        let edge = colour_at(&mesh, pos2(99.0, 50.0)).unwrap();
        assert!(close(edge, g.at(49.0 / 75.0)), "{edge:?}");

        let sweep = Gradient::new(GradientKind::Sweep, 0.0, vec![(0.0, hex(0xFF0000)), (1.0, hex(0x0000FF))])
            .unwrap();
        let mesh = gradient_mesh(rect, 10.0, &sweep, 1.0, 0.0);
        let corner_cut = 4.0 * 100.0 * (1.0 - std::f32::consts::FRAC_PI_4);
        assert!((triangles_area(&mesh) - (10_000.0 - corner_cut)).abs() < 8.0);
        // angle 0 starts straight up and runs anticlockwise: a quarter turn later is the left side
        let left = colour_at(&mesh, pos2(5.0, 50.0)).unwrap();
        assert!(close(left, sweep.at(0.25)), "{left:?}");
    }

    #[test]
    fn the_glow_fades_out_like_the_rings() {
        assert_eq!(glow_strength(-3.0, 0.26), 0.0);
        let near = glow_strength(4.0, 0.26);
        let far = glow_strength(100.0, 0.26);
        assert!(near > far && far > 0.0);
        assert!((near - 0.26 * 0.16 * (1.0f32 - 1.0 / 27.0).powi(2)).abs() < 1e-4);
        assert_eq!(glow_strength(170.0, 0.26), 0.0);
        assert!(rounded_distance(50.0, 50.0, 100.0, 100.0, 10.0) < 0.0);
        assert!((rounded_distance(110.0, 50.0, 100.0, 100.0, 10.0) - 10.0).abs() < 1e-4);
    }
}
