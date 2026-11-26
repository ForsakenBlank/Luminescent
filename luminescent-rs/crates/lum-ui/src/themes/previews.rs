//! The little pictures of a theme: a mock-up of the overlay, a floating panel, a notification card and an
//! on-screen warning, painted in any palette without switching to it (`ui/themes_page.py` `paint_*`). They
//! draw at a fixed size and scale, and note where each colour is used so a click on the picture can find it.

use egui::{Align2, Color32, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, pos2, vec2};

use crate::fonts::{self, Weight};
use crate::paint;
use crate::text::Text;
use crate::theme::{Palette, Role, Tokens, alpha};

/// Where each colour role is drawn, so the picture can be pointed at.
#[derive(Debug, Default, Clone)]
pub struct Reg {
    items: Vec<(Rect, Vec<Role>)>,
}

impl Reg {
    pub fn add(&mut self, rect: Rect, roles: &[Role]) {
        self.items.push((rect, roles.to_vec()));
    }

    /// The first colour of the smallest thing under `p`.
    pub fn at(&self, p: Pos2) -> Option<Role> {
        self.items
            .iter()
            .filter(|(r, _)| r.contains(p))
            .min_by(|a, b| (a.0.width() * a.0.height()).total_cmp(&(b.0.width() * b.0.height())))
            .and_then(|(_, roles)| roles.first().copied())
    }

    pub fn rects(&self, role: Role) -> Vec<Rect> {
        self.items.iter().filter(|(_, roles)| roles.contains(&role)).map(|(r, _)| *r).collect()
    }
}

/// Everything a preview needs to draw: the painter, the palette as tokens at 100 % text size.
pub struct Canvas<'a> {
    pub painter: &'a Painter,
    pub tokens: Tokens,
}

impl<'a> Canvas<'a> {
    pub fn new(painter: &'a Painter, palette: &Palette) -> Canvas<'a> {
        Canvas { painter, tokens: Tokens::new(palette.clone(), 1.0) }
    }

    fn p(&self) -> &Palette {
        &self.tokens.p
    }

    fn fill(&self, rect: Rect, radius: f32, role: Role) {
        paint::fill(self.painter, rect, radius, &self.tokens, role, 1.0);
    }

    #[allow(clippy::too_many_arguments)]
    fn text(
        &self,
        rect: Rect,
        align: Align2,
        text: &str,
        colour: Color32,
        px: f32,
        weight: Weight,
        mono: bool,
    ) {
        let font = fonts::font(px, weight, mono);
        let pos = match align {
            Align2::LEFT_CENTER => rect.left_center(),
            Align2::RIGHT_CENTER => rect.right_center(),
            Align2::CENTER_CENTER => rect.center(),
            Align2::LEFT_TOP => rect.left_top(),
            _ => rect.center(),
        };
        self.painter.with_clip_rect(rect.expand(2.0)).text(pos, align, text, font, colour);
    }

    fn spaced(&self, rect: Rect, text: &str, colour: Color32, px: f32, spacing: f32) {
        let job = Text::new(text).px(px).semibold().mono().spacing(spacing).colour(colour).job(&self.tokens);
        let galley = self.painter.layout_job(job);
        let at = pos2(rect.min.x, rect.center().y - galley.size().y / 2.0);
        self.painter.with_clip_rect(rect).galley(at, galley, colour);
    }
}

fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(pos2(x, y), vec2(w, h))
}

