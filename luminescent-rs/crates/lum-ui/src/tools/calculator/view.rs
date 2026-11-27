//! The calculator itself, drawn into whatever room it is given: the floating panel, the overlay's Calculator
//! page or a dashboard box. It holds what is typed; the history and pins live in the `Book`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use egui::{Align, Key, Layout, Modifiers, Rect, Sense, Stroke, TextEdit, Ui, UiBuilder, pos2, vec2};
use jiff::civil::{Date, DateTime};
use lum_calc::{Base, Book, Entry};

use crate::fonts::Weight;
use crate::text::{Text, caps};
use crate::theme::{Tokens, Tone};
use crate::tools::clip;
use crate::widgets::{Button, menu};

const ROW_H: f32 = 19.0;
const ROW_GAP: f32 = 7.0;
const KEY_H: f32 = 36.0;
const KEY_GAP: f32 = 4.0;
const KEYS: [[&str; 5]; 5] = [
    ["C", "(", ")", "%", "÷"],
    ["7", "8", "9", "√", "×"],
    ["4", "5", "6", "^", "−"],
    ["1", "2", "3", "π", "+"],
    ["0", ".", "ans", "⌫", "="],
];
const OPS: [&str; 10] = ["÷", "×", "−", "+", "%", "^", "√", "π", "(", ")"];

pub const PIN_COLOURS: [(&str, &str); 6] = [
    ("Red", "#E5484D"),
    ("Orange", "#F76B15"),
    ("Yellow", "#FFC53D"),
    ("Green", "#30A46C"),
    ("Blue", "#3E63DD"),
    ("Purple", "#8E4EC6"),
];

/// What the calculator needs to know about the world to work a sum out.
pub struct Env<'a> {
    pub rates: &'a HashMap<String, f64>,
    pub today: Date,
    pub now: DateTime,
}

/// Things the view can not do by itself: they need a dialog, the overlay or the settings.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Pin(usize),
    Rename(usize),
    NewFolder(usize),
    ClearHistory,
    OpenSettings,
    RefreshRates,
    Notify(String),
    /// Ctrl+Enter worked the sum out and copied it: close the panel (or the overlay).
    Close,
}

#[derive(Debug)]
pub struct View {
    /// Tells the panel's input from the page's: both can be on screen.
    name: &'static str,
    pub input: String,
    pub preview: String,
    recall: Option<usize>,
    pub keypad: bool,
    /// The page makes everything bigger and always has its keypad.
    pub embedded: bool,
    focus: bool,
    seen: String,
    close_at: Option<Instant>,
}

impl View {
    pub fn new(name: &'static str, embedded: bool) -> View {
        View {
            name,
            input: String::new(),
            preview: String::new(),
            recall: None,
            keypad: embedded,
            embedded,
            focus: true,
            seen: String::new(),
            close_at: None,
        }
    }

    pub fn focus_input(&mut self) {
        self.focus = true;
    }

    fn input_id(&self) -> egui::Id {
        egui::Id::unique(("calculator-input", self.name))
    }

    /// Types `text` where the cursor is.
    pub fn insert(&mut self, ctx: &egui::Context, text: &str) {
        let id = self.input_id();
        let mut state = TextEdit::load_state(ctx, id).unwrap_or_default();
        let at = state.cursor.char_range().map_or(self.input.chars().count(), |r| {
            usize::from(r.primary.index.max(r.secondary.index)).min(self.input.chars().count())
        });
        let byte = self.input.char_indices().nth(at).map_or(self.input.len(), |(b, _)| b);
        self.input.insert_str(byte, text);
        let end = egui::text::CCursor::new(at + text.chars().count());
        state.cursor.set_char_range(Some(egui::text::CCursorRange::one(end)));
        state.store(ctx, id);
        self.focus = true;
    }

    pub fn load(&mut self, text: &str) {
        self.input = text.to_owned();
        self.focus = true;
    }

    fn backspace(&mut self) {
        self.input.pop();
    }

