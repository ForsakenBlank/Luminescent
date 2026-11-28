//! The node picker: a search box over every node type. With a wire being dragged it lists only the nodes that
//! have a port that fits. Each row shows the node's name and what it does, so you can pick without hovering.

use egui::{Align2, Context, Key, Pos2, Rect, Sense, Ui, pos2, vec2};
use lum_automation::spec::{self, NodeSpec};

use super::node::{category_colour, compatible, ports_of};
use crate::fonts::Weight;
use crate::theme::Tokens;
use crate::widgets::TextInput;

const WIDTH: f32 = 440.0;
const HEIGHT: f32 = 560.0;

pub struct Picker {
    /// Where it opens on the screen.
    pub at: Pos2,
    /// Where the new node goes on the canvas.
    pub scene: Pos2,
    /// The wire being dragged: from which node and port.
    pub link: Option<(String, String)>,
    search: String,
    current: usize,
    fresh: bool,
}

pub enum Picked {
    Open,
    Closed,
    Type(String),
}

fn help_line(help: &str) -> String {
    if help.chars().count() < 140 {
        return help.to_owned();
    }
    let cut: String = help.chars().take(139).collect();
    match cut.rsplit_once(' ') {
        Some((head, _)) => format!("{head}…"),
        None => format!("{cut}…"),
    }
}

impl Picker {
    pub fn new(at: Pos2, scene: Pos2, link: Option<(String, String)>) -> Picker {
        Picker { at, scene, link, search: String::new(), current: 0, fresh: true }
    }

    fn fits(&self, t: &NodeSpec) -> bool {
        match &self.link {
            None => true,
            Some((_, want)) => ports_of(t).iter().any(|p| compatible(want, p)),
        }
    }

    /// The types it lists, by category, as the Python list orders them.
    fn listed(&self) -> Vec<(&'static spec::Category, Vec<&'static NodeSpec>)> {
        let q = self.search.trim().to_lowercase();
        let specs = spec::specs();
        specs
            .categories
            .iter()
            .filter_map(|cat| {
                let types: Vec<&NodeSpec> = specs
                    .nodes
                    .iter()
                    .filter(|t| t.category == cat.id && self.fits(t))
                    .filter(|t| {
                        q.is_empty()
                            || t.title.to_lowercase().contains(&q)
                            || t.help.to_lowercase().contains(&q)
                            || t.id.contains(&q)
                    })
                    .collect();
                (!types.is_empty()).then_some((cat, types))
            })
            .collect()
    }

    pub fn show(&mut self, ctx: &Context, screen: Rect) -> Picked {
        let tokens = Tokens::of(ctx);
        // the click that opened it is still in this frame's input
        let opening = self.fresh;
        let size = vec2(tokens.fs(WIDTH), tokens.fs(HEIGHT));
        let at = pos2(
            self.at.x.min(screen.max.x - size.x).max(screen.min.x),
            self.at.y.min(screen.max.y - size.y).max(screen.min.y),
        );
        let listed = self.listed();
        let count: usize = listed.iter().map(|(_, t)| t.len()).sum();
        let (up, down, enter, escape) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, Key::Enter),
                i.consume_key(egui::Modifiers::NONE, Key::Escape),
            )
        });
        if escape {
            return Picked::Closed;
        }
        if down && self.current + 1 < count {
            self.current += 1;
        }
        if up {
            self.current = self.current.saturating_sub(1);
        }
        self.current = self.current.min(count.saturating_sub(1));
        let mut picked = None;
        if enter && count > 0 {
            picked = listed.iter().flat_map(|(_, t)| t.iter()).nth(self.current).map(|t| t.id.clone());
        }
        let area = egui::Area::new(egui::Id::unique("automation-picker"))
            .order(egui::Order::Foreground)
            .fixed_pos(at)
            .pivot(Align2::LEFT_TOP)
            .show(ctx, |ui| {
                egui::Frame::menu(ui.style()).inner_margin(10.0).show(ui, |ui| {
                    ui.set_min_size(size - vec2(20.0, 20.0));
                    ui.set_max_size(size - vec2(20.0, 20.0));
                    let before = self.search.clone();
                    let search = ui.add(
                        TextInput::new(&mut self.search)
                            .hint("Add a node: type to search…")
                            .id(egui::Id::unique("automation-picker-search")),
                    );
                    if std::mem::take(&mut self.fresh) {
                        search.request_focus();
                    }
                    if self.search != before {
                        self.current = 0;
                    }
                    ui.add_space(8.0);
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        let mut index = 0;
                        for (cat, types) in &listed {
                            let colour = category_colour(&cat.id);
                            ui.label(
                                egui::RichText::new(cat.label.to_uppercase())
                                    .font(tokens.font(11.0, Weight::SemiBold, true))
                                    .color(colour),
                            );
                            for t in types {
                                let current = index == self.current;
                                let row = row(ui, &tokens, t, colour, current);
                                if current && (up || down) {
                                    row.scroll_to_me(None);
                                }
                                if row.clicked() {
                                    picked = Some(t.id.clone());
                                }
                                index += 1;
                            }
                            ui.add_space(6.0);
                        }
                    });
                });
            });
        if let Some(id) = picked {
            return Picked::Type(id);
        }
        let outside = !opening
            && ctx.input(|i| {
                i.pointer.any_pressed()
                    && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p))
            });
        if outside { Picked::Closed } else { Picked::Open }
    }
}

fn row(ui: &mut Ui, tokens: &Tokens, t: &NodeSpec, colour: egui::Color32, current: bool) -> egui::Response {
    let p = &tokens.p;
    let width = ui.available_width();
    let title_font = tokens.font(14.0, Weight::SemiBold, false);
    let help_font = tokens.font(12.0, Weight::Regular, false);
    let title = ui.painter().layout_no_wrap(t.title.clone(), title_font, p.text_primary);
    let help = (!t.help.is_empty())
        .then(|| ui.painter().layout(help_line(&t.help), help_font, p.text_muted, width - 34.0));
    let height = 14.0 + title.size().y + help.as_ref().map_or(0.0, |h| h.size().y + 2.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &t.title));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if current || response.hovered() {
            painter.rect_filled(rect, tokens.rs() - 4.0, p.bg_card_hover);
        }
        let bar = egui::Rect::from_min_size(
            rect.min + vec2(10.0, 7.0),
            vec2(4.0, tokens.fs(30.0).min(height - 14.0)),
        );
        painter.rect_filled(bar, 2.0, colour);
        let mut y = rect.min.y + 7.0;
        let title_h = title.size().y;
        painter.galley(pos2(rect.min.x + 24.0, y), title, p.text_primary);
        y += title_h + 2.0;
        if let Some(help) = help {
            painter.galley(pos2(rect.min.x + 24.0, y), help, p.text_muted);
        }
    }
    response
}
