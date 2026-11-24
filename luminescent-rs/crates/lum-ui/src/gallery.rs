//! Every widget of the kit on one page, in Dark or Light: what `--spike gallery` opens, what the snapshot tests
//! photograph and what `--smoke` builds. When a widget changes, this page shows it next to all the others.

use egui::{Color32, Id, Rect, ScrollArea, Sense, Ui, Vec2, pos2, vec2};

use crate::anim::Motion;
use crate::dialog::{AskDialog, Icon, Outcome};
use crate::fonts::Weight;
use crate::paint;
use crate::panel::{FloatingPanel, Geometry};
use crate::text::{Text, caps, hint, label, mono};
use crate::theme::{DANGER, Gradient, GradientKind, OK, Palette, Role, Tokens, Tone, WARNING};
use crate::widgets::{
    self, Anchor, Button, Checkbox, ColourButton, Combo, DimOverlay, HotkeyField, IconButton, OpacitySlider,
    Slider, SpinBox, Switch, TextArea, TextInput, Variant, menu,
};

const PLACES: [&str; 3] = ["Cursor", "Primary", "Centre"];
const TEXT_SIZES: [f32; 6] = [0.85, 1.0, 1.1, 1.25, 1.4, 1.5];

pub struct Gallery {
    pub light: bool,
    text_size: usize,
    /// Off for snapshots and the smoke test, so nothing waits for an animation.
    animations: bool,
    installed: Option<(bool, usize, bool)>,
    name: String,
    bad: String,
    command: String,
    secret: String,
    note: String,
    place: usize,
    seconds: f64,
    volume: f64,
    opacity: f32,
    ticks: [bool; 2],
    switches: [bool; 2],
    menu_ticks: [bool; 2],
    selected_row: usize,
    picked_row: Option<usize>,
    anchor: Option<Anchor>,
    pub panel: FloatingPanel,
    dialog: Option<AskDialog>,
    last_answer: String,
    dim: bool,
}

impl Default for Gallery {
    fn default() -> Gallery {
        Gallery::new()
    }
}

impl Gallery {
    pub fn new() -> Gallery {
        Gallery {
            light: false,
            text_size: 1,
            animations: true,
            installed: None,
            name: "Shopping list".to_owned(),
            bad: "ctrl+shift+".to_owned(),
            command: "~/scripts/backup.sh --quiet".to_owned(),
            secret: "hunter2".to_owned(),
            note: "Milk\nEggs\nThe good bread".to_owned(),
            place: 2,
            seconds: 15.0,
            volume: 64.0,
            opacity: 0.85,
            ticks: [true, false],
            switches: [true, false],
            menu_ticks: [true, false],
            selected_row: 1,
            picked_row: None,
            anchor: None,
            panel: FloatingPanel::new(
                "gallery",
                "Notes",
                Geometry { x: 820.0, y: 120.0, w: 340.0, h: 240.0 },
            ),
            dialog: None,
            last_answer: String::new(),
            dim: false,
        }
    }

    /// For tests: the given theme, animations off, the panel open.
    pub fn still(light: bool) -> Gallery {
        let mut gallery = Gallery::new();
        gallery.light = light;
        gallery.animations = false;
        gallery.panel.open();
        gallery
    }

    /// Opens one of the dialogs (0 to 4), for the smoke test and snapshots.
    pub fn open_dialog(&mut self, which: usize) {
        let id = Id::unique("gallery-dialog");
        self.dialog = Some(match which {
            0 => AskDialog::message(
                id,
                "Saved",
                "Your notes were saved to the backup folder.",
                Icon::Information,
            ),
            1 => AskDialog::error(
                id,
                "Could not save",
                "The folder is read-only. Pick another one in Settings.",
            ),
            2 => AskDialog::confirm(
                id,
                "Delete note",
                "Delete \"Shopping list\"? This can not be undone.",
                "Delete",
                "Keep",
                true,
            ),
            3 => AskDialog::choose(
                id,
                "Paste",
                "The clipboard has text and a picture.",
                &["Text", "Picture"],
                "Cancel",
            ),
            _ => AskDialog::ask_text(id, "Rename", "New name for the note:", "Shopping list"),
        });
    }