    fn live(&mut self, book: &Book, env: &Env<'_>) {
        if self.seen == self.input {
            return;
        }
        self.seen.clone_from(&self.input);
        let t = self.input.trim();
        self.preview = if t.is_empty() {
            String::new()
        } else {
            book.compute(t, env.rates, env.today).map(|c| format!("= {}", c.result)).unwrap_or_default()
        };
    }

    /// Works the sum out, adds it to the history and clears the box. `false` when it is not a sum (the preview
    /// says why).
    fn calculate(&mut self, book: &mut Book, env: &Env<'_>) -> bool {
        let t = self.input.trim().to_owned();
        if t.is_empty() {
            return false;
        }
        match book.compute(&t, env.rates, env.today) {
            Ok(done) => {
                book.add_history(&done.expr, done.value.as_ref(), &done.result, env.now);
                self.input.clear();
                self.seen.clear();
                self.preview.clear();
                self.recall = None;
                true
            }
            Err(e) => {
                self.preview = e.preview();
                false
            }
        }
    }

    /// Copies what the box works out to, or the last result when it is empty.
    fn copy_result(&mut self, book: &Book, env: &Env<'_>) -> bool {
        let t = self.input.trim();
        let text = if !t.is_empty() {
            match book.compute(t, env.rates, env.today) {
                Ok(done) => match done.value {
                    Some(value) => book.plain(&entry_of(&done.result, Some(&value))),
                    None => done.result,
                },
                Err(_) => {
                    self.preview = "⚠ nothing to copy".to_owned();
                    return false;
                }
            }
        } else if let Some(last) = book.history().last() {
            book.plain(last)
        } else {
            return false;
        };
        clip::copy(text);
        self.preview = "copied ✓".to_owned();
        true
    }

    fn press(&mut self, key: &str, book: &mut Book, env: &Env<'_>, ctx: &egui::Context) {
        match key {
            "=" => {
                self.calculate(book, env);
            }
            "C" => self.input.clear(),
            "⌫" => self.backspace(),
            "√" => self.insert(ctx, "sqrt("),
            "π" => self.insert(ctx, "pi"),
            "−" => self.insert(ctx, "-"),
            other => self.insert(ctx, other),
        }
        self.focus = true;
    }

