//! A big warning in the middle of the screen, above everything (`ui/alert.py`): the Automation "Warning on
//! screen" node, and "Show a notification" set to "A warning in the middle". Unlike a card it stays until you
//! click "Got it" (or press Enter, Esc or Space), unless it was given a time to close by itself.
//!
//! A new warning with the same title replaces the old one, so a node that fires every few seconds does not pile
//! them up.

use egui::{
    Align, Area, Color32, Context, Id, Key, Layout, Order, Rect, ScrollArea, Sense, Stroke, StrokeKind, Ui,
    UiBuilder, ViewportBuilder, ViewportId, pos2, vec2,
};
use lum_platform::desktop::APP_ID;

use crate::anim::{self, Ease, Kind};
use crate::fonts::Weight;
use crate::paint;
use crate::text::Text;
use crate::theme::{DANGER, OK, Role, Tokens, Tone};

pub const LEVELS: &[(&str, &str)] = &[
    ("warning", "Warning (amber)"),
    ("danger", "Danger (red)"),
    ("info", "Info (theme colour)"),
    ("ok", "Good news (green)"),
];

const AMBER: Color32 = Color32::from_rgb(0xF2, 0xB7, 0x05);

fn colour(level: &str, tokens: &Tokens) -> Color32 {
    match level {
        "warning" => AMBER,
        "danger" => DANGER,
        "ok" => OK,
        _ => tokens.p.primary,
    }
}

fn icon(level: &str) -> &'static str {
    match level {
        "danger" => "⛔",
        "info" => "✦",
        "ok" => "✔",
        _ => "⚠",
    }
}

#[derive(Debug, Clone)]
struct Alert {
    serial: u64,
    title: String,
    message: String,
    level: String,
    /// When it closes by itself (`ctx` time), set on its first frame from `seconds`.
    closes_at: Option<f64>,
    seconds: f64,
}

#[derive(Debug, Default)]
pub struct Alerts {
    open: Vec<Alert>,
    serial: u64,
    /// The screen they open on (the one with the mouse), in desktop coordinates.
    screen: Option<Rect>,
}

fn base_id() -> Id {
    Id::unique("lum-alerts")
}

impl Alerts {
    pub fn is_empty(&self) -> bool {
        self.open.is_empty()
    }

    /// Shows a warning; one with the same title goes first. `seconds` 0 keeps it until Got it.
    pub fn show_alert(
        &mut self,
        title: &str,
        message: &str,
        level: &str,
        seconds: f64,
        screen: Option<Rect>,
    ) {
        self.open.retain(|a| a.title != title);
        self.serial += 1;
        self.screen = screen.or(self.screen);
        self.open.push(Alert {
            serial: self.serial,
            title: title.to_owned(),
            message: message.chars().take(20_000).collect(),
            level: level.to_owned(),
            closes_at: None,
            seconds,
        });
    }

    /// Draws the warnings for this frame. Gives back the titles of the ones that closed.
    pub fn show(&mut self, ctx: &Context) -> Vec<String> {
        let mut closed = Vec::new();
        if self.open.is_empty() {
            return closed;
        }
        let base = Tokens::of(ctx);
        // Themes → Settings… → Warnings on screen
        let tokens = base.for_section("alerts").unwrap_or_else(|| (*base).clone());
        let screen = self.screen.unwrap_or_else(|| crate::window::fallback_screen(ctx).rect);
        let now = ctx.input(|i| i.time);
        let width = 640.0_f32.min((screen.width() / 3.0).max(420.0)).floor();
        for alert in &mut self.open {
            if alert.closes_at.is_none() && alert.seconds > 0.0 {
                alert.closes_at = Some(now + alert.seconds);
            }
            let id = base_id().with(alert.serial);
            let height = ctx.data(|d| d.get_temp::<f32>(id.with("height"))).unwrap_or(260.0);
            let at =
                pos2((screen.center().x - width / 2.0).round(), (screen.center().y - height / 2.0).round());
            let mut done = false;
            let mut measured = height;
            if ctx.embed_viewports() {
                let origin = ctx.content_rect().min + (at - screen.min);
                Area::new(id).order(Order::Foreground).fixed_pos(origin).show(ctx, |ui| {
                    let mut child = ui.new_child(
                        UiBuilder::new().max_rect(Rect::from_min_size(ui.cursor().min, vec2(width, 4000.0))),
                    );
                    (done, measured) = draw(&mut child, alert, &tokens, now, screen.height());
                    ui.allocate_rect(
                        Rect::from_min_size(ui.cursor().min, vec2(width, measured)),
                        Sense::hover(),
                    );
                });
            } else {
                let title = format!("Luminescent warning {}", alert.serial);
                let builder = ViewportBuilder::default()
                    .with_title(title.clone())
                    .with_app_id(APP_ID)
                    .with_decorations(false)
                    .with_transparent(true)
                    .with_always_on_top()
                    .with_taskbar(false)
                    .with_resizable(false)
                    .with_active(true)
                    .with_position(at)
                    .with_inner_size(vec2(width, height));
                ctx.show_viewport_immediate(ViewportId::from_hash_of(id), builder, |ui, _| {
                    let rect = ui.max_rect();
                    let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
                    (done, measured) = draw(&mut child, alert, &tokens, now, screen.height());
                });
                crate::window::shown(ctx, &title, Some(at), true);
                if (measured - height).abs() > 0.5 {
                    ctx.send_viewport_cmd_to(
                        ViewportId::from_hash_of(id),
                        egui::ViewportCommand::InnerSize(vec2(width, measured)),
                    );
                }
            }
            ctx.data_mut(|d| d.insert_temp(id.with("height"), measured));
            if done {
                closed.push(alert.title.clone());
            } else if let Some(at) = alert.closes_at {
                // the count down shows whole seconds
                let left = at - now;
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(left.fract().max(0.05)));
            }
        }
        if !closed.is_empty() {
            self.open.retain(|a| !closed.contains(&a.title));
            ctx.request_repaint();
        }
        closed
    }
}

