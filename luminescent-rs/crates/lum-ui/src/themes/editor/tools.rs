//! The quick tools of the theme editor (`BulkDialog` in `ui/colour_tools.py`) for the theme's own colours:
//! each one makes a new set of colours from a few choices, shows it next to the old one, and applies as a
//! single step to undo.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

use egui::{Align, Context, Layout, Rect, Ui, pos2, vec2};
use lum_theme::tones::{self, Adjust, Colours, Main, Rng};
use lum_theme::{Palette as EnginePalette, Rgb};
use serde_json::Map;

use crate::panel::{FloatingPanel, Geometry};
use crate::picker::{ColourDialog, Picked, swatch};
use crate::text::{caps, hint, label, mono};
use crate::theme::{Palette, Tokens, Tone};
use crate::themes::previews::{Canvas, Reg, paint_luminescent};
use crate::widgets::{Button, Checkbox, Slider, segmented, tip};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Main,
    Harmony,
    Picture,
    Surprise,
    Adjust,
    Follow,
    Variance,
    Flip,
}

/// Every tool with its label and what it does, in the order of the buttons.
pub const TOOLS: [(Tool, &str, &str); 8] = [
    (
        Tool::Main,
        "✦  Main colours",
        "Pick a few colours (accent, background, text…) and every other colour is made from them, with proper \
         lighter and darker steps.",
    ),
    (Tool::Harmony, "◑  Harmony", "Colours that go together, around your accent."),
    (Tool::Picture, "▣  From a picture", "Take the colours from your wallpaper or any picture."),
    (Tool::Surprise, "⚄  Surprise me", "A random scheme that still works. Press Again until you like one."),
    (
        Tool::Adjust,
        "≋  Adjust all",
        "Sliders for every colour at once: hue, brightness, contrast, colourfulness, warmth.",
    ),
    (
        Tool::Follow,
        "◎  Follow the accent",
        "Make every green (or any hue you choose) take your accent's hue instead.",
    ),
    (
        Tool::Variance,
        "▤  Add variance",
        "Colours that came out the same (everything #FFFFFF) get a few steps between them.",
    ),
    (Tool::Flip, "◐  Light ⇄ dark", "Turn a dark scheme into a light one or the other way round."),
];

impl Tool {
    fn entry(self) -> (&'static str, &'static str) {
        TOOLS.iter().find(|t| t.0 == self).map_or(("", ""), |t| (t.1, t.2))
    }

    /// The label without its icon.
    pub fn name(self) -> &'static str {
        self.entry().0.split_once("  ").map_or("", |(_, name)| name)
    }
}

/// The groups of colours a tool may change (`LUM_GROUPS`).
const GROUPS: [(&str, &str, &[&str]); 4] = [
    ("accent", "Accent", &["primary", "primary_hover", "primary_muted", "primary_text", "on_primary"]),
    (
        "backgrounds",
        "Backgrounds",
        &["bg_base", "bg_card", "bg_card_hover", "bg_frame", "bg_header", "border"],
    ),
    ("text", "Text", &["text_primary", "text_secondary", "text_muted"]),
    ("folders", "Folders", &["folder_color", "folder_border"]),
];

const WIDTH: f32 = 1080.0;
const HEIGHT: f32 = 720.0;
const CONTROLS_WIDTH: f32 = 346.0;

/// The pixels of a picture, scaled down, from a thread of their own.
struct Loading {
    rx: Receiver<Option<Vec<[u8; 4]>>>,
}

pub struct BulkDialog {
    panel: FloatingPanel,
    tool: Tool,
    /// The colours as they are in the theme, each as a plain `#RRGGBB`.
    before: Colours,
    after: Colours,
    palette: EnginePalette,
    groups: Vec<(&'static str, &'static str, Vec<&'static str>)>,
    ticks: Vec<bool>,
    light: bool,
    main: Main,
    adjust: Adjust,
    step: f64,
    families: HashSet<&'static str>,
    follow: Rgb,
    harmony: &'static str,
    picks: Vec<Rgb>,
    picture_main: Option<Main>,
    loading: Option<Loading>,
    seed: u64,
    changes: Map<String, serde_json::Value>,
    stale: bool,
    close: bool,
    /// The swatch the colour dialog is open for, and its colour before, for Cancel.
    asked: Option<(String, Rgb)>,
    pub applied: Option<(Map<String, serde_json::Value>, String)>,
    pub finished: bool,
}

impl std::fmt::Debug for BulkDialog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BulkDialog").field("tool", &self.tool).finish()
    }
}

