//! The middle column: the settings of whichever part is picked on the left.

use egui::{Align, Color32, Frame, Id, Margin, Ui, vec2};
use lum_platform::hotkeys::keys;
use lum_theme::{Gradient, Rgb, Role, ThemeStore, palette};
use serde_json::Value;

use super::tools::{TOOLS, Tool};
use super::{EditorOut, Page, ThemeEditor, combo_of};
use crate::picker::{ChipFill, Picked, chip, swatch};
use crate::text::{caps, hint, label};
use crate::theme::{Tokens, Tone, alpha, colour};
use crate::themes::Env;
use crate::themes::page::{FileAsk, FileJob};
use crate::widgets::{Button, Checkbox, Combo, HotkeyField, IconButton, Slider, Surface, TextInput, tip};

/// Where a colour row writes to.
#[derive(Debug, Clone)]
enum Scope {
    /// The theme's own colours.
    Global,
    /// The colours one part (a module, notifications, warnings) has of its own.
    Section(String),
}

/// The roles of the Colours page in their cards.
const GLOBAL_CARDS: [(&str, &[Role]); 4] = [
    ("ACCENT", &[Role::Primary, Role::PrimaryHover, Role::PrimaryMuted, Role::PrimaryText, Role::OnPrimary]),
    (
        "BACKGROUNDS",
        &[Role::BgBase, Role::BgCard, Role::BgCardHover, Role::BgFrame, Role::BgHeader, Role::Border],
    ),
    ("TEXT", &[Role::TextPrimary, Role::TextSecondary, Role::TextMuted]),
    ("FOLDERS", &[Role::FolderColor, Role::FolderBorder]),
];

const SECTION_CARDS: [(&str, &[Role]); 3] = [
    ("ACCENT SHADES", &[Role::PrimaryHover, Role::PrimaryMuted, Role::PrimaryText, Role::OnPrimary]),
    (
        "BACKGROUNDS",
        &[Role::BgBase, Role::BgCard, Role::BgCardHover, Role::BgFrame, Role::BgHeader, Role::Border],
    ),
    ("TEXT", &[Role::TextPrimary, Role::TextSecondary, Role::TextMuted]),
];

const IMAGES: [&str; 6] = ["png", "jpg", "jpeg", "bmp", "gif", "webp"];

fn card<R>(ui: &mut Ui, title: &str, text: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let tokens = Tokens::of(ui.ctx());
    let margin = Margin { left: 16, right: 16, top: 12, bottom: 14 };
    Surface::card(&tokens)
        .margin(margin)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.add(caps(title).px(10.5).semibold().spacing(0.14).tone(Tone::Accent));
            if !text.is_empty() {
                ui.add(hint(text).px(11.5).wrap());
                ui.add_space(2.0);
            }
            add(ui)
        })
        .inner
}

fn head(ui: &mut Ui, title: &str, text: &str) {
    ui.add(label(title).px(18.0).semibold());
    if !text.is_empty() {
        ui.add(hint(text).px(12.0).wrap());
    }
}

