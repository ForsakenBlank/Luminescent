//! Screen Doodle: draw on top of everything.
//!
//! The draw key is a hold key (a press and a release bind on Hyprland, the keyboard hook on Windows). A missed
//! release can not leave the overlay swallowing clicks: while the key is held the system is asked every 100 ms
//! whether it still is (`HotkeyManager`), Esc stops drawing, and there is a panic hotkey and a tray action that
//! resets it. The overlays let every click through and never take the keyboard, except that they take the
//! mouse while drawing is on. The rules for Alt as the draw key are in `lum_screen::doodle::hold`.
//!
//! Undo is one history shared by every screen: Alt+F undoes the last stroke wherever it was drawn. The text
//! tool: click where the text goes, let go of the draw key and type; Enter puts it on the drawing, Esc throws
//! it away. The mouse wheel changes the brush size; the pointer is a circle of the real size.

mod overlay;

use std::time::Duration;

use egui::{Context, ViewportBuilder, ViewportId};
use lum_core::actions::Action;
use lum_core::module::{Module, ModuleCx, StartError};
use lum_platform::desktop::{self, APP_ID};
use lum_platform::hotkeys;
use lum_screen::IRect;
use lum_screen::doodle::compose::{self, Layer};
use lum_screen::doodle::{Effect, History, Hold, Mark, Settings, Tool};

use crate::window::{self, Screen};
use crate::{Bind, FixedBind, HoldEvent, Inbox, Request, Services, UiCx, UiModule};
use overlay::{Act, Look, Overlay};

/// The doodle's windows are titled per screen with this, so Hyprland's cover rule opens each full size on its
/// own monitor. Not `NO_ACTIVATE`: Hyprland's no-focus rule would also keep the mouse from reaching it.
pub const COVER_PREFIX: &str = "Luminescent doodle";

const TITLE: &str = "Screen Doodle";

enum Cmd {
    Toggle,
    Clear,
    Undo,
    Blackboard,
    Panic,
    Copy,
    Escape,
    /// The window that had the keyboard when drawing started (to give it back).
    Focused(Option<String>),
    Copied(String),
}

pub struct DoodleModule {
    inbox: Inbox<Cmd>,
    services: Services,
    running: bool,
    settings: Settings,
    hold: Hold,
    /// The system would not take the draw key as a hold key: it toggles instead.
    hold_refused: bool,
    /// The draw key is down, as the hotkeys last said.
    held: bool,
    overlays: Vec<Overlay>,
    history: History,
    tool: Tool,
    fill: bool,
    show_bg: bool,
    /// When the solo delay runs out (seconds on the app's clock).
    solo_at: Option<f64>,
    /// When the brush size changed by the wheel is saved (quietly, a second later).
    size_save_at: Option<f64>,
    masked: bool,
    prev_focus: Option<String>,
    /// Hold key events, taken at the top of the next frame (they need the context).
    pending_hold: Vec<HoldEvent>,
}

impl std::fmt::Debug for DoodleModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DoodleModule")
            .field("running", &self.running)
            .field("overlays", &self.overlays)
            .finish()
    }
}

