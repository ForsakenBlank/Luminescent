//! The overlay's own elements (the logo, the LUMINESCENT title, the clock, the date, the status chips), as a
//! layer you can rearrange (`ui/chrome.py`): ✎ (top right) lets you drag them anywhere on any screen, add
//! more, duplicate them, make them bigger or smaller and freeze them where they are (right-click). Outside edit
//! mode they take no clicks, so clicks reach the dashboard underneath.
//!
//! Settings `ui.chrome_items`: `[{"id", "kind", "screen", "x", "y", "scale", "frozen"}]`. `screen` "" is the
//! screen the overlay opens on; x / y are pixels on that screen. An empty list is the default layout.

use std::sync::Arc;

use egui::{
    Align, Galley, Id, Layout, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, UiBuilder, Vec2, pos2, vec2,
};
use serde_json::{Map, Value, json};

use crate::dialog::{AskDialog, Outcome};
use crate::paint;
use crate::text::Text;
use crate::theme::{Role, Tokens, Tone, alpha};
use crate::widgets::{Button, menu};

/// What can be on the layer, with the names the + Add menu shows.
pub const KINDS: &[(&str, &str)] = &[
    ("logo", "Logo and title"),
    ("mark", "Sparkle"),
    ("title", "LUMINESCENT title"),
    ("clock", "Clock and date"),
    ("time", "Time"),
    ("date", "Date"),
    ("status", "Status chips (timers, Hearing Guard, next reminder)"),
    ("timers", "Running timers"),
    ("reminder", "Next reminder"),
    ("media", "Now playing"),
    ("system", "CPU and memory"),
    ("greeting", "Greeting (Good morning…)"),
    ("text", "Your own text"),
];

const SIZES: &[(f64, &str)] = &[(0.75, "Small"), (1.0, "Normal"), (1.5, "Large"), (2.0, "Huge")];
const CLOCKISH: &[&str] = &["clock", "time", "date"];
/// One or more chips that can be empty.
const LINES: &[&str] = &["status", "timers", "reminder", "media", "system"];
/// The room around an element for its edit outline.
const PAD: f32 = 6.0;

fn name_of(kind: &str) -> &str {
    KINDS.iter().find(|(k, _)| *k == kind).map_or(kind, |(_, name)| name)
}

fn new_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()),
    );
    format!("{:08x}", h.finish() as u32)
}

fn item(kind: &str, x: f32, y: f32) -> Map<String, Value> {
    let Value::Object(map) =
        json!({"id": new_id(), "kind": kind, "screen": "", "x": x, "y": y, "scale": 1.0, "frozen": false})
    else {
        unreachable!("a json object literal is an object")
    };
    map
}

/// The layout Luminescent always had: logo top left, the clock under it, the chips under that.
pub fn default_items(top: f32) -> Vec<Map<String, Value>> {
    vec![
        item("logo", 36.0 - PAD, top + 30.0 - PAD),
        item("clock", 38.0 - PAD, top + 82.0 - PAD),
        item("status", 36.0 - PAD, top + 164.0 - PAD),
    ]
}

/// What the elements show, from the rest of the app: the time and date already written out, and the lines of
/// the chips as (icon, text).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChromeData {
    /// `14:30`
    pub time: String,
    /// `THURSDAY 26 NOVEMBER`
    pub date: String,
    /// The hour of the day, for the greeting.
    pub hour: u8,
    /// A new version, running timers, the Hearing Guard level, the next reminder.
    pub status: Vec<(String, String)>,
    pub timers: Vec<(String, String)>,
    pub media: Vec<(String, String)>,
    pub system: Vec<(String, String)>,
}

impl ChromeData {
    fn lines(&self, kind: &str) -> Vec<(String, String)> {
        match kind {
            "status" => self.status.clone(),
            "timers" => self.timers.clone(),
            "reminder" => self.status.iter().filter(|(icon, _)| icon == "⏰").cloned().collect(),
            "media" => self.media.clone(),
            "system" => self.system.clone(),
            _ => Vec::new(),
        }
    }

