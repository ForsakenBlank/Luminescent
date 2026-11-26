//! The Luminescent overlay: a full-screen hub, the way game bars do it (`ui/overlay.py`).
//!
//! One frameless, always-on-top, see-through window per screen. Every screen is dimmed and dotted edge to
//! edge. The screen it opens on (the one with the mouse, or the primary one) also carries the page switcher
//! hanging from the top-centre edge and the round ✕ top-right. The Dashboard and Automation fill the screen;
//! the other pages sit in a centred panel below the switcher. ✕, Esc or the hotkey only hide it: the app keeps
//! running, so it reopens at once.
//!
//! Nothing runs while it is hidden: its windows are gone and nothing asks for frames. What it paints is made of
//! textures that are built once (see `backdrop.rs`), so opening it and switching pages costs a few quads.
//!
//! The overlay does not know the pages themselves. The app hands [`MainOverlay::show`] a function that draws
//! the page asked for into the space it is given; a page that panics while drawing is reported back so the app
//! can throw its state away and build it again (`repair` in the Python app).

pub mod backdrop;
pub mod chrome;
pub mod pages;
pub mod switcher;

use std::panic::AssertUnwindSafe;

use egui::{
    Align, Area, Color32, Context, Event, Id, Key, Layout, Modifiers, Order, Rect, Sense, Ui, UiBuilder,
    ViewportBuilder, ViewportId, pos2, vec2,
};
use lum_core::settings::Settings;
use lum_platform::desktop::{self, APP_ID, marker};
use lum_platform::hotkeys::{self, Os};

use crate::anim::{self, Ease, Kind};
use crate::fonts::Weight;
use crate::text::Text;
use crate::theme::{Tokens, Tone};
use crate::widgets::IconButton;
use crate::window::{self, Screen};

pub use backdrop::Backdrop;
pub use chrome::{ChromeData, ChromeLayer};

/// The overlay's windows are titled per monitor, so on Hyprland a cover rule opens each full size on its own
/// screen.
pub const COVER_PREFIX: &str = "Luminescent overlay";

/// What the overlay takes from the settings. The app makes a new one whenever the settings change.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// How dark the backdrop is, 0 to 1.
    pub dim: f32,
    /// The switcher's pages: key, name and tooltip.
    pub nav: Vec<(String, String, String)>,
    /// The overlay hotkey in capitals, shown top right.
    pub hotkey_hint: String,
    pub safe_mode: bool,
    pub click_outside_closes: bool,
    pub type_to_search: bool,
    pub remember_page: bool,
    pub last_page: String,
    /// Open on the screen with the mouse instead of the primary one.
    pub on_cursor_screen: bool,
    /// Hyprland blurs what is behind the backdrop.
    pub blur: bool,
    /// `ui.chrome_items`, as saved (empty for the default layout).
    pub chrome_items: Vec<serde_json::Value>,
    pub show_clock: bool,
    pub show_status: bool,
    /// The dashboard's snap grid, which chrome elements snap to as well.
    pub snap: f32,
}

impl Default for Config {
    /// The settings' defaults.
    fn default() -> Config {
        let order: Vec<String> = pages::NAV_SECTIONS.iter().map(|(k, _)| (*k).to_owned()).collect();
        Config {
            dim: 0.64,
            nav: nav_entries(&order, |_| String::new()),
            hotkey_hint: hotkeys::pretty("ctrl+shift+space", Os::current()).to_uppercase(),
            safe_mode: false,
            click_outside_closes: true,
            type_to_search: true,
            remember_page: false,
            last_page: "dashboard".to_owned(),
            on_cursor_screen: true,
            blur: true,
            chrome_items: Vec::new(),
            show_clock: true,
            show_status: true,
            snap: 8.0,
        }
    }
}

