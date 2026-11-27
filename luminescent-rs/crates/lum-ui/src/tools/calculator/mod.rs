//! The Calculator: a floating panel, the overlay's Calculator page and a dashboard box, all showing the same
//! history and pins (`calculator.json`). The sums are worked out in `lum-calc`; `view.rs` draws them.

mod view;

use std::time::{Duration, Instant};

use egui::{Rect, Ui, pos2, vec2};
use jiff::ZonedDateTime;
use lum_calc::{Base, Book, Options, Rates, evaluate, fmt};
use lum_core::actions::Action;
use lum_core::module::{Module, ModuleCx, StartError};
use lum_core::settings::Settings;

use crate::dialog::{AskDialog, DialogWindow, Outcome};
use crate::overlay::PageFrame;
use crate::palette::Entry;
use crate::panel::{FloatingPanel, Geometry, centred_on};
use crate::theme::Tokens;
use crate::tools::{dialog_id, dialog_window};
use crate::widgets::{IconButton, IconShape, menu};
use crate::{Bind, Inbox, Request, Services, UiCx, UiModule};
use view::{Env, Event, View, base_button};

pub use view::PIN_COLOURS;

const SAVE_AFTER: Duration = Duration::from_millis(500);
const DEFAULT_SIZE: (f32, f32) = (296.0, 440.0);

#[derive(Debug)]
enum Cmd {
    Toggle,
    Rates(Rates),
}

#[derive(Debug)]
enum Ask {
    Pin(usize),
    Rename(usize),
    Folder(usize),
    ClearHistory,
    Message,
}

pub struct CalculatorModule {
    services: Services,
    inbox: Inbox<Cmd>,
    book: Option<Book>,
    rates: Rates,
    panel: FloatingPanel,
    panel_view: View,
    page_view: View,
    dialog: Option<(DialogWindow, Ask)>,
    save_due: Option<Instant>,
    /// The pass the page was last drawn in, to notice it coming back and put the cursor in the box.
    page_pass: u64,
}

impl std::fmt::Debug for CalculatorModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CalculatorModule").field("running", &self.book.is_some()).finish_non_exhaustive()
    }
}

fn options(settings: &Settings) -> Options {
    Options {
        decimals: settings.get_i64("calculator.decimals").unwrap_or(10).clamp(0, 15) as usize,
        thousands: settings.get_bool("calculator.thousands", true),
        degrees: settings.get_str("calculator.angle").unwrap_or("deg") == "deg",
        currency: settings.get_bool("calculator.currency", true),
        history_limit: settings.get_i64("calculator.history_limit").unwrap_or(200).max(0) as usize,
    }
}

fn opacity(settings: &Settings) -> f32 {
    settings.get_f64("calculator.bg_opacity").unwrap_or(0.92) as f32
}

impl CalculatorModule {
    pub fn new(services: &Services) -> CalculatorModule {
        let geometry = Geometry { x: 0.0, y: 0.0, w: DEFAULT_SIZE.0, h: DEFAULT_SIZE.1 };
        CalculatorModule {
            services: services.clone(),
            inbox: Inbox::new(services),
            book: None,
            rates: Rates::default(),
            panel: FloatingPanel::new("calculator", "Calculator", geometry)
                .min_size(vec2(240.0, 260.0))
                .drag_needs_edit_mode(false),
            panel_view: View::new("panel", false),
            page_view: View::new("page", true),
            dialog: None,
            save_due: None,
            page_pass: 0,
        }
    }

    fn remember_panel(&mut self) {
        let Some(book) = self.book.as_mut() else { return };
        let g = self.panel.geometry;
        let panel = book.panel_mut();
        for (key, value) in [("x", g.x), ("y", g.y), ("w", g.w), ("h", g.h)] {
            panel.insert(key.into(), serde_json::json!(value.round() as i64));
        }
        panel.insert("keypad".into(), serde_json::json!(self.panel_view.keypad));
    }

