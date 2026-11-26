//! The hotkey cheat sheet: every hotkey, grouped by module, over a dimmed screen (`ui/cheatsheet.py`).
//!
//! Hold mode (the default): it stays up while the hotkey's keys are held and goes when you let go; the hotkey
//! manager watches the keys, and only while it is up. Toggle mode: press again or Esc. Where the system can not
//! tell whether the keys are held (Hyprland only knows some keys), it works in toggle mode by itself. Modules
//! that are switched off are greyed out.

use egui::{
    Area, Context, Galley, Id, Key, Order, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, UiBuilder,
    ViewportBuilder, ViewportId, pos2, vec2,
};
use lum_platform::desktop::{self, APP_ID, marker};
use std::sync::Arc;

use crate::paint;
use crate::text::Text;
use crate::theme::{Role, Tokens, Tone, alpha};
use crate::window::Screen;

pub const COVER_PREFIX: &str = "Luminescent hotkeys";

/// One card: the module's name, whether it is on, and its hotkeys as (label, keys as they are written).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub name: String,
    pub on: bool,
    pub rows: Vec<(String, String)>,
}

#[derive(Debug, Default)]
pub struct CheatSheet {
    open: bool,
    /// Press again or Esc closes it, instead of letting go of the keys.
    pub toggle_mode: bool,
    groups: Vec<Group>,
    screens: Vec<Screen>,
    focus_pending: bool,
}

fn base_id() -> Id {
    Id::unique("lum-cheatsheet")
}

impl CheatSheet {
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Shows it on every screen, the cards on the first (primary) one.
    pub fn open(&mut self, groups: Vec<Group>, screens: Vec<Screen>, toggle_mode: bool) {
        self.groups = groups;
        self.screens = screens;
        self.toggle_mode = toggle_mode;
        self.open = true;
        self.focus_pending = toggle_mode;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.groups.clear();
    }

    /// The keys can not be watched after all: it stays until the hotkey is pressed again or Esc.
    pub fn switch_to_toggle(&mut self) {
        self.toggle_mode = true;
        self.focus_pending = true;
    }

    /// Draws it for this frame; gives back true when it was closed (Esc or a click in toggle mode).
    pub fn show(&mut self, ctx: &Context) -> bool {
        if !self.open {
            return false;
        }
        let mut close = false;
        if self.screens.is_empty() {
            self.screens.push(crate::window::fallback_screen(ctx));
        }
        if ctx.embed_viewports() {
            let rect = ctx.content_rect();
            Area::new(base_id().with("embedded")).order(Order::Foreground).fixed_pos(rect.min).show(
                ctx,
                |ui| {
                    let rect = Rect::from_min_size(ui.cursor().min, rect.size());
                    ui.allocate_rect(rect, Sense::hover());
                    let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
                    close |= self.draw(&mut child, true);
                },
            );
        } else {
            let focus_now = std::mem::take(&mut self.focus_pending);
            for i in 0..self.screens.len() {
                let screen = self.screens[i].clone();
                let primary = i == 0;
                let mut title = desktop::cover_title(COVER_PREFIX, &screen.name);
                // in hold mode it never takes the keyboard from the app you are in
                if !(self.toggle_mode && primary) {
                    title.push(marker::NO_ACTIVATE);
                }
                let builder = ViewportBuilder::default()
                    .with_title(title.clone())
                    .with_app_id(APP_ID)
                    .with_decorations(false)
                    .with_transparent(true)
                    .with_always_on_top()
                    .with_taskbar(false)
                    .with_resizable(false)
                    .with_active(self.toggle_mode && primary)
                    .with_position(screen.rect.min)
                    .with_inner_size(screen.rect.size());
                ctx.show_viewport_immediate(
                    ViewportId::from_hash_of(("lum-cheatsheet", &screen.name)),
                    builder,
                    |ui, _| {
                        let rect = ui.max_rect();
                        let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
                        close |= self.draw(&mut child, primary);
                    },
                );
                crate::window::shown(ctx, &title, None, self.toggle_mode && primary);
                if focus_now && primary {
                    crate::window::placer(ctx).focus(&title);
                }
            }
        }
        if close {
            self.close();
            ctx.request_repaint();
        }
        close
    }

    fn draw(&self, ui: &mut Ui, with_cards: bool) -> bool {
        let tokens = Tokens::of(ui.ctx());
        let rect = ui.max_rect();
        let painter = ui.painter();
        painter.rect_filled(rect, 0.0, alpha(tokens.scrim(), 0.64));
        paint::fill(painter, rect, 0.0, &tokens, Role::BgBase, 0.55);
        let closing = self.toggle_mode && {
            let click = ui.interact(rect, base_id().with("dim"), Sense::click()).clicked();
            click || ui.input(|i| i.key_pressed(Key::Escape))
        };
        if with_cards {
            self.cards(ui, rect, &tokens);
        }
        closing
    }