impl BulkDialog {
    /// `before` has the theme's colours; `palette` is the theme's own look for the pictures (corner radius and
    /// the like).
    pub fn new(tool: Tool, before: Colours, palette: EnginePalette, picture: Option<PathBuf>) -> BulkDialog {
        let groups: Vec<_> = GROUPS
            .iter()
            .map(|(id, title, roles)| {
                (*id, *title, roles.iter().copied().filter(|r| before.contains_key(*r)).collect())
            })
            .collect();
        let light = tones::tone(color_of(&before, "bg_base", Rgb::from_u32(0x131317))) > 55.0;
        let main = tones::main_from_roles(&before);
        let follow = Rgb::parse(&main.accent).unwrap_or(Rgb::from_u32(0x7B6CF6));
        let loading = picture.map(|path| {
            let (tx, rx) = channel();
            let from = path.clone();
            // a big wallpaper takes a moment to read, so the dialog opens first
            let started = std::thread::Builder::new().name("theme-picture-colours".into()).spawn(move || {
                let _ = tx.send(read_pixels(&from));
            });
            let rx = if started.is_ok() { rx } else { channel().1 };
            Loading { rx }
        });
        let mut panel = FloatingPanel::new(
            "theme-tool",
            tool.name(),
            Geometry { x: 100.0, y: 60.0, w: WIDTH, h: HEIGHT },
        )
        .min_size(vec2(820.0, 520.0));
        panel.opacity = 1.0;
        BulkDialog {
            panel,
            tool,
            after: before.clone(),
            before,
            palette,
            ticks: vec![true; groups.len()],
            groups,
            light,
            main,
            adjust: Adjust::default(),
            step: 4.0,
            families: HashSet::from(["greens"]),
            follow,
            harmony: "matching",
            picks: Vec::new(),
            picture_main: None,
            loading,
            seed: clock_seed(),
            changes: Map::new(),
            stale: true,
            close: false,
            asked: None,
            applied: None,
            finished: false,
        }
    }

    pub fn open(&mut self, near: Rect) {
        let size = vec2(WIDTH.min(near.width().max(820.0)), HEIGHT.min(near.height().max(520.0)));
        self.panel.geometry = Geometry::from_rect(Rect::from_center_size(near.center(), size));
        self.panel.open();
    }

