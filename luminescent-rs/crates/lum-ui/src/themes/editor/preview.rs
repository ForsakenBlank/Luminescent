//! The right-hand column: what the part being edited looks like, drawn live in the theme as it is being
//! edited. On the Colours page a click on the picture finds that colour's row.

use egui::{Align2, Rect, Sense, Ui, pos2, vec2};
use lum_theme::Role;

use super::{Page, ThemeEditor};
use crate::fonts::Weight;
use crate::text::caps;
use crate::theme::{Palette, Tokens, Tone};
use crate::themes::Env;
use crate::themes::previews::{
    Canvas, Reg, paint_alert, paint_luminescent, paint_overlay, paint_panel, paint_toast,
};

/// What each colour is used for, shown when the pointer is on it in the picture.
pub(super) fn role_help(role: Role) -> &'static str {
    match role {
        Role::Primary => "Buttons, switches, sliders, the page switcher and accent lines.",
        Role::PrimaryMuted => "Behind selected rows, tags and ticked buttons.",
        Role::PrimaryText => "Links, counts and other accent-coloured text.",
        Role::OnPrimary => "Text on accent-coloured buttons.",
        Role::BgBase => "Panels, pages, menus and dialogs.",
        Role::BgCard => "Cards, buttons and list boxes.",
        Role::BgFrame => "Text fields, the settings sidebar and other insets.",
        Role::BgHeader => "Panel headers and footers.",
        _ => "",
    }
}

fn height_of(page: &Page, w: f32) -> f32 {
    match page {
        Page::Colours => (w * 0.6).min(230.0) + 14.0 + 250.0,
        Page::General | Page::Applied => (w * 0.6).min(260.0) + 16.0 + 250.0,
        Page::Desktop(_) => 0.0,
        Page::Section(key) if key == "toasts" => 168.0,
        Page::Section(key) if key == "alerts" => 170.0,
        Page::Section(_) => 276.0 + 22.0 + (w * 0.45).min(170.0),
    }
}

impl ThemeEditor {
    pub(super) fn preview(&mut self, ui: &mut Ui, env: &Env<'_>) {
        let tokens = Tokens::of(ui.ctx());
        ui.add(caps("PREVIEW").px(10.5).semibold().spacing(0.14).tone(Tone::Muted));
        ui.add_space(8.0);
        let width = ui.available_width();
        let height = height_of(&self.page, width);
        if height <= 0.0 {
            return;
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("theme-editor-preview").show(
            ui,
            |ui| {
                let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
                let painter = ui.painter().with_clip_rect(rect);
                let base = self.ui_palette();
                let mut reg = Reg::default();
                let at = rect.min;
                match self.page.clone() {
                    Page::Colours => {
                        let lit = self.hover.or(self.flash.map(|f| f.0));
                        paint_luminescent(&Canvas::new(&painter, &base), at, width, lit, &mut reg);
                    }
                    Page::General | Page::Applied => {
                        let canvas = Canvas::new(&painter, &base);
                        let h = (width * 0.6).min(260.0);
                        paint_overlay(&canvas, Rect::from_min_size(at, vec2(width, h)), h / 112.0, &mut reg);
                        let panel = Rect::from_min_size(at + vec2(0.0, h + 16.0), vec2(width, 250.0));
                        paint_panel(&canvas, panel, "A PANEL", &mut reg);
                    }
                    Page::Desktop(_) => {}
                    Page::Section(key) => self.paint_section(&painter, rect, &key, &base, env, &tokens),
                }
                self.point_at_preview(ui, &response, &reg);
                self.reg = reg;
            },
        );
    }

    fn paint_section(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        key: &str,
        base: &Palette,
        env: &Env<'_>,
        tokens: &Tokens,
    ) {
        let section = self.section(key);
        let own = Palette::from(&self.palette().with_section(&section));
        let canvas = Canvas::new(painter, &own);
        let (at, w) = (rect.min, rect.width());
        let mut unused = Reg::default();
        match key {
            "toasts" => {
                let style = env.toast_style.as_str();
                let first = Rect::from_min_size(at, vec2(w, 64.0));
                paint_toast(&canvas, first, "Luminescent", "Notes copied to the clipboard.", style);
                let second = Rect::from_min_size(at + vec2(0.0, 76.0), vec2(w, 92.0));
                let long = "You have used 80 % of today's safe listening allowance. A longer message wraps onto more lines and the card grows with it.";
                paint_toast(&canvas, second, "Hearing Guard", long, style);
            }
            "alerts" => paint_alert(&canvas, Rect::from_min_size(at, vec2(w, 170.0))),
            _ => {
                let panel = Rect::from_min_size(at, vec2(w, 260.0));
                paint_panel(&canvas, panel, &self.section_title(key), &mut unused);
                let y = at.y + 276.0;
                painter.text(
                    pos2(at.x, y + 8.0),
                    Align2::LEFT_CENTER,
                    "THE REST OF THE APP",
                    tokens.font(9.5, Weight::SemiBold, true),
                    base.text_muted,
                );
                let h = (w * 0.45).min(170.0);
                let rest = Canvas::new(painter, base);
                paint_overlay(
                    &rest,
                    Rect::from_min_size(pos2(at.x, y + 22.0), vec2(w, h)),
                    h / 112.0,
                    &mut unused,
                );
            }
        }
    }

    /// The colours page: the pointer on something shows what it is, and a click goes to its row.
    fn point_at_preview(&mut self, ui: &Ui, response: &egui::Response, reg: &Reg) {
        if self.page != Page::Colours {
            return;
        }
        let Some(pos) = response.hover_pos() else { return };
        let Some(role) = reg.at(pos) else { return };
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        self.hover = Some(role);
        let text = format!("{}\n{}\nClick to find it on the left", role.label(), role_help(role));
        response.clone().on_hover_text(text);
        if response.clicked() {
            self.jump = Some(role);
        }
    }
}