    /// Something on screen changes every second (a timer, CPU use), not only every minute.
    pub fn ticks_every_second(&self) -> bool {
        !(self.timers.is_empty() && self.system.is_empty() && self.status.iter().all(|(i, _)| i != "⏱"))
    }
}

pub fn greeting(hour: u8) -> &'static str {
    match hour {
        0..=4 => "Good night",
        5..=11 => "Good morning",
        12..=17 => "Good afternoon",
        _ => "Good evening",
    }
}

fn get_f(map: &Map<String, Value>, key: &str, default: f32) -> f32 {
    map.get(key).and_then(Value::as_f64).map_or(default, |v| v as f32)
}

fn get_s<'a>(map: &'a Map<String, Value>, key: &str) -> &'a str {
    map.get(key).and_then(Value::as_str).unwrap_or("")
}

enum Part {
    Sparkle(f32),
    Text(Arc<Galley>),
    Chip(Vec2, Arc<Galley>, Arc<Galley>),
}

/// An element's content, measured: where each part goes from the top-left of the content.
struct Laid {
    size: Vec2,
    parts: Vec<(Vec2, Part)>,
}

impl Laid {
    fn row(parts: Vec<(Vec2, Part)>, gap: f32) -> Laid {
        let height = parts.iter().map(|(size, _)| size.y).fold(0.0, f32::max);
        let mut x = 0.0;
        let mut placed = Vec::new();
        for (i, (size, part)) in parts.into_iter().enumerate() {
            if i > 0 {
                x += gap;
            }
            placed.push((vec2(x, (height - size.y) / 2.0), part));
            x += size.x;
        }
        Laid { size: vec2(x, height), parts: placed }
    }

    fn column(parts: Vec<(Vec2, Part)>, gap: f32) -> Laid {
        let mut y = 0.0;
        let mut width: f32 = 0.0;
        let mut placed = Vec::new();
        for (i, (size, part)) in parts.into_iter().enumerate() {
            if i > 0 {
                y += gap;
            }
            placed.push((vec2(0.0, y), part));
            y += size.y;
            width = width.max(size.x);
        }
        Laid { size: vec2(width, y), parts: placed }
    }
}

fn text(ui: &Ui, tokens: &Tokens, t: Text) -> (Vec2, Part) {
    let galley = ui.painter().layout_job(t.job(tokens));
    (galley.size(), Part::Text(galley))
}

fn lay_out(ui: &Ui, tokens: &Tokens, map: &Map<String, Value>, data: &ChromeData) -> Option<Laid> {
    let kind = get_s(map, "kind");
    let s = get_f(map, "scale", 1.0).clamp(0.25, 4.0);
    let sparkle = (Vec2::splat(30.0 * s), Part::Sparkle(30.0 * s));
    let title = || text(ui, tokens, Text::new("LUMINESCENT").px(15.0 * s).semibold().mono().spacing(0.3));
    let time = || text(ui, tokens, Text::new(&data.time).px(26.0 * s).medium().mono().spacing(-0.02));
    let date = || {
        text(ui, tokens, Text::new(&data.date).px(10.5 * s).medium().mono().spacing(0.12).tone(Tone::Muted))
    };
    let big = |words: String| text(ui, tokens, Text::new(words).px(18.0 * s).medium());
    let laid = match kind {
        "logo" => Laid::row(vec![sparkle, title()], 14.0 * s),
        "mark" => Laid::row(vec![sparkle], 0.0),
        "title" => Laid::row(vec![title()], 0.0),
        "clock" => Laid::column(vec![time(), date()], 2.0 * s),
        "time" => Laid::row(vec![time()], 0.0),
        "date" => Laid::row(vec![date()], 0.0),
        "greeting" => {
            let name = get_s(map, "text").trim();
            let words = greeting(data.hour).to_owned();
            Laid::row(vec![big(if name.is_empty() { words } else { format!("{words}, {name}") })], 0.0)
        }
        "text" => {
            let words = get_s(map, "text").trim();
            if words.is_empty() {
                return None;
            }
            Laid::row(vec![big(words.to_owned())], 0.0)
        }
        _ if LINES.contains(&kind) => {
            let lines = data.lines(kind);
            if lines.is_empty() {
                return None;
            }
            let chips = lines
                .into_iter()
                .map(|(icon, words)| {
                    let icon = ui.painter().layout_job(Text::new(icon).px(11.0 * s).job(tokens));
                    let words = ui.painter().layout_job(
                        Text::new(words).px(11.5 * s).medium().mono().tone(Tone::Secondary).job(tokens),
                    );
                    let size = vec2(
                        10.0 + icon.size().x + 8.0 + words.size().x + 12.0,
                        icon.size().y.max(words.size().y) + 10.0,
                    );
                    (size, Part::Chip(size, icon, words))
                })
                .collect();
            Laid::column(chips, 6.0 * s)
        }
        _ => return None,
    };
    Some(laid)
}

