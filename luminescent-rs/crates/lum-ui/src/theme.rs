//! The palette and the design tokens every widget reads, from `core/theme.py`.
//!
//! The theme store (`lum-theme`) works out the colours; this turns them into egui colours. Each frame widgets
//! read the current [`Tokens`] from the context, so a theme change restyles everything on the next frame with
//! nothing to rebuild.

use std::sync::Arc;

use egui::style::{ScrollStyle, Selection, TextCursorStyle, WidgetVisuals, Widgets};
use egui::{Color32, CornerRadius, FontId, Margin, Shadow, Stroke, Style, TextStyle, Vec2, Visuals};

use crate::fonts::{self, Weight};

pub const DANGER: Color32 = Color32::from_rgb(0xE8, 0x64, 0x5E);
pub const WARNING: Color32 = Color32::from_rgb(0xE0, 0xAE, 0x3F);
pub const OK: Color32 = Color32::from_rgb(0x3F, 0xBF, 0x74);

pub use lum_theme::{GradientKind, Role};

/// A colour role painted as a gradient. `angle` works like CSS: the direction it runs in, 0 = upwards,
/// 90 = to the right.
#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    pub kind: GradientKind,
    pub angle: f32,
    /// Sorted by position, at least two.
    pub stops: Vec<(f32, Color32)>,
}

impl Gradient {
    pub fn new(kind: GradientKind, angle: f32, mut stops: Vec<(f32, Color32)>) -> Option<Gradient> {
        if stops.len() < 2 {
            return None;
        }
        for stop in &mut stops {
            stop.0 = stop.0.clamp(0.0, 1.0);
        }
        stops.sort_by(|a, b| a.0.total_cmp(&b.0));
        Some(Gradient { kind, angle: angle.rem_euclid(360.0), stops })
    }

    /// The colour at position `t` (0 to 1), like `gradient_at`.
    pub fn at(&self, t: f32) -> Color32 {
        let first = self.stops[0];
        if t <= first.0 {
            return first.1;
        }
        for pair in self.stops.windows(2) {
            let ((p0, c0), (p1, c1)) = (pair[0], pair[1]);
            if t <= p1 {
                let f = if p1 <= p0 { 0.0 } else { (t - p0) / (p1 - p0) };
                return mix(c0, c1, f);
            }
        }
        self.stops[self.stops.len() - 1].1
    }
}

/// `a` blended towards `b` by `t`, like `theme.mix` (Python's `round` sends halves to the even number).
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = f64::from(t);
    let ch = |x: u8, y: u8| {
        (f64::from(x) + (f64::from(y) - f64::from(x)) * t).round_ties_even().clamp(0.0, 255.0) as u8
    };
    Color32::from_rgba_unmultiplied(ch(a.r(), b.r()), ch(a.g(), b.g()), ch(a.b(), b.b()), ch(a.a(), b.a()))
}

/// `colour` with an alpha from 0 to 1, like `theme.rgba`.
pub fn alpha(colour: Color32, a: f32) -> Color32 {
    let [r, g, b, _] = colour.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(r, g, b, (a.clamp(0.0, 1.0) * 255.0) as u8)
}

pub fn luminance(c: Color32) -> f32 {
    0.299 * f32::from(c.r()) + 0.587 * f32::from(c.g()) + 0.114 * f32::from(c.b())
}

pub fn colour(c: lum_theme::Rgb) -> Color32 {
    Color32::from_rgb(c.r, c.g, c.b)
}

impl From<&lum_theme::Gradient> for Gradient {
    fn from(g: &lum_theme::Gradient) -> Gradient {
        Gradient {
            kind: g.kind,
            angle: g.angle as f32,
            stops: g.stops.iter().map(|&(p, c)| (p as f32, colour(c))).collect(),
        }
    }
}