/// A small mock-up of the overlay: page switcher, a title, three cards.
pub fn paint_overlay(c: &Canvas<'_>, area: Rect, s: f32, reg: &mut Reg) {
    let p = c.p().clone();
    reg.add(area, &[Role::BgBase]);
    let rad = (f32::from(p.radius) - 4.0).max(0.0) * s;
    c.fill(area, rad, Role::BgBase);
    let clip = Canvas { painter: &c.painter.with_clip_rect(area), tokens: c.tokens.clone() };
    let pad = 10.0 * s;
    let w = area.width();
    // the page switcher pill hanging from the top
    let nav = r(area.min.x + w * 0.28, area.min.y, w * 0.44, 15.0 * s);
    clip.fill(Rect::from_min_max(nav.min - vec2(0.0, 8.0 * s), nav.max), 8.0 * s, Role::BgHeader);
    reg.add(nav, &[Role::BgHeader]);
    let pill = r(nav.min.x + 4.0 * s, nav.min.y + 3.0 * s, nav.width() * 0.3, 9.0 * s);
    reg.add(pill, &[Role::Primary]);
    clip.fill(pill, 4.5 * s, Role::Primary);
    clip.fill(r(nav.min.x, nav.min.y, nav.width(), (1.5 * s).max(1.0)), 0.0, Role::Primary);
    for i in 0..2 {
        let x = pill.max.x + (8.0 + i as f32 * 26.0) * s;
        clip.painter.rect_filled(
            r(x, pill.min.y + 3.0 * s, 18.0 * s, 3.0 * s),
            1.5,
            alpha(p.text_secondary, 0.6),
        );
        reg.add(r(x, pill.min.y, 18.0 * s, 9.0 * s), &[Role::TextSecondary]);
    }
    // a title, an accent tick
    let top = nav.max.y + 9.0 * s;
    clip.painter.rect_filled(r(area.min.x + pad, top, 46.0 * s, 5.0 * s), 2.5 * s, p.text_primary);
    reg.add(r(area.min.x + pad, top - 2.0 * s, 46.0 * s, 9.0 * s), &[Role::TextPrimary]);
    let tick = r(area.min.x + pad, top + 10.0 * s, 22.0 * s, (1.5 * s).max(1.0));
    clip.fill(tick, 0.0, Role::Primary);
    // three cards, the last one shorter
    let ctop = tick.max.y + 7.0 * s;
    let avail = area.max.y - pad - ctop;
    let cw = (w - 2.0 * pad - 12.0 * s) / 3.0;
    for i in 0..3 {
        let card = r(
            area.min.x + pad + i as f32 * (cw + 6.0 * s),
            ctop,
            cw,
            avail * if i == 2 { 0.62 } else { 1.0 },
        );
        clip.fill(card, 4.0 * s, Role::BgCard);
        clip.painter.rect_stroke(card, 4.0 * s, Stroke::new(1.0, p.border), StrokeKind::Inside);
        reg.add(card, &[Role::BgCard, Role::Border]);
        let dot = r(card.min.x + 6.0 * s, card.min.y + 6.0 * s, 5.0 * s, 5.0 * s);
        reg.add(dot, &[Role::Primary]);
        clip.painter.circle_filled(dot.center(), 2.5 * s, p.primary);
        for j in 0..3 {
            let colour = alpha(if j > 0 { p.text_secondary } else { p.text_primary }, 0.7);
            clip.painter.rect_filled(
                r(
                    card.min.x + 6.0 * s,
                    card.min.y + (16.0 + j as f32 * 8.0) * s,
                    card.width() * (0.7 - j as f32 * 0.15),
                    3.0 * s,
                ),
                1.5,
                colour,
            );
        }
    }
}