/// The switcher's entries: key, name and a tooltip with Ctrl+1…9 and the page's own hotkey, if it has one.
fn nav_entries(order: &[String], own_hotkey: impl Fn(&str) -> String) -> Vec<(String, String, String)> {
    order
        .iter()
        .enumerate()
        .map(|(i, key)| {
            let name = pages::title(key).unwrap_or(key).to_owned();
            let jump = if i < 9 { format!("Ctrl+{}", i + 1) } else { String::new() };
            let own = own_hotkey(key);
            let tip = match (own.is_empty(), jump.is_empty()) {
                (true, _) => jump,
                (false, true) => own,
                (false, false) => format!("{own}   ·   {jump}"),
            };
            (key.clone(), name, tip)
        })
        .collect()
}

impl Config {
    pub fn from_settings(settings: &Settings, safe_mode: bool) -> Config {
        let os = Os::current();
        let order = pages::nav_order(&settings.get_str_list("ui.nav_order"));
        let nav = nav_entries(&order, |key| {
            let own = pages::NAV_SECTIONS.iter().any(|(k, _)| *k == key);
            let combo =
                if own { settings.get_str(&pages::hotkey_setting(key)).unwrap_or_default() } else { "" };
            if combo.is_empty() { String::new() } else { hotkeys::pretty(combo, os) }
        });
        let performance = settings.get_bool("ui.performance_mode", false);
        Config {
            dim: settings.get_f64("ui.overlay_dim").unwrap_or(0.64) as f32,
            nav,
            hotkey_hint: hotkeys::pretty(settings.get_str("ui.overlay_hotkey").unwrap_or_default(), os)
                .to_uppercase(),
            safe_mode,
            click_outside_closes: settings.get_bool("ui.click_outside_closes", true),
            type_to_search: settings.get_bool("ui.type_to_search", true),
            remember_page: settings.get_bool("ui.remember_page", false),
            last_page: settings.get_str("ui.last_page").unwrap_or("dashboard").to_owned(),
            on_cursor_screen: settings.get_str("ui.overlay_screen").unwrap_or("cursor") == "cursor",
            blur: settings.get_bool("ui.blur", true) && !performance,
            chrome_items: settings
                .get("ui.chrome_items")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default(),
            show_clock: settings.get_bool("ui.show_clock", true),
            show_status: settings.get_bool("ui.show_status", true),
            snap: settings.get_f64("dashboard.snap").unwrap_or(8.0).max(1.0) as f32,
        }
    }

    /// Where the overlay opens: the Dashboard, or the page you were on (Settings → Overlay).
    pub fn start_page(&self) -> String {
        if self.remember_page && self.nav.iter().any(|(k, _, _)| *k == self.last_page) {
            return self.last_page.clone();
        }
        "dashboard".to_owned()
    }
}

/// Where the app should draw a page this frame.
#[derive(Debug, Clone)]
pub struct PageSlot<'a> {
    pub key: &'a str,
    /// The page's space in desktop coordinates, so dialogs and windows it opens can go in front of it.
    pub on_desktop: Rect,
    /// Set when this is another screen than the switcher's; only the Dashboard has anything there.
    pub extra_screen: Option<&'a str>,
    /// The name of the screen this is drawn on.
    pub screen: &'a str,
    /// The whole window of this screen in the `Ui`'s own coordinates: the Dashboard's boxes are placed in it.
    pub canvas: Rect,
    /// Add this to a point of the window to get its place on the desktop.
    pub to_desktop: egui::Vec2,
    /// Every screen of the overlay, in desktop coordinates.
    pub screens: &'a [Screen],
    /// How far down the logo, clock and chips reach on the switcher's screen (0 on the others), so the
    /// Dashboard's boxes start below them.
    pub chrome_bottom: f32,
}

/// What a page says about itself after a frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PageFrame {
    /// It uses Esc and typed keys itself right now (a dialog of its own is open).
    pub busy: bool,
}

/// Draws one page into the `Ui` it is given.
pub type PageFn<'a> = dyn FnMut(&mut Ui, &PageSlot<'_>) -> PageFrame + 'a;