impl From<&lum_theme::Palette> for Palette {
    fn from(p: &lum_theme::Palette) -> Palette {
        let c = |role: Role| colour(p.get(role));
        Palette {
            primary: c(Role::Primary),
            primary_hover: c(Role::PrimaryHover),
            primary_muted: c(Role::PrimaryMuted),
            primary_text: c(Role::PrimaryText),
            on_primary: c(Role::OnPrimary),
            bg_base: c(Role::BgBase),
            bg_card: c(Role::BgCard),
            bg_card_hover: c(Role::BgCardHover),
            bg_frame: c(Role::BgFrame),
            bg_header: c(Role::BgHeader),
            border: c(Role::Border),
            text_primary: c(Role::TextPrimary),
            text_secondary: c(Role::TextSecondary),
            text_muted: c(Role::TextMuted),
            folder_color: c(Role::FolderColor),
            folder_border: c(Role::FolderBorder),
            radius: p.radius.clamp(0, 40) as u8,
            font_family: p.font_family.clone(),
            glow: p.glow,
            background_image: p.background_image.clone(),
            image_opacity: p.image_opacity.clamp(0.0, 1.0) as f32,
            gradients: p.gradients.iter().map(|(r, g)| (*r, Gradient::from(g))).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Palette {
    pub primary: Color32,
    pub primary_hover: Color32,
    pub primary_muted: Color32,
    /// Accent-coloured text (links, PINNED, "+" glyphs).
    pub primary_text: Color32,
    /// Text drawn on an accent-coloured background.
    pub on_primary: Color32,
    pub bg_base: Color32,
    pub bg_card: Color32,
    pub bg_card_hover: Color32,
    pub bg_frame: Color32,
    pub bg_header: Color32,
    pub border: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,
    pub folder_color: Color32,
    pub folder_border: Color32,
    pub radius: u8,
    pub font_family: String,
    /// The accent glow around the overlay (off in performance mode).
    pub glow: bool,
    /// Drawn behind the overlay content, empty for none.
    pub background_image: String,
    pub image_opacity: f32,
    pub gradients: Vec<(Role, Gradient)>,
}

impl Palette {
    /// Near-black with a bluey-purple accent (the default).
    pub fn dark() -> Palette {
        Palette::from(&lum_theme::Palette::dark())
    }

    /// White with a yellow accent.
    pub fn light() -> Palette {
        Palette::from(&lum_theme::Palette::light())
    }

    pub fn colour(&self, role: Role) -> Color32 {
        match role {
            Role::Primary => self.primary,
            Role::PrimaryHover => self.primary_hover,
            Role::PrimaryMuted => self.primary_muted,
            Role::PrimaryText => self.primary_text,
            Role::OnPrimary => self.on_primary,
            Role::BgBase => self.bg_base,
            Role::BgCard => self.bg_card,
            Role::BgCardHover => self.bg_card_hover,
            Role::BgFrame => self.bg_frame,
            Role::BgHeader => self.bg_header,
            Role::Border => self.border,
            Role::TextPrimary => self.text_primary,
            Role::TextSecondary => self.text_secondary,
            Role::TextMuted => self.text_muted,
            Role::FolderColor => self.folder_color,
            Role::FolderBorder => self.folder_border,
        }
    }

    pub fn gradient(&self, role: Role) -> Option<&Gradient> {
        self.gradients.iter().find(|(r, _)| *r == role).map(|(_, g)| g)
    }

    /// The background is light (picks the light tray icon, the pale scrim and so on).
    pub fn is_light(&self) -> bool {
        luminance(self.bg_base) > 140.0
    }
}

/// How text and colours are picked: a palette plus the Text size setting and the hover switch.
#[derive(Debug, Clone, PartialEq)]
pub struct Tokens {
    pub p: Palette,
    /// Settings → Overlay & Interface → Text size, 0.8 to 1.6.
    pub text_scale: f32,
    /// Settings → Animations → Hover effects.
    pub hover: bool,
    /// Settings → Overlay & Interface → Performance mode: no glow, no background image, no animation.
    pub performance: bool,
    /// The looks a theme gives single modules, notifications (`toasts`) and warnings (`alerts`), as whole palettes.
    pub sections: Vec<(String, Palette)>,
}

/// The colour a label takes, as the `tone` property in the Python stylesheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tone {
    #[default]
    Primary,
    Secondary,
    Muted,
    /// `primary_text`: accent-coloured text.
    Accent,
    /// The accent itself.
    Accent2,
    Danger,
}

fn tokens_id() -> egui::Id {
    egui::Id::unique("lum-tokens")
}

impl Tokens {
    pub fn new(p: Palette, text_scale: f32) -> Tokens {
        let text_scale = if text_scale.is_finite() { text_scale.clamp(0.8, 1.6) } else { 1.0 };
        Tokens { p, text_scale, hover: true, performance: false, sections: Vec::new() }
    }

    /// The palette with the Text size, Hover effects and Performance mode settings applied.
    pub fn with_settings(p: Palette, settings: &lum_core::settings::Settings) -> Tokens {
        let mut tokens = Tokens::new(p, settings.get_f64("ui.text_scale").unwrap_or(1.0) as f32);
        tokens.hover = settings.get_bool("animations.hover", true);
        tokens.performance = settings.get_bool("ui.performance_mode", false);
        tokens
    }

    /// The tokens of a theme: its palette and what it gives single modules, with the settings applied.
    pub fn of_theme(
        palette: &lum_theme::Palette,
        sections: &[(String, lum_theme::Section)],
        settings: &lum_core::settings::Settings,
    ) -> Tokens {
        let mut tokens = Tokens::with_settings(Palette::from(palette), settings);
        tokens.sections = sections
            .iter()
            .map(|(key, section)| (key.clone(), Palette::from(&palette.with_section(section))))
            .collect();
        tokens
    }

    /// These tokens with one module's own look (`with module_colours(key)`), or `None` when the theme gives it
    /// none. Draw that module's windows with them.
    pub fn for_section(&self, key: &str) -> Option<Tokens> {
        let (_, p) = self.sections.iter().find(|(k, _)| k == key)?;
        Some(Tokens { p: p.clone(), sections: Vec::new(), ..self.clone() })
    }

    /// The accent glow around the overlay and panels is wanted.
    pub fn glow(&self) -> bool {
        self.p.glow && !self.performance
    }

    pub fn dark() -> Tokens {
        Tokens::new(Palette::dark(), 1.0)
    }

    pub fn light() -> Tokens {
        Tokens::new(Palette::light(), 1.0)
    }

    /// The tokens the context was given last (Dark until something installs others).
    pub fn of(ctx: &egui::Context) -> Arc<Tokens> {
        ctx.data(|d| d.get_temp::<Arc<Tokens>>(tokens_id())).unwrap_or_else(|| Arc::new(Tokens::dark()))
    }

    /// Makes these the tokens every widget reads and restyles egui's own widgets to match.
    pub fn install(self, ctx: &egui::Context) {
        let style = Arc::new(self.style());
        let theme = if self.p.is_light() { egui::Theme::Light } else { egui::Theme::Dark };
        ctx.set_style_of(egui::Theme::Dark, style.clone());
        ctx.set_style_of(egui::Theme::Light, style);
        ctx.set_theme(theme);
        ctx.data_mut(|d| d.insert_temp(tokens_id(), Arc::new(self)));
    }

    /// The corner radius of panels and big cards.
    pub fn r(&self) -> f32 {
        f32::from(self.p.radius)
    }

    /// Small controls: buttons, inputs, hovered rows (never sharp unless the theme is).
    pub fn rs(&self) -> f32 {
        let r = self.r();
        r.min((r - 4.0).max(6.0))
    }

    /// Boxes inside a panel.
    pub fn rb(&self) -> f32 {
        (self.r() - 2.0).max(0.0)
    }

    /// A font size in pixels at the chosen text size, like `fs()`.
    pub fn fs(&self, px: f32) -> f32 {
        (px * self.text_scale).max(1.0)
    }

    /// `qfont(px, weight, mono)`: scaled with the text size.
    pub fn font(&self, px: f32, weight: Weight, mono: bool) -> FontId {
        fonts::font(self.fs(px), weight, mono)
    }

    pub fn tone(&self, tone: Tone) -> Color32 {
        match tone {
            Tone::Primary => self.p.text_primary,
            Tone::Secondary => self.p.text_secondary,
            Tone::Muted => self.p.text_muted,
            Tone::Accent => self.p.primary_text,
            Tone::Accent2 => self.p.primary,
            Tone::Danger => DANGER,
        }
    }

    /// The tint of a dimmed screen: near-black on dark themes, a pale paper colour on light ones.
    pub fn scrim(&self) -> Color32 {
        if self.p.is_light() { Color32::from_rgb(236, 236, 230) } else { Color32::from_rgb(6, 6, 10) }
    }

    /// A border that leans towards the accent: hovered cards and boxes.
    pub fn hover_border(&self, amount: f32) -> Color32 {
        mix(self.p.border, self.p.primary, amount)
    }

    pub fn glow_alpha(&self) -> f32 {
        if self.p.is_light() { 0.32 } else { 0.26 }
    }

    /// egui's own widgets (text fields, scroll bars, menus, tooltips, combo pop-ups) in the stylesheet's look.
    pub fn style(&self) -> Style {
        let p = &self.p;
        let light = p.is_light();
        let rs = CornerRadius::from(self.rs());
        let state = |bg: Color32, stroke: Color32, fg: Color32| WidgetVisuals {
            bg_fill: bg,
            weak_bg_fill: bg,
            bg_stroke: Stroke::new(1.0, stroke),
            corner_radius: rs,
            fg_stroke: Stroke::new(1.0, fg),
            expansion: 0.0,
        };
        let widgets = Widgets {
            noninteractive: WidgetVisuals {
                bg_fill: p.bg_base,
                weak_bg_fill: p.bg_base,
                bg_stroke: Stroke::new(1.0, p.border),
                corner_radius: rs,
                fg_stroke: Stroke::new(1.0, p.text_primary),
                expansion: 0.0,
            },
            inactive: state(p.bg_card, p.border, p.text_primary),
            hovered: state(p.bg_card_hover, if self.hover { p.text_muted } else { p.border }, p.text_primary),
            active: state(p.bg_card_hover, p.text_muted, p.text_primary),
            open: state(p.bg_card_hover, p.border, p.text_primary),
        };
        let shadow = Shadow {
            offset: [0, 6],
            blur: 18,
            spread: 0,
            color: Color32::from_black_alpha(if light { 30 } else { 110 }),
        };
        let visuals = Visuals {
            dark_mode: !light,
            override_text_color: None,
            weak_text_color: Some(p.text_muted),
            widgets,
            selection: Selection { bg_fill: p.primary_muted, stroke: Stroke::new(1.0, p.text_primary) },
            hyperlink_color: p.primary_text,
            faint_bg_color: p.bg_card,
            extreme_bg_color: p.bg_frame,
            text_edit_bg_color: Some(p.bg_frame),
            code_bg_color: p.bg_frame,
            warn_fg_color: WARNING,
            error_fg_color: DANGER,
            window_corner_radius: CornerRadius::from(self.r() + 4.0),
            window_shadow: shadow,
            window_fill: p.bg_base,
            window_stroke: Stroke::new(1.0, p.border),
            window_highlight_topmost: false,
            menu_corner_radius: CornerRadius::from(self.rb()),
            panel_fill: p.bg_base,
            popup_shadow: shadow,
            text_cursor: TextCursorStyle { stroke: Stroke::new(1.5, p.primary), ..Default::default() },
            button_frame: true,
            collapsing_header_frame: false,
            indent_has_left_vline: false,
            striped: false,
            slider_trailing_fill: true,
            interact_cursor: Some(egui::CursorIcon::PointingHand),
            image_loading_spinners: false,
            ..if light { Visuals::light() } else { Visuals::dark() }
        };

        let mut style = Style { visuals, ..Style::default() };
        style.text_styles = [
            (TextStyle::Small, self.font(11.0, Weight::Regular, false)),
            (TextStyle::Body, self.font(13.0, Weight::Regular, false)),
            (TextStyle::Button, self.font(13.0, Weight::Medium, false)),
            (TextStyle::Heading, self.font(20.0, Weight::SemiBold, false)),
            (TextStyle::Monospace, self.font(12.0, Weight::Regular, true)),
        ]
        .into();
        let s = &mut style.spacing;
        s.item_spacing = Vec2::new(8.0, 6.0);
        s.button_padding = Vec2::new(12.0, 6.0);
        s.interact_size = Vec2::new(24.0, 30.0);
        s.menu_margin = Margin::same(5);
        s.window_margin = Margin::same(14);
        s.icon_width = 16.0;
        s.icon_width_inner = 10.0;
        s.icon_spacing = 9.0;
        s.slider_rail_height = 4.0;
        s.combo_height = 300.0;
        s.scroll = ScrollStyle { bar_width: 8.0, floating: true, ..ScrollStyle::thin() };
        style.interaction.tooltip_delay = 0.4;
        // the Animator decides how long things take; egui's own fades stay short
        style.animation_time = 0.12;
        style
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(rgb: u32) -> Color32 {
        colour(lum_theme::Rgb::from_u32(rgb))
    }

    #[test]
    fn radii_follow_the_stylesheet() {
        let t = Tokens::dark();
        assert_eq!((t.r(), t.rs(), t.rb()), (10.0, 6.0, 8.0));
        let mut sharp = Tokens::dark();
        sharp.p.radius = 0;
        assert_eq!((sharp.rs(), sharp.rb()), (0.0, 0.0));
        let mut round = Tokens::dark();
        round.p.radius = 20;
        assert_eq!(round.rs(), 16.0);
    }

    #[test]
    fn text_scale_is_clamped_like_python() {
        assert_eq!(Tokens::new(Palette::dark(), 3.0).text_scale, 1.6);
        assert_eq!(Tokens::new(Palette::dark(), 0.1).text_scale, 0.8);
        assert_eq!(Tokens::new(Palette::dark(), f32::NAN).text_scale, 1.0);
        assert_eq!(Tokens::new(Palette::dark(), 1.25).fs(12.0), 15.0);
    }

    #[test]
    fn colour_maths_matches_python() {
        // theme.mix("#7B6CF6", "#FFFFFF", 0.35), how derive_primary_text lightens the accent
        assert_eq!(mix(hex(0x7B6CF6), hex(0xFFFFFF), 0.35), hex(0xA99FF9));
        assert_eq!(mix(hex(0x000000), hex(0x030303), 0.5), hex(0x020202), "1.5 rounds to the even 2");
        assert!(Palette::light().is_light() && !Palette::dark().is_light());
        let g = Gradient::new(GradientKind::Linear, -90.0, vec![(1.0, hex(0x000000)), (0.0, hex(0xFFFFFF))])
            .expect("two stops");
        assert_eq!(g.angle, 270.0);
        assert_eq!(g.at(0.0), hex(0xFFFFFF));
        assert_eq!(g.at(0.5), hex(0x808080));
        assert!(Gradient::new(GradientKind::Radial, 0.0, vec![(0.0, hex(0))]).is_none());
    }
}