    pub fn target(&self) -> &'static str {
        match self.tool {
            Tool::Follow => "follow",
            Tool::Harmony => "harmony",
            _ => "main",
        }
    }

    /// The colour dialog gave back a colour for one of the swatches here (`key` is a main colour, `follow` or
    /// `harmony`).
    pub fn picked(&mut self, key: &str, colour: Rgb) {
        match key {
            "follow" => self.follow = colour,
            "harmony" => self.main.accent = colour.hex(),
            other => self.main.set(other, colour.hex()),
        }
        self.stale = true;
    }

    /// The colour dialog was cancelled: the swatch goes back to what it was.
    pub fn cancel_pick(&mut self, key: &str) {
        if let Some((asked, before)) = self.asked.take()
            && asked == key
        {
            self.picked(key, before);
        }
    }

    fn set_light(&mut self, light: bool) {
        if light == self.light {
            return;
        }
        self.light = light;
        if self.tool == Tool::Main {
            // flip the background and the text to match, keep the accents
            for key in ["background", "text"] {
                let value = self.main.get(key).to_owned();
                if !value.is_empty() {
                    let flipped = tones::flip(&Colours::from([("x".to_owned(), value)]));
                    self.main.set(key, flipped.get("x").cloned().unwrap_or_default());
                }
            }
        }
        self.stale = true;
    }

    fn scheme(main: &Main) -> Colours {
        tones::lum_roles(main)
    }

    fn compute(&self) -> Colours {
        let before = &self.before;
        match self.tool {
            Tool::Main => Self::scheme(&self.main),
            Tool::Harmony => {
                let accent = Rgb::parse(&self.main.accent).unwrap_or(Rgb::from_u32(0x7B6CF6));
                Self::scheme(&tones::harmony(accent, self.harmony, self.light, None, None))
            }
            Tool::Surprise => Self::scheme(&tones::surprise(self.light, &mut Rng::new(self.seed))),
            Tool::Picture => self.picture_main.as_ref().map_or_else(|| before.clone(), Self::scheme),
            Tool::Adjust => tones::adjust(before, self.adjust),
            Tool::Follow => {
                let mut families: Vec<&str> = self.families.iter().copied().collect();
                families.sort_unstable();
                tones::follow_hue(before, &families, self.follow, &[])
            }
            Tool::Flip => tones::flip(before),
            Tool::Variance => self.variance(),
        }
    }

    /// Colours in the same group that came out the same get steps between them, using the steps a scheme made
    /// from the current main colours would have.
    fn variance(&self) -> Colours {
        let fresh = Self::scheme(&self.main);
        let mut out = self.before.clone();
        let step = self.step;
        for (_, _, keys) in &self.groups {
            let mut seen: Vec<(String, Vec<&str>)> = Vec::new();
            for key in keys {
                let Some(value) = self.before.get(*key).filter(|v| Rgb::is_hex(v)) else { continue };
                let value = value.to_uppercase();
                match seen.iter_mut().find(|(v, _)| *v == value) {
                    Some((_, same)) => same.push(key),
                    None => seen.push((value, vec![key])),
                }
            }
            for (_, same) in seen.iter().filter(|(_, same)| same.len() > 1) {
                for (n, key) in same.iter().enumerate() {
                    let current = self.before.get(*key).cloned().unwrap_or_default();
                    match fresh.get(*key) {
                        Some(new) if !new.eq_ignore_ascii_case(&current) => {
                            out.insert((*key).to_owned(), new.clone());
                        }
                        _ if n > 0 => {
                            if let Some(c) = Rgb::parse(&current) {
                                let by = if self.light { -step } else { step } * n as f64;
                                out.insert((*key).to_owned(), tones::shift_tone(c, by).hex());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        out
    }

    fn allowed(&self) -> HashSet<&str> {
        self.groups
            .iter()
            .zip(&self.ticks)
            .filter(|(_, on)| **on)
            .flat_map(|((_, _, keys), _)| keys.iter().copied())
            .collect()
    }

    fn update_after(&mut self) {
        let new = self.compute();
        let allowed = self.allowed();
        self.after = self
            .before
            .iter()
            .map(|(k, old)| {
                let value = if allowed.contains(k.as_str()) { new.get(k).unwrap_or(old) } else { old };
                (k.clone(), value.clone())
            })
            .collect();
        self.changes = self
            .before
            .iter()
            .filter(|(k, old)| {
                let new = &self.after[*k];
                Rgb::is_hex(old) && Rgb::is_hex(new) && !new.eq_ignore_ascii_case(old)
            })
            .map(|(k, _)| (k.clone(), serde_json::json!(self.after[k].to_uppercase())))
            .collect();
        self.stale = false;
    }

    fn poll_picture(&mut self, ctx: &Context) {
        let Some(loading) = &self.loading else { return };
        match loading.rx.try_recv() {
            Ok(pixels) => {
                let pixels = pixels.unwrap_or_default();
                self.picks = tones::from_picture(&pixels, 8);
                self.picture_main = tones::main_from_picture(&pixels, self.light);
                self.loading = None;
                self.stale = true;
                ctx.request_repaint();
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.loading = None;
                self.stale = true;
            }
        }
    }

    /// Draws the dialog. `picker` opens for the swatches here; what it gives back goes to `picked`.
    pub fn show(&mut self, ctx: &Context, picker: &mut ColourDialog) {
        self.poll_picture(ctx);
        if self.stale {
            self.update_after();
        }
        let near = self.panel.geometry.rect();
        let mut panel = self.panel.clone();
        let response = panel.show(ctx, |ui| self.body(ui, picker, near));
        self.panel = panel;
        if std::mem::take(&mut self.close) || response.escape && !picker.is_open() {
            self.panel.close();
        }
        if !self.panel.is_open() {
            self.finished = true;
        }
    }

    fn body(&mut self, ui: &mut Ui, picker: &mut ColourDialog, near: Rect) {
        let tokens = Tokens::of(ui.ctx());
        let full = ui.available_rect_before_wrap();
        let footer = Rect::from_min_max(pos2(full.min.x, full.max.y - 50.0), full.max);
        let controls = Rect::from_min_size(full.min, vec2(CONTROLS_WIDTH, full.height() - 50.0));
        let view = Rect::from_min_max(
            pos2(controls.max.x + 8.0, full.min.y + 8.0),
            pos2(full.max.x - 16.0, footer.min.y),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(controls.shrink2(vec2(12.0, 8.0))), |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                self.controls(ui, picker, near, &tokens);
            });
        });
        self.before_and_after(ui, view);
        ui.scope_builder(egui::UiBuilder::new().max_rect(footer.shrink2(vec2(16.0, 8.0))), |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if ui.add(Button::primary("Use these colours")).clicked() {
                    if !self.changes.is_empty() {
                        let label = format!("{} ({} colours)", self.tool.name(), self.changes.len());
                        self.applied = Some((self.changes.clone(), label));
                    }
                    self.close = true;
                }
                if ui.add(Button::ghost("Cancel")).clicked() {
                    self.close = true;
                }
                if matches!(self.tool, Tool::Surprise | Tool::Harmony | Tool::Picture)
                    && tip(ui.add(Button::ghost("⚄  Again")), "Another one").clicked()
                {
                    self.again();
                }
                let n = self.changes.len();
                let text = if n == 0 {
                    "Nothing changes yet.".to_owned()
                } else {
                    format!(
                        "{n} colour{} will change. It is one step to undo.",
                        if n == 1 { "" } else { "s" }
                    )
                };
                ui.add(label(text).px(11.0).tone(Tone::Muted));
            });
        });
    }

    fn again(&mut self) {
        self.seed = clock_seed();
        if self.tool == Tool::Harmony {
            let kinds: Vec<&str> = tones::HARMONIES.iter().map(|h| h.0).collect();
            let at = kinds.iter().position(|k| *k == self.harmony).unwrap_or(0);
            self.harmony = tones::HARMONIES[(at + 1) % kinds.len()].0;
        }
        if self.tool == Tool::Picture && !self.picks.is_empty() {
            let pick = self.picks[(self.seed % self.picks.len() as u64) as usize];
            self.pick_accent(pick);
        }
        self.stale = true;
    }

    fn pick_accent(&mut self, colour: Rgb) {
        let mut main = self.picture_main.clone().unwrap_or_default();
        main.accent = tones::fit_accent(colour, self.light).hex();
        self.picture_main = Some(main);
        self.stale = true;
    }

    fn mode_row(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.add(label("Make it").px(12.0));
            if let Some(i) = segmented(ui, &["Dark", "Light"], usize::from(self.light)) {
                self.set_light(i == 1);
            }
        });
    }

    fn colour_row(
        &mut self,
        ui: &mut Ui,
        picker: &mut ColourDialog,
        near: Rect,
        text: &str,
        key: &str,
        value: Rgb,
    ) {
        ui.horizontal(|ui| {
            ui.add_sized([110.0, 20.0], label(text).px(12.0));
            if swatch(ui, Some(&Picked::Colour(value)), "", 140.0, text).clicked() {
                self.asked = Some((key.to_owned(), value));
                picker.open(&format!("editor:tool:{key}"), text, Picked::Colour(value), false, near);
            }
        });
    }

    fn controls(&mut self, ui: &mut Ui, picker: &mut ColourDialog, near: Rect, tokens: &Tokens) {
        ui.spacing_mut().item_spacing = vec2(6.0, 8.0);
        ui.add(hint(self.tool.entry().1).px(11.5).wrap());
        ui.add_space(4.0);
        self.tool_controls(ui, picker, near, tokens);
        ui.add_space(6.0);
        ui.add(caps("COLOURS IT MAY CHANGE").px(9.5).tone(Tone::Muted).spacing(0.14));
        for ((_, title, keys), on) in self.groups.iter().zip(self.ticks.iter_mut()) {
            let response = ui.add(Checkbox::new(on, *title));
            tip(response.clone(), format!("{} colours", keys.len()));
            if response.changed() {
                self.stale = true;
            }
        }
    }

    fn tool_controls(&mut self, ui: &mut Ui, picker: &mut ColourDialog, near: Rect, tokens: &Tokens) {
        match self.tool {
            Tool::Main => {
                self.mode_row(ui);
                for (key, text) in
                    tones::MAIN.iter().take(5).filter(|(k, _)| matches!(*k, "accent" | "background" | "text"))
                {
                    let value = Rgb::parse(self.main.get(key)).unwrap_or(Rgb::BLACK);
                    self.colour_row(ui, picker, near, text, key, value);
                }
                ui.add(
                    hint("Only the accent, background and text are needed; the rest are worked out.")
                        .px(10.5)
                        .wrap(),
                );
            }
            Tool::Adjust => {
                let rows = [
                    ("Hue", -180.0, 180.0, "°"),
                    ("Brightness", -100.0, 100.0, ""),
                    ("Contrast", -100.0, 100.0, ""),
                    ("Colourfulness", -100.0, 100.0, ""),
                    ("Warmth", -100.0, 100.0, ""),
                ];
                let a = &mut self.adjust;
                let mut changed = false;
                for (value, (name, lo, hi, unit)) in
                    [&mut a.hue, &mut a.brightness, &mut a.contrast, &mut a.saturation, &mut a.warmth]
                        .into_iter()
                        .zip(rows)
                {
                    ui.horizontal(|ui| {
                        ui.add_sized([104.0, 20.0], label(name).px(12.0));
                        changed |=
                            ui.add(Slider::new(value, lo..=hi).step(1.0).width(130.0).name(name)).changed();
                        ui.add(mono(format!("{}{unit}", *value as i32)).px(11.0).tone(Tone::Secondary));
                    });
                }
                if ui.add(Button::ghost("Put the sliders back")).clicked() {
                    *a = Adjust::default();
                    changed = true;
                }
                self.stale |= changed;
            }
            Tool::Follow => {
                self.colour_row(ui, picker, near, "Take the hue of", "follow", self.follow);
                ui.add(label("Which colours follow it").px(12.0));
                let counts = self.family_counts();
                let mut families: Vec<(&'static str, String)> = tones::FAMILIES
                    .iter()
                    .map(|(key, name, _)| {
                        (*key, format!("{name} ({})", counts.iter().find(|c| c.0 == *key).map_or(0, |c| c.1)))
                    })
                    .collect();
                let others: usize = counts.iter().filter(|c| c.0 != "greys").map(|c| c.1).sum();
                families.push(("all", format!("Every colourful one ({others})")));
                for (key, text) in families {
                    let mut on = self.families.contains(key);
                    if ui.add(Checkbox::new(&mut on, text)).changed() {
                        if on {
                            self.families.insert(key);
                        } else {
                            self.families.remove(key);
                        }
                        self.stale = true;
                    }
                }
            }
            Tool::Variance => {
                self.mode_row(ui);
                ui.horizontal(|ui| {
                    ui.add_sized([104.0, 20.0], label("Step size").px(12.0));
                    if ui
                        .add(Slider::new(&mut self.step, 2.0..=10.0).step(1.0).width(130.0).name("Step size"))
                        .changed()
                    {
                        self.stale = true;
                    }
                    ui.add(mono(format!("{}", self.step as i32)).px(11.0).tone(Tone::Secondary));
                });
            }
            Tool::Harmony => {
                self.mode_row(ui);
                let accent = Rgb::parse(&self.main.accent).unwrap_or(Rgb::from_u32(0x7B6CF6));
                self.colour_row(ui, picker, near, "Around", "harmony", accent);
                for (key, name, description) in tones::HARMONIES {
                    let response = tip(ui.add(Button::ghost(name).checked(key == self.harmony)), description);
                    if response.clicked() {
                        self.harmony = key;
                        self.stale = true;
                    }
                }
            }
            Tool::Picture => {
                self.mode_row(ui);
                ui.add(label("Colours in the picture (click one to make it the accent)").px(12.0).wrap());
                ui.horizontal_wrapped(|ui| {
                    for c in self.picks.clone() {
                        let chip = crate::picker::chip(
                            ui,
                            crate::picker::ChipFill::Colour(crate::theme::colour(c)),
                            26.0,
                            false,
                            &c.hex(),
                            tokens,
                        );
                        if chip.clicked() {
                            self.pick_accent(c);
                        }
                    }
                });
                if self.loading.is_some() {
                    ui.add(hint("Reading the picture…").px(11.5));
                } else if self.picks.is_empty() {
                    ui.add(label("Could not read that picture.").px(11.5).tone(Tone::Secondary));
                }
            }
            Tool::Surprise => self.mode_row(ui),
            Tool::Flip => {
                ui.add(
                    hint("Every colour keeps its hue; light ones become dark and dark ones light.")
                        .px(11.5)
                        .wrap(),
                );
            }
        }
    }

    fn family_counts(&self) -> Vec<(&'static str, usize)> {
        let mut counts: Vec<(&'static str, usize)> = Vec::new();
        for value in self.before.values() {
            if let Some(c) = Rgb::parse(value).filter(|_| Rgb::is_hex(value)) {
                let family = tones::family(c);
                match counts.iter_mut().find(|e| e.0 == family) {
                    Some(e) => e.1 += 1,
                    None => counts.push((family, 1)),
                }
            }
        }
        counts
    }

    fn before_and_after(&self, ui: &mut Ui, view: Rect) {
        let half = ((view.width() - 18.0) / 2.0).floor();
        for (i, (title, colours)) in
            [("BEFORE", &self.before), ("AFTER", &self.after)].into_iter().enumerate()
        {
            let x = view.min.x + i as f32 * (half + 18.0);
            let tokens = Tokens::of(ui.ctx());
            let colour = if i == 0 { tokens.p.text_muted } else { tokens.p.primary_text };
            let heading = Rect::from_min_size(pos2(x, view.min.y), vec2(half, 16.0));
            ui.painter().text(
                heading.left_center(),
                egui::Align2::LEFT_CENTER,
                title,
                tokens.font(9.5, crate::fonts::Weight::SemiBold, true),
                colour,
            );
            let palette = self.palette_with(colours);
            let painter = ui.painter().with_clip_rect(Rect::from_min_size(
                pos2(x, view.min.y + 22.0),
                vec2(half, view.height() - 22.0),
            ));
            let canvas = Canvas::new(&painter, &palette);
            paint_luminescent(&canvas, pos2(x, view.min.y + 22.0), half, None, &mut Reg::default());
        }
    }

    fn palette_with(&self, colours: &Colours) -> Palette {
        let mut p = self.palette.clone();
        p.gradients.clear();
        for (key, value) in colours {
            if let (Some(role), Some(c)) = (lum_theme::Role::from_key(key), Rgb::parse(value)) {
                p.set(role, c);
            }
        }
        Palette::from(&p)
    }
}

fn color_of(colours: &Colours, key: &str, default: Rgb) -> Rgb {
    colours.get(key).and_then(|v| Rgb::parse(v)).unwrap_or(default)
}

fn read_pixels(path: &std::path::Path) -> Option<Vec<[u8; 4]>> {
    let image = image::open(path).ok()?.thumbnail(72, 72).to_rgba8();
    Some(image.pixels().map(|p| p.0).collect())
}

fn clock_seed() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64)
}
