//! Luminescent-style notifications (`ui/toast.py`): cards in the theme's colours, stacked in a corner of the
//! primary screen, that fade out by themselves. They never take the keyboard.
//!
//! Settings → General → Notifications sets where they go, how wide they are, the accent style (a soft glow, a
//! stripe, a line along the top, an outline or none), how long they stay, how see-through they are, the text
//! size and the icon; Themes → Settings… → Notifications gives them colours of their own.
//!
//! Long messages are never cut off: the card grows to fit up to about half the screen. Anything longer shows the
//! first lines and "Click to read it all"; a click opens it fully. Hover a card to keep it, ✕ to close it,
//! right-click to copy the text. Errors ("… error", "failed") get a red accent.

use std::sync::Arc;

use egui::{
    Align2, Area, Color32, Context, Galley, Id, Order, Painter, Pos2, Rect, Sense, Stroke, StrokeKind, Ui,
    UiBuilder, Vec2, ViewportBuilder, ViewportId, pos2, vec2,
};
use lum_core::settings::Settings;
use lum_platform::desktop::{APP_ID, marker};
use serde_json::{Map, Value};

use crate::anim::{self, Ease, Kind};
use crate::fonts::Weight;
use crate::paint;
use crate::text::Text;
use crate::theme::{DANGER, Gradient, GradientKind, Role, Tokens, WARNING, alpha};
use crate::widgets::IconButton;

const GAP: f32 = 10.0;
const MARGIN: f32 = 16.0;
const COLLAPSED_LINES: usize = 6;

pub const STYLES: &[(&str, &str)] = &[
    ("glow", "A soft glow of the accent"),
    ("stripe", "A stripe down the left side"),
    ("top", "A line along the top"),
    ("outline", "An accent-coloured outline"),
    ("none", "No accent"),
];

pub const POSITIONS: &[(&str, &str)] = &[
    ("tr", "Top right"),
    ("tc", "Top centre"),
    ("tl", "Top left"),
    ("br", "Bottom right"),
    ("bc", "Bottom centre"),
    ("bl", "Bottom left"),
];

/// How the cards look and behave, from Settings → General.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub position: String,
    pub width: f32,
    pub style: String,
    /// How long they stay, as a factor of what each one asks for.
    pub time: f32,
    pub most: usize,
    pub opacity: f32,
    pub text: f32,
    pub icon: bool,
}

impl Default for Look {
    fn default() -> Look {
        Look {
            position: "tr".into(),
            width: 360.0,
            style: "glow".into(),
            time: 1.0,
            most: 5,
            opacity: 1.0,
            text: 1.0,
            icon: true,
        }
    }
}

impl Look {
    pub fn from_settings(settings: &Settings) -> Look {
        Look::from_values(settings.all())
    }