fn paint_laid(ui: &Ui, tokens: &Tokens, laid: Laid, at: Pos2) {
    let painter = ui.painter();
    for (offset, part) in laid.parts {
        let min = at + offset;
        match part {
            Part::Sparkle(size) => {
                paint::sparkle(painter, min + Vec2::splat(size / 2.0), size, tokens.p.primary)
            }
            Part::Text(galley) => painter.galley(min, galley, tokens.p.text_primary),
            Part::Chip(size, icon, words) => {
                let rect = Rect::from_min_size(min, size);
                paint::fill(painter, rect, 13.0, tokens, Role::BgBase, 0.88);
                painter.rect_stroke(rect, 13.0, Stroke::new(1.0, tokens.p.border), StrokeKind::Inside);
                let mid = rect.center().y;
                let icon_at = pos2(rect.min.x + 10.0, mid - icon.size().y / 2.0);
                let words_at = pos2(icon_at.x + icon.size().x + 8.0, mid - words.size().y / 2.0);
                painter.galley(icon_at, icon, tokens.p.text_primary);
                painter.galley(words_at, words, tokens.p.text_secondary);
            }
        }
    }
}

/// What the layer needs to know about where it is drawn.
pub struct Place<'a> {
    /// This screen's name and the switcher's screen's name.
    pub screen: &'a str,
    pub host: &'a str,
    /// Every screen's name, for "Move to screen".
    pub screens: &'a [String],
    /// The page shown: the node editor uses the whole main screen, so the clock and chips stay off it.
    pub page: &'a str,
    pub show_clock: bool,
    pub show_status: bool,
    /// The dashboard's snap grid (`dashboard.snap`), 1 for none.
    pub snap: f32,
    /// What bars take at the top of the switcher's screen.
    pub top: f32,
}

#[derive(Debug, Clone, Copy)]
struct Drag {
    index: usize,
    origin: Pos2,
}

#[derive(Debug, Default)]
pub struct ChromeLayer {
    items: Vec<Map<String, Value>>,
    /// The settings list the items came from, so they are read again only when it changes.
    source: Option<Vec<Value>>,
    pub editing: bool,
    drag: Option<Drag>,
    ask: Option<(String, AskDialog)>,
    /// Changed since it was last handed out for saving.
    changed: bool,
}

impl ChromeLayer {
    /// Reads `ui.chrome_items` again when the settings hold a different list (not while you are editing it).
    pub fn load(&mut self, saved: &[Value], top: f32) {
        if self.editing || self.source.as_deref() == Some(saved) {
            return;
        }
        let known = |kind: &str| KINDS.iter().any(|(k, _)| *k == kind);
        self.items =
            saved.iter().filter_map(|v| v.as_object()).filter(|m| known(get_s(m, "kind"))).cloned().collect();
        if self.items.is_empty() {
            self.items = default_items(top);
        }
        self.source = Some(saved.to_vec());
    }

    pub fn items(&self) -> &[Map<String, Value>] {
        &self.items
    }