    /// The keys the box listens to: Enter, Ctrl+Enter, ↑ and ↓ through the history, and Ctrl+C on an empty box.
    fn keys(&mut self, ui: &Ui, book: &mut Book, env: &Env<'_>, events: &mut Vec<Event>) {
        if !ui.memory(|m| m.has_focus(self.input_id())) {
            return;
        }
        let (ctrl_enter, enter, up, down, copy) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::COMMAND, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                self.input.is_empty() && i.events.iter().any(|e| matches!(e, egui::Event::Copy)),
            )
        });
        if ctrl_enter {
            // calculate, copy and close (or just copy and close the last result)
            let worked = self.input.trim().is_empty() || self.calculate(book, env);
            if worked && self.copy_result(book, env) {
                self.close_at = Some(Instant::now() + Duration::from_millis(250));
            }
        } else if enter {
            self.calculate(book, env);
        }
        if up || down {
            let count = book.history().len();
            if count > 0 {
                let at = self.recall.unwrap_or(count);
                let at = if up { at.saturating_sub(1) } else { (at + 1).min(count) };
                self.recall = Some(at);
                self.input = book.history().get(at).map_or(String::new(), |h| h.expr().to_owned());
                self.seen.clear();
            }
        }
        if copy && let Some(last) = book.history().last() {
            clip::copy(book.plain(last));
            self.preview = "copied ✓".to_owned();
        }
        if let Some(at) = self.close_at
            && Instant::now() >= at
        {
            self.close_at = None;
            events.push(Event::Close);
        }
    }

    /// Draws the calculator into `rect`.
    pub fn show(&mut self, ui: &mut Ui, rect: Rect, book: &mut Book, env: &Env<'_>) -> Vec<Event> {
        let tokens = Tokens::of(ui.ctx());
        let mut events = Vec::new();
        let keypad_h = if self.keypad { 5.0 * KEY_H + 4.0 * KEY_GAP + 10.0 } else { 0.0 };
        let input_px = if self.embedded { 32.0 } else { 26.0 };
        let display_h = 14.0 + 22.0 + 8.0 + input_px * 1.5 + 12.0;
        let tape = Rect::from_min_max(
            rect.min,
            pos2(rect.max.x, (rect.max.y - keypad_h - display_h).max(rect.min.y)),
        );
        let display =
            Rect::from_min_max(pos2(rect.min.x, tape.max.y), pos2(rect.max.x, tape.max.y + display_h));
        let keys = Rect::from_min_max(pos2(rect.min.x, display.max.y), rect.max);

        self.keys(ui, book, env, &mut events);
        self.tape(ui, tape, book, &tokens, &mut events);
        self.display(ui, display, book, env, input_px, &tokens, &mut events);
        if self.keypad {
            self.keypad(ui, keys, book, env);
        }
        self.live(book, env);
        if let Some(at) = self.close_at {
            ui.ctx().request_repaint_after(at.saturating_duration_since(Instant::now()));
        }
        events
    }

    fn tape(&mut self, ui: &mut Ui, rect: Rect, book: &mut Book, tokens: &Tokens, events: &mut Vec<Event>) {
        let inner = Rect::from_min_max(rect.min + vec2(14.0, 10.0), rect.max - vec2(14.0, 3.0));
        ui.painter().hline(rect.x_range(), rect.max.y - 0.5, Stroke::new(1.0, tokens.p.border));
        let mut tape = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Min)));
        tape.set_clip_rect(inner.intersect(ui.clip_rect()));
        tape.spacing_mut().item_spacing.y = ROW_GAP;
        let mut act = None;
        self.pins(&mut tape, book, tokens, &mut act);
        self.history(&mut tape, book, tokens, &mut act);
        if let Some(act) = act {
            self.apply(ui.ctx(), act, book, events);
        }
    }

    fn pins(&mut self, ui: &mut Ui, book: &Book, tokens: &Tokens, act: &mut Option<Act>) {
        let pins = book.pins();
        if pins.is_empty() {
            return;
        }
        let mut folders: Vec<&str> = pins.iter().map(Entry::folder).collect();
        folders.sort_by_key(|f| (!f.is_empty(), f.to_lowercase()));
        folders.dedup();
        let rows: usize = folders
            .iter()
            .map(|f| pins.iter().filter(|p| p.folder() == *f).count() + usize::from(!f.is_empty()))
            .sum();
        let height = rows.min(6) as f32 * (ROW_H + ROW_GAP);
        egui::ScrollArea::vertical()
            .id_salt(("pins", self.name))
            .max_height(height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = ROW_GAP;
                for folder in folders {
                    if !folder.is_empty() {
                        header_row(ui, folder, None, tokens);
                    }
                    for (i, pin) in pins.iter().enumerate().filter(|(_, p)| p.folder() == folder) {
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                        paint_row(ui, rect, &response, tokens, pin.var(), pin.result(), true, pin.colour());
                        let tip = format!("{}\n{}\nClick to use it in a sum", pin.name(), pin.expr());
                        let response = response.on_hover_text(tip);
                        if response.clicked() {
                            *act = Some(Act::Insert(pin.var().to_owned()));
                        }
                        response.context_menu(|ui| pin_menu(ui, book, i, pin, act));
                    }
                }
            });
    }

    fn history(&mut self, ui: &mut Ui, book: &Book, tokens: &Tokens, act: &mut Option<Act>) {
        egui::ScrollArea::vertical()
            .id_salt(("history", self.name))
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = ROW_GAP;
                let mut last_day = "";
                for (i, h) in book.history().iter().enumerate() {
                    let day = h.time().get(..10).unwrap_or_default();
                    if day != last_day {
                        last_day = day;
                        header_row(ui, &day_name(day, jiff::ZonedDateTime::now().date()), None, tokens);
                    }
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                    paint_row(ui, rect, &response, tokens, h.expr(), h.result(), false, "");
                    let response = response.on_hover_text(
                        "Click: insert the result · double-click: edit the sum · right-click: pin, copy…",
                    );
                    if response.double_clicked() {
                        *act = Some(Act::Load(h.expr().to_owned()));
                    } else if response.clicked() {
                        let text = if h.value().is_some() { book.plain(h) } else { h.result().to_owned() };
                        *act = Some(Act::Insert(text));
                    }
                    response.context_menu(|ui| history_menu(ui, book, i, h, act));
                }
            });
    }

    #[allow(clippy::too_many_arguments)]
    fn display(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        book: &Book,
        env: &Env<'_>,
        input_px: f32,
        tokens: &Tokens,
        events: &mut Vec<Event>,
    ) {
        let inner = Rect::from_min_max(rect.min + vec2(14.0, 14.0), rect.max - vec2(14.0, 12.0));
        let mut col = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Min)));
        col.spacing_mut().item_spacing.y = 8.0;
        col.allocate_ui_with_layout(vec2(inner.width(), 22.0), Layout::left_to_right(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let tip = "Copy the result (Ctrl+Enter: calculate, copy and close)";
            if ui
                .add(
                    crate::widgets::IconButton::shape(crate::widgets::IconShape::Copy)
                        .size(22.0)
                        .ghost()
                        .tooltip(tip),
                )
                .clicked()
            {
                self.copy_result(book, env);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add(Text::new(self.preview.clone()).px(12.0).mono().tone(Tone::Muted));
            });
        });
        let font = tokens.font(input_px, Weight::Medium, true);
        let id = self.input_id();
        let edit = TextEdit::singleline(&mut self.input)
            .id(id)
            .font(font)
            .hint_text("12*4.5 + 10%")
            .align(egui::Align2::RIGHT_CENTER)
            .frame(egui::Frame::NONE)
            .desired_width(inner.width())
            .text_color(tokens.p.text_primary);
        let response = col.add(edit);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, "Calculator input"));
        if std::mem::take(&mut self.focus) {
            response.request_focus();
        }
        let _ = events;
    }

    fn keypad(&mut self, ui: &mut Ui, rect: Rect, book: &mut Book, env: &Env<'_>) {
        let tokens = Tokens::of(ui.ctx());
        let left = rect.min.x + 10.0;
        let width = (rect.width() - 20.0 - 4.0 * KEY_GAP) / 5.0;
        let mut pressed = None;
        for (r, row) in KEYS.iter().enumerate() {
            for (c, key) in row.iter().enumerate() {
                let min =
                    pos2(left + c as f32 * (width + KEY_GAP), rect.min.y + r as f32 * (KEY_H + KEY_GAP));
                let area = Rect::from_min_size(min, vec2(width, KEY_H));
                if key_button(ui, self.name, area, key, &tokens).clicked() {
                    pressed = Some(*key);
                }
            }
        }
        if let Some(key) = pressed {
            self.press(key, book, env, ui.ctx());
        }
    }

    fn apply(&mut self, ctx: &egui::Context, act: Act, book: &mut Book, events: &mut Vec<Event>) {
        match act {
            Act::Insert(text) => self.insert(ctx, &text),
            Act::Load(text) => self.load(&text),
            Act::CopyText(text) => {
                clip::copy(text);
                self.preview = "copied ✓".to_owned();
            }
            Act::DeleteHistory(i) => book.delete_history(i),
            Act::Pin(i) => events.push(Event::Pin(i)),
            Act::Rename(i) => events.push(Event::Rename(i)),
            Act::NewFolder(i) => events.push(Event::NewFolder(i)),
            Act::Recalc(i) => {
                if let Err(message) = book.recalc_pin(i) {
                    events.push(Event::Notify(message));
                }
            }
            Act::SetPin(i, folder, colour) => book.set_pin(i, folder.as_deref(), colour.as_deref()),
            Act::Unpin(i) => book.unpin(i),
        }
    }
}