    fn install_look(&mut self, ctx: &egui::Context) {
        let wanted = (self.light, self.text_size, self.animations);
        if self.installed == Some(wanted) {
            return;
        }
        if self.installed.is_none() {
            crate::fonts::install(ctx);
        }
        let palette = if self.light { Palette::light() } else { Palette::dark() };
        Tokens::new(palette, TEXT_SIZES[self.text_size]).install(ctx);
        if self.animations { Motion::default() } else { Motion::off() }.install(ctx);
        self.installed = Some(wanted);
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        self.install_look(ui.ctx());
        let tokens = Tokens::of(ui.ctx());
        let full = ui.max_rect();
        paint::fill(ui.painter(), full, 0.0, &tokens, Role::BgBase, 1.0);
        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                ui.vertical(|ui| {
                    ui.set_max_width(full.width() - 40.0);
                    self.heading(ui);
                    self.text(ui);
                    self.buttons(ui);
                    self.inputs(ui);
                    self.choices(ui);
                    self.cards_and_lists(ui);
                    self.painting(ui);
                    self.windows(ui);
                    ui.add_space(24.0);
                });
            });
        });
        let ctx = ui.ctx().clone();
        let mut edit_mode = self.panel.edit_mode;
        let response = self.panel.show_with(
            &ctx,
            |ui| {
                ui.add(IconButton::new("⋯").ghost().tooltip("More"));
            },
            |ui| {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.add(label("A floating panel: drag the header, resize from the edges."));
                        ui.add(hint("Esc or ✕ closes it."));
                        ui.add_space(6.0);
                        ui.add(Switch::new(&mut edit_mode, "Edit mode"));
                    });
                });
            },
        );
        self.panel.edit_mode = edit_mode;
        if response.closed || response.escape {
            self.panel.close();
        }
        if let Some(dialog) = self.dialog.as_mut() {
            match dialog.show(&ctx) {
                Outcome::Open => {}
                Outcome::Accepted(answer) => {
                    self.last_answer = format!("Answered {answer:?} {}", dialog.text());
                    self.dialog = None;
                }
                Outcome::Cancelled => {
                    self.last_answer = "Cancelled".to_owned();
                    self.dialog = None;
                }
            }
        }
        if self.dim {
            let screen = ctx.content_rect();
            let dim = DimOverlay { opacity: 0.35, click_through: false };
            if dim.show(&ctx, &[screen]) {
                self.dim = false;
            }
            // without separate windows the dim can not be clicked, so any click ends it
            if ctx.embed_viewports() && ctx.input(|i| i.pointer.any_click()) {
                self.dim = false;
            }
        }
    }

    fn heading(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            paint::sparkle(
                ui.painter(),
                ui.cursor().min + vec2(10.0, 14.0),
                18.0,
                Tokens::of(ui.ctx()).p.primary,
            );
            ui.add_space(26.0);
            ui.add(Text::new("Widget gallery").px(20.0).semibold());
            ui.add_space(16.0);
            if ui.add(Button::new("Dark").variant(Variant::Pill).checked(!self.light)).clicked() {
                self.light = false;
            }
            if ui.add(Button::new("Light").variant(Variant::Pill).checked(self.light)).clicked() {
                self.light = true;
            }
            ui.add_space(16.0);
            ui.add(hint("Text size"));
            let labels: Vec<String> = TEXT_SIZES.iter().map(|s| format!("{:.0} %", s * 100.0)).collect();
            ui.add(Combo::new(Id::unique("gallery-text-size"), &mut self.text_size, &labels).width(100.0));
        });
    }

    fn text(&mut self, ui: &mut Ui) {
        widgets::section_header(ui, "Text");
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 18.0;
            for (name, tone) in [
                ("Primary", Tone::Primary),
                ("Secondary", Tone::Secondary),
                ("Muted", Tone::Muted),
                ("Accent", Tone::Accent),
                ("Accent 2", Tone::Accent2),
                ("Danger", Tone::Danger),
            ] {
                ui.add(label(name).tone(tone));
            }
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 18.0;
            ui.add(caps("Mono caps label"));
            ui.add(caps("Edit").px(9.5).spacing(0.12).tone(Tone::Accent));
            ui.add(mono("ctrl+shift+space").tone(Tone::Secondary));
            ui.add(Text::new("Page title").px(15.0).semibold());
            ui.add(hint("A hint under a control"));
        });
        ui.horizontal(|ui| {
            ui.set_max_width(260.0);
            ui.add(label("A long name that does not fit gets an ellipsis at the end").elide());
        });
        ui.add(
            label(
                "Body text wraps onto more lines when the width runs out, the way help texts under settings do. \
                 Everything here restyles the moment the theme changes.",
            )
            .tone(Tone::Secondary)
            .wrap(),
        );
    }

    fn buttons(&mut self, ui: &mut Ui) {
        widgets::section_header(ui, "Buttons");
        ui.horizontal_wrapped(|ui| {
            for (name, variant) in [
                ("Normal", Variant::Normal),
                ("Primary", Variant::Primary),
                ("Ghost", Variant::Ghost),
                ("Outline", Variant::Outline),
                ("Danger", Variant::Danger),
                ("Pill", Variant::Pill),
                ("Pill accent", Variant::PillAccent),
            ] {
                ui.add(Button::new(name).variant(variant));
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.add(Button::new("Checked").checked(true));
            ui.add(Button::ghost("Checked ghost").checked(true));
            ui.add_enabled(false, Button::new("Disabled"));
            ui.add_enabled(false, Button::primary("Disabled primary"));
            ui.add_enabled(false, Button::danger("Disabled danger"));
            ui.add(IconButton::close());
            ui.add(IconButton::new("⋯").tooltip("More"));
            ui.add(IconButton::new("↻").ghost().tooltip("Refresh"));
            let tip = ui.add(Button::new("Hover me").variant(Variant::Outline));
            widgets::tip(tip, "Tooltips use the small type of QToolTip.");
        });
    }

    fn inputs(&mut self, ui: &mut Ui) {
        widgets::section_header(ui, "Inputs");
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            ui.add(TextInput::new(&mut self.name).hint("Name").width(220.0));
            ui.add(TextInput::new(&mut self.bad).invalid(true).mono().name("Hotkey").width(200.0));
            ui.add(TextInput::new(&mut self.command).mono().name("Command").width(260.0));
            ui.add(TextInput::new(&mut self.secret).password().name("Password").width(140.0));
        });
        ui.add_space(4.0);
        ui.scope(|ui| {
            ui.set_max_width(520.0);
            ui.add(TextArea::new(&mut self.note).rows(3).hint("Write something"));
        });
    }

    fn choices(&mut self, ui: &mut Ui) {
        widgets::section_header(ui, "Choices");
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            ui.add(Combo::new(Id::unique("gallery-place"), &mut self.place, &PLACES));
            ui.add(SpinBox::new(&mut self.seconds, 0.0..=600.0).suffix(" s").name("Seconds"));
            ui.add(Slider::new(&mut self.volume, 0.0..=100.0).width(200.0).name("Volume"));
            ui.add(OpacitySlider::new(&mut self.opacity));
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 18.0;
            let [a, b] = &mut self.ticks;
            ui.add(Checkbox::new(a, "Show the clock"));
            ui.add(Checkbox::new(b, "Start with the computer"));
            let [c, d] = &mut self.switches;
            ui.add(Switch::new(c, "Hover effects"));
            ui.add(Switch::new(d, "Performance mode"));
            let mut off = false;
            ui.add_enabled(false, Checkbox::new(&mut off, "Disabled"));
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            ui.add(ColourButton::new(Tokens::of(ui.ctx()).p.primary));
            ui.add(ColourButton::new(DANGER));
            ui.add(ColourButton::new(OK));
            ui.add(ColourButton::new(WARNING));
        });
        HotkeyField::new("ctrl+shift+space").show(ui);
        HotkeyField::new("").show(ui);
        HotkeyField::new("ctrl+alt+n").recording(true).show(ui);
        HotkeyField::new("ctrl+shift+").warning(Some("A combo needs a key after the +")).show(ui);
    }

    fn cards_and_lists(&mut self, ui: &mut Ui) {
        widgets::section_header(ui, "Cards and lists");
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            ui.vertical(|ui| {
                ui.set_width(300.0);
                let card = widgets::card(ui, |ui| {
                    ui.set_width(276.0);
                    ui.add(caps("Card"));
                    ui.add(label("Right-click for its menu.").tone(Tone::Secondary));
                    ui.add(hint(if self.last_answer.is_empty() {
                        "No dialog answered yet"
                    } else {
                        &self.last_answer
                    }));
                });
                card.response.context_menu(|ui| {
                    menu::heading(ui, "Note");
                    menu::item_with_hint(ui, "Open", "Enter");
                    menu::item(ui, "Rename…");
                    menu::separator(ui);
                    let [a, b] = &mut self.menu_ticks;
                    menu::check_item(ui, "Pinned", a);
                    menu::check_item(ui, "Show on start", b);
                    menu::separator(ui);
                    ui.add_enabled_ui(false, |ui| menu::item(ui, "Move to another board"));
                    ui.menu_button("More", |ui| {
                        menu::item(ui, "Duplicate");
                        menu::item(ui, "Export…");
                    });
                });
            });
            ui.vertical(|ui| {
                ui.set_width(240.0);
                widgets::list_frame(ui, |ui| {
                    ui.set_width(234.0);
                    for (i, name) in ["Notes", "Timers", "Calculator"].iter().enumerate() {
                        if widgets::row(ui, i == self.selected_row, |ui| {
                            ui.add(label(*name));
                        })
                        .response
                        .clicked()
                        {
                            self.selected_row = i;
                        }
                    }
                });
            });
            ui.vertical(|ui| {
                ui.set_width(280.0);
                ui.add(hint("10 000 rows, only the visible ones are built"));
                let tokens = Tokens::of(ui.ctx());
                let picked = widgets::list_frame(ui, |ui| {
                    ui.set_width(274.0);
                    widgets::virtual_list(ui, Id::unique("gallery-rows"), 10_000, 26.0, 130.0, |ui, i| {
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
                        if response.hovered() || self.picked_row == Some(i) {
                            ui.painter().rect_filled(rect, tokens.rs(), tokens.p.bg_card_hover);
                        }
                        ui.painter().text(
                            rect.left_center() + vec2(8.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            format!("Row {i:05}"),
                            tokens.font(12.0, Weight::Regular, true),
                            tokens.p.text_primary,
                        );
                        response
                    })
                })
                .inner;
                if picked.is_some() {
                    self.picked_row = picked;
                }
            });
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if let Some(anchor) = widgets::anchor_bar(ui, "Snap to") {
                self.anchor = Some(anchor);
            }
            ui.add_space(12.0);
            ui.add(hint(match self.anchor {
                Some(a) => format!("Snapped to {}", a.key()),
                None => "Pick a corner".to_owned(),
            }));
        });
    }

    fn painting(&mut self, ui: &mut Ui) {
        widgets::section_header(ui, "Painting");
        let tokens = Tokens::of(ui.ctx());
        let stops = vec![
            (0.0, tokens.p.primary),
            (0.55, Color32::from_rgb(0x3D, 0xBE, 0x96)),
            (1.0, tokens.p.bg_frame),
        ];
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            for (name, kind, angle) in [
                ("LINEAR 135°", GradientKind::Linear, 135.0),
                ("RADIAL", GradientKind::Radial, 0.0),
                ("SWEEP", GradientKind::Sweep, 0.0),
            ] {
                let (rect, _) = ui.allocate_exact_size(vec2(170.0, 96.0), Sense::hover());
                if let Some(g) = Gradient::new(kind, angle, stops.clone()) {
                    paint::gradient(ui.painter(), rect, tokens.rb(), &g, 1.0);
                }
                ui.painter().text(
                    rect.left_bottom() + vec2(10.0, -10.0),
                    egui::Align2::LEFT_BOTTOM,
                    name,
                    tokens.font(10.0, Weight::SemiBold, true),
                    Color32::WHITE,
                );
            }
            // a corner of the overlay: the scrim, the dot grid, the top halo and a panel with its glow
            let (area, _) = ui.allocate_exact_size(vec2(420.0, 200.0), Sense::hover());
            let painter = ui.painter().with_clip_rect(area);
            widgets::dim::scrim(&painter, area, &tokens, 0.64);
            paint::dot_grid(&painter, area, &tokens);
            paint::top_halo(&painter, area, &tokens);
            let panel = Rect::from_center_size(area.center() + vec2(0.0, 20.0), vec2(180.0, 90.0));
            paint::panel_glow(&painter, panel, &tokens);
            paint::fill(&painter, panel, tokens.r(), &tokens, Role::BgBase, 1.0);
            painter.rect_stroke(
                panel,
                tokens.r(),
                egui::Stroke::new(1.0, tokens.p.border),
                egui::StrokeKind::Inside,
            );
            paint::sparkle(&painter, panel.center(), 28.0, tokens.p.primary);
        });
    }

    fn windows(&mut self, ui: &mut Ui) {
        widgets::section_header(ui, "Panels and dialogs");
        ui.horizontal_wrapped(|ui| {
            if ui
                .add(Button::primary(if self.panel.is_open() { "Close the panel" } else { "Open a panel" }))
                .clicked()
            {
                self.panel.toggle();
            }
            for (i, name) in ["Message", "Error", "Confirm", "Choose", "Ask text"].iter().enumerate() {
                if ui.add(Button::new(*name)).clicked() {
                    self.open_dialog(i);
                }
            }
            if ui.add(Button::ghost("Dim the screen")).clicked() {
                self.dim = true;
            }
        });
    }
}

/// Builds the gallery on a context with no window, in both themes, with the panel and each dialog open: every
/// widget's code runs once. `--smoke` calls it; gives back what went wrong.
pub fn build_headless() -> Result<(), String> {
    for light in [false, true] {
        let ctx = egui::Context::default();
        let mut gallery = Gallery::still(light);
        for (frame, dialog) in [None, Some(0), Some(1), Some(2), Some(3), Some(4)].into_iter().enumerate() {
            if let Some(which) = dialog {
                gallery.open_dialog(which);
            }
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), Vec2::new(1280.0, 1800.0))),
                time: Some(frame as f64),
                ..Default::default()
            };
            let output = crate::headless_pass(&ctx, input, |ui| gallery.ui(ui));
            let triangles = ctx.tessellate(output.shapes, output.pixels_per_point);
            if triangles.is_empty() {
                return Err(format!(
                    "the gallery drew nothing ({} theme)",
                    if light { "Light" } else { "Dark" }
                ));
            }
        }
        if !gallery.panel.is_open() {
            return Err("the panel closed by itself".to_owned());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn builds_without_a_window() {
        super::build_headless().unwrap();
    }
}
