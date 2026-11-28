//! One node on the canvas: where its ports and fields sit, how it is painted, and its field editors. Sizes are
//! the Python page's, in canvas units (the canvas scales them with its zoom).

use std::sync::Arc;

use egui::{Color32, Galley, Painter, Pos2, Rect, Stroke, StrokeKind, Ui, UiBuilder, Vec2, pos2, vec2};
use lum_automation::Status;
use lum_automation::spec::{self, Choices, FieldSpec, NodeSpec};
use serde_json::Value;

use crate::fonts::Weight;
use crate::theme::{self, Tokens};
use crate::widgets::{Button, Checkbox, Combo, HotkeyField, TextArea, TextInput, recorded_key};

pub const HEAD: f32 = 32.0;
pub const ROW: f32 = 22.0;
pub const PAD: f32 = 12.0;
pub const FOOT: f32 = 24.0;
pub const VALUE_COLOUR: Color32 = Color32::from_rgb(0x4C, 0xA7, 0xF5);

/// The colour of a category's header bar and outputs.
pub fn category_colour(cat: &str) -> Color32 {
    spec::category(cat)
        .and_then(|c| lum_theme::Rgb::parse(&c.colour))
        .map_or(Color32::from_rgb(0x88, 0x88, 0x88), |c| Color32::from_rgb(c.r, c.g, c.b))
}

/// What a port carries: `val:` and `field:` ports carry values, `in:` and `out:` pulses.
pub fn is_value(port: &str) -> bool {
    port.starts_with("val:") || port.starts_with("field:")
}

/// A port a wire starts from.
pub fn is_source(port: &str) -> bool {
    port.starts_with("out:") || port.starts_with("val:")
}

/// Whether two ports can be wired together, in either order.
pub fn compatible(a: &str, b: &str) -> bool {
    let (a, b) = (a.split(':').next().unwrap_or_default(), b.split(':').next().unwrap_or_default());
    matches!((a, b), ("out", "in") | ("in", "out") | ("val", "field") | ("field", "val"))
}

fn port_label(port: &str) -> String {
    port.split_once(':').map_or(port, |(_, p)| p).replace('_', " ")
}

/// Every port of a node type, as the picker checks a dragged wire against them.
pub fn ports_of(spec: &NodeSpec) -> Vec<String> {
    let mut out: Vec<String> = spec.ins.iter().map(|p| format!("in:{p}")).collect();
    out.extend(spec.outs.iter().map(|p| format!("out:{p}")));
    out.extend(spec.values.iter().map(|p| format!("val:{p}")));
    out.extend(spec.fields.iter().filter(|f| f.pin).map(|f| format!("field:{}", f.key)));
    out
}

pub struct FieldRow {
    pub field: &'static FieldSpec,
    pub label: Option<(Pos2, Arc<Galley>)>,
    /// Where the editor goes, relative to the node.
    pub editor: Rect,
}

/// Where everything of one node sits, relative to its top-left corner.
pub struct Layout {
    pub size: Vec2,
    pub ports: Vec<(String, Pos2)>,
    pub fields: Vec<FieldRow>,
    pub run: Option<Rect>,
}

impl Layout {
    pub fn port(&self, port: &str) -> Option<Pos2> {
        self.ports.iter().find(|(p, _)| p == port).map(|(_, at)| *at)
    }
}

fn editor_height(spec: &NodeSpec, f: &FieldSpec) -> f32 {
    match f.kind.as_str() {
        "bool" => 24.0,
        "multiline" => match spec.id.as_str() {
            "command" => 96.0,
            "comment" => 140.0,
            _ => 64.0,
        },
        _ => 30.0,
    }
}