    /// The list to save into `ui.chrome_items`, once after each change.
    pub fn take_changes(&mut self) -> Option<Vec<Value>> {
        if !std::mem::take(&mut self.changed) {
            return None;
        }
        let list: Vec<Value> = self.items.iter().cloned().map(Value::Object).collect();
        self.source = Some(list.clone());
        Some(list)
    }

    fn save(&mut self) {
        self.changed = true;
    }

    pub fn set_editing(&mut self, on: bool, saved_is_empty: bool) {
        if on == self.editing {
            return;
        }
        self.editing = on;
        self.drag = None;
        if on && saved_is_empty {
            // the default layout becomes yours to change
            self.save();
        }
    }

    /// The text dialog is open, so Esc and typing belong to it.
    pub fn busy(&self) -> bool {
        self.ask.is_some()
    }

    fn screen_of<'a>(map: &'a Map<String, Value>, place: &Place<'a>) -> &'a str {
        let screen = get_s(map, "screen");
        if screen.is_empty() || !place.screens.iter().any(|s| s == screen) { place.host } else { screen }
    }

    fn wanted(&self, map: &Map<String, Value>, place: &Place<'_>) -> bool {
        if self.editing {
            return true;
        }
        let kind = get_s(map, "kind");
        if CLOCKISH.contains(&kind) && !place.show_clock {
            return false;
        }
        if kind == "status" && !place.show_status {
            return false;
        }
        let on_host = Self::screen_of(map, place) == place.host;
        !(on_host && place.page == "automation")
    }

    /// Draws the elements that belong on this screen, plus the edit bar on the switcher's screen while editing.
    pub fn show(&mut self, ui: &mut Ui, place: &Place<'_>, data: &ChromeData) {
        let tokens = Tokens::of(ui.ctx());
        let screen = ui.max_rect();
        let mut menu_act: Option<(usize, MenuAct)> = None;
        for index in 0..self.items.len() {
            let map = &self.items[index];
            if Self::screen_of(map, place) != place.screen || !self.wanted(map, place) {
                continue;
            }
            let laid = lay_out(ui, &tokens, map, data);
            if laid.is_none() && !self.editing {
                continue;
            }
            let hint = laid.is_none().then(|| {
                ui.painter().layout_job(
                    Text::new(name_of(get_s(map, "kind"))).px(11.0).tone(Tone::Muted).job(&tokens),
                )
            });
            let content =
                laid.as_ref().map_or_else(|| hint.as_ref().map_or(Vec2::ZERO, |h| h.size()), |l| l.size);
            let at = screen.min + vec2(get_f(map, "x", 0.0), get_f(map, "y", 0.0));
            let frame = Rect::from_min_size(at, content + Vec2::splat(2.0 * PAD));
            let frozen = map.get("frozen").and_then(Value::as_bool).unwrap_or(false);
            if self.editing {
                paint_edit_frame(ui, &tokens, frame, frozen);
            }
            match laid {
                Some(laid) => paint_laid(ui, &tokens, laid, frame.min + Vec2::splat(PAD)),
                None => {
                    if let Some(hint) = hint {
                        ui.painter().galley(frame.min + Vec2::splat(PAD), hint, tokens.p.text_muted);
                    }
                }
            }
            if !self.editing {
                continue;
            }
            let id = Id::unique(("lum-chrome-item", get_s(&self.items[index], "id")));
            let response =
                ui.interact(frame, id, if frozen { Sense::click() } else { Sense::click_and_drag() });
            if !frozen {
                ui.ctx().set_cursor_icon(if response.dragged() {
                    egui::CursorIcon::Grabbing
                } else if response.hovered() {
                    egui::CursorIcon::Grab
                } else {
                    ui.ctx().output(|o| o.cursor_icon)
                });
            }
            if response.drag_started() {
                self.drag = Some(Drag { index, origin: frame.min - screen.min.to_vec2() });
            }
            if let Some(drag) = self.drag.filter(|d| d.index == index) {
                if response.dragged() {
                    let total = ui.input(|i| {
                        i.pointer
                            .press_origin()
                            .zip(i.pointer.latest_pos())
                            .map_or(Vec2::ZERO, |(a, b)| b - a)
                    });
                    let p = drag.origin + total;
                    let g = place.snap.max(1.0);
                    let snap = |v: f32| if g > 1.0 { (v / g).round_ties_even() * g } else { v.round() };
                    let x = snap(p.x).clamp(0.0, (screen.width() - frame.width()).max(0.0));
                    let y = snap(p.y).clamp(0.0, (screen.height() - frame.height()).max(0.0));
                    let map = &mut self.items[index];
                    map.insert("x".into(), json!(x.round() as i64));
                    map.insert("y".into(), json!(y.round() as i64));
                }
                if response.drag_stopped() {
                    self.drag = None;
                    self.save();
                }
            }
            let tip = if frozen { " · frozen (right-click → Unfreeze)" } else { "" };
            let kind = get_s(&self.items[index], "kind").to_owned();
            let response = response
                .on_hover_text(format!("{}{tip}\nDrag to move · right-click for more", name_of(&kind)));
            response.context_menu(|ui| {
                if let Some(act) = item_menu(ui, &self.items[index], place) {
                    menu_act = Some((index, act));
                }
            });
        }
        if let Some((index, act)) = menu_act {
            self.act(index, act, screen.size());
        }
        if self.editing && place.screen == place.host {
            self.edit_bar(ui, place, screen);
        }
        self.dialog(ui.ctx());
    }