#[derive(Debug)]
enum Act {
    Insert(String),
    Load(String),
    CopyText(String),
    DeleteHistory(usize),
    Pin(usize),
    Rename(usize),
    NewFolder(usize),
    Recalc(usize),
    SetPin(usize, Option<String>, Option<String>),
    Unpin(usize),
}

fn entry_of(result: &str, value: Option<&lum_calc::Num>) -> Entry {
    let mut map = serde_json::Map::new();
    map.insert("result".into(), serde_json::json!(result));
    map.insert("value".into(), value.map_or(serde_json::Value::Null, lum_calc::Num::to_json));
    Entry(map)
}

/// "Today", "Yesterday", "Mon 23 Nov" (with the year when it is not this one).
pub fn day_name(day: &str, today: Date) -> String {
    let Ok(d) = day.parse::<Date>() else { return "Earlier".to_owned() };
    if d == today {
        return "Today".to_owned();
    }
    if today.yesterday().is_ok_and(|y| y == d) {
        return "Yesterday".to_owned();
    }
    let text = lum_calc::fmt_date(d);
    // fmt_date is "Mon 23 Nov 2026"
    let mut parts = text.rsplitn(2, ' ');
    let year = parts.next().unwrap_or_default();
    let rest = parts.next().unwrap_or_default();
    if d.year() == today.year() { rest.to_owned() } else { format!("{rest} {year}") }
}