pub fn layout(painter: &Painter, tokens: &Tokens, spec: &'static NodeSpec) -> Layout {
    let w = spec.width as f32;
    let left: Vec<String> = spec.ins.iter().map(|p| format!("in:{p}")).collect();
    let mut right: Vec<String> = spec.outs.iter().map(|p| format!("out:{p}")).collect();
    right.extend(spec.values.iter().map(|p| format!("val:{p}")));
    let rows = left.len().max(right.len());
    let y0 = HEAD + 6.0;
    let mut ports = Vec::new();
    for (i, p) in left.into_iter().enumerate() {
        ports.push((p, pos2(0.0, y0 + i as f32 * ROW + ROW / 2.0)));
    }
    for (i, p) in right.into_iter().enumerate() {
        ports.push((p, pos2(w, y0 + i as f32 * ROW + ROW / 2.0)));
    }
    let mut y = y0 + rows as f32 * ROW + if rows > 0 { 6.0 } else { 0.0 };
    let mut fields = Vec::new();
    if !spec.fields.is_empty() {
        let inner = w - 2.0 * PAD;
        let label_font = tokens.font(10.5, Weight::Regular, false);
        for (i, f) in spec.fields.iter().enumerate() {
            if i > 0 {
                y += 4.0;
            }
            let label = (!f.label.is_empty() && f.kind != "bool").then(|| {
                let galley = painter.layout(f.label.clone(), label_font.clone(), tokens.p.text_muted, inner);
                let at = pos2(PAD, y);
                y += galley.size().y + 4.0;
                (at, galley)
            });
            let h = editor_height(spec, f);
            let editor = Rect::from_min_size(pos2(PAD, y), vec2(inner, h));
            if f.pin {
                ports.push((format!("field:{}", f.key), pos2(0.0, editor.center().y)));
            }
            y += h;
            fields.push(FieldRow { field: f, label, editor });
        }
        y += 6.0;
    }
    let run = (spec.is_trigger() || !spec.ins.is_empty())
        .then(|| Rect::from_min_size(pos2(w - 30.0, 6.0), vec2(22.0, 20.0)));
    Layout { size: vec2(w, y + FOOT), ports, fields, run }
}

/// How a node looks this frame.
pub struct Look<'a> {
    pub spec: &'static NodeSpec,
    pub title: &'a str,
    pub selected: bool,
    /// It glows for a moment after a pulse: how much (0 to 1).
    pub flash: f32,
    pub status: Status,
    /// Which of its ports have a wire.
    pub wired: &'a dyn Fn(&str) -> bool,
}

/// The node's body, header, ports and status line, at `origin` on the canvas.
pub fn paint(painter: &Painter, tokens: &Tokens, origin: Pos2, layout: &Layout, look: &Look<'_>) {
    let p = &tokens.p;
    let colour = category_colour(&look.spec.category);
    let r = Rect::from_min_size(origin, layout.size);
    let w = layout.size.x;
    if look.flash > 0.0 {
        painter.rect_stroke(
            r.expand(3.0),
            13.0,
            Stroke::new(6.0, colour.gamma_multiply(0.55 * look.flash)),
            StrokeKind::Middle,
        );
    }
    // selected looks like a selected dashboard box: accent edge and a faint accent tint
    let fill = if look.selected { theme::mix(p.bg_card, p.primary, 0.07) } else { p.bg_card };
    painter.rect_filled(r, 10.0, fill);
    let head = Rect::from_min_size(origin, vec2(w, HEAD));
    painter.rect_filled(head.shrink(0.5), egui::CornerRadius { nw: 10, ne: 10, sw: 0, se: 0 }, p.bg_header);
    painter.line_segment(
        [pos2(r.min.x + 0.5, head.max.y), pos2(r.max.x - 0.5, head.max.y)],
        Stroke::new(1.0, p.border),
    );
    painter.rect_stroke(
        r,
        10.0,
        Stroke::new(1.0, if look.selected { p.primary } else { p.border }),
        StrokeKind::Inside,
    );
    painter.rect_filled(Rect::from_min_size(origin + vec2(10.0, 11.0), vec2(3.0, HEAD - 22.0)), 0.0, colour);

    let title_font = tokens.font(12.0, Weight::SemiBold, false);
    let room = w - 24.0 - if layout.run.is_some() { 34.0 } else { 8.0 };
    let title = elide(painter, look.title, &title_font, room);
    let galley = painter.layout_no_wrap(title, title_font, p.text_primary);
    painter.galley(origin + vec2(20.0, (HEAD - galley.size().y) / 2.0), galley, p.text_primary);
    if let Some(run) = layout.run {
        let run = run.translate(origin.to_vec2());
        painter.rect_filled(run, 5.0, p.bg_card);
        painter.rect_stroke(run, 5.0, Stroke::new(1.0, p.border), StrokeKind::Inside);
        let font = tokens.font(10.0, Weight::SemiBold, false);
        let g = painter.layout_no_wrap("▶".to_owned(), font, p.primary_text);
        painter.galley(run.center() - g.size() / 2.0, g, p.primary_text);
    }

    let port_font = tokens.font(10.5, Weight::Medium, true);
    for (port, at) in &layout.ports {
        if port.starts_with("field:") {
            continue;
        }
        let g = painter.layout_no_wrap(port_label(port), port_font.clone(), p.text_secondary);
        let y = origin.y + at.y - g.size().y / 2.0;
        let x = if port.starts_with("in:") { origin.x + 12.0 } else { origin.x + w - 12.0 - g.size().x };
        painter.galley(pos2(x, y), g, p.text_secondary);
    }
    for (port, at) in &layout.ports {
        paint_port(painter, tokens, port, origin + at.to_vec2(), colour, (look.wired)(port));
    }

    for row in &layout.fields {
        if let Some((at, galley)) = &row.label {
            painter.galley(origin + at.to_vec2(), galley.clone(), p.text_muted);
        }
    }

    let status = &look.status.text;
    if !status.is_empty() {
        let font = tokens.font(10.5, Weight::Medium, true);
        let tone = if status.starts_with('⚠') { theme::DANGER } else { p.text_muted };
        let text = elide(painter, status, &font, w - 24.0);
        let g = painter.layout_no_wrap(text, font, tone);
        let y = origin.y + layout.size.y - FOOT + (FOOT - 4.0 - g.size().y) / 2.0;
        painter.galley(pos2(origin.x + 12.0, y), g, tone);
    }
    if let Some(progress) = look.status.progress {
        let bar = Rect::from_min_size(pos2(origin.x + 10.0, r.max.y - 4.0), vec2(w - 20.0, 2.0));
        painter.rect_filled(bar, 0.0, p.border);
        let done = Rect::from_min_size(bar.min, vec2(bar.width() * progress.clamp(0.0, 1.0), 2.0));
        painter.rect_filled(done, 0.0, colour);
    }
}