    fn edit_bar(&mut self, ui: &mut Ui, place: &Place<'_>, screen: Rect) {
        let tokens = Tokens::of(ui.ctx());
        let size = vec2(760.0, 42.0);
        let mut bar = Rect::from_min_size(
            pos2(screen.center().x - size.x / 2.0, screen.min.y + place.top + 72.0),
            size,
        );
        // measured on the last frame, so it sits centred once it knows its width
        let id = Id::unique("lum-chrome-bar");
        if let Some(width) = ui.ctx().data(|d| d.get_temp::<f32>(id)) {
            bar = Rect::from_center_size(bar.center(), vec2(width, size.y));
        }
        paint::fill(ui.painter(), bar, 22.0, &tokens, Role::BgBase, 1.0);
        ui.painter().rect_stroke(bar, 22.0, Stroke::new(1.0, tokens.p.border), StrokeKind::Inside);
        ui.interact(bar, id.with("bar"), Sense::click());
        let inner = Rect::from_min_max(bar.min + vec2(14.0, 5.0), bar.max - vec2(5.0, 5.0));
        let mut row =
            ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)));
        row.spacing_mut().item_spacing.x = 6.0;
        row.add(Text::new("EDITING THE LAYOUT").px(10.5).semibold().mono().spacing(0.1).tone(Tone::Accent));
        row.add(Text::new("drag to move · right-click to freeze, copy, resize").px(12.0).tone(Tone::Muted));
        let add = row.add(Button::new("+ Add"));
        let mut added = None;
        egui::Popup::menu(&add).show(|ui| {
            for (kind, label) in KINDS {
                if menu::item(ui, label).clicked() {
                    added = Some(*kind);
                }
            }
        });
        let reset = row
            .add(Button::ghost("Reset"))
            .on_hover_text("Back to the logo, clock and chips in the top-left corner");
        let done = row.add(Button::primary("Done"));
        let used = row.min_rect().right() + 5.0 - bar.min.x;
        ui.ctx().data_mut(|d| d.insert_temp(id, used));
        if let Some(kind) = added {
            self.add(kind, place, screen.size());
        }
        if reset.clicked() {
            self.items = default_items(place.top);
            self.save();
        }
        if done.clicked() {
            self.set_editing(false, false);
        }
    }

    fn add(&mut self, kind: &str, place: &Place<'_>, screen: Vec2) {
        let mut map = item(kind, (screen.x / 2.0 - 80.0).round(), place.top + 140.0);
        if kind == "text" {
            map.insert("text".into(), json!("Your text"));
        }
        let id = get_s(&map, "id").to_owned();
        self.items.push(map);
        if kind == "text" {
            self.ask_text(&id);
        }
        self.save();
    }

    fn ask_text(&mut self, id: &str) {
        let Some(map) = self.items.iter().find(|m| get_s(m, "id") == id) else { return };
        let own = get_s(map, "kind") == "text";
        let dialog = AskDialog::ask_text(
            Id::unique(("lum-chrome-text", id)),
            if own { "Your own text" } else { "Greeting" },
            if own { "Text:" } else { "Your name (empty for none):" },
            get_s(map, "text"),
        );
        self.ask = Some((id.to_owned(), dialog));
    }

    fn dialog(&mut self, ctx: &egui::Context) {
        let Some((id, dialog)) = &mut self.ask else { return };
        match dialog.show(ctx) {
            Outcome::Open => {}
            Outcome::Cancelled => self.ask = None,
            Outcome::Accepted(_) => {
                let (id, text) = (id.clone(), dialog.text().to_owned());
                self.ask = None;
                if let Some(map) = self.items.iter_mut().find(|m| get_s(m, "id") == id) {
                    map.insert("text".into(), json!(text));
                    self.save();
                }
            }
        }
    }

    fn act(&mut self, index: usize, act: MenuAct, screen: Vec2) {
        if index >= self.items.len() {
            return;
        }
        match act {
            MenuAct::Freeze(on) => {
                self.items[index].insert("frozen".into(), json!(on));
            }
            MenuAct::Duplicate => {
                let mut copy = self.items[index].clone();
                copy.insert("id".into(), json!(new_id()));
                copy.insert("frozen".into(), json!(false));
                copy.insert("x".into(), json!(get_f(&copy, "x", 0.0) + 24.0));
                copy.insert("y".into(), json!(get_f(&copy, "y", 0.0) + 24.0));
                self.items.push(copy);
            }
            MenuAct::EditText => {
                let id = get_s(&self.items[index], "id").to_owned();
                self.ask_text(&id);
                return;
            }
            MenuAct::Size(scale) => {
                self.items[index].insert("scale".into(), json!(scale));
            }
            MenuAct::Screen(name) => {
                // a new screen: keep it on it (each screen is at least this big)
                let map = &mut self.items[index];
                let x = get_f(map, "x", 0.0).min(screen.x - 80.0).max(0.0);
                let y = get_f(map, "y", 0.0).min(screen.y - 40.0).max(0.0);
                map.insert("screen".into(), json!(name));
                map.insert("x".into(), json!(x));
                map.insert("y".into(), json!(y));
            }
            MenuAct::Remove => {
                self.items.remove(index);
            }
            MenuAct::Done => {
                self.set_editing(false, false);
                return;
            }
        }
        self.save();
    }

    /// The lowest point of the elements in the top-left part of the switcher's screen (dashboard boxes start
    /// below it), with room for two more status chips.
    pub fn bottom_on_host(&self, ui: &Ui, place: &Place<'_>, data: &ChromeData) -> f32 {
        let tokens = Tokens::of(ui.ctx());
        let mut y: f32 = 0.0;
        for map in &self.items {
            if Self::screen_of(map, place) != place.host || get_f(map, "x", 0.0) > 520.0 {
                continue;
            }
            let status = get_s(map, "kind") == "status";
            let laid = if self.wanted(map, place) { lay_out(ui, &tokens, map, data) } else { None };
            if laid.is_none() && !status {
                continue;
            }
            let h = laid.map_or(0.0, |l| l.size.y + 2.0 * PAD);
            y = y.max(get_f(map, "y", 0.0) + h + if status { 2.0 * 34.0 } else { 0.0 });
        }
        y
    }
}

