//! Every pulse on its way along a wire, as a glowing dot with a peek at what it carries (`PulseLayer`).

use egui::{Color32, Painter, Stroke, StrokeKind, pos2, vec2};

use super::canvas::{self, Layouts};
use super::node::{self, VALUE_COLOUR};
use crate::fonts::Weight;
use crate::theme::Tokens;

/// How long a dot takes from one end of its wire to the other.
pub const DURATION: f64 = 0.55;
/// More than this at once could not be told apart anyway; the rest are dropped.
const MAX: usize = 300;

struct Dot {
    wire: String,
    started: f64,
    text: String,
}

#[derive(Default)]
pub struct Pulses {
    dots: Vec<Dot>,
}

impl Pulses {
    pub fn add(&mut self, wire: &str, text: &str, now: f64) {
        if self.dots.len() < MAX {
            self.dots.push(Dot { wire: wire.to_owned(), started: now, text: text.to_owned() });
        }
    }

    /// Drops the dots that arrived. Gives back whether any are still on their way.
    pub fn step(&mut self, now: f64) -> bool {
        self.dots.retain(|d| now - d.started < DURATION);
        !self.dots.is_empty()
    }

    pub fn clear(&mut self) {
        self.dots.clear();
    }

    pub fn len(&self) -> usize {
        self.dots.len()
    }

    pub fn paint(
        &self,
        painter: &Painter,
        tokens: &Tokens,
        wires: &[serde_json::Value],
        layouts: &Layouts,
        now: f64,
    ) {
        let font = tokens.font(10.0, Weight::Medium, true);
        for dot in &self.dots {
            let Some(w) = wires.iter().find(|w| w["id"] == dot.wire.as_str()) else { continue };
            let Some((a, b)) = canvas::ends(w, layouts) else { continue };
            let t = ((now - dot.started) / DURATION).clamp(0.0, 1.0) as f32;
            let at = canvas::point_on(a, b, 1.0 - (1.0 - t) * (1.0 - t));
            let value = w["src"][1].as_str().is_some_and(|p| p.starts_with("val:"));
            let colour = if value {
                VALUE_COLOUR
            } else {
                w["src"][0]
                    .as_str()
                    .and_then(|nid| layouts.get(nid))
                    .map_or(tokens.p.primary, |(_, l)| node::category_colour(&l.spec.category))
            };
            painter.circle_filled(at, 10.0, colour.gamma_multiply(0.3));
            painter.circle_filled(at, 5.0, colour);
            painter.circle_filled(at, 2.0, Color32::WHITE);
            if dot.text.is_empty() {
                continue;
            }
            let line = dot.text.replace('\n', " ⏎ ");
            let short: String = line.chars().take(28).collect();
            let short = if line.chars().count() > 28 { format!("{short}…") } else { short };
            let galley = painter.layout_no_wrap(short, font.clone(), tokens.p.text_primary);
            let rect =
                egui::Rect::from_min_size(pos2(at.x + 10.0, at.y - 26.0), vec2(galley.size().x + 12.0, 18.0));
            painter.rect_filled(rect, 6.0, tokens.p.bg_base);
            painter.rect_stroke(rect, 6.0, Stroke::new(1.0, colour), StrokeKind::Inside);
            painter.galley(rect.center() - galley.size() / 2.0, galley, tokens.p.text_primary);
        }
    }
}