pub fn paint_port(painter: &Painter, tokens: &Tokens, port: &str, at: Pos2, colour: Color32, wired: bool) {
    let p = &tokens.p;
    if is_value(port) {
        let pts = vec![at + vec2(0.0, -6.0), at + vec2(6.0, 0.0), at + vec2(0.0, 6.0), at + vec2(-6.0, 0.0)];
        let fill = if wired { VALUE_COLOUR } else { p.bg_card };
        painter.add(egui::Shape::convex_polygon(pts, fill, Stroke::new(1.6, VALUE_COLOUR)));
    } else {
        let c = if port.starts_with("out:") { colour } else { p.primary };
        painter.circle(at, 5.5, if wired { c } else { p.bg_card }, Stroke::new(1.6, c));
    }
}

/// The text cut to fit `width`, with … at the end when it had to be cut.
pub fn elide(painter: &Painter, text: &str, font: &egui::FontId, width: f32) -> String {
    let fits = |t: &str| painter.layout_no_wrap(t.to_owned(), font.clone(), Color32::WHITE).size().x <= width;
    if fits(text) {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let candidate: String = chars[..mid].iter().collect::<String>() + "…";
        if fits(&candidate) { lo = mid } else { hi = mid - 1 }
    }
    chars[..lo].iter().collect::<String>() + "…"
}

/// A field's value as its box shows it: whole numbers without `.0`, as Python's `QLineEdit` did.
fn line_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Number(n) if n.as_f64().is_some_and(|f| f.fract() == 0.0 && !n.is_i64() && !n.is_u64()) => {
            format!("{}", n.as_f64().unwrap_or_default() as i64)
        }
        other => lum_automation::text::py_str(other),
    }
}

/// What a number box stores: a whole number, a decimal, or the text when it is not a number yet.
fn number_value(text: &str) -> Value {
    match text.trim().replace(',', ".").parse::<f64>() {
        Ok(v) if v.is_finite() && v.fract() == 0.0 && v.abs() < 1e15 => Value::from(v as i64),
        Ok(v) if v.is_finite() => Value::from(v),
        _ => Value::from(text),
    }
}

/// Where the editors ask the app for things: dynamic choice lists, and whether a field's hotkey is being
/// recorded.
pub struct FieldEnv<'a> {
    pub choices: &'a dyn Fn(&str) -> Vec<(String, String)>,
    pub recording: Option<&'a str>,
}

/// What a field editor did this frame.
pub enum FieldDid {
    Changed(Value),
    /// Its Record button: start (true) or stop listening for keys.
    Record(bool),
    /// Its … button: pick a file or folder.
    Browse,
}

/// The editors of one node's fields. `wired` fields (a ◆ wire plugged in) are greyed and read only.
pub fn fields(
    ui: &mut Ui,
    origin: Pos2,
    nid: &str,
    layout: &Layout,
    values: &Value,
    wired: &dyn Fn(&str) -> bool,
    env: &FieldEnv<'_>,
) -> Vec<(String, FieldDid)> {
    let mut did = Vec::new();
    for row in &layout.fields {
        let f = row.field;
        let rect = row.editor.translate(origin.to_vec2());
        let value = values.get(&f.key).unwrap_or(&f.default);
        let id = egui::Id::unique(("automation-field", nid, &f.key));
        let mut child = ui.new_child(UiBuilder::new().max_rect(rect).id_salt(id));
        if wired(&format!("field:{}", f.key)) {
            child.disable();
            child.set_opacity(0.6);
        }
        if let Some(d) = editor(&mut child, id, rect, f, value, env) {
            did.push((f.key.clone(), d));
        }
    }
    did
}