impl DoodleModule {
    pub fn new(services: &Services) -> DoodleModule {
        DoodleModule {
            inbox: Inbox::new(services),
            services: services.clone(),
            running: false,
            settings: Settings::default(),
            hold: Hold::default(),
            hold_refused: false,
            held: false,
            overlays: Vec::new(),
            history: History::default(),
            tool: Tool::Pen,
            fill: false,
            show_bg: false,
            solo_at: None,
            size_save_at: None,
            masked: false,
            prev_focus: None,
            pending_hold: Vec::new(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.hold.is_active()
    }

    pub fn tool(&self) -> Tool {
        self.tool
    }

    pub fn fill(&self) -> bool {
        self.fill
    }

    pub fn blackboard(&self) -> bool {
        self.show_bg
    }

    pub fn brush_size(&self) -> u32 {
        self.settings.effective_size()
    }

    /// How many marks each screen holds (tests).
    pub fn marks(&self) -> Vec<usize> {
        self.overlays.iter().map(|o| o.canvas.marks().len()).collect()
    }

    /// The marks on one screen (tests).
    pub fn marks_on(&self, screen: usize) -> &[Mark] {
        self.overlays.get(screen).map_or(&[], |o| o.canvas.marks())
    }

    pub fn undo_steps(&self) -> usize {
        self.history.len()
    }

    fn hold_mode(&self) -> bool {
        self.settings.hold_mode && !self.hold_refused
    }

    fn notify(&self, text: &str, ms: u32) {
        self.services.send(Request::Notify { title: TITLE.into(), text: text.into(), ms });
    }

    /// One overlay per screen, made again (a monitor may have come or gone since). Drawings are kept only while
    /// the screens are the same.
    fn ensure_overlays(&mut self, ctx: &Context) {
        let screens = window::screens_in(ctx);
        let same = screens.len() == self.overlays.len()
            && screens
                .iter()
                .zip(&self.overlays)
                .all(|(s, o)| s.name == o.screen.name && s.rect == o.screen.rect);
        if same {
            return;
        }
        if !self.overlays.is_empty() {
            tracing::info!("the screens changed: new doodle overlays");
        }
        self.overlays = screens.into_iter().enumerate().map(|(i, s)| Overlay::new(s, i == 0)).collect();
        self.history.clear();
    }

    fn look(&self, now: f64) -> Look<'_> {
        Look {
            active: self.hold.is_active(),
            show_bg: self.show_bg,
            settings: &self.settings,
            tool: self.tool,
            fill: self.fill,
            now,
        }
    }

    fn apply(&mut self, ctx: &Context, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Activate => self.activated(ctx),
                Effect::Deactivate => self.deactivated(),
                Effect::StartTimer(ms) => {
                    self.solo_at = Some(ctx.input(|i| i.time) + ms as f64 / 1000.0);
                    ctx.request_repaint_after(Duration::from_millis(ms));
                }
                Effect::StopTimer => self.solo_at = None,
                Effect::DiscardText => {
                    for o in &mut self.overlays {
                        o.text = None;
                    }
                }
                Effect::Clear => self.clear(),
            }
        }
        ctx.request_repaint();
    }

    fn activated(&mut self, ctx: &Context) {
        if let Some(tool) = self.settings.start_tool {
            self.tool = tool;
        }
        if !self.overlays.iter().any(|o| o.canvas.has_content()) {
            self.ensure_overlays(ctx);
        }
        // drawing clicks give the overlay the keyboard; remember who had it to hand it back afterwards
        let inbox = self.inbox.clone();
        let spawned = std::thread::Builder::new()
            .name("doodle-focus".into())
            .spawn(move || inbox.push(Cmd::Focused(desktop::active_window())));
        if let Err(e) = spawned {
            tracing::warn!("could not ask which window has the keyboard: {e}");
        }
    }

    fn deactivated(&mut self) {
        self.masked = false;
        for o in &mut self.overlays {
            o.stop();
        }
        if let Some(prev) = self.prev_focus.take() {
            let spawned = std::thread::Builder::new()
                .name("doodle-focus".into())
                .spawn(move || desktop::refocus(&prev));
            if let Err(e) = spawned {
                tracing::warn!("could not give the keyboard back: {e}");
            }
        }
    }

    /// One step of the shared history; the screens of steps that fell off keep their oldest mark for good.
    fn record(&mut self, screens: Vec<usize>) {
        for i in self.history.record(screens, self.settings.undo_levels) {
            if let Some(o) = self.overlays.get_mut(i) {
                o.canvas.bake_oldest();
            }
        }
    }

    fn clear(&mut self) {
        self.record((0..self.overlays.len()).collect());
        for o in &mut self.overlays {
            o.canvas.add(Mark::Clear);
        }
    }

    fn undo(&mut self) {
        if let Some(screens) = self.history.undo() {
            for i in screens {
                if let Some(o) = self.overlays.get_mut(i) {
                    o.canvas.undo();
                }
            }
        }
    }

    fn panic(&mut self, ctx: &Context) {
        self.show_bg = false;
        let effects = self.hold.panic(self.held);
        for o in &mut self.overlays {
            o.text = None;
        }
        self.clear();
        self.apply(ctx, effects);
    }

    fn take_commands(&mut self, ctx: &Context) {
        for cmd in self.inbox.take() {
            match cmd {
                Cmd::Toggle => {
                    let effects = self.hold.toggle();
                    self.apply(ctx, effects);
                }
                Cmd::Clear => {
                    self.ensure_overlays(ctx);
                    self.clear();
                }
                Cmd::Undo => self.undo(),
                Cmd::Blackboard => {
                    self.ensure_overlays(ctx);
                    self.show_bg = !self.show_bg;
                }
                Cmd::Panic => self.panic(ctx),
                Cmd::Copy => self.copy_screen(ctx),
                Cmd::Escape => {
                    let effects = self.hold.escape();
                    self.apply(ctx, effects);
                }
                Cmd::Focused(addr) => {
                    if self.hold.is_active() {
                        self.prev_focus = addr;
                    }
                }
                Cmd::Copied(text) => self.notify(&text, 2500),
            }
        }
    }

    /// Screenshots every screen and paints the drawings (and the blackboard) on top, on a worker thread. On
    /// Wayland the compositor's copy already has our windows in it, drawing and blackboard included.
    fn copy_screen(&mut self, ctx: &Context) {
        self.ensure_overlays(ctx);
        let with_drawing = !cfg!(target_os = "linux");
        let screens: Vec<Screen> = self.overlays.iter().map(|o| o.screen.clone()).collect();
        let drawings: Vec<_> =
            self.overlays.iter().map(|o| with_drawing.then(|| o.canvas.pixmap().clone())).collect();
        let bg = (self.show_bg && with_drawing).then_some(self.settings.bg_opacity);
        let (capture, inbox, save) = (window::capture(ctx), self.inbox.clone(), self.settings.save_copies);
        let spawned = std::thread::Builder::new().name("doodle-copy".into()).spawn(move || {
            let monitors: Vec<_> = screens.iter().map(Screen::monitor).collect();
            let shots = capture.grab(&monitors);
            let scale = screens.iter().map(|s| if cfg!(windows) { 1.0 } else { s.scale }).fold(1.0, f32::max);
            let layers: Vec<Layer<'_>> = screens
                .iter()
                .zip(&shots)
                .zip(&drawings)
                .map(|((s, shot), drawing)| Layer {
                    rect: IRect::new(
                        s.rect.min.x as i32,
                        s.rect.min.y as i32,
                        s.rect.width() as i32,
                        s.rect.height() as i32,
                    ),
                    shot: shot.as_ref().ok(),
                    drawing: drawing.as_ref(),
                    blackboard: bg,
                })
                .collect();
            let picture = compose::compose(&layers, scale);
            inbox.push(Cmd::Copied(put_away(picture, save)));
        });
        if let Err(e) = spawned {
            tracing::warn!("could not start copying the screen: {e}");
        }
    }

    fn draw(&mut self, ctx: &Context) {
        let now = ctx.input(|i| i.time);
        let active = self.hold.is_active();
        let wanted: Vec<bool> = self
            .overlays
            .iter()
            .map(|o| active || self.show_bg || o.canvas.has_content() || o.text.is_some())
            .collect();
        if !wanted.contains(&true) {
            return;
        }
        let mut acts = Vec::new();
        let embedded = ctx.embed_viewports();
        let mut overlays = std::mem::take(&mut self.overlays);
        {
            let look = self.look(now);
            for (i, o) in overlays.iter_mut().enumerate() {
                if !wanted[i] {
                    continue;
                }
                let mut mine = Vec::new();
                show_overlay(ctx, o, &look, embedded, &mut mine);
                acts.extend(mine.into_iter().map(|a| (i, a)));
            }
        }
        self.overlays = overlays;
        for (i, act) in acts {
            self.act(ctx, i, act);
        }
        if let Some(at) = self.size_save_at {
            if now >= at {
                self.size_save_at = None;
                let key = format!("doodle.{}", self.settings.size_key());
                self.services.send(Request::SetSetting { key, value: self.settings.effective_size().into() });
            } else {
                ctx.request_repaint_after(Duration::from_secs_f64(at - now));
            }
        }
    }

    fn act(&mut self, ctx: &Context, i: usize, act: Act) {
        match act {
            Act::Record => self.record(vec![i]),
            Act::Add(mark) => {
                if let Some(o) = self.overlays.get_mut(i) {
                    o.canvas.add(mark);
                }
            }
            Act::Tool(tool) => self.tool = tool,
            Act::ToggleFill => self.fill = !self.fill,
            Act::Close => {
                let effects = self.hold.deactivate();
                self.apply(ctx, effects);
            }
            Act::Size(steps) => {
                self.settings.change_size(steps);
                self.size_save_at = Some(ctx.input(|inp| inp.time) + 1.0);
            }
            Act::TextOpened => {
                self.hold.text_opened();
                if let Some(o) = self.overlays.get(i) {
                    let title = desktop::cover_title(COVER_PREFIX, &o.screen.name);
                    window::placer(ctx).focus(&title);
                }
            }
            Act::TextClosed(mark) => {
                if let Some(mark) = mark {
                    self.record(vec![i]);
                    if let Some(o) = self.overlays.get_mut(i) {
                        o.canvas.add(mark);
                    }
                }
                let effects = self.hold.text_closed();
                self.apply(ctx, effects);
            }
            Act::MaskMenuKey => {
                if !self.masked
                    && self.hold_mode()
                    && hotkeys::keys::is_modifier(&hotkeys::normalize(&self.settings.hotkey))
                {
                    hotkeys::mask_menu_key();
                }
                self.masked = true;
            }
        }
    }
}