impl ThemeEditor {
    pub(super) fn middle(
        &mut self,
        ui: &mut Ui,
        store: &mut ThemeStore,
        picker: &mut crate::picker::ColourDialog,
        env: &Env<'_>,
        out: &mut EditorOut,
    ) {
        let salt = format!("{:?}", self.page);
        egui::ScrollArea::vertical().id_salt(("theme-editor-page", salt)).auto_shrink([false, false]).show(
            ui,
            |ui| {
                let margin = Margin { left: 20, right: 20, top: 18, bottom: 20 };
                Frame::NONE.inner_margin(margin).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 12.0;
                    match self.page.clone() {
                        Page::General => self.general(ui, store, env, out),
                        Page::Colours => self.colours(ui, store, picker, out),
                        Page::Applied => self.applied(ui, env, out),
                        Page::Desktop(key) => {
                            head(ui, &self.section_title(key), "");
                            ui.add(hint("This page comes in a later build.").px(12.0));
                        }
                        Page::Section(key) => self.section_page(ui, picker, &key),
                    }
                });
            },
        );
    }

    // ── General ────────────────────────────────────────────────────────────

    fn general(&mut self, ui: &mut Ui, store: &ThemeStore, env: &Env<'_>, out: &mut EditorOut) {
        head(ui, "General", "The name, type and shape of the theme, and a picture behind the overlay.");
        let mut changed = false;
        card(ui, "NAME AND TYPE", "", |ui| {
            ui.add(label("Name").px(12.5));
            changed |= ui.add(TextInput::new(&mut self.fields.name).name("Name")).changed();
            ui.add(label("Font").px(12.5));
            changed |= ui.add(TextInput::new(&mut self.fields.font).name("Font")).changed();
        });
        card(ui, "KEY", "", |ui| {
            ui.add(
                hint("Press it anywhere to switch to this theme. Next and previous theme keys are in Settings → Overlay & Interface.")
                    .px(11.0)
                    .wrap(),
            );
            let response = HotkeyField::new(&self.fields.hotkey)
                .recording(self.recording)
                .warning(Some(&self.key_warning))
                .show(ui);
            if response.field.clicked() || response.record.clicked() {
                self.recording = !self.recording;
                out.suspend_keys = Some(self.recording);
            }
            if response.clear.clicked() {
                self.fields.hotkey.clear();
                changed = true;
                self.key_changed(store, env);
            }
            if self.recording {
                self.record_key(ui, store, env, out, &mut changed);
            }
        });
        card(ui, "SHAPE", "", |ui| {
            ui.add(label("Corner radius").px(12.5));
            changed |= slider_row(ui, &mut self.fields.radius, 0.0, 30.0, "px", "Corner radius");
            changed |=
                ui.add(Checkbox::new(&mut self.fields.glow, "Accent glow around the overlay")).changed();
        });
        let pictures = card(ui, "PICTURES", "", |ui| {
            let mut asks = Vec::new();
            let a = path_row(
                ui,
                "Background image",
                &mut self.fields.background_image,
                "Shown behind the overlay content.",
                "background_image",
                &mut asks,
            );
            ui.add(label("Image opacity").px(12.5));
            let b = slider_row(ui, &mut self.fields.image_opacity, 0.0, 100.0, "%", "Image opacity");
            let c = path_row(
                ui,
                "Card icon",
                &mut self.fields.icon,
                "Shown on the theme card. Empty: the background image or a preview.",
                "icon",
                &mut asks,
            );
            (a | b | c, asks)
        });
        changed |= pictures.0;
        out.files.extend(pictures.1);
        if changed {
            self.change();
        }
    }

    fn record_key(
        &mut self,
        ui: &mut Ui,
        store: &ThemeStore,
        env: &Env<'_>,
        out: &mut EditorOut,
        changed: &mut bool,
    ) {
        let events = ui.input(|i| i.events.clone());
        for event in events {
            if let egui::Event::Key { key, pressed: true, modifiers, repeat: false, .. } = event {
                self.recording = false;
                out.suspend_keys = Some(false);
                if let Some(combo) = combo_of(modifiers, key) {
                    self.fields.hotkey = combo;
                    *changed = true;
                    self.key_changed(store, env);
                }
                break;
            }
        }
    }

    /// What is wrong with the key, or what else uses it (`_key_conflict`).
    fn key_changed(&mut self, store: &ThemeStore, env: &Env<'_>) {
        let combo = keys::normalize(&self.fields.hotkey);
        self.key_warning = if combo.is_empty() {
            String::new()
        } else if let invalid = keys::validate(&combo, keys::Os::current())
            && !invalid.is_empty()
        {
            invalid
        } else if let Some(other) =
            store.themes().iter().find(|o| o.id() != self.id && keys::normalize(o.hotkey()) == combo)
        {
            format!("Already used by the theme {}", other.name())
        } else {
            (env.key_owner)(&combo).map_or_else(String::new, |owner| format!("Already used by {owner}"))
        };
    }

    // ── Colours ────────────────────────────────────────────────────────────

    fn colours(
        &mut self,
        ui: &mut Ui,
        store: &mut ThemeStore,
        picker: &mut crate::picker::ColourDialog,
        out: &mut EditorOut,
    ) {
        head(
            ui,
            "Colours",
            "Every colour of the theme. Any of them can be a gradient: open one and switch to Gradient. Modules, notifications and warnings use these unless you give them their own (left). Hover a colour to see it in the preview.",
        );
        ui.add(caps("QUICK TOOLS").px(9.5).semibold().spacing(0.14).tone(Tone::Muted));
        self.tool_bar(ui, out);
        self.recent_card(ui);
        let favourites: Vec<Role> = store
            .favourites()
            .iter()
            .filter_map(|f| f.strip_prefix("lum:").and_then(Role::from_key))
            .collect();
        if !favourites.is_empty() {
            card(ui, "★ FAVOURITES", "", |ui| {
                for role in &favourites {
                    if let Some(on) = self.colour_row(ui, picker, *role, &Scope::Global, Some(true), true) {
                        store.set_favourite(&format!("lum:{}", role.key()), on);
                    }
                }
            });
        }
        let starred = store.favourites();
        for (title, roles) in GLOBAL_CARDS {
            card(ui, title, "", |ui| {
                for role in roles {
                    let fav = starred.contains(&format!("lum:{}", role.key()));
                    if let Some(on) = self.colour_row(ui, picker, *role, &Scope::Global, Some(fav), false) {
                        store.set_favourite(&format!("lum:{}", role.key()), on);
                    }
                }
            });
        }
    }

    fn tool_bar(&mut self, ui: &mut Ui, out: &mut EditorOut) {
        let columns = (((ui.available_width() + 6.0) / 146.0).floor() as usize).clamp(1, 4);
        let width = ((ui.available_width() - 6.0 * (columns - 1) as f32) / columns as f32).floor();
        let mut chosen: Option<Tool> = None;
        egui::Grid::new("theme-editor-tools").spacing([6.0, 6.0]).show(ui, |ui| {
            for (i, (tool, text, description)) in TOOLS.iter().enumerate() {
                let button = if *tool == Tool::Main { Button::primary(*text) } else { Button::ghost(*text) };
                if tip(ui.add(button.min_width(width)), *description).clicked() {
                    chosen = Some(*tool);
                }
                if i % columns == columns - 1 {
                    ui.end_row();
                }
            }
        });
        if let Some(tool) = chosen {
            self.open_tool(tool, out);
        }
    }

    /// The last changes, old → new, each with a button to put the old colours back.
    fn recent_card(&mut self, ui: &mut Ui) {
        let tokens = Tokens::of(ui.ctx());
        let mut back: Option<usize> = None;
        let mut undo = false;
        let mut redo = false;
        let (can_undo, can_redo) = (self.history.can_undo(), self.history.can_redo());
        let entries: Vec<(usize, lum_theme::history::Recent)> =
            self.recents.iter().cloned().enumerate().filter(|(_, r)| r.scope == "lum").take(8).collect();
        card(ui, "RECENTLY CHANGED", "", |ui| {
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.add_enabled_ui(can_redo, |ui| {
                        redo = tip(
                            ui.add(IconButton::new("↷").size(26.0).font_px(13.0).ghost()),
                            "Redo (Ctrl+Shift+Z)",
                        )
                        .clicked();
                    });
                    ui.add_enabled_ui(can_undo, |ui| {
                        undo = tip(
                            ui.add(IconButton::new("↶").size(26.0).font_px(13.0).ghost()),
                            "Undo (Ctrl+Z)",
                        )
                        .clicked();
                    });
                });
            });
            if entries.is_empty() {
                ui.add(
                    hint("Nothing yet. Every change shows up here with a button to undo just that one.")
                        .px(11.0)
                        .wrap(),
                );
            }
            for (i, r) in &entries {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let keys: Vec<&String> = r.new.keys().collect();
                    if keys.len() == 1 {
                        value_chip(ui, r.old.get(keys[0]), &tokens);
                        ui.add(hint("→").px(11.0));
                        value_chip(ui, r.new.get(keys[0]), &tokens);
                    } else {
                        strip(ui, r, &keys);
                    }
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        let text = if keys.len() == 1 {
                            "Put the old colour back".to_owned()
                        } else {
                            format!("Put these {} colours back", keys.len())
                        };
                        if tip(ui.add(IconButton::new("↺").size(24.0).font_px(12.0).ghost()), text).clicked()
                        {
                            back = Some(*i);
                        }
                        ui.add(label(r.label.clone()).px(11.5).elide());
                    });
                });
            }
        });
        if undo {
            self.undo();
        }
        if redo {
            self.redo();
        }
        if let Some(i) = back {
            self.reset_recent(i);
        }
    }

    /// One colour: its name, what it colours and a swatch, with a button to make it a gradient. A part's own
    /// colours may also be unset (they then follow the theme).
    #[allow(clippy::too_many_arguments)]
    fn colour_row(
        &mut self,
        ui: &mut Ui,
        picker: &mut crate::picker::ColourDialog,
        role: Role,
        scope: &Scope,
        fav: Option<bool>,
        copy: bool,
    ) -> Option<bool> {
        let (value, unset, target, title) = match scope {
            Scope::Global => (
                Some(self.global(role)),
                false,
                format!("editor:role:{}", role.key()),
                role.label().to_owned(),
            ),
            Scope::Section(key) => (
                self.section_value(key, role),
                true,
                format!("editor:section:{key}:{}", role.key()),
                format!("{}: {}", self.section_title(key), role.label()),
            ),
        };
        let mut clear = false;
        let mut open: Option<Picked> = None;
        let mut gradient = false;
        let mut starred = None;
        let row = ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            if let Some(on) = fav {
                let glyph = if on { "★" } else { "☆" };
                let star = IconButton::new(glyph)
                    .size(24.0)
                    .font_px(14.0)
                    .ghost()
                    .tooltip("Favourite: pin it at the top");
                if ui.add(star).clicked() {
                    starred = Some(!on);
                }
            }
            let controls = 170.0 + 26.0 + if unset { 26.0 } else { 0.0 } + 30.0;
            let room = (ui.available_width() - controls).max(80.0);
            ui.vertical(|ui| {
                ui.set_width(room);
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.add(label(role.label()).px(12.5));
                let help = super::preview::role_help(role);
                if !help.is_empty() {
                    ui.add(hint(help).px(10.5).wrap());
                }
            });
            if swatch(ui, value.as_ref(), if unset { "Theme" } else { "" }, 170.0, &title).clicked() {
                open = Some(value.clone().unwrap_or_else(|| self.global(role)));
            }
            let make = IconButton::new("◐").size(26.0).font_px(12.0).ghost().tooltip(
                "Make it a gradient: adds a second colour that goes with it, then opens the gradient editor",
            );
            if ui.add(make).clicked() {
                gradient = true;
            }
            if unset {
                let back = IconButton::close()
                    .size(26.0)
                    .font_px(11.0)
                    .ghost()
                    .tooltip("Back to the theme's colour");
                clear = ui.add(back).clicked();
            }
        });
        let rect = row.response.rect;
        let tokens = Tokens::of(ui.ctx());
        if matches!(scope, Scope::Global) && !copy {
            if ui.rect_contains_pointer(rect) && ui.is_enabled() {
                self.hover = Some(role);
            }
            let flashing = self.flash.is_some_and(|(r, _)| r == role);
            if flashing {
                ui.painter().rect_filled(rect.expand2(vec2(4.0, 2.0)), 8.0, alpha(tokens.p.primary, 0.12));
            }
            if self.jump == Some(role) {
                ui.scroll_to_rect(rect.expand(40.0), Some(Align::Center));
                let until = ui.input(|i| i.time) + 1.4;
                self.flash = Some((role, until));
                self.jump = None;
            }
        }
        if gradient {
            let start = match &value {
                Some(Picked::Gradient(g)) => Picked::Gradient(g.clone()),
                Some(Picked::Colour(c)) => Picked::Gradient(Gradient::from_colour(*c)),
                None => Picked::Gradient(Gradient::from_colour(self.global(role).first())),
            };
            self.write(scope, role, Some(&start));
            open = Some(start);
        }
        if clear {
            self.write(scope, role, None);
        }
        if let Some(start) = open {
            self.pick(picker, target, &title, start, true);
        }
        starred
    }

    fn write(&mut self, scope: &Scope, role: Role, value: Option<&Picked>) {
        match (scope, value) {
            (Scope::Global, Some(v)) => self.set_global_tracked(role, v),
            (Scope::Section(key), v) => self.set_section_role(key, role, v),
            (Scope::Global, None) => {}
        }
    }

    // ── When it is applied ─────────────────────────────────────────────────

    fn applied(&mut self, ui: &mut Ui, env: &Env<'_>, out: &mut EditorOut) {
        head(ui, "When it is applied", "What else changes when you switch to this theme.");
        let mut changed = false;
        card(ui, "DESKTOP", "", |ui| {
            let mut asks = Vec::new();
            let before = self.fields.wallpaper.clone();
            changed |= path_row(
                ui,
                "Desktop wallpaper",
                &mut self.fields.wallpaper,
                "A picture becomes the desktop wallpaper (through Noctalia, swww, hyprpaper or swaybg). A Wallpaper Engine project.json (or a file in its folder) is played with linux-wallpaperengine. Empty: the wallpaper is left alone.",
                "wallpaper",
                &mut asks,
            );
            if before != self.fields.wallpaper {
                self.wallpaper_missing = missing(&self.fields.wallpaper);
            } else if !self.checked_wallpaper {
                self.wallpaper_missing = missing(&self.fields.wallpaper);
                self.checked_wallpaper = true;
            }
            if self.wallpaper_missing {
                ui.add(
                    label(
                        "This file was not found, so switching to the theme leaves the wallpaper as it is.",
                    )
                    .px(11.5)
                    .tone(Tone::Danger)
                    .wrap(),
                );
            }
            out.files.extend(asks);
        });
        card(
            ui,
            "SETTINGS PROFILE",
            "Profiles are made in Settings (top of the page). Each holds its own hotkeys, sizes, sounds…",
            |ui| {
                let mut names = vec!["Default settings".to_owned()];
                let mut ids = vec![String::new()];
                for (id, name) in &env.profiles {
                    names.push(format!("Profile: {name}"));
                    ids.push(id.clone());
                }
                let mut index = ids.iter().position(|id| *id == self.fields.profile).unwrap_or(0);
                if ui
                    .add(Combo::new(Id::unique("theme-editor-profile"), &mut index, &names).width(260.0))
                    .changed()
                {
                    self.fields.profile = ids[index].clone();
                    changed = true;
                }
            },
        );
        if changed {
            self.change();
        }
    }

    // ── a module, notifications or warnings ────────────────────────────────

    fn section_page(&mut self, ui: &mut Ui, picker: &mut crate::picker::ColourDialog, key: &str) {
        let title = self.section_title(key);
        let about = super::EXTRA
            .iter()
            .find(|e| e.0 == key)
            .map_or_else(|| format!("Every window of {title}."), |e| e.3.to_owned());
        head(
            ui,
            &title,
            &format!("{about} Anything left on THEME follows the theme, so it changes with it."),
        );
        let scope = Scope::Section(key.to_owned());
        card(
            ui,
            "ACCENT",
            "Quickest: one colour, and its lighter, darker and muted shades are worked out for you.",
            |ui| {
                let accent = self.section(key).accent.map(Picked::Colour);
                let mut open = false;
                let mut clear = false;
                let mut generate = false;
                ui.horizontal(|ui| {
                    open = swatch(ui, accent.as_ref(), "Theme", 170.0, &format!("{title}: accent")).clicked();
                    clear = ui
                        .add(
                            IconButton::close()
                                .size(26.0)
                                .font_px(11.0)
                                .ghost()
                                .tooltip("Back to the theme's accent"),
                        )
                        .clicked();
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        let match_all = Button::ghost("Match all its colours to it");
                        generate = tip(
                            ui.add(match_all),
                            "Backgrounds, text and borders for this part only, picked to go with its accent",
                        )
                        .clicked();
                    });
                });
                if open {
                    let start = accent.clone().unwrap_or_else(|| self.global(Role::Primary));
                    self.pick(
                        picker,
                        format!("editor:section:{key}:accent"),
                        &format!("{title}: accent"),
                        start,
                        false,
                    );
                }
                if clear {
                    self.set_section_accent(key, None);
                }
                if generate {
                    self.generate_section(key);
                }
            },
        );
        for (card_title, roles) in SECTION_CARDS {
            card(ui, card_title, "", |ui| {
                for role in roles {
                    self.colour_row(ui, picker, *role, &scope, None, false);
                }
            });
        }
        card(ui, "SHAPE", "", |ui| {
            let current = self.section(key).radius;
            let mut own = current.is_some();
            let mut radius = current.map_or(self.fields.radius, |r| r as f64);
            let mut changed = ui.add(Checkbox::new(&mut own, "Its own corner radius")).changed();
            ui.add_enabled_ui(own, |ui| {
                changed |= slider_row(ui, &mut radius, 0.0, 30.0, "px", "Its own corner radius");
            });
            if changed {
                self.set_section_radius(key, own.then_some(radius as i64));
            }
        });
        ui.horizontal(|ui| {
            ui.menu_button("Copy the look of…", |ui| {
                if crate::widgets::menu::item(ui, "The theme's own colours (clear this part)").clicked() {
                    self.copy_section(key, None);
                }
                let others: Vec<String> =
                    self.t.sections().into_iter().map(|(k, _)| k).filter(|k| k != key).collect();
                if !others.is_empty() {
                    crate::widgets::menu::separator(ui);
                }
                for other in others {
                    let name = self.section_title(&other);
                    if crate::widgets::menu::item(ui, &name).clicked() {
                        self.copy_section(key, Some(&other));
                    }
                }
            });
        });
    }

    fn generate_section(&mut self, key: &str) {
        let s = self.section(key);
        let Some(accent) = s.accent else {
            self.message("Match its colours", "Pick an accent for it first.");
            return;
        };
        let background = s.colour(Role::BgBase).unwrap_or_else(|| self.palette().get(Role::BgBase));
        let light = background.luminance() > 140.0;
        for (role, value) in palette::generate(accent, light) {
            if role != "primary"
                && let (Some(role), Some(c)) = (Role::from_key(&role), Rgb::parse(&value))
            {
                self.set_section_role(key, role, Some(&Picked::Colour(c)));
            }
        }
    }
}