/// What happened on the overlay this frame, for the app to act on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Out {
    /// It finished going away (after its fade).
    pub closed: bool,
    /// Typing on the Dashboard or Themes page: open the command palette with this text.
    pub palette: Option<String>,
    /// The first frame of an open was drawn (for the open latency).
    pub first_frame: bool,
    /// The page shown changed to this one.
    pub page_shown: Option<String>,
    /// This page panicked while drawing: throw its state away and build it again.
    pub repair: Option<String>,
    /// The chrome elements changed: save this list as `ui.chrome_items`.
    pub chrome: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Closed,
    Open,
    Closing,
}

#[derive(Debug)]
pub struct MainOverlay {
    state: State,
    current: String,
    screens: Vec<Screen>,
    /// The screen with the switcher and the panel.
    host: usize,
    /// Counts opens, so each one fades in afresh.
    serial: u64,
    /// Counts page switches while open, so each new page fades in.
    switches: u64,
    /// The page before the switch filled the screen, so the whole panel fades in, not just the page.
    from_full: bool,
    focus_pending: bool,
    first_frame_pending: bool,
    backdrop: Backdrop,
    pub chrome: ChromeLayer,
}

impl Default for MainOverlay {
    fn default() -> MainOverlay {
        MainOverlay {
            state: State::Closed,
            current: "dashboard".to_owned(),
            screens: Vec::new(),
            host: 0,
            serial: 0,
            switches: 0,
            from_full: false,
            focus_pending: false,
            first_frame_pending: false,
            backdrop: Backdrop::default(),
            chrome: ChromeLayer::default(),
        }
    }
}

fn base_id() -> Id {
    Id::unique("lum-overlay")
}

impl MainOverlay {
    /// Open (or still fading out): its windows are on screen.
    pub fn is_visible(&self) -> bool {
        self.state != State::Closed
    }

    /// Open and staying open; a second press of the hotkey hides it.
    pub fn is_open(&self) -> bool {
        self.state == State::Open
    }

    pub fn current(&self) -> &str {
        &self.current
    }

    pub fn screens(&self) -> &[Screen] {
        &self.screens
    }

    /// The screen with the switcher, once it has been opened.
    pub fn host_screen(&self) -> Option<&Screen> {
        self.screens.get(self.host)
    }

    /// Picks the screen to open on: the one with the mouse when the settings say so, else the primary one
    /// (listed first).
    pub fn host_for(screens: &[Screen], on_cursor_screen: bool) -> usize {
        if on_cursor_screen { window::cursor_screen(screens).unwrap_or(0) } else { 0 }
    }

    /// Shows the overlay on `page` across `screens`. When it is open already it only switches page (it does
    /// not jump to the screen the mouse is on while it is up).
    pub fn open(&mut self, page: &str, screens: Vec<Screen>, host: usize) {
        let page = if pages::title(page).is_some() { page } else { "dashboard" };
        if self.state == State::Open {
            self.show_page(page);
            return;
        }
        self.screens = screens;
        self.host = host.min(self.screens.len().saturating_sub(1));
        self.state = State::Open;
        self.serial += 1;
        self.switches = 0;
        self.focus_pending = true;
        self.first_frame_pending = true;
        self.current = page.to_owned();
    }

    /// Fades it out; [`Out::closed`] says when it is gone.
    pub fn hide(&mut self) {
        if self.state == State::Open {
            self.state = State::Closing;
            self.chrome.set_editing(false, false);
        }
    }

    /// Switches page while it is open (or picks the page it opens on next).
    pub fn show_page(&mut self, key: &str) {
        if key == self.current || pages::title(key).is_none() {
            return;
        }
        self.from_full = pages::is_full(&self.current);
        self.current = key.to_owned();
        if self.state == State::Open {
            self.switches += 1;
        }
    }