fn header_row(ui: &mut Ui, text: &str, colour: Option<egui::Color32>, tokens: &Tokens) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
    let colour = colour.unwrap_or(tokens.p.text_muted);
    let galley =
        ui.painter().layout_job(caps(text).px(9.5).semibold().spacing(0.1).colour(colour).job(tokens));
    let y = rect.center().y;
    ui.painter().galley(pos2(rect.min.x, y - galley.size().y / 2.0), galley.clone(), colour);
    let x = rect.min.x + galley.size().x + 8.0;
    ui.painter().hline(x..=rect.max.x, y, Stroke::new(1.0, tokens.p.border));
}

#[allow(clippy::too_many_arguments)]
fn paint_row(
    ui: &Ui,
    rect: Rect,
    response: &egui::Response,
    tokens: &Tokens,
    left: &str,
    result: &str,
    pinned: bool,
    colour: &str,
) {
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect, 0.0, tokens.p.bg_card_hover);
    }
    let font = tokens.font(12.0, Weight::Regular, true);
    let right = painter.layout_no_wrap(result.to_owned(), font.clone(), tokens.p.text_primary);
    let right_pos = pos2(rect.max.x - right.size().x - 2.0, rect.center().y - right.size().y / 2.0);
    let avail = (rect.width() - 16.0 - right.size().x - 12.0).max(10.0);
    painter.galley(right_pos, right.clone(), tokens.p.text_primary);
    if pinned {
        let tint = parse_hex(colour).unwrap_or(tokens.p.primary_text);
        painter.text(pos2(rect.min.x, rect.center().y), egui::Align2::LEFT_CENTER, "⌖", font.clone(), tint);
    }
    let mut job = egui::text::LayoutJob::simple_singleline(left.to_owned(), font, tokens.p.text_muted);
    job.wrap.max_width = avail;
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');
    let galley = painter.layout_job(job);
    painter.galley(
        pos2(rect.min.x + 16.0, rect.center().y - galley.size().y / 2.0),
        galley,
        tokens.p.text_muted,
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{left} {result}")));
}

