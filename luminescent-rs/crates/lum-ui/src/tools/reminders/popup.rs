//! The pop-up a reminder shows when it goes off: a card in the theme's colours with the reminder, when it
//! repeats, and Snooze (pick how long) and Done. It rises into place while it fades in, never takes the keyboard
//! from what you are doing, and runs out by itself when the settings say so.

use std::time::Instant;

use egui::{
    Area, Context, Id, Order, Pos2, Rect, Sense, Stroke, UiBuilder, ViewportBuilder, ViewportId, pos2, vec2,
};
use lum_platform::desktop::{APP_ID, marker};
use serde_json::Value;

use crate::anim::{self, Ease, Kind};
use crate::paint;
use crate::text::Text;
use crate::theme::{Tokens, Tone, alpha};
use crate::toast::paint_card;
use crate::widgets::{Button, IconButton, Variant, menu};
use lum_calc::reminder::{SNOOZE_CHOICES, describe};

pub const WIDTH: f32 = 380.0;
const RISE: f32 = 16.0;

/// What a click on the pop-up asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Done,
    Snooze(i64),
}

#[derive(Debug)]
pub struct Popup {
    pub serial: u64,
    pub rem: Value,
    pub snooze_minutes: i64,
    pub timeout_s: u64,
    /// Where it sits on its screen: one of the nine anchor keys.
    pub anchor: String,
    pub screen: Rect,
    /// "14:30": when it went off.
    pub time: String,
    started: Instant,
    /// Its height on the last frame, for stacking the others and sizing its own window.
    pub height: f32,
}

impl Popup {
    pub fn new(
        serial: u64,
        rem: Value,
        snooze_minutes: i64,
        timeout_s: u64,
        anchor: &str,
        screen: Rect,
        time: &str,
    ) -> Popup {
        Popup {
            serial,
            rem,
            snooze_minutes,
            timeout_s,
            anchor: anchor.to_owned(),
            screen,
            time: time.to_owned(),
            started: Instant::now(),
            height: 150.0,
        }
    }

    fn id(&self) -> Id {
        Id::unique(("lum-reminder-popup", self.serial))
    }

    /// Bottom anchors stack upwards and rise into place; the others stack downwards and stay put.
    pub fn at_bottom(&self) -> bool {
        self.anchor.starts_with('b')
    }

    /// How much of the auto-close time is left, 1 down to 0. `None` when it stays until dismissed.
    pub fn left(&self) -> Option<f32> {
        (self.timeout_s > 0).then(|| 1.0 - self.started.elapsed().as_secs_f32() / self.timeout_s as f32)
    }

    /// Where its window goes, `offset` below or above the edge for the popups that came before it.
    pub fn place(&self, offset: f32) -> Pos2 {
        let (g, w, h) = (self.screen, WIDTH, self.height);
        let margin = 16.0;
        let x = match self.anchor.as_bytes().get(1) {
            Some(b'l') => g.min.x + margin,
            Some(b'c') => g.min.x + (g.width() - w) / 2.0,
            _ => g.max.x - w - margin,
        };
        let y = match self.anchor.as_bytes().first() {
            Some(b't') => g.min.y + margin + offset,
            Some(b'm') => g.min.y + (g.height() - h) / 2.0,
            _ => g.max.y - h - margin - offset,
        };
        pos2(x.round(), y.round())
    }

    /// Draws it for this frame at `at`. Gives back what was clicked.
    pub fn show(&mut self, ctx: &Context, at: Pos2) -> Option<Answer> {
        let tokens = Tokens::of(ctx);
        let toasts = tokens.for_section("toasts").unwrap_or_else(|| (*tokens).clone());
        let id = self.id();
        let fade = anim::value_from(ctx, id.with("in"), 0.0, 1.0, Kind::Panels, 200.0, Ease::Out);
        let rise = anim::value_from(ctx, id.with("rise"), RISE, 0.0, Kind::Panels, 280.0, Ease::Move);
        let size = vec2(WIDTH, self.height + RISE);
        let mut answer = None;
        let mut height = self.height;
        let draw = |ui: &mut egui::Ui, popup: &Popup, answer: &mut Option<Answer>, height: &mut f32| {
            ui.multiply_opacity(fade);
            // it rises into place from below; the window has room for that under the card
            let top = if popup.at_bottom() { rise } else { 0.0 };
            let rect = Rect::from_min_size(ui.max_rect().min + vec2(0.0, top), vec2(WIDTH, popup.height));
            paint_card(ui.painter(), rect, &toasts, "glow", toasts.p.primary, 1.0);
            if let Some(left) = popup.left() {
                let bar = Rect::from_min_size(pos2(rect.min.x, rect.max.y - 3.0), vec2(rect.width(), 3.0));
                let clip = ui.painter().with_clip_rect(rect.shrink(0.5));
                clip.rect_filled(bar, 0.0, alpha(toasts.p.primary, 0.15));
                let kept = Rect::from_min_size(bar.min, vec2(bar.width() * left.clamp(0.0, 1.0), 3.0));
                clip.rect_filled(kept, 0.0, toasts.p.primary);
            }
            *height = content(ui, popup, rect, &toasts, answer);
        };
        if ctx.embed_viewports() {
            Area::new(id).order(Order::Foreground).fixed_pos(at).show(ctx, |ui| {
                ui.allocate_rect(Rect::from_min_size(ui.cursor().min, size), Sense::hover());
                let area = Rect::from_min_size(ui.min_rect().min, size);
                let mut child = ui.new_child(UiBuilder::new().max_rect(area));
                draw(&mut child, self, &mut answer, &mut height);
            });
        } else {
            let title = format!("Luminescent reminder{}", marker::NO_ACTIVATE);
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
                ViewportId::from_hash_of(("lum-reminder", self.serial)),
                builder,
                |ui, _| {
                    let area = ui.max_rect();
                    let mut child = ui.new_child(UiBuilder::new().max_rect(area));
                    draw(&mut child, self, &mut answer, &mut height);
                },
            );
            crate::window::shown(ctx, &title, Some(at), false);
        }
        self.height = height;
        if self.left().is_some_and(|l| l <= 0.0) && answer.is_none() {
            answer = Some(Answer::Done);
        }
        if self.left().is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        answer
    }
}