/// The warning's box; gives back whether it was closed and how tall it is.
fn draw(ui: &mut Ui, alert: &Alert, tokens: &Tokens, now: f64, screen_h: f32) -> (bool, f32) {
    let c = colour(&alert.level, tokens);
    let fade = anim::value_from(
        ui.ctx(),
        base_id().with(("in", alert.serial)),
        0.0,
        1.0,
        Kind::Panels,
        160.0,
        Ease::Out,
    );
    ui.multiply_opacity(fade);
    let outer = ui.max_rect();
    let width = outer.width();
    let background = ui.painter().add(egui::Shape::Noop);
    let inner = Rect::from_min_max(outer.min + vec2(34.0, 28.0), pos2(outer.max.x - 34.0, outer.max.y));
    let mut body = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Min)));
    body.spacing_mut().item_spacing.y = 12.0;
    body.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 14.0;
        ui.label(
            egui::RichText::new(icon(&alert.level)).font(tokens.font(34.0, Weight::SemiBold, false)).color(c),
        );
        let mut job = Text::new(&alert.title).px(24.0).semibold().job(tokens);
        job.wrap.max_width = ui.available_width();
        ui.add(egui::Label::new(job).wrap());
    });
    if !alert.message.is_empty() {
        // the whole message, however long: it scrolls once it would be taller than about half the screen
        let job = Text::new(&alert.message).px(16.0).tone(Tone::Secondary).job(tokens);
        ScrollArea::vertical().max_height(screen_h * 0.5).auto_shrink([false, true]).show(&mut body, |ui| {
            ui.add(egui::Label::new(job).wrap().selectable(true));
        });
    }
    let mut close = false;
    body.horizontal(|ui| {
        if let Some(at) = alert.closes_at {
            let left = (at - now).ceil().max(0.0);
            if left <= 0.0 {
                close = true;
            }
            ui.add(Text::new(format!("closes in {left:.0} s")).px(12.0).tone(Tone::Muted));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let text = egui::RichText::new("Got it")
                .font(tokens.font(14.0, Weight::SemiBold, false))
                .color(Color32::from_rgb(0x11, 0x11, 0x11));
            let button = egui::Button::new(text)
                .fill(c)
                .stroke(Stroke::NONE)
                .corner_radius(tokens.rs())
                .min_size(vec2(120.0, 36.0));
            if ui.add(button).clicked() {
                close = true;
            }
        });
    });
    let height = (body.min_rect().max.y - outer.min.y + 24.0).ceil();
    let frame = Rect::from_min_size(outer.min, vec2(width, height));
    let radius = tokens.r() + 4.0;
    ui.painter().set(
        background,
        egui::Shape::Vec(vec![
            paint::fill_shape(ui.ctx(), frame, radius, tokens, Role::BgBase, 1.0),
            egui::Shape::rect_stroke(frame.shrink(1.0), radius, Stroke::new(2.0, c), StrokeKind::Inside),
        ]),
    );
    let keys = ui.input(|i| [Key::Enter, Key::Escape, Key::Space].iter().any(|k| i.key_pressed(*k)));
    (close || keys, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_warning_with_the_same_title_replaces_the_old_one() {
        let mut alerts = Alerts::default();
        alerts.show_alert("CPU is hot", "92 °C", "warning", 0.0, None);
        alerts.show_alert("Disk", "", "info", 0.0, None);
        alerts.show_alert("CPU is hot", "95 °C", "danger", 0.0, None);
        assert_eq!(alerts.open.len(), 2);
        assert_eq!(alerts.open[1].message, "95 °C");
        assert_eq!(icon("danger"), "⛔");
        assert_eq!(colour("ok", &Tokens::dark()), OK);
    }
}