    fn place_panel(&mut self, book: &Book) {
        let saved = book.panel();
        let num = |key: &str| saved.get(key).and_then(serde_json::Value::as_f64).map(|v| v as f32);
        let (w, h) = (num("w").unwrap_or(DEFAULT_SIZE.0), num("h").unwrap_or(DEFAULT_SIZE.1));
        let (x, y) = match (num("x"), num("y")) {
            (Some(x), Some(y)) => (x, y),
            _ => {
                let screens = crate::window::screens();
                let at = crate::window::cursor_screen(&screens).or((!screens.is_empty()).then_some(0));
                at.map_or((100.0, 100.0), |i| {
                    let p = centred_on(vec2(w, h), screens[i].rect);
                    (p.x, p.y)
                })
            }
        };
        self.panel.geometry = Geometry { x, y, w, h };
        self.panel_view.keypad = saved.get("keypad").and_then(serde_json::Value::as_bool).unwrap_or(false);
    }

    fn toggle(&mut self) {
        let Some(book) = self.book.as_mut() else { return };
        if self.panel.is_open() {
            self.panel.close();
            book.panel_mut().insert("visible".into(), serde_json::json!(false));
        } else {
            self.panel.open();
            self.panel_view.focus_input();
            book.panel_mut().insert("visible".into(), serde_json::json!(true));
        }
        self.remember_panel();
    }

    fn take_commands(&mut self) {
        for cmd in self.inbox.take() {
            match cmd {
                Cmd::Toggle => self.toggle(),
                Cmd::Rates(rates) => self.rates = rates,
            }
        }
    }

    fn fetch_rates(&self, force: bool, wait: Duration) {
        let inbox = self.inbox.clone();
        let path = self.services.data_file("currency.json");
        let spawned = std::thread::Builder::new().name("currency".into()).spawn(move || {
            std::thread::sleep(wait);
            if let Some(rates) = lum_calc::refresh_rates(&path, force) {
                inbox.push(Cmd::Rates(rates));
            }
        });
        if let Err(e) = spawned {
            tracing::warn!("could not start the currency update: {e}");
        }
    }

    fn open_dialog(&mut self, dialog: AskDialog, ask: Ask) {
        self.dialog = Some((dialog_window(dialog), ask));
    }

    fn handle(&mut self, event: Event, embedded: bool) {
        match event {
            Event::Pin(i) => {
                let d = AskDialog::ask_text(
                    dialog_id("pin"),
                    "Pin",
                    "Name (you can then use it in sums, e.g. rent/4):",
                    "",
                );
                self.open_dialog(d, Ask::Pin(i));
            }
            Event::Rename(i) => {
                let current = self.book.as_ref().and_then(|b| b.pins().get(i)).map(|p| p.name().to_owned());
                let d = AskDialog::ask_text(
                    dialog_id("rename"),
                    "Rename pin",
                    "New name:",
                    &current.unwrap_or_default(),
                );
                self.open_dialog(d, Ask::Rename(i));
            }
            Event::NewFolder(i) => {
                let d = AskDialog::ask_text(
                    dialog_id("folder"),
                    "Pin folder",
                    "Folder name (e.g. Bills, Work):",
                    "",
                );
                self.open_dialog(d, Ask::Folder(i));
            }
            Event::ClearHistory => {
                let d = AskDialog::confirm(
                    dialog_id("clear"),
                    "Clear history",
                    "Delete the whole calculator history? Pins are kept.",
                    "Clear",
                    "Cancel",
                    true,
                );
                self.open_dialog(d, Ask::ClearHistory);
            }
            Event::OpenSettings => self.services.send(Request::OpenSettings("calculator".into())),
            Event::RefreshRates => self.fetch_rates(true, Duration::ZERO),
            Event::Notify(text) => {
                self.services.send(Request::Notify { title: "Calculator".into(), text, ms: 4000 });
            }
            Event::Close => {
                if embedded {
                    self.services.send(Request::HideOverlay);
                } else {
                    self.panel.close();
                    if let Some(book) = self.book.as_mut() {
                        book.panel_mut().insert("visible".into(), serde_json::json!(false));
                    }
                }
            }
        }
    }

