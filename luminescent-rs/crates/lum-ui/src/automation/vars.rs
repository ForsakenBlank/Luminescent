//! The Variables panel: every variable with its value, to add, change or delete before Save.

use egui::{Context, Ui, vec2};
use serde_json::{Map, Value};

use crate::text::Text;
use crate::theme::{Tokens, Tone};
use crate::widgets::{Button, IconButton, TextInput};

pub struct VarsPanel {
    rows: Vec<(String, String)>,
    /// Something was typed: the panel no longer follows the engine.
    edited: bool,
    fresh: bool,
}

pub enum VarsDone {
    Open,
    Cancelled,
    Saved(Map<String, Value>),
}

impl VarsPanel {
    pub fn new(vars: &Map<String, Value>) -> VarsPanel {
        let mut rows: Vec<(String, String)> =
            vars.iter().map(|(k, v)| (k.clone(), lum_automation::text::py_str(v))).collect();
        rows.sort();
        VarsPanel { rows, edited: false, fresh: true }
    }

    /// A node changed a variable while the panel is open: show it, unless something was typed already.
    pub fn engine_changed(&mut self) {
        self.fresh = true;
    }

    pub fn needs_reload(&self) -> bool {
        self.fresh && !self.edited
    }

    pub fn reload(&mut self, vars: &Map<String, Value>) {
        *self = VarsPanel { fresh: false, ..VarsPanel::new(vars) };
    }

    pub fn show(&mut self, ctx: &Context) -> VarsDone {
        self.fresh = false;
        let tokens = Tokens::of(ctx);
        let mut done = VarsDone::Open;
        let modal = egui::Modal::new(egui::Id::unique("automation-vars")).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.add(Text::new("Variables").px(16.0).semibold());
            ui.add_space(4.0);
            ui.add(
                Text::new("Kept between restarts. Use them with a Variable node, Set variable, or {var:name} in any field.")
                    .px(12.0)
                    .tone(Tone::Muted)
                    .wrap(),
            );
            ui.add_space(8.0);
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| self.table(ui, &tokens));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add(Button::new("+ Add")).clicked() {
                    self.rows.push(("name".to_owned(), String::new()));
                    self.edited = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(Button::primary("Save")).clicked() {
                        let vars = self
                            .rows
                            .iter()
                            .filter(|(k, _)| !k.trim().is_empty())
                            .map(|(k, v)| (k.trim().to_owned(), Value::String(v.clone())))
                            .collect();
                        done = VarsDone::Saved(vars);
                    }
                    if ui.add(Button::ghost("Cancel")).clicked() {
                        done = VarsDone::Cancelled;
                    }
                });
            });
        });
        if matches!(done, VarsDone::Open) && modal.should_close() {
            done = VarsDone::Cancelled;
        }
        done
    }

    fn table(&mut self, ui: &mut Ui, tokens: &Tokens) {
        let mut gone = None;
        ui.horizontal(|ui| {
            ui.add_sized(
                vec2(180.0, 18.0),
                egui::Label::new(egui::RichText::new("Name").color(tokens.p.text_muted)),
            );
            ui.add(egui::Label::new(egui::RichText::new("Value").color(tokens.p.text_muted)));
        });
        for (i, (name, value)) in self.rows.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                let a = ui.add(TextInput::new(name).width(180.0).name(format!("Variable {} name", i + 1)));
                let b = ui.add(TextInput::new(value).width(270.0).name(format!("Variable {} value", i + 1)));
                if a.changed() || b.changed() {
                    self.edited = true;
                }
                if ui.add(IconButton::close().tooltip("Delete")).clicked() {
                    gone = Some(i);
                }
            });
        }
        if let Some(i) = gone {
            self.rows.remove(i);
            self.edited = true;
        }
    }
}