    fn cards(&self, ui: &Ui, rect: Rect, tokens: &Tokens) {
        let wide = rect.width() >= 1400.0;
        let side = if wide { 140.0 } else { 60.0 };
        let area = Rect::from_min_max(rect.min + vec2(side, 80.0), rect.max - vec2(side, 60.0));
        let painter = ui.painter();
        let p = &tokens.p;

        // the head: the mark, HOTKEYS, a rule, and how to close it
        let job = |t: Text| painter.layout_job(t.job(tokens));
        let heading = job(Text::new("HOTKEYS").px(13.0).semibold().mono().spacing(0.28));
        let how = if self.toggle_mode { "PRESS AGAIN OR ESC TO CLOSE" } else { "RELEASE TO CLOSE" };
        let how = job(Text::new(how).px(11.0).mono().spacing(0.06).tone(Tone::Muted));
        let mid = area.min.y + heading.size().y.max(16.0) / 2.0;
        paint::sparkle(painter, pos2(area.min.x + 8.0, mid), 16.0, p.primary);
        let heading_at = pos2(area.min.x + 32.0, mid - heading.size().y / 2.0);
        let rule_from = heading_at.x + heading.size().x + 16.0;
        let how_at = pos2(area.max.x - how.size().x, mid - how.size().y / 2.0);
        painter.galley(heading_at, heading, p.text_primary);
        painter.hline(rule_from..=how_at.x - 16.0, mid, Stroke::new(1.0, p.border));
        painter.galley(how_at, how, p.text_muted);

        let cols = if wide { 4 } else { 3 };
        let gap = 14.0;
        let card_w = ((area.width() - gap * (cols - 1) as f32) / cols as f32).floor();
        let mut y = mid + heading_height(tokens) / 2.0 + 36.0;
        for row in self.groups.chunks(cols) {
            let laid: Vec<LaidCard> = row.iter().map(|g| lay_out_card(ui, tokens, g, card_w)).collect();
            let height = laid.iter().map(|c| c.height).fold(0.0, f32::max);
            for (i, card) in laid.into_iter().enumerate() {
                let at = pos2(area.min.x + i as f32 * (card_w + gap), y);
                paint_card(ui, tokens, Rect::from_min_size(at, vec2(card_w, height)), card);
            }
            y += height + gap;
            if y > area.max.y {
                break;
            }
        }
    }
}

fn heading_height(tokens: &Tokens) -> f32 {
    tokens.fs(13.0) * 1.3
}

struct LaidCard {
    on: bool,
    title: Arc<Galley>,
    rows: Vec<(Arc<Galley>, Arc<Galley>)>,
    height: f32,
}

const CARD_PAD: (f32, f32) = (18.0, 16.0);

fn lay_out_card(ui: &Ui, tokens: &Tokens, group: &Group, width: f32) -> LaidCard {
    let painter = ui.painter();
    let on = group.on;
    let name = if on { group.name.to_uppercase() } else { format!("{}  (OFF)", group.name.to_uppercase()) };
    let title = painter.layout_job(
        Text::new(name)
            .px(10.5)
            .semibold()
            .mono()
            .spacing(0.14)
            .tone(if on { Tone::Primary } else { Tone::Muted })
            .job(tokens),
    );
    let os = lum_platform::hotkeys::Os::current();
    let inner = width - 2.0 * CARD_PAD.0;
    let mut height = CARD_PAD.1 + title.size().y + 12.0;
    let mut rows = Vec::new();
    for (label, combo) in &group.rows {
        let keys = painter.layout_job(
            Text::new(lum_platform::hotkeys::pretty(combo, os))
                .px(11.0)
                .mono()
                .tone(if on { Tone::Primary } else { Tone::Muted })
                .job(tokens),
        );
        let mut job =
            Text::new(label).px(13.0).tone(if on { Tone::Secondary } else { Tone::Muted }).job(tokens);
        job.wrap.max_width = (inner - keys.size().x - 10.0).max(40.0);
        let label = painter.layout_job(job);
        height += label.size().y.max(keys.size().y) + 8.0;
        rows.push((label, keys));
    }
    height += CARD_PAD.1 - 8.0;
    LaidCard { on, title, rows, height }
}

fn paint_card(ui: &Ui, tokens: &Tokens, rect: Rect, card: LaidCard) {
    let painter = ui.painter();
    let p = &tokens.p;
    paint::fill(painter, rect, tokens.r(), tokens, Role::BgCard, 1.0);
    painter.rect_stroke(rect, tokens.r(), Stroke::new(1.0, p.border), StrokeKind::Inside);
    let mut at = rect.min + vec2(CARD_PAD.0, CARD_PAD.1);
    let tick_mid = at.y + card.title.size().y / 2.0;
    let tick = Rect::from_min_size(pos2(at.x, tick_mid - 5.5), vec2(3.0, 11.0));
    painter.rect_filled(tick, 1.0, if card.on { p.primary } else { p.text_muted });
    painter.galley(pos2(at.x + 11.0, at.y), card.title.clone(), p.text_primary);
    at.y += card.title.size().y + 12.0;
    let right = rect.max.x - CARD_PAD.0;
    for (label, keys) in card.rows {
        let h = label.size().y.max(keys.size().y);
        painter.galley(at, label, p.text_secondary);
        painter.galley(Pos2::new(right - keys.size().x, at.y), keys, p.text_primary);
        at.y += h + 8.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_mode_takes_the_keyboard_and_hold_mode_does_not() {
        let mut sheet = CheatSheet::default();
        sheet.open(Vec::new(), Vec::new(), false);
        assert!(sheet.is_open() && !sheet.focus_pending);
        sheet.switch_to_toggle();
        assert!(sheet.toggle_mode && sheet.focus_pending);
        sheet.close();
        assert!(!sheet.is_open());
    }
}