enum MenuAct {
    Freeze(bool),
    Duplicate,
    EditText,
    Size(f64),
    Screen(String),
    Remove,
    Done,
}

fn item_menu(ui: &mut Ui, map: &Map<String, Value>, place: &Place<'_>) -> Option<MenuAct> {
    let mut act = None;
    let frozen = map.get("frozen").and_then(Value::as_bool).unwrap_or(false);
    let label = if frozen { "Unfreeze (can be moved again)" } else { "Freeze (stays where it is)" };
    if menu::item(ui, label).clicked() {
        act = Some(MenuAct::Freeze(!frozen));
    }
    if menu::item(ui, "Duplicate").clicked() {
        act = Some(MenuAct::Duplicate);
    }
    match get_s(map, "kind") {
        "text" if menu::item(ui, "Edit text…").clicked() => act = Some(MenuAct::EditText),
        "greeting" if menu::item(ui, "Your name…").clicked() => act = Some(MenuAct::EditText),
        _ => {}
    }
    let scale = map.get("scale").and_then(Value::as_f64).unwrap_or(1.0);
    ui.menu_button("Size", |ui| {
        for (value, name) in SIZES {
            let mut on = (scale - value).abs() < 0.01;
            if menu::check_item(ui, name, &mut on).clicked() {
                act = Some(MenuAct::Size(*value));
                ui.close();
            }
        }
    });
    if place.screens.len() > 1 {
        let current = get_s(map, "screen");
        ui.menu_button("Move to screen", |ui| {
            let mark = |on: bool| if on { "●  " } else { "     " };
            if menu::item(ui, &format!("{}The screen the overlay opens on", mark(current.is_empty())))
                .clicked()
            {
                act = Some(MenuAct::Screen(String::new()));
            }
            for name in place.screens {
                if menu::item(ui, &format!("{}{name}", mark(current == name))).clicked() {
                    act = Some(MenuAct::Screen(name.clone()));
                }
            }
        });
    }
    menu::separator(ui);
    if menu::item(ui, "Remove").clicked() {
        act = Some(MenuAct::Remove);
    }
    menu::separator(ui);
    if menu::item(ui, "Done editing").clicked() {
        act = Some(MenuAct::Done);
    }
    act
}