    fn show_dialog(&mut self, ctx: &egui::Context) {
        let Some((window, ask)) = self.dialog.as_mut() else { return };
        let outcome = window.show(ctx);
        let text = window.dialog.text().trim().to_owned();
        let Some(book) = self.book.as_mut() else { return };
        let mut failed = None;
        match (outcome, &*ask) {
            (Outcome::Open, _) => return,
            (Outcome::Accepted(_), Ask::Pin(i)) => failed = book.pin(*i, &text).err(),
            (Outcome::Accepted(_), Ask::Rename(i)) => failed = book.rename_pin(*i, &text).err(),
            (Outcome::Accepted(_), Ask::Folder(i)) if !text.is_empty() => book.set_pin(*i, Some(&text), None),
            (Outcome::Accepted(_), Ask::ClearHistory) => book.clear_history(),
            _ => {}
        }
        self.dialog = None;
        if let Some(message) = failed {
            self.open_dialog(AskDialog::error(dialog_id("problem"), "Pin", &message), Ask::Message);
        }
    }

    fn after(&mut self, ctx: &egui::Context) {
        let Some(book) = self.book.as_mut() else { return };
        if book.is_dirty() && self.save_due.is_none() {
            self.save_due = Some(Instant::now() + SAVE_AFTER);
        }
        if let Some(due) = self.save_due {
            if Instant::now() >= due {
                self.save_due = None;
                self.remember_panel_into(ctx);
            } else {
                ctx.request_repaint_after(due.saturating_duration_since(Instant::now()));
            }
        }
    }

    fn remember_panel_into(&mut self, _ctx: &egui::Context) {
        self.remember_panel();
        if let Some(book) = self.book.as_mut() {
            book.save();
        }
    }

    /// The panel's header buttons: what was clicked, applied once the panel is drawn.
    fn header(ui: &mut Ui, base: Base, currency: bool, rates_date: &str) -> HeaderAct {
        let mut act = HeaderAct::default();
        // right to left: the ⋯ menu, the keypad, then DEC
        let more = ui.add(IconButton::new("⋯").size(26.0).font_px(12.0).ghost().tooltip("More"));
        egui::Popup::menu(&more).show(|ui| {
            if menu::item(ui, "Clear history…").clicked() {
                act.clear = true;
            }
            if menu::item(ui, "Keypad").clicked() {
                act.keypad = true;
            }
            ui.menu_button("Show results in", |ui| {
                for b in Base::ALL {
                    let mut on = b == base;
                    if menu::check_item(ui, b.label(), &mut on).clicked() {
                        act.base = Some(b);
                        ui.close();
                    }
                }
            });
            if currency {
                let label = if rates_date.is_empty() {
                    "Update currency rates".to_owned()
                } else {
                    format!("Update currency rates (from {rates_date})")
                };
                if menu::item(ui, &label).clicked() {
                    act.rates = true;
                }
            }
            if menu::item(ui, "Calculator settings…").clicked() {
                act.settings = true;
            }
        });
        if ui
            .add(IconButton::shape(IconShape::Keypad).size(26.0).ghost().tooltip("Show / hide the keypad"))
            .clicked()
        {
            act.keypad = true;
        }
        if base_button(ui, base).clicked() {
            act.base = Some(base.next());
        }
        act
    }

    fn env_today() -> (jiff::civil::Date, jiff::civil::DateTime) {
        let now = ZonedDateTime::now();
        (now.date(), now.datetime())
    }
}

#[derive(Debug, Default)]
struct HeaderAct {
    clear: bool,
    keypad: bool,
    base: Option<Base>,
    rates: bool,
    settings: bool,
}