fn missing(text: &str) -> bool {
    let path = text.trim().trim_matches('"');
    if path.is_empty() {
        return false;
    }
    let expanded = match path.strip_prefix("~/") {
        Some(rest) => lum_core::paths::user_home().join(rest),
        None => std::path::PathBuf::from(path),
    };
    !expanded.exists()
}

/// A slider with its value next to it. Gives back whether it moved.
fn slider_row(ui: &mut Ui, value: &mut f64, lo: f64, hi: f64, unit: &str, name: &str) -> bool {
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 70.0).max(100.0);
        let moved = ui.add(Slider::new(value, lo..=hi).step(1.0).width(width).name(name)).changed();
        ui.add(crate::text::mono(format!("{} {unit}", *value as i64)).px(11.5).tone(Tone::Secondary));
        moved
    })
    .inner
}

/// A label, a text box with Browse… and a clear button, and a line of help under it.
fn path_row(
    ui: &mut Ui,
    title: &str,
    text: &mut String,
    help: &str,
    field: &str,
    asks: &mut Vec<FileAsk>,
) -> bool {
    ui.add(label(title).px(12.5));
    let mut changed = false;
    ui.horizontal(|ui| {
        let room = (ui.available_width() - 130.0).max(120.0);
        changed |= ui.add(TextInput::new(text).width(room).name(title)).changed();
        if ui.add(Button::new("Browse…")).clicked() {
            let request = lum_platform::dialogs::Request::new("Choose a file")
                .folder(Some(lum_core::paths::user_home()))
                .filter("Images", &IMAGES);
            let request = if field == "wallpaper" {
                lum_platform::dialogs::Request::new("Choose a file")
                    .folder(Some(lum_core::paths::user_home()))
            } else {
                request
            };
            asks.push(FileAsk { job: FileJob::Browse(field.to_owned()), save: false, request });
        }
        if ui.add(IconButton::close().size(26.0).font_px(11.0).ghost().tooltip("Clear")).clicked() {
            text.clear();
            changed = true;
        }
    });
    ui.add(hint(help).px(11.0).wrap());
    changed
}