/// The picture on the clipboard, and in ~/Pictures/Luminescent too when asked; what the notification says.
fn put_away(picture: lum_screen::Frame, save: bool) -> String {
    let clip = lum_platform::clipboard::Picture {
        width: picture.width as usize,
        height: picture.height as usize,
        rgba: picture.rgba.clone(),
    };
    if let Err(e) = lum_platform::clipboard::set_picture(&clip) {
        return format!("Could not copy the screen: {e}");
    }
    let mut msg = "Screenshot with your drawing copied to the clipboard.".to_owned();
    if save {
        let folder = lum_core::paths::user_home().join("Pictures").join("Luminescent");
        let stamp = jiff::ZonedDateTime::now().strftime("%Y%m%d-%H%M%S");
        let path = folder.join(format!("doodle-{stamp}.png"));
        let saved = std::fs::create_dir_all(&folder)
            .map_err(|e| e.to_string())
            .and_then(|()| picture.to_image().save(&path).map_err(|e| e.to_string()));
        match saved {
            Ok(()) => msg.push_str(&format!("\nSaved to {}", path.display())),
            Err(e) => msg.push_str(&format!("\nCould not save it: {e}")),
        }
    }
    msg
}

/// One overlay for this frame: a window of its own over its screen, or drawn in place (tests).
fn show_overlay(ctx: &Context, o: &mut Overlay, look: &Look<'_>, embedded: bool, acts: &mut Vec<Act>) {
    let rect = o.screen.rect;
    let id = egui::Id::unique(("lum-doodle", &o.screen.name));
    if embedded {
        egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(rect.min).constrain(false).show(
            ctx,
            |ui| {
                ui.set_clip_rect(rect);
                o.frame(ui, rect, look, acts);
            },
        );
        return;
    }
    let title = desktop::cover_title(COVER_PREFIX, &o.screen.name);
    let takes_mouse = look.active || o.text.is_some();
    let builder = ViewportBuilder::default()
        .with_title(title.clone())
        .with_app_id(APP_ID)
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top()
        .with_taskbar(false)
        .with_resizable(false)
        .with_active(false)
        .with_mouse_passthrough(!takes_mouse)
        .with_position(rect.min)
        .with_inner_size(rect.size());
    ctx.show_viewport_immediate(
        ViewportId::from_hash_of(("lum-doodle", &o.screen.name)),
        builder,
        |ui, _| {
            let area = egui::Rect::from_min_size(egui::Pos2::ZERO, ui.ctx().content_rect().size());
            o.frame(ui, area, look, acts);
        },
    );
    window::shown(ctx, &title, None, false);
    let r = rect;
    window::placer(ctx).cover(&title, (r.min.x as i32, r.min.y as i32, r.width() as i32, r.height() as i32));
}