    /// The look for these values, saved or not (the Settings page's "Show a notification" tries unsaved ones).
    pub fn from_values(values: &Map<String, Value>) -> Look {
        let get = |key: &str| values.get("general").and_then(|s| s.get(key));
        let text = |key: &str, default: &'static str| get(key).and_then(Value::as_str).unwrap_or(default);
        let number = |key: &str| get(key).and_then(Value::as_f64);
        let style = text("toast_style", "glow");
        let position = text("toast_position", "tr");
        Look {
            position: if POSITIONS.iter().any(|(k, _)| *k == position) { position } else { "tr" }.into(),
            width: number("toast_width").filter(|w| *w > 0.0).unwrap_or(360.0) as f32,
            style: if STYLES.iter().any(|(k, _)| *k == style) { style } else { "glow" }.into(),
            time: number("toast_time").filter(|t| *t > 0.0).unwrap_or(1.0) as f32,
            most: number("toast_max").filter(|n| *n > 0.0).unwrap_or(5.0).max(1.0) as usize,
            opacity: number("toast_opacity").unwrap_or(1.0).clamp(0.3, 1.0) as f32,
            text: number("toast_text").filter(|t| *t > 0.0).unwrap_or(1.0) as f32,
            icon: get("toast_icon").is_none_or(|v| v.as_bool().unwrap_or(true)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Error,
    Warning,
}

/// Errors and warnings are told apart by their title, as in Python.
pub fn level_of(title: &str) -> Level {
    let low = title.to_lowercase();
    if low.contains("error") || low.contains("failed") || low.contains("could not") {
        Level::Error
    } else if low.starts_with('⚠') || low.contains("warning") {
        Level::Warning
    } else {
        Level::Info
    }
}

impl Level {
    fn colour(self) -> Option<Color32> {
        match self {
            Level::Error => Some(DANGER),
            Level::Warning => Some(WARNING),
            Level::Info => None,
        }
    }

    fn icon(self) -> Option<&'static str> {
        match self {
            Level::Error => Some("✕"),
            Level::Warning => Some("!"),
            Level::Info => None,
        }
    }
}

/// The card: a rounded background (its own gradient if the theme gives one), a 1 px border, and the accent in
/// the chosen style, kept inside the rounded shape so nothing pokes out of the corners.
pub fn paint_card(
    painter: &Painter,
    rect: Rect,
    tokens: &Tokens,
    style: &str,
    accent: Color32,
    opacity: f32,
) {
    let rad = tokens.r();
    let shape = rect.shrink(0.5);
    paint::fill(painter, shape, rad, tokens, Role::BgBase, opacity);
    let clip = painter.with_clip_rect(shape.intersect(painter.clip_rect()));
    match style {
        "glow" => {
            let centre = rect.min + vec2(22.0, 20.0);
            let reach = (rect.width() * 0.75).max(120.0);
            paint::radial_glow(painter, shape, rad, centre, reach, alpha(accent, 0.22 * opacity));
        }
        "stripe" => {
            let stripe = Rect::from_min_size(rect.min, vec2(4.0, rect.height()));
            if accent == tokens.p.primary {
                paint::fill(&clip, stripe, 0.0, tokens, Role::Primary, 1.0);
            } else {
                clip.rect_filled(stripe, 0.0, accent);
            }
        }
        "top" => {
            let stops = vec![(0.0, accent), (0.65, accent), (1.0, alpha(accent, 0.0))];
            if let Some(g) = Gradient::new(GradientKind::Linear, 90.0, stops) {
                paint::gradient(&clip, Rect::from_min_size(rect.min, vec2(rect.width(), 2.5)), 0.0, &g, 1.0);
            }
        }
        _ => {}
    }
    let edge = if style == "outline" { Stroke::new(1.5, accent) } else { Stroke::new(1.0, tokens.p.border) };
    painter.rect_stroke(shape, rad, edge, StrokeKind::Inside);
}

struct Card {
    serial: u64,
    title: String,
    message: String,
    level: Level,
    /// How long it stays, in seconds, before hover and expanding change it.
    life: f64,
    /// When it starts to fade out (`ctx` time); `None` while the mouse is on it.
    until: Option<f64>,
    expanded: bool,
    closing: bool,
    copied: bool,
    /// Its height on the last frame, for stacking.
    height: f32,
}

/// What the app should do after a frame of the cards.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ToastOut {
    /// Put this on the clipboard (a right-click on a card).
    pub copy: Option<String>,
}

#[derive(Default)]
pub struct Toasts {
    cards: Vec<Card>,
    serial: u64,
}

impl std::fmt::Debug for Toasts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Toasts").field("cards", &self.cards.len()).finish()
    }
}

fn base_id() -> Id {
    Id::unique("lum-toasts")
}

struct Laid {
    title: Arc<Galley>,
    message: Option<Arc<Galley>>,
    more: Option<Arc<Galley>>,
    size: Vec2,
}

impl Toasts {
    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    pub fn len(&self) -> usize {
        self.cards.len()
    }