/// A floating panel: header, accent tick, a card, a field, a button, a ticked box and a slider.
pub fn paint_panel(c: &Canvas<'_>, area: Rect, title: &str, reg: &mut Reg) {
    let p = c.p().clone();
    let rad = f32::from(p.radius);
    reg.add(area, &[Role::BgBase, Role::Border]);
    c.fill(area, rad, Role::BgBase);
    c.painter.rect_stroke(area, rad, Stroke::new(1.0, p.border), StrokeKind::Inside);
    // the header: rounded at the top only
    let head = r(area.min.x + 1.0, area.min.y + 1.0, area.width() - 2.0, 34.0);
    let clip = Canvas { painter: &c.painter.with_clip_rect(head), tokens: c.tokens.clone() };
    clip.fill(Rect::from_min_max(head.min, head.max + vec2(0.0, rad)), (rad - 1.0).max(0.0), Role::BgHeader);
    reg.add(head, &[Role::BgHeader]);
    c.painter.hline(head.x_range(), head.max.y, Stroke::new(1.0, p.border));
    let tick = r(head.min.x, head.max.y - 1.0, 36.0, 2.0);
    c.fill(tick, 0.0, Role::Primary);
    reg.add(tick.expand2(vec2(0.0, 2.0)), &[Role::Primary]);
    c.spaced(
        Rect::from_min_max(head.min + vec2(14.0, 0.0), head.max - vec2(30.0, 0.0)),
        &title.to_uppercase(),
        p.text_primary,
        10.0,
        0.16,
    );
    c.text(
        Rect::from_min_max(head.min, head.max - vec2(12.0, 0.0)),
        Align2::RIGHT_CENTER,
        "✕",
        p.text_secondary,
        11.0,
        Weight::Regular,
        false,
    );

    let (x, mut y, w) = (area.min.x + 14.0, head.max.y + 12.0, area.width() - 28.0);
    let card = r(x, y, w, 58.0);
    c.fill(card, (rad - 2.0).max(0.0), Role::BgCard);
    c.painter.rect_stroke(card, (rad - 2.0).max(0.0), Stroke::new(1.0, p.border), StrokeKind::Inside);
    reg.add(card, &[Role::BgCard, Role::Border]);
    reg.add(
        Rect::from_min_max(card.min + vec2(12.0, 8.0), card.max - vec2(80.0, 32.0)),
        &[Role::TextPrimary],
    );
    reg.add(
        Rect::from_min_max(card.min + vec2(12.0, 26.0), card.max - vec2(80.0, 16.0)),
        &[Role::TextSecondary],
    );
    reg.add(Rect::from_min_max(card.min + vec2(12.0, 40.0), card.max - vec2(80.0, 2.0)), &[Role::TextMuted]);
    c.text(
        Rect::from_min_max(card.min + vec2(12.0, 8.0), card.max - vec2(8.0, 32.0)),
        Align2::LEFT_CENTER,
        "Heading text",
        p.text_primary,
        12.5,
        Weight::SemiBold,
        false,
    );
    c.text(
        Rect::from_min_max(card.min + vec2(12.0, 26.0), card.max - vec2(8.0, 16.0)),
        Align2::LEFT_CENTER,
        "Secondary text",
        p.text_secondary,
        11.0,
        Weight::Regular,
        false,
    );
    c.text(
        Rect::from_min_max(card.min + vec2(12.0, 40.0), card.max - vec2(8.0, 2.0)),
        Align2::LEFT_CENTER,
        "Muted · hint",
        p.text_muted,
        10.5,
        Weight::Regular,
        false,
    );
    let badge = r(card.max.x - 62.0, card.min.y + 9.0, 52.0, 18.0);
    c.fill(badge, 9.0, Role::PrimaryMuted);
    reg.add(badge, &[Role::PrimaryMuted, Role::PrimaryText]);
    c.spaced(Rect::from_center_size(badge.center(), vec2(32.0, 18.0)), "TAG", p.primary_text, 9.0, 0.1);

    y = card.max.y + 10.0;
    let field = r(x, y, w * 0.62, 28.0);
    let small = (rad - 4.0).max(0.0);
    c.fill(field, small, Role::BgFrame);
    c.painter.rect_stroke(field, small, Stroke::new(1.0, p.primary), StrokeKind::Inside);
    reg.add(field, &[Role::BgFrame]);
    c.text(
        Rect::from_min_max(field.min + vec2(10.0, 0.0), field.max),
        Align2::LEFT_CENTER,
        "A field",
        p.text_primary,
        11.5,
        Weight::Regular,
        false,
    );
    let button = r(field.max.x + 8.0, y, w - field.width() - 8.0, 28.0);
    c.fill(button, small, Role::Primary);
    reg.add(button, &[Role::Primary]);
    reg.add(
        Rect::from_min_max(
            button.min + vec2(button.width() * 0.25, 6.0),
            button.max - vec2(button.width() * 0.25, 6.0),
        ),
        &[Role::OnPrimary],
    );
    c.text(button, Align2::CENTER_CENTER, "Button", p.on_primary, 11.5, Weight::SemiBold, false);

    y = button.max.y + 12.0;
    let tickbox = r(x, y + 2.0, 14.0, 14.0);
    c.fill(tickbox, 4.0, Role::Primary);
    c.text(tickbox, Align2::CENTER_CENTER, "✓", p.on_primary, 9.0, Weight::SemiBold, false);
    c.text(
        r(x + 22.0, y, w, 18.0),
        Align2::LEFT_CENTER,
        "A switch that is on",
        p.text_primary,
        11.5,
        Weight::Regular,
        false,
    );

    y += 28.0;
    let track = r(x, y, w, 4.0);
    c.painter.rect_filled(track, 2.0, p.bg_frame);
    c.painter.rect_stroke(track, 2.0, Stroke::new(1.0, p.border), StrokeKind::Inside);
    let done = r(x, y, w * 0.58, 4.0);
    c.fill(done, 2.0, Role::Primary);
    c.painter.circle(pos2(done.max.x, y + 2.0), 6.0, p.text_primary, Stroke::new(2.0, p.primary));
}