impl Module for DoodleModule {
    fn key(&self) -> &'static str {
        "doodle"
    }

    fn title(&self) -> &'static str {
        TITLE
    }

    fn start(&mut self, cx: &mut ModuleCx<'_>) -> Result<(), StartError> {
        let section =
            cx.settings.section("doodle").cloned().map(serde_json::Value::Object).unwrap_or_default();
        self.settings = Settings::from_section(&section);
        let own = self.settings.own_keys(hotkeys::normalize);
        let s = &self.settings;
        self.hold = Hold::new(s.hold_mode, s.solo_only, s.solo_delay_ms, s.clear_on_release, own);
        self.hold_refused = false;
        self.held = false;
        self.running = true;
        Ok(())
    }

    fn stop(&mut self, _cx: &mut ModuleCx<'_>) {
        self.hold.panic(false);
        self.show_bg = false;
        self.overlays.clear();
        self.history.clear();
        self.solo_at = None;
        if self.size_save_at.take().is_some() {
            let key = format!("doodle.{}", self.settings.size_key());
            self.services.send(Request::SetSetting { key, value: self.settings.effective_size().into() });
        }
        self.running = false;
    }

    fn actions(&self) -> Vec<Action> {
        vec![
            self.inbox.action("panic", "Clear doodles & stop drawing", || Cmd::Panic),
            self.inbox.action("copy", "Copy the screen with the drawing", || Cmd::Copy),
            self.inbox.action("blackboard", "Toggle the doodle blackboard", || Cmd::Blackboard),
        ]
    }
}