    /// Adds a card; the oldest go when there are more than `look.most`. `ms` is how long it asks to stay.
    pub fn push(&mut self, title: &str, message: &str, ms: u32, look: &Look) {
        while self.cards.len() >= look.most.max(1) {
            self.cards.remove(0);
        }
        self.serial += 1;
        // long messages stay up longer: about a second for every 15 words past the first few
        let words = message.split_whitespace().count();
        let ms = (f64::from(ms) * f64::from(look.time)).max(1500.0) + words.saturating_sub(12) as f64 * 70.0;
        self.cards.push(Card {
            serial: self.serial,
            title: title.to_owned(),
            message: message.chars().take(6000).collect(),
            level: level_of(title),
            life: ms / 1000.0,
            until: None,
            expanded: false,
            closing: false,
            copied: false,
            height: 46.0,
        });
    }

    pub fn clear(&mut self) {
        self.cards.clear();
    }

    /// Draws the cards over `screen` (the primary screen's free area) for this frame.
    pub fn show(&mut self, ctx: &Context, look: &Look, screen: Rect) -> ToastOut {
        let mut out = ToastOut::default();
        if self.cards.is_empty() {
            return out;
        }
        let base = Tokens::of(ctx);
        let tokens = base.for_section("toasts").unwrap_or_else(|| (*base).clone());
        let now = ctx.input(|i| i.time);
        let places = self.places(look, screen);
        let mut gone = Vec::new();
        let mut next_wake: Option<f64> = None;
        for (index, at) in places.into_iter().enumerate() {
            let card = &mut self.cards[index];
            if card.until.is_none() && !card.closing {
                card.until = Some(now + card.life);
            }
            if let Some(until) = card.until
                && now >= until
            {
                card.closing = true;
            }
            let fade = if card.closing {
                anim::value_from(
                    ctx,
                    base_id().with(("out", card.serial)),
                    1.0,
                    0.0,
                    Kind::Toasts,
                    180.0,
                    Ease::In,
                )
            } else {
                anim::value_from(
                    ctx,
                    base_id().with(("in", card.serial)),
                    0.0,
                    1.0,
                    Kind::Toasts,
                    170.0,
                    Ease::Out,
                )
            };
            if card.closing && fade <= 0.0 {
                gone.push(card.serial);
                continue;
            }
            let size = vec2(look.width, card.height);
            let mut drawn = Drawn::default();
            if ctx.embed_viewports() {
                let origin = ctx.content_rect().min + (at - screen.min);
                Area::new(base_id().with(("card", card.serial)))
                    .order(Order::Foreground)
                    .fixed_pos(origin)
                    .show(ctx, |ui| {
                        let rect = Rect::from_min_size(ui.cursor().min, size);
                        ui.allocate_rect(rect, Sense::hover());
                        let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
                        drawn = draw_card(&mut child, card, &tokens, look, screen.height(), fade);
                    });
            } else {
                let title = format!("Luminescent toast {}{}", card.serial, marker::NO_ACTIVATE);
                let builder = ViewportBuilder::default()
                    .with_title(title.clone())
                    .with_app_id(APP_ID)
                    .with_decorations(false)
                    .with_transparent(true)
                    .with_always_on_top()
                    .with_taskbar(false)
                    .with_resizable(false)
                    .with_active(false)
                    .with_position(at)
                    .with_inner_size(size);
                ctx.show_viewport_immediate(
                    ViewportId::from_hash_of(("lum-toast", card.serial)),
                    builder,
                    |ui, _| {
                        let rect = ui.max_rect();
                        let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
                        drawn = draw_card(&mut child, card, &tokens, look, screen.height(), fade);
                    },
                );
                crate::window::shown(ctx, &title, Some(at), false);
            }
            card.height = drawn.height;
            if drawn.hovered {
                // hovering keeps it up
                card.until = None;
            } else if card.until.is_none() && !card.closing {
                card.until = Some(now + if card.expanded { (card.life / 2.0).max(2.5) } else { 1.5 });
            }
            if drawn.close {
                card.closing = true;
            }
            if drawn.expand {
                card.expanded = true;
            }
            if drawn.copy {
                card.copied = true;
                out.copy = Some(format!("{}\n{}", card.title, card.message).trim().to_owned());
            }
            if let Some(until) = card.until {
                next_wake = Some(next_wake.map_or(until, |w: f64| w.min(until)));
            }
        }
        if !gone.is_empty() {
            self.cards.retain(|c| !gone.contains(&c.serial));
            ctx.request_repaint();
        }
        if let Some(at) = next_wake {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64((at - now).max(0.0)));
        }
        out
    }

    /// Where each card goes, newest nearest the edge, from the sizes of the last frame.
    fn places(&self, look: &Look, g: Rect) -> Vec<Pos2> {
        let bottom = look.position.starts_with('b');
        let mut y = if bottom { g.max.y - MARGIN } else { g.min.y + MARGIN };
        let mut places = vec![Pos2::ZERO; self.cards.len()];
        for (i, card) in self.cards.iter().enumerate().rev() {
            let x = if look.position.ends_with('l') {
                g.min.x + MARGIN
            } else if look.position.ends_with('c') {
                (g.center().x - look.width / 2.0).floor()
            } else {
                g.max.x - look.width - MARGIN
            };
            if bottom {
                y -= card.height;
                places[i] = pos2(x, y);
                y -= GAP;
            } else {
                places[i] = pos2(x, y);
                y += card.height + GAP;
            }
        }
        places
    }
}