    /// Gets the expensive parts ready before the first open: the picture is decoded, the glow, halo and dot
    /// textures are made for each screen and the switcher's and chrome's text is in the font atlas. Nothing is
    /// shown: it all goes through a painter that draws nowhere.
    pub fn prewarm(&mut self, ctx: &Context, screens: &[Screen], config: &Config) {
        let tokens = Tokens::of(ctx);
        self.backdrop.prepare(ctx, &tokens);
        let layer = egui::LayerId::new(Order::Background, base_id().with("prewarm"));
        let painter = ctx.layer_painter(layer).with_clip_rect(Rect::NOTHING);
        let host = MainOverlay::host_for(screens, config.on_cursor_screen);
        for (i, screen) in screens.iter().enumerate() {
            let rect = Rect::from_min_size(pos2(0.0, 0.0), screen.rect.size());
            self.backdrop.paint(&painter, rect, &tokens, config.dim, i == host);
            let (top, bottom) = screen.insets();
            backdrop::panel(&painter, backdrop::panel_rect(rect.size(), top, bottom), &tokens);
        }
        for (_, name, _) in &config.nav {
            drop(painter.layout_no_wrap(
                name.clone(),
                tokens.font(14.0, Weight::Medium, false),
                Color32::WHITE,
            ));
        }
        for text in ["LUMINESCENT", "0123456789:", &config.hotkey_hint] {
            for (px, mono) in [(15.0, true), (26.0, true), (10.5, true), (10.0, true), (11.5, true)] {
                drop(painter.layout_no_wrap(
                    text.to_owned(),
                    tokens.font(px, Weight::Medium, mono),
                    Color32::WHITE,
                ));
            }
        }
        tracing::debug!("overlay prewarmed for {} screens", screens.len());
    }

    /// Draws the overlay for this frame on every screen (nothing while it is hidden). Call it every frame.
    pub fn show(&mut self, ctx: &Context, config: &Config, data: &ChromeData, page: &mut PageFn<'_>) -> Out {
        let mut out = Out::default();
        if self.state == State::Closed {
            return out;
        }
        if self.screens.is_empty() {
            self.screens.push(window::fallback_screen(ctx));
        }
        let reveal = match self.state {
            State::Closing => anim::value_from(
                ctx,
                base_id().with(("hide", self.serial)),
                1.0,
                0.0,
                Kind::Overlay,
                120.0,
                Ease::In,
            ),
            _ => anim::value_from(
                ctx,
                base_id().with(("show", self.serial)),
                0.0,
                1.0,
                Kind::Overlay,
                150.0,
                Ease::Out,
            ),
        };
        if self.state == State::Closing && reveal <= 0.0 {
            self.state = State::Closed;
            anim::forget(ctx, base_id().with(("show", self.serial)));
            anim::forget(ctx, base_id().with(("hide", self.serial)));
            out.closed = true;
            ctx.request_repaint();
            return out;
        }
        if self.first_frame_pending {
            switcher::snap(ctx, base_id().with("switcher"));
        }
        let top = self.screens.get(self.host).map_or(0.0, |s| s.insets().0);
        self.chrome.load(&config.chrome_items, top);
        let pass = Pass { reveal, config, data };

        if ctx.embed_viewports() {
            // no separate windows (tests, --smoke): the switcher's screen over this whole window
            let rect = ctx.content_rect();
            Area::new(base_id().with("embedded")).order(Order::Background).fixed_pos(rect.min).show(
                ctx,
                |ui| {
                    let rect = Rect::from_min_size(ui.cursor().min, rect.size());
                    ui.allocate_rect(rect, Sense::hover());
                    let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
                    self.draw_screen(&mut child, self.host, &pass, page, &mut out);
                },
            );
        } else {
            let focus_now = std::mem::take(&mut self.focus_pending);
            for i in 0..self.screens.len() {
                let screen = self.screens[i].clone();
                let host = i == self.host;
                let title = title(&screen, host, config.blur);
                let builder = ViewportBuilder::default()
                    .with_title(title.clone())
                    .with_app_id(APP_ID)
                    .with_decorations(false)
                    .with_transparent(true)
                    .with_always_on_top()
                    .with_taskbar(false)
                    .with_resizable(false)
                    .with_active(host)
                    .with_position(screen.rect.min)
                    .with_inner_size(screen.rect.size());
                let id = ViewportId::from_hash_of(("lum-overlay", &screen.name));
                ctx.show_viewport_immediate(id, builder, |ui, _| {
                    let rect = ui.max_rect();
                    let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
                    self.draw_screen(&mut child, i, &pass, page, &mut out);
                });
                window::shown(ctx, &title, None, host);
                if host && focus_now {
                    window::placer(ctx).focus(&title);
                }
            }
        }
        if std::mem::take(&mut self.first_frame_pending) {
            out.first_frame = true;
        }
        out.chrome = self.chrome.take_changes();
        out
    }