/// A notification card the way `ui/toast.py` draws it, in one of its styles (`glow`, `stripe`, `top`, `outline`).
pub fn paint_toast(c: &Canvas<'_>, area: Rect, title: &str, text: &str, style: &str) {
    let p = c.p().clone();
    crate::toast::paint_card(c.painter, area, &c.tokens, style, p.primary, 1.0);
    c.text(
        r(area.min.x + 40.0, area.min.y + 10.0, area.width() - 52.0, 18.0),
        Align2::LEFT_CENTER,
        title,
        p.text_primary,
        12.5,
        Weight::SemiBold,
        false,
    );
    let body = r(area.min.x + 40.0, area.min.y + 28.0, area.width() - 52.0, area.height() - 34.0);
    let job = Text::new(text).px(11.0).colour(p.text_secondary).wrap().job(&c.tokens);
    let mut job = job;
    job.wrap.max_width = body.width();
    let galley = c.painter.layout_job(job);
    c.painter.with_clip_rect(body).galley(body.min, galley, p.text_secondary);
    paint::sparkle(c.painter, pos2(area.min.x + 22.5, area.min.y + 19.5), 6.5, p.primary);
}

/// The big warning in the middle of the screen.
pub fn paint_alert(c: &Canvas<'_>, area: Rect) {
    let p = c.p().clone();
    let amber = Color32::from_rgb(0xF2, 0xB7, 0x05);
    c.fill(area, f32::from(p.radius) + 4.0, Role::BgBase);
    c.painter.rect_stroke(
        area.shrink(1.0),
        f32::from(p.radius) + 4.0,
        Stroke::new(2.0, amber),
        StrokeKind::Inside,
    );
    c.text(
        r(area.min.x + 20.0, area.min.y + 16.0, 40.0, 40.0),
        Align2::LEFT_CENTER,
        "⚠",
        amber,
        26.0,
        Weight::SemiBold,
        false,
    );
    c.text(
        r(area.min.x + 64.0, area.min.y + 18.0, area.width() - 80.0, 36.0),
        Align2::LEFT_CENTER,
        "Battery low",
        p.text_primary,
        18.0,
        Weight::SemiBold,
        false,
    );
    let body = r(area.min.x + 22.0, area.min.y + 62.0, area.width() - 44.0, 50.0);
    c.text(
        body,
        Align2::LEFT_TOP,
        "Plug the charger in before it switches off.",
        p.text_secondary,
        12.5,
        Weight::Regular,
        false,
    );
    let button = r(area.max.x - 116.0, area.min.y + 120.0, 96.0, 32.0);
    c.painter.rect_filled(button, (f32::from(p.radius) - 4.0).max(0.0), amber);
    c.text(
        button,
        Align2::CENTER_CENTER,
        "Got it",
        Color32::from_rgb(0x11, 0x11, 0x11),
        12.0,
        Weight::SemiBold,
        false,
    );
}

/// Luminescent's overlay and a panel in the given palette, `w` wide. Gives back the height used. With a role
/// to `highlight`, every place it is used gets a ring.
pub fn paint_luminescent(
    c: &Canvas<'_>,
    origin: Pos2,
    w: f32,
    highlight: Option<Role>,
    reg: &mut Reg,
) -> f32 {
    let h = (w * 0.6).min(230.0);
    paint_overlay(c, Rect::from_min_size(origin, vec2(w, h)), h / 112.0, reg);
    paint_panel(c, Rect::from_min_size(origin + vec2(0.0, h + 14.0), vec2(w, 250.0)), "A PANEL", reg);
    if let Some(role) = highlight {
        for rect in reg.rects(role) {
            let ring = rect.expand(2.0);
            c.painter.add(Shape::rect_stroke(
                ring,
                5.0,
                Stroke::new(4.0, Color32::from_black_alpha(170)),
                StrokeKind::Middle,
            ));
            c.painter.add(Shape::rect_stroke(ring, 5.0, Stroke::new(2.0, c.p().primary), StrokeKind::Middle));
        }
    }
    h + 14.0 + 250.0
}

/// The corner radius of an overlay mock-up in a card, for the clip (`radius - 4`).
pub fn card_radius(radius: u8) -> f32 {
    (f32::from(radius) - 4.0).max(0.0)
}