impl Module for CalculatorModule {
    fn key(&self) -> &'static str {
        "calculator"
    }

    fn title(&self) -> &'static str {
        "Calculator"
    }

    fn start(&mut self, cx: &mut ModuleCx<'_>) -> Result<(), StartError> {
        let mut book = Book::load(&self.services.data_file("calculator.json"));
        book.options = options(cx.settings);
        self.rates = Rates::load(&self.services.data_file("currency.json"));
        self.panel.opacity = opacity(cx.settings);
        self.place_panel(&book);
        let visible = book.panel().get("visible").and_then(serde_json::Value::as_bool).unwrap_or(false);
        self.book = Some(book);
        if options(cx.settings).currency {
            self.fetch_rates(false, Duration::from_secs(8));
        }
        if visible {
            self.panel.open();
        }
        Ok(())
    }

    fn stop(&mut self, _cx: &mut ModuleCx<'_>) {
        let was_open = self.panel.is_open();
        self.remember_panel();
        if let Some(mut book) = self.book.take() {
            // a restart should bring the panel back
            book.panel_mut().insert("visible".into(), serde_json::json!(was_open));
            book.flush();
        }
        self.panel.close();
        self.dialog = None;
    }

    fn apply_settings(&mut self, cx: &mut ModuleCx<'_>) -> Result<(), StartError> {
        self.panel.opacity = opacity(cx.settings);
        if let Some(book) = self.book.as_mut() {
            book.options = options(cx.settings);
            book.reformat();
        }
        Ok(())
    }

    fn actions(&self) -> Vec<Action> {
        vec![self.inbox.action("toggle", "Calculator", || Cmd::Toggle)]
    }
}

impl UiModule for CalculatorModule {
    fn binds(&self) -> Vec<Bind> {
        vec![Bind { setting: "hotkey", id: "toggle", label: "calculator" }]
    }

    fn bind_pressed(&mut self, id: &str) {
        if id == "toggle" {
            self.inbox.push(Cmd::Toggle);
        }
    }