    fn draw_screen(
        &mut self,
        ui: &mut Ui,
        index: usize,
        pass: &Pass<'_>,
        page: &mut PageFn<'_>,
        out: &mut Out,
    ) {
        let (config, data) = (pass.config, pass.data);
        let tokens = Tokens::of(ui.ctx());
        let rect = ui.max_rect();
        let screen = self.screens[index].clone();
        let host = index == self.host;
        let (top, bottom) = screen.insets();
        ui.multiply_opacity(pass.reveal);
        self.backdrop.paint(ui.painter(), rect, &tokens, config.dim, host);
        // the dimmed screen goes first, so everything drawn on it takes clicks before it does
        let background = ui.interact(rect, base_id().with(("backdrop", index)), Sense::click());
        let surface = Rect::from_min_max(rect.min + vec2(0.0, top), rect.max - vec2(0.0, bottom));
        let to_desktop = screen.rect.min - rect.min;
        let full = pages::is_full(&self.current);
        let mut frame = PageFrame::default();

        let names: Vec<String> = self.screens.iter().map(|s| s.name.clone()).collect();
        let host_name = self.screens[self.host].name.clone();
        let host_top = self.screens[self.host].insets().0;
        let current = self.current.clone();
        let screens = self.screens.clone();
        let place = chrome::Place {
            screen: &screen.name,
            host: &host_name,
            screens: &names,
            page: &current,
            show_clock: config.show_clock,
            show_status: config.show_status,
            snap: config.snap,
            top: host_top,
        };
        let chrome_bottom = if host && self.current == "dashboard" {
            self.chrome.bottom_on_host(ui, &place, data)
        } else {
            0.0
        };
        if host {
            if full {
                let slot = PageSlot {
                    key: &self.current,
                    on_desktop: surface.translate(to_desktop),
                    extra_screen: None,
                    screen: &screen.name,
                    canvas: rect,
                    to_desktop,
                    screens: &screens,
                    chrome_bottom,
                };
                frame = self.draw_page(ui, surface, &slot, false, page, out);
            } else {
                let panel = backdrop::panel_rect(rect.size(), top, bottom).translate(rect.min.to_vec2());
                let fade = self.page_fade(ui.ctx(), self.from_full);
                ui.scope(|ui| {
                    ui.multiply_opacity(fade);
                    backdrop::panel(ui.painter(), panel, &tokens);
                });
                ui.interact(panel, base_id().with("panel"), Sense::click());
                let inner = panel.shrink(1.0);
                let slot = PageSlot {
                    key: &self.current,
                    on_desktop: inner.translate(to_desktop),
                    extra_screen: None,
                    screen: &screen.name,
                    canvas: rect,
                    to_desktop,
                    screens: &screens,
                    chrome_bottom: 0.0,
                };
                frame = self.draw_page(ui, inner, &slot, true, page, out);
            }
            self.chrome.show(ui, &place, data);
            let (clicked, _) = switcher::show(
                ui,
                base_id().with("switcher"),
                rect,
                rect.min.y + top,
                &config.nav,
                &self.current,
            );
            if let Some(key) = clicked {
                self.switch_to(&key, out);
            }
            match self.top_right(ui, rect, top, config) {
                Some(TopRight::Close) => self.hide(),
                Some(TopRight::Edit) => {
                    let on = !self.chrome.editing;
                    self.chrome.set_editing(on, config.chrome_items.is_empty());
                }
                None => {}
            }
        } else if self.current == "dashboard" {
            let slot = PageSlot {
                key: &self.current,
                on_desktop: surface.translate(to_desktop),
                extra_screen: Some(&screen.name),
                screen: &screen.name,
                canvas: rect,
                to_desktop,
                screens: &screens,
                chrome_bottom: 0.0,
            };
            frame = self.draw_page(ui, surface, &slot, false, page, out);
        }
        if !host {
            self.chrome.show(ui, &place, data);
        }

        // the Dashboard works like a desktop: clicking it never closes the overlay
        let empty_space = !full || (!host && self.current != "dashboard");
        if background.clicked() && empty_space && config.click_outside_closes {
            self.hide();
        }
        self.keys(ui, config, frame, out);
    }

