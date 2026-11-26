//! The page switcher hanging from the top-centre edge of the screen: a 2 px accent line over a row of pills
//! (Dashboard · Themes · Clipboard · Settings, in the order Settings gives), with a marker that slides to the
//! page you pick.

use egui::{Color32, CornerRadius, CursorIcon, Id, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use crate::anim::{self, Ease, Kind};
use crate::fonts::Weight;
use crate::paint;
use crate::theme::{Role, Tokens};

const PAD: (f32, f32, f32, f32) = (6.0, 6.0, 6.0, 7.0);
const GAP: f32 = 2.0;
const BUTTON_PAD: (f32, f32) = (20.0, 9.0);
const PILL_RADIUS: f32 = 16.0;

fn marker_ids(base: Id) -> (Id, Id) {
    (base.with("marker-x"), base.with("marker-w"))
}

/// Makes the marker jump to its place on the next frame instead of sliding there (the overlay just opened).
pub fn snap(ui_ctx: &egui::Context, base: Id) {
    let (x, w) = marker_ids(base);
    anim::forget(ui_ctx, x);
    anim::forget(ui_ctx, w);
}

/// Draws the switcher with its top edge at `top`, centred in `screen`. `pages` is (key, name, tooltip).
/// Gives back the page clicked, if one was, and the switcher's rect.
pub fn show(
    ui: &mut Ui,
    base: Id,
    screen: Rect,
    top: f32,
    pages: &[(String, String, String)],
    current: &str,
) -> (Option<String>, Rect) {
    let tokens = Tokens::of(ui.ctx());
    let p = &tokens.p;
    let font = tokens.font(14.0, Weight::Medium, false);
    let galleys: Vec<_> = pages
        .iter()
        .map(|(_, name, _)| ui.painter().layout_no_wrap(name.clone(), font.clone(), Color32::PLACEHOLDER))
        .collect();
    let height = galleys.iter().map(|g| g.size().y).fold(0.0, f32::max) + 2.0 * BUTTON_PAD.1;
    let widths: Vec<f32> = galleys.iter().map(|g| g.size().x + 2.0 * BUTTON_PAD.0).collect();
    let inner = widths.iter().sum::<f32>() + GAP * widths.len().saturating_sub(1) as f32;
    let nav_size = vec2(inner + PAD.0 + PAD.2, height + PAD.1 + PAD.3);
    let left = (screen.center().x - nav_size.x / 2.0).round();
    let line = Rect::from_min_size(pos2(left, top), vec2(nav_size.x, 2.0));
    let nav = Rect::from_min_size(pos2(left, line.max.y), nav_size);
    let dock = line.union(nav);

    let painter = ui.painter().clone();
    let bottom_round = CornerRadius { nw: 0, ne: 0, sw: PILL_RADIUS as u8, se: PILL_RADIUS as u8 };
    painter.rect_filled(nav, bottom_round, p.bg_base);
    // the border runs down the sides and along the bottom, not across the top
    painter.with_clip_rect(Rect::from_min_max(nav.min + vec2(0.0, 1.0), nav.max)).rect_stroke(
        nav,
        bottom_round,
        Stroke::new(1.0, p.border),
        StrokeKind::Inside,
    );
    paint::fill(&painter, line, 0.0, &tokens, Role::Primary, 1.0);
    // a click between the pills is not a click on the dimmed screen
    ui.interact(dock, base.with("dock"), Sense::click());

    let mut rects = Vec::with_capacity(pages.len());
    let mut x = nav.min.x + PAD.0;
    for w in &widths {
        rects.push(Rect::from_min_size(pos2(x, nav.min.y + PAD.1), vec2(*w, height)));
        x += w + GAP;
    }
    if let Some(i) = pages.iter().position(|(key, _, _)| key == current) {
        let (xid, wid) = marker_ids(base);
        let target = rects[i];
        let mx = anim::value(ui.ctx(), xid, target.min.x, Kind::Nav, 230.0, Ease::Move);
        let mw = anim::value(ui.ctx(), wid, target.width(), Kind::Nav, 230.0, Ease::Move);
        let marker = Rect::from_min_size(pos2(mx, target.min.y), vec2(mw.max(0.0), target.height()));
        paint::fill(&painter, marker, PILL_RADIUS, &tokens, Role::Primary, 1.0);
    }

    let mut clicked = None;
    for (((key, name, tip), rect), galley) in pages.iter().zip(&rects).zip(galleys) {
        let response = ui.interact(*rect, base.with(("page", key)), Sense::click());
        let checked = key == current;
        response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, checked, name));
        if response.hovered() && !checked {
            painter.rect_filled(*rect, PILL_RADIUS, p.bg_card_hover);
        }
        let colour = if checked {
            p.on_primary
        } else if response.hovered() {
            p.text_primary
        } else {
            p.text_secondary
        };
        painter.galley(rect.center() - galley.size() / 2.0, galley, colour);
        let response = if tip.is_empty() { response } else { response.on_hover_text(tip) };
        if response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if response.clicked() {
            clicked = Some(key.clone());
        }
    }
    (clicked, dock)
}