    fn page_key(&self) -> Option<&'static str> {
        Some("calculator")
    }

    fn reset_page(&mut self) {
        self.page_view = View::new("page", true);
    }

    fn tray_actions(&self) -> Vec<(String, String)> {
        vec![("toggle".into(), "Calculator".into())]
    }

    fn show(&mut self, cx: &mut UiCx<'_>) {
        self.take_commands();
        if self.book.is_none() {
            return;
        }
        self.show_dialog(cx.ctx);
        if self.panel.is_open() {
            self.draw_panel(cx.ctx);
        }
        self.after(cx.ctx);
    }

    fn page(&mut self, ui: &mut Ui, rect: Rect, cx: &mut UiCx<'_>) -> PageFrame {
        self.take_commands();
        let Some(book) = self.book.as_mut() else { return PageFrame::default() };
        let pass = cx.ctx.cumulative_pass_nr();
        if self.page_pass + 1 < pass {
            self.page_view.focus_input();
        }
        self.page_pass = pass;
        let tokens = Tokens::of(cx.ctx);
        let width = (rect.width() - 48.0).clamp(460.0_f32.min(rect.width()), 620.0_f32.max(460.0));
        let height = (rect.height() - 40.0).max(260.0).min(rect.height());
        let card = Rect::from_center_size(
            pos2(rect.center().x, rect.min.y + 16.0 + height / 2.0),
            vec2(width, height),
        );
        ui.painter().rect_filled(card, tokens.r(), tokens.p.bg_base);
        ui.painter().rect_stroke(
            card,
            tokens.r(),
            egui::Stroke::new(1.0, tokens.p.border),
            egui::StrokeKind::Inside,
        );
        let (today, now) = Self::env_today();
        let env = Env { rates: self.rates.map(), today, now };
        let events = self.page_view.show(ui, card, book, &env);
        for event in events {
            self.handle(event, true);
        }
        self.show_dialog(cx.ctx);
        self.after(cx.ctx);
        PageFrame { busy: self.dialog.is_some() }
    }

    fn palette_entries(&self) -> Vec<Entry> {
        let Some(book) = &self.book else { return Vec::new() };
        book.pins()
            .iter()
            .map(|p| {
                Entry::new(
                    format!("calc:pin:{}", p.var()),
                    format!("📌 {} = {}", p.var(), p.result()),
                    "calculator pin · Enter copies",
                    "calc",
                )
            })
            .collect()
    }

    /// A sum typed into the palette shows its answer, as long as it has a digit and an operator in it.
    fn palette_live(&self, text: &str) -> Vec<Entry> {
        let t = text.trim();
        let operator = t.chars().skip(1).any(|c| "+-*/^%()".contains(c));
        if t.is_empty() || !t.chars().any(|c| c.is_ascii_digit()) || !operator {
            return Vec::new();
        }
        let Some(book) = &self.book else { return Vec::new() };
        let Ok(value) = evaluate(t, &book.scope()) else { return Vec::new() };
        let shown = fmt(&value, 10, true, Base::Dec);
        vec![Entry::new(format!("calc:sum:{t}"), format!("= {shown}"), format!("{t} · Enter copies"), "calc")]
    }

    fn run_palette(&mut self, id: &str, services: &Services) -> bool {
        let Some(rest) = id.strip_prefix("calc:") else { return false };
        let Some(book) = self.book.as_mut() else { return true };
        if let Some(var) = rest.strip_prefix("pin:") {
            let result = book.pins().iter().find(|p| p.var() == var).map(|p| p.result().replace(',', ""));
            if let Some(result) = result {
                crate::tools::clip::copy(result);
            }
        } else if let Some(sum) = rest.strip_prefix("sum:")
            && let Ok(value) = evaluate(sum, &book.scope())
        {
            let shown = fmt(&value, 10, true, Base::Dec);
            crate::tools::clip::copy(shown.replace(',', ""));
            let (_, now) = Self::env_today();
            book.add_history(sum, Some(&value), "", now);
            services.send(Request::Notify {
                title: "Calculator".into(),
                text: format!("{sum} = {shown}  (copied)"),
                ms: 2500,
            });
        }
        true
    }
}

impl CalculatorModule {
    fn draw_panel(&mut self, ctx: &egui::Context) {
        let Some(book) = self.book.as_mut() else { return };
        let (today, now) = Self::env_today();
        let env = Env { rates: self.rates.map(), today, now };
        let (base, currency, rates_date) = (book.base(), book.options.currency, self.rates.date().to_owned());
        let mut head = HeaderAct::default();
        let mut events = Vec::new();
        let view = &mut self.panel_view;
        let response = self.panel.show_with(
            ctx,
            |ui| head = Self::header(ui, base, currency, &rates_date),
            |ui| events = view.show(ui, ui.max_rect(), book, &env),
        );
        if head.keypad {
            self.panel_view.keypad = !self.panel_view.keypad;
        }
        if let Some(b) = head.base {
            book.set_base(b);
        }
        if head.keypad || response.geometry_changed {
            self.remember_panel();
            if let Some(book) = self.book.as_mut() {
                book.mark_dirty();
            }
        }
        if response.escape {
            if self.panel_view.input.is_empty() {
                self.panel.close();
            } else {
                self.panel_view.input.clear();
                self.panel_view.focus_input();
            }
        }
        if (response.closed || response.escape)
            && let Some(book) = self.book.as_mut()
        {
            book.panel_mut().insert("visible".into(), serde_json::json!(self.panel.is_open()));
        }
        for event in events {
            self.handle(event, false);
        }
        if head.clear {
            self.handle(Event::ClearHistory, false);
        }
        if head.rates {
            self.handle(Event::RefreshRates, false);
        }
        if head.settings {
            self.handle(Event::OpenSettings, false);
        }
    }
}