pub fn parse_hex(text: &str) -> Option<egui::Color32> {
    let hex = text.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(hex, 16).ok()?;
    Some(egui::Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

fn key_button(ui: &mut Ui, name: &str, rect: Rect, key: &str, tokens: &Tokens) -> egui::Response {
    let response = ui.interact(rect, egui::Id::unique(("calc-key", name, key)), Sense::click());
    let p = &tokens.p;
    let operator = OPS.contains(&key);
    let (fill, ink, hover) = if key == "=" {
        (p.primary, p.on_primary, p.primary_hover)
    } else if operator || matches!(key, "C" | "ans" | "⌫") {
        (p.bg_frame, if operator { p.primary_text } else { p.text_primary }, p.bg_card_hover)
    } else {
        (p.bg_card, p.text_primary, p.bg_card_hover)
    };
    let fill = if response.hovered() { hover } else { fill };
    ui.painter().rect_filled(rect, 6.0, fill);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        key,
        tokens.font(13.0, Weight::Medium, true),
        ink,
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, key));
    response
}

fn history_menu(ui: &mut Ui, book: &Book, index: usize, h: &Entry, act: &mut Option<Act>) {
    if menu::item(ui, "Copy result").clicked() {
        *act = Some(Act::CopyText(book.plain(h)));
    }
    if menu::item(ui, "Copy sum = result").clicked() {
        *act = Some(Act::CopyText(format!("{} = {}", h.expr(), h.result())));
    }
    if menu::item(ui, "Edit the sum").clicked() {
        *act = Some(Act::Load(h.expr().to_owned()));
    }
    if h.value().is_some() && menu::item(ui, "📌  Pin with a name…").clicked() {
        *act = Some(Act::Pin(index));
    }
    menu::separator(ui);
    if menu::item(ui, "Delete").clicked() {
        *act = Some(Act::DeleteHistory(index));
    }
}

fn pin_menu(ui: &mut Ui, book: &Book, index: usize, pin: &Entry, act: &mut Option<Act>) {
    if menu::item(ui, "Copy result").clicked() {
        *act = Some(Act::CopyText(book.plain(pin)));
    }
    if menu::item(ui, &format!("Insert '{}'", pin.var())).clicked() {
        *act = Some(Act::Insert(pin.var().to_owned()));
    }
    if menu::item(ui, "Edit the sum").clicked() {
        *act = Some(Act::Load(pin.expr().to_owned()));
    }
    if menu::item(ui, "Recalculate").clicked() {
        *act = Some(Act::Recalc(index));
    }
    if menu::item(ui, "Rename…").clicked() {
        *act = Some(Act::Rename(index));
    }
    ui.menu_button("Folder", |ui| {
        let mut folders: Vec<&str> =
            book.pins().iter().map(Entry::folder).filter(|f| !f.is_empty()).collect();
        folders.sort_by_key(|f| f.to_lowercase());
        folders.dedup();
        for f in folders {
            let mut on = pin.folder() == f;
            if menu::check_item(ui, f, &mut on).clicked() {
                *act = Some(Act::SetPin(index, Some(f.to_owned()), None));
                ui.close();
            }
        }
        menu::separator(ui);
        if menu::item(ui, "New folder…").clicked() {
            *act = Some(Act::NewFolder(index));
        }
        if !pin.folder().is_empty() && menu::item(ui, "Out of the folder").clicked() {
            *act = Some(Act::SetPin(index, Some(String::new()), None));
        }
    });
    ui.menu_button("Colour", |ui| {
        for (name, hex) in PIN_COLOURS {
            let mut on = pin.colour() == hex;
            if menu::check_item(ui, name, &mut on).clicked() {
                *act = Some(Act::SetPin(index, None, Some(hex.to_owned())));
                ui.close();
            }
        }
        menu::separator(ui);
        if menu::item(ui, "Accent colour").clicked() {
            *act = Some(Act::SetPin(index, None, Some(String::new())));
        }
    });
    menu::separator(ui);
    if menu::item(ui, "Unpin").clicked() {
        *act = Some(Act::Unpin(index));
    }
}

/// The base button in the panel's header: DEC, HEX, BIN or OCT.
pub fn base_button(ui: &mut Ui, base: Base) -> egui::Response {
    ui.add(Button::ghost(base.label()))
        .on_hover_text("Show whole-number results in decimal, hex, binary or octal")
}
