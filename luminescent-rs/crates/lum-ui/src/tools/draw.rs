//! Small painters the tools' pages share: one-line text cut to a width, the mono capitals used for headings and
//! the cards the pages are built of.

use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, Galley, Rect, Stroke, StrokeKind, Ui};

use crate::fonts::Weight;
use crate::theme::Tokens;

/// One line of text, cut with "…" when it does not fit `width`.
pub fn line(
    ui: &Ui,
    text: &str,
    px: f32,
    weight: Weight,
    mono: bool,
    colour: Color32,
    width: f32,
) -> Arc<Galley> {
    let tokens = Tokens::of(ui.ctx());
    let mut job = LayoutJob::single_section(
        text.to_owned(),
        TextFormat { font_id: tokens.font(px, weight, mono), color: colour, ..Default::default() },
    );
    job.wrap.max_width = width.max(8.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    ui.painter().layout_job(job)
}

/// Text that wraps onto as many lines as it needs within `width`.
pub fn wrapped(ui: &Ui, text: &str, px: f32, colour: Color32, width: f32) -> Arc<Galley> {
    let tokens = Tokens::of(ui.ctx());
    let mut job = LayoutJob::single_section(
        text.to_owned(),
        TextFormat { font_id: tokens.font(px, Weight::Regular, false), color: colour, ..Default::default() },
    );
    job.wrap.max_width = width.max(8.0);
    ui.painter().layout_job(job)
}

/// The mono capitals over a column or a box ("MOST USED", "LAST 30 DAYS").
pub fn caps(ui: &Ui, text: &str, px: f32, colour: Color32) -> Arc<Galley> {
    let tokens = Tokens::of(ui.ctx());
    let mut job = LayoutJob::single_section(
        text.to_uppercase(),
        TextFormat {
            font_id: tokens.font(px, Weight::SemiBold, true),
            color: colour,
            extra_letter_spacing: px * 0.12,
            ..Default::default()
        },
    );
    job.wrap.max_rows = 1;
    ui.painter().layout_job(job)
}

/// A card: the theme's card colour, its hairline and its corner radius.
pub fn card(ui: &Ui, rect: Rect, tokens: &Tokens) {
    ui.painter().rect_filled(rect, tokens.r(), tokens.p.bg_card);
    ui.painter().rect_stroke(rect, tokens.r(), Stroke::new(1.0, tokens.p.border), StrokeKind::Inside);
}

/// `#rgb` or `#rrggbb` as a colour.
pub fn hex(text: &str) -> Option<Color32> {
    let hex = text.strip_prefix('#')?;
    let n = u32::from_str_radix(hex, 16).ok()?;
    match hex.len() {
        6 => Some(Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8)),
        3 => {
            let (r, g, b) = ((n >> 8) as u8 & 15, (n >> 4) as u8 & 15, n as u8 & 15);
            Some(Color32::from_rgb(r * 17, g * 17, b * 17))
        }
        _ => None,
    }
}