#[derive(Default)]
struct Drawn {
    height: f32,
    hovered: bool,
    close: bool,
    expand: bool,
    copy: bool,
}

fn lay_out(ui: &Ui, card: &Card, tokens: &Tokens, look: &Look, screen_h: f32) -> Laid {
    let s = look.text;
    let icon = if look.icon { 30.0 } else { 0.0 };
    let width = look.width - 16.0 - 12.0 - icon - 12.0 - 20.0;
    let painter = ui.painter();
    let mut job = Text::new(&card.title).px(13.0 * s).semibold().job(tokens);
    job.wrap.max_width = width;
    let title = painter.layout_job(job);
    let (mut message, mut more) = (None, None);
    if !card.message.is_empty() {
        let full = |rows: usize| {
            let mut job = Text::new(&card.message).px(12.0 * s).tone(crate::Tone::Secondary).job(tokens);
            job.wrap.max_width = width;
            job.wrap.max_rows = rows;
            job.wrap.overflow_character = Some('…');
            painter.layout_job(job)
        };
        let line = tokens.fs(12.0 * s) * 1.3;
        let limit = COLLAPSED_LINES.max((screen_h * 0.45 / line) as usize);
        let whole = full(usize::MAX);
        if !card.expanded && whole.rows.len() > limit {
            // show what fits, then "click to read it all"
            message = Some(full(COLLAPSED_LINES));
            let more_job = Text::new("Click to read it all")
                .px(10.5 * s)
                .medium()
                .mono()
                .spacing(0.06)
                .colour(tokens.p.primary_text);
            more = Some(painter.layout_job(more_job.job(tokens)));
        } else {
            message = Some(whole);
        }
    }
    if card.copied {
        more = Some(
            painter.layout_job(
                Text::new("Copied")
                    .px(10.5 * s)
                    .medium()
                    .mono()
                    .spacing(0.06)
                    .colour(tokens.p.primary_text)
                    .job(tokens),
            ),
        );
    }
    let mut h = 12.0 + 1.0 + title.size().y;
    if let Some(m) = &message {
        h += 3.0 + m.size().y;
    }
    if let Some(m) = &more {
        h += 3.0 + m.size().y;
    }
    h += 13.0;
    Laid { title, message, more, size: vec2(look.width, h.min(screen_h * 0.9).max(46.0)) }
}