    /// The page with its fade after a switch, inside `rect` and clipped to it.
    fn draw_page(
        &self,
        ui: &mut Ui,
        rect: Rect,
        slot: &PageSlot<'_>,
        in_panel: bool,
        page: &mut PageFn<'_>,
        out: &mut Out,
    ) -> PageFrame {
        let fade = self.page_fade(ui.ctx(), false);
        let mut child = ui.new_child(UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min)));
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        child.multiply_opacity(fade);
        if in_panel {
            child.shrink_clip_rect(rect.shrink(0.5));
        }
        let drawn = std::panic::catch_unwind(AssertUnwindSafe(|| page(&mut child, slot)));
        drawn.unwrap_or_else(|_| {
            tracing::error!("the {} page failed while drawing: building it again", slot.key);
            out.repair = Some(slot.key.to_owned());
            PageFrame::default()
        })
    }

    /// How far the page shown last has faded in (1 when nothing was switched since it opened).
    fn page_fade(&self, ctx: &Context, only_if_from_full: bool) -> f32 {
        if self.switches == 0 || (only_if_from_full && !self.from_full) {
            return 1.0;
        }
        let id = base_id().with(("page", self.serial, self.switches));
        anim::value_from(ctx, id, 0.0, 1.0, Kind::Pages, 170.0, Ease::Out)
    }

    fn switch_to(&mut self, key: &str, out: &mut Out) {
        if key != self.current {
            self.show_page(key);
            out.page_shown = Some(key.to_owned());
        }
    }

    /// The overlay hotkey, SAFE MODE when it applies, ✎ and the round ✕.
    fn top_right(&self, ui: &mut Ui, screen: Rect, top: f32, config: &Config) -> Option<TopRight> {
        let right = pos2(screen.max.x - 28.0, screen.min.y + top + 26.0);
        let area = Rect::from_min_max(pos2(right.x - 600.0, right.y), pos2(right.x, right.y + 36.0));
        let mut clicked = None;
        let mut row =
            ui.new_child(UiBuilder::new().max_rect(area).layout(Layout::right_to_left(Align::Center)));
        row.spacing_mut().item_spacing.x = 14.0;
        let tip = "Hide (Esc). Luminescent keeps running; quit from Settings → General.";
        if row.add(IconButton::new("✕").size(36.0).font_px(13.0).tooltip(tip)).clicked() {
            clicked = Some(TopRight::Close);
        }
        let tip = "Edit the layout: move, add, copy, resize or freeze the logo, clock, date and status chips";
        if row.add(IconButton::new("✎").size(36.0).font_px(13.0).tooltip(tip)).clicked() {
            clicked = Some(TopRight::Edit);
        }
        if config.safe_mode {
            row.add(Text::new("SAFE MODE").px(10.0).semibold().mono().spacing(0.08).tone(Tone::Danger));
        }
        if !config.hotkey_hint.is_empty() {
            row.add(Text::new(&config.hotkey_hint).px(10.0).medium().mono().spacing(0.08).tone(Tone::Muted));
        }
        clicked
    }

    /// Esc, Ctrl+1…9, and typing on the Dashboard or Themes page, which opens the palette with what was typed.
    fn keys(&mut self, ui: &mut Ui, config: &Config, frame: PageFrame, out: &mut Out) {
        let typing = ui.ctx().memory(|m| m.focused().is_some());
        let (escape, jump, typed) = ui.input_mut(|i| {
            let escape = i.key_pressed(Key::Escape);
            let digits = [
                Key::Num1,
                Key::Num2,
                Key::Num3,
                Key::Num4,
                Key::Num5,
                Key::Num6,
                Key::Num7,
                Key::Num8,
                Key::Num9,
            ];
            let jump = digits.iter().position(|k| i.consume_key(Modifiers::CTRL, *k));
            let plain = !(i.modifiers.ctrl || i.modifiers.alt || i.modifiers.mac_cmd);
            let typed: String = if plain {
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        Event::Text(t) => Some(t.as_str()),
                        _ => None,
                    })
                    .collect()
            } else {
                String::new()
            };
            (escape, jump, typed)
        });
        if let Some(n) = jump {
            if let Some((key, _, _)) = config.nav.get(n) {
                let key = key.clone();
                self.switch_to(&key, out);
            }
            return;
        }
        if frame.busy || self.chrome.busy() {
            return;
        }
        if escape && self.chrome.editing {
            // Esc leaves edit mode before it closes anything
            self.chrome.set_editing(false, false);
            return;
        }
        if escape {
            self.hide();
            return;
        }
        let printable =
            !typed.is_empty() && typed.chars().all(|c| !c.is_control()) && !typed.trim().is_empty();
        let searchable = matches!(self.current.as_str(), "dashboard" | "themes");
        if printable && config.type_to_search && searchable && !typing {
            out.palette = Some(typed);
        }
    }
}