/// The card's content; gives back the height it needs.
fn content(
    ui: &mut egui::Ui,
    popup: &Popup,
    card: Rect,
    tokens: &Tokens,
    answer: &mut Option<Answer>,
) -> f32 {
    let p = &tokens.p;
    let inner = Rect::from_min_max(card.min + vec2(16.0, 14.0), pos2(card.max.x - 12.0, card.max.y));
    let mut ui =
        ui.new_child(UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)));
    ui.spacing_mut().item_spacing = vec2(0.0, 12.0);
    let title =
        popup.rem.get("title").and_then(Value::as_str).filter(|t| !t.is_empty()).unwrap_or("Reminder");
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        let (badge, _) = ui.allocate_exact_size(vec2(38.0, 38.0), Sense::hover());
        ui.painter().circle_filled(badge.center(), 19.0, alpha(p.primary, 0.16));
        ui.painter().circle_stroke(badge.center(), 18.5, Stroke::new(1.0, alpha(p.primary, 0.35)));
        paint::sparkle(ui.painter(), badge.center(), 9.0, p.primary);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            ui.set_width(WIDTH - 16.0 - 12.0 - 38.0 - 12.0 - 24.0 - 12.0);
            ui.add(
                Text::new(format!("REMINDER  \u{b7}  {}", popup.time))
                    .px(9.5)
                    .semibold()
                    .mono()
                    .spacing(0.14)
                    .tone(Tone::Accent),
            );
            ui.add(egui::Label::new(Text::new(title).px(15.5).semibold().job(tokens)).wrap());
        });
        if ui.add(IconButton::close().size(24.0).tooltip("Close")).clicked() {
            *answer = Some(Answer::Done);
        }
    });
    let indent = 50.0;
    let message = popup.rem.get("message").and_then(Value::as_str).unwrap_or_default().trim();
    if !message.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(indent);
            ui.add(
                egui::Label::new(Text::new(message).px(13.0).tone(Tone::Secondary).job(tokens))
                    .wrap()
                    .selectable(true),
            );
        });
    }
    let once = popup.rem["schedule"].get("type").and_then(Value::as_str) == Some("once");
    let repeat = describe(&popup.rem);
    if !once && !repeat.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(indent);
            ui.add(Text::new(format!("\u{21bb}  {repeat}")).px(11.0).medium().tone(Tone::Muted));
        });
    }
    ui.add_space(-6.0);
    ui.horizontal(|ui| {
        ui.add_space(indent);
        ui.spacing_mut().item_spacing.x = 8.0;
        let room = ui.available_width() - 34.0 - 8.0 * 2.0 - 4.0;
        let wide = (room / 2.0).floor();
        let snooze = ui.add_sized(
            vec2(wide, 32.0),
            Button::new(format!("Snooze {} min", popup.snooze_minutes)).variant(Variant::Outline),
        );
        if snooze.clicked() {
            *answer = Some(Answer::Snooze(popup.snooze_minutes));
        }
        let more = ui.add_sized(vec2(34.0, 32.0), Button::new("\u{25be}").variant(Variant::Outline));
        egui::Popup::menu(&more).show(|ui| {
            for n in SNOOZE_CHOICES {
                let label = if n < 60 { format!("{n} minutes") } else { "1 hour".to_owned() };
                if menu::item(ui, &label).clicked() {
                    *answer = Some(Answer::Snooze(n));
                }
            }
        });
        if ui.add_sized(vec2(wide, 32.0), Button::primary("Done")).clicked() {
            *answer = Some(Answer::Done);
        }
    });
    ui.add_space(4.0);
    (ui.min_rect().max.y - card.min.y + 12.0).max(80.0)
}