fn draw_card(ui: &mut Ui, card: &Card, tokens: &Tokens, look: &Look, screen_h: f32, fade: f32) -> Drawn {
    let laid = lay_out(ui, card, tokens, look, screen_h);
    let rect = Rect::from_min_size(ui.max_rect().min, laid.size);
    ui.multiply_opacity(fade);
    let accent = card.level.colour().unwrap_or(tokens.p.primary);
    paint_card(ui.painter(), rect, tokens, &look.style, accent, look.opacity);
    let response = ui.interact(rect, base_id().with(("hit", card.serial)), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Alert, true, &card.title));
    let hovered = ui.rect_contains_pointer(rect);
    let painter = ui.painter();
    let mut x = rect.min.x + 16.0;
    let top = rect.min.y + 12.0;
    if look.icon {
        let centre = pos2(x + 9.0, top + 9.0);
        match card.level.icon() {
            Some(glyph) => {
                painter.circle_filled(centre, 9.0, accent);
                painter.text(
                    centre,
                    Align2::CENTER_CENTER,
                    glyph,
                    tokens.font(10.0 * look.text, Weight::SemiBold, false),
                    Color32::WHITE,
                );
            }
            None => paint::sparkle(painter, centre, 15.0, tokens.p.primary),
        }
        x += 30.0;
    }
    let mut y = top + 1.0;
    painter.galley(pos2(x, y), laid.title.clone(), tokens.p.text_primary);
    y += laid.title.size().y;
    if let Some(m) = laid.message {
        y += 3.0;
        painter.galley(pos2(x, y), m.clone(), tokens.p.text_secondary);
        y += m.size().y;
    }
    let mut expand = false;
    if let Some(m) = laid.more {
        y += 3.0;
        painter.galley(pos2(x, y), m, tokens.p.primary_text);
        expand = !card.copied;
    }
    let mut drawn = Drawn { height: laid.size.y, hovered, ..Drawn::default() };
    // ✕ only while the mouse is on the card; the text keeps its width either way
    if hovered {
        let close = Rect::from_min_size(pos2(rect.max.x - 12.0 - 20.0, top), vec2(20.0, 20.0));
        if ui.put(close, IconButton::new("✕").size(20.0).font_px(10.0).ghost().tooltip("Close")).clicked() {
            drawn.close = true;
            return drawn;
        }
    }
    if response.secondary_clicked() {
        drawn.copy = true;
    } else if response.clicked() {
        if expand {
            drawn.expand = true;
        } else {
            drawn.close = true;
        }
    }
    drawn
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_come_from_the_title() {
        assert_eq!(level_of("Luminescent error"), Level::Error);
        assert_eq!(level_of("Update failed"), Level::Error);
        assert_eq!(level_of("⚠ Disk nearly full"), Level::Warning);
        assert_eq!(level_of("Theme: Dark"), Level::Info);
    }

    #[test]
    fn the_oldest_go_when_there_are_too_many_and_long_text_stays_longer() {
        let look = Look { most: 2, ..Look::default() };
        let mut toasts = Toasts::default();
        for n in 0..3 {
            toasts.push(&format!("T{n}"), "", 1000, &look);
        }
        assert_eq!(toasts.len(), 2);
        assert_eq!(toasts.cards[0].title, "T1");
        assert!((toasts.cards[0].life - 1.5).abs() < 1e-9, "at least 1.5 s");
        let words = vec!["word"; 42].join(" ");
        toasts.push("Long", &words, 4000, &look);
        assert!((toasts.cards[1].life - (4.0 + 30.0 * 0.07)).abs() < 1e-9);
    }

    #[test]
    fn cards_stack_from_the_corner_newest_nearest() {
        let look = Look::default();
        let mut toasts = Toasts::default();
        toasts.push("old", "", 4000, &look);
        toasts.push("new", "", 4000, &look);
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1920.0, 1080.0));
        let places = toasts.places(&look, screen);
        assert_eq!(places[1], pos2(1920.0 - 360.0 - 16.0, 16.0));
        assert_eq!(places[0].y, 16.0 + 46.0 + 10.0);
        let bottom = Look { position: "bl".into(), ..Look::default() };
        let places = toasts.places(&bottom, screen);
        assert_eq!(places[1], pos2(16.0, 1080.0 - 16.0 - 46.0));
    }
}