impl UiModule for DoodleModule {
    fn binds(&self) -> Vec<Bind> {
        let mut binds = Vec::new();
        if !self.hold_mode() {
            binds.push(Bind { setting: "hotkey", id: "toggle", label: "toggle drawing" });
        }
        binds.extend([
            Bind { setting: "clear_hotkey", id: "clear", label: "clear" },
            Bind { setting: "undo_hotkey", id: "undo", label: "undo" },
            Bind { setting: "bg_hotkey", id: "blackboard", label: "blackboard" },
            Bind { setting: "exit_hotkey", id: "panic", label: "panic" },
            Bind { setting: "copy_hotkey", id: "copy", label: "copy screen + drawing" },
        ]);
        binds
    }

    fn fixed_binds(&self) -> Vec<FixedBind> {
        // Esc is bound for as long as the module runs, without taking it from the app you are in. Holding the
        // draw key adds itself to the combo, so bind that too.
        let esc =
            |combo: String| FixedBind { combo, id: "escape", label: "stop drawing", non_consuming: true };
        let mut out = vec![esc("esc".to_owned())];
        let hold = hotkeys::normalize(&self.settings.hotkey);
        if self.hold_mode() && hotkeys::keys::is_modifier(&hold) {
            out.push(esc(format!("{hold}+esc")));
        }
        out
    }

    fn hold_setting(&self) -> Option<&'static str> {
        self.settings.hold_mode.then_some("hotkey")
    }

    fn hold_event(&mut self, event: HoldEvent) {
        match event {
            HoldEvent::Down => self.held = true,
            HoldEvent::Up => self.held = false,
            HoldEvent::Refused(_) => {
                self.hold_refused = true;
                self.hold.hold_mode = false;
                return;
            }
            HoldEvent::Interrupted(_) => {}
        }
        self.pending_hold.push(event);
        self.services.wake();
    }

    fn bind_pressed(&mut self, id: &str) {
        let cmd = match id {
            "toggle" => Cmd::Toggle,
            "clear" => Cmd::Clear,
            "undo" => Cmd::Undo,
            "blackboard" => Cmd::Blackboard,
            "panic" => Cmd::Panic,
            "copy" => Cmd::Copy,
            "escape" => Cmd::Escape,
            _ => return,
        };
        self.inbox.push(cmd);
    }

    fn tray_actions(&self) -> Vec<(String, String)> {
        vec![("panic".to_owned(), "Clear doodles & stop drawing".to_owned())]
    }

    fn show(&mut self, cx: &mut UiCx<'_>) {
        if !self.running {
            return;
        }
        let ctx = cx.ctx.clone();
        for event in std::mem::take(&mut self.pending_hold) {
            let effects = match event {
                HoldEvent::Down => self.hold.pressed(),
                HoldEvent::Up => self.hold.released(),
                HoldEvent::Interrupted(name) => {
                    let drawing = self.overlays.iter().any(Overlay::is_drawing);
                    self.hold.interrupted(&name, drawing)
                }
                HoldEvent::Refused(_) => Vec::new(),
            };
            self.apply(&ctx, effects);
        }
        self.take_commands(&ctx);
        if let Some(at) = self.solo_at {
            let now = ctx.input(|i| i.time);
            if now >= at {
                self.solo_at = None;
                let other = hotkeys::other_modifiers_down(&self.settings.hotkey);
                let effects = self.hold.solo_elapsed(self.held, other);
                self.apply(&ctx, effects);
            } else {
                ctx.request_repaint_after(Duration::from_secs_f64(at - now));
            }
        }
        self.draw(&ctx);
    }
}