fn value_chip(ui: &mut Ui, value: Option<&Value>, tokens: &Tokens) {
    match value.and_then(super::picked_from_value) {
        Some(Picked::Colour(c)) => {
            chip(ui, ChipFill::Colour(colour(c)), 18.0, false, &c.hex(), tokens);
        }
        Some(Picked::Gradient(g)) => {
            chip(ui, ChipFill::Gradient(crate::theme::Gradient::from(&g)), 18.0, false, "gradient", tokens);
        }
        None => {
            chip(ui, ChipFill::Colour(Color32::TRANSPARENT), 18.0, false, "not set", tokens);
        }
    }
}

/// Old colours on top, new below: what a quick tool changed.
fn strip(ui: &mut Ui, r: &lum_theme::history::Recent, keys: &[&String]) {
    let shown = keys.len().min(12);
    let (rect, response) = ui.allocate_exact_size(vec2(shown as f32 * 9.0 + 2.0, 20.0), egui::Sense::hover());
    for (row, values) in [&r.old, &r.new].into_iter().enumerate() {
        for (i, key) in keys.iter().take(shown).enumerate() {
            let c = values
                .get(*key)
                .and_then(super::picked_from_value)
                .map_or(Color32::TRANSPARENT, |p| colour(p.first()));
            let cell = egui::Rect::from_min_size(
                rect.min + vec2(1.0 + i as f32 * 9.0, 1.0 + row as f32 * 9.0),
                vec2(8.0, 8.0),
            );
            ui.painter().rect_filled(cell, 0.0, c);
        }
    }
    tip(response, "Top: before · bottom: after");
}