fn paint_edit_frame(ui: &Ui, tokens: &Tokens, frame: Rect, frozen: bool) {
    let painter = ui.painter();
    painter.rect_filled(frame, 8.0, alpha(tokens.p.bg_base, 0.55));
    if frozen {
        painter.rect_stroke(frame, 8.0, Stroke::new(1.0, tokens.p.border), StrokeKind::Inside);
    } else {
        let mut outline = paint::rounded_outline(frame.shrink(0.5), 8.0);
        if let Some(first) = outline.first().copied() {
            outline.push(first);
        }
        painter.add(Shape::dashed_line(&outline, Stroke::new(1.0, tokens.p.primary), 4.0, 3.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_layout_and_unknown_kinds() {
        let mut layer = ChromeLayer::default();
        layer.load(&[], 40.0);
        let kinds: Vec<&str> = layer.items().iter().map(|m| get_s(m, "kind")).collect();
        assert_eq!(kinds, ["logo", "clock", "status"]);
        assert_eq!(get_f(&layer.items()[0], "y", 0.0), 64.0);
        let saved =
            vec![json!({"id": "a", "kind": "date", "x": 5, "y": 6, "mine": 1}), json!({"kind": "rocket"})];
        layer.load(&saved, 0.0);
        assert_eq!(layer.items().len(), 1);
        assert_eq!(layer.items()[0].get("mine"), Some(&json!(1)), "keys it does not know stay");
        assert!(layer.take_changes().is_none());
    }

    #[test]
    fn editing_the_default_layout_saves_it() {
        let mut layer = ChromeLayer::default();
        layer.load(&[], 0.0);
        layer.set_editing(true, true);
        let saved = layer.take_changes().expect("the default layout is saved");
        assert_eq!(saved.len(), 3);
        layer.set_editing(false, false);
        layer.load(&saved, 0.0);
        assert_eq!(layer.items().len(), 3);
    }

    #[test]
    fn greetings_by_the_hour() {
        assert_eq!(greeting(3), "Good night");
        assert_eq!(greeting(9), "Good morning");
        assert_eq!(greeting(14), "Good afternoon");
        assert_eq!(greeting(21), "Good evening");
    }
}