fn editor(
    ui: &mut Ui,
    id: egui::Id,
    rect: Rect,
    f: &FieldSpec,
    value: &Value,
    env: &FieldEnv<'_>,
) -> Option<FieldDid> {
    match f.kind.as_str() {
        "bool" => {
            let mut on = lum_automation::text::truthy(value);
            ui.add(Checkbox::new(&mut on, f.label.clone()))
                .changed()
                .then(|| FieldDid::Changed(Value::Bool(on)))
        }
        "multiline" => {
            let mut text = lum_automation::text::py_str(value);
            let name =
                [&f.label, &f.hint, &f.key].into_iter().find(|s| !s.is_empty()).cloned().unwrap_or_default();
            let rows = ((rect.height() - 14.0) / 16.0).max(2.0) as usize;
            let changed = egui::ScrollArea::vertical()
                .id_salt(id)
                .max_height(rect.height())
                .show(ui, |ui| {
                    ui.add(TextArea::new(&mut text).rows(rows).hint(f.hint.clone()).name(name)).changed()
                })
                .inner;
            changed.then(|| FieldDid::Changed(Value::String(text)))
        }
        "choice" => {
            let options = match &f.choices {
                Choices::Fixed(list) => list.clone(),
                Choices::Dynamic(name) => (env.choices)(name),
            };
            let current = lum_automation::text::py_str(value);
            let mut labels: Vec<String> = options.iter().map(|(_, l)| l.clone()).collect();
            let mut values: Vec<String> = options.iter().map(|(v, _)| v.clone()).collect();
            let found = values.iter().position(|v| *v == current);
            let mut index = match found {
                Some(i) => i,
                None if !current.is_empty() => {
                    labels.push(format!("(missing) {current}"));
                    values.push(current.clone());
                    labels.len() - 1
                }
                None => 0,
            };
            let before = index;
            ui.add(Combo::new(id, &mut index, &labels).width(rect.width()));
            (index != before && index < values.len())
                .then(|| FieldDid::Changed(Value::String(values[index].clone())))
        }
        "hotkey" => {
            let combo = lum_automation::text::py_str(value);
            let recording = env.recording == Some(f.key.as_str());
            let response = HotkeyField::new(&combo).recording(recording).show(ui);
            if response.clear.clicked() {
                return Some(FieldDid::Changed(Value::String(String::new())));
            }
            if response.record.clicked() || response.field.clicked() {
                return Some(FieldDid::Record(!recording));
            }
            if recording && let Some(got) = recorded_key(ui) {
                return Some(match got {
                    Some(combo) => FieldDid::Changed(Value::String(combo)),
                    None => FieldDid::Record(false),
                });
            }
            None
        }
        kind => {
            let mut text = line_text(value);
            let hint = if f.hint.is_empty() && kind == "time" { "16:00".to_owned() } else { f.hint.clone() };
            let browse = matches!(kind, "file" | "folder");
            let mut did = None;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let width = if browse { rect.width() - 34.0 } else { rect.width() };
                let name = [&f.label, &f.hint, &f.key]
                    .into_iter()
                    .find(|s| !s.is_empty())
                    .cloned()
                    .unwrap_or_default();
                let edit = TextInput::new(&mut text).hint(hint).width(width).name(name).id(id.with("line"));
                if ui.add(edit).changed() {
                    did = Some(FieldDid::Changed(if kind == "number" {
                        number_value(&text)
                    } else {
                        Value::String(text.clone())
                    }));
                }
                if browse && ui.add(Button::ghost("…")).on_hover_text("Choose").clicked() {
                    did = Some(FieldDid::Browse);
                }
            });
            did
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports_wire_only_to_their_kind() {
        assert!(compatible("out:done", "in:run"));
        assert!(compatible("field:text", "val:text"));
        assert!(!compatible("out:done", "field:text"));
        assert!(!compatible("in:run", "in:stop"));
        assert_eq!(number_value("5"), Value::from(5));
        assert_eq!(number_value("2,5"), Value::from(2.5));
        assert_eq!(number_value("soon"), Value::from("soon"));
        assert_eq!(line_text(&serde_json::json!(60.0)), "60");
        assert_eq!(line_text(&serde_json::json!(0.5)), "0.5");
    }
}