/// What every screen of one frame shares.
struct Pass<'a> {
    /// How far it has faded in (or out), 0 to 1.
    reveal: f32,
    config: &'a Config,
    data: &'a ChromeData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TopRight {
    Edit,
    Close,
}

fn title(screen: &Screen, host: bool, blur: bool) -> String {
    let mut title = desktop::cover_title(COVER_PREFIX, &screen.name);
    title.push(marker::NO_PIN);
    if !host {
        title.push(marker::NO_ACTIVATE);
    }
    if blur {
        title.push(marker::BLUR);
    }
    title
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_opens_where_the_settings_say() {
        let mut config = Config::default();
        assert_eq!(config.start_page(), "dashboard");
        config.remember_page = true;
        config.last_page = "themes".into();
        assert_eq!(config.start_page(), "themes");
        config.last_page = "stats".into();
        assert_eq!(config.start_page(), "dashboard", "a page no longer in the switcher");
    }

    #[test]
    fn the_switcher_lists_hotkeys_in_its_tooltips() {
        let config = Config::default();
        let keys: Vec<&str> = config.nav.iter().map(|(k, _, _)| k.as_str()).collect();
        assert_eq!(keys, ["dashboard", "automation", "themes", "clipboard", "settings"]);
        assert_eq!(config.nav[2].2, "Ctrl+3");
        assert_eq!(config.hotkey_hint, "CTRL+SHIFT+SPACE");
    }

    #[test]
    fn showing_a_page_while_hidden_only_picks_it() {
        let mut overlay = MainOverlay::default();
        overlay.show_page("themes");
        assert_eq!(overlay.current(), "themes");
        assert!(!overlay.is_visible());
        overlay.open(
            "nonsense",
            vec![Screen::new("A", Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0)))],
            4,
        );
        assert_eq!(overlay.current(), "dashboard");
        assert!(overlay.is_open());
        overlay.hide();
        assert!(overlay.is_visible() && !overlay.is_open());
    }
}
