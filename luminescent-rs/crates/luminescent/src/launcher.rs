//! Starting, running and stopping the app: the Rust side of `Launcher.py`.
//!
//! The main thread owns the app state and answers requests; with a display it is eframe's UI thread (see
//! `shell.rs`), without one it waits on the channels itself. Everything that waits (the ctl socket, signals,
//! timers, backups) runs on a small tokio runtime, sends a message and wakes the main thread.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use anyhow::Context;
use crossbeam_channel::{Receiver, Sender, select};
use jiff::ZonedDateTime;
use lum_core::actions::ActionRegistry;
use lum_core::clock::{Clock, SystemClock};
use lum_core::events::EventBus;
use lum_core::guard::{self, StartGuard};
use lum_core::module::{ModuleCx, ModuleHost, Notifier};
use lum_core::settings::Settings;
use lum_core::usage::{self, Usage};
use lum_core::{APP_NAME, Paths, VERSION, backup, logging};
use lum_platform::desktop::{Desktop, DesktopEvent};
use lum_platform::hotkeys::{self, HotkeyEvent, HotkeyManager};
use lum_platform::instance;
use lum_platform::ipc::{self, CtlEndpoint, CtlRequest};
use lum_platform::keepawake::KeepAwake;
use lum_platform::tray::{self, MenuItem, Tray, TrayEvent};
use lum_theme::{ThemeEvent, ThemeStore};
use lum_ui::overlay::{Config as OverlayConfig, MainOverlay, PageFrame, PageSlot};
use lum_ui::{Tokens, UiModule};
use serde_json::json;
use tokio::runtime::Runtime;

use crate::ctl::{self, CtlCommand};
use crate::{idle, shell, smoke};

pub struct Options {
    pub smoke: bool,
    pub bench: bool,
    pub safe_mode: bool,
    /// Run without windows even when there is a display.
    pub headless: bool,
    /// Measure idle CPU, wake-ups and memory for this many seconds, print them and quit.
    pub idle_check: Option<u64>,
    pub renderer: shell::Renderer,
}

/// What services tell the main thread.
#[derive(Debug)]
pub enum AppMsg {
    Quit(&'static str),
    Emit {
        name: String,
        text: String,
    },
    /// `--idle-check` is done.
    IdleReport(idle::Numbers),
    /// `--idle-check` opens the overlay (true) and hides it again before it measures.
    IdleOverlay(bool),
    Desktop(DesktopEvent),
    Tray(TrayEvent),
    /// Keep awake changed (or could not), with the error text.
    KeepAwake(bool, Option<String>),
    /// A file dialog the Themes window asked for was answered (`None` when it was cancelled).
    ThemeFile(lum_ui::themes::page::FileJob, Option<std::path::PathBuf>),
}

/// Notifications go to the log, and to the desktop once windows run and Settings → General allows them.
/// Phase 5 adds Luminescent's own cards (`general.notify_style`).
#[derive(Debug, Default)]
struct AppNotifier {
    /// The app runs with windows, so there is a desktop to show them on.
    running: bool,
    /// Settings → General → Show notifications.
    desktop: bool,
}

impl Notifier for AppNotifier {
    fn notify(&self, title: &str, message: &str, ms: u32) {
        tracing::info!("notify: {title} - {message}");
        if self.running && self.desktop {
            lum_platform::notify::show(title, message, ms, None);
        }
    }

    fn toast(&self, title: &str, message: &str, ms: u32) {
        tracing::info!("toast: {title} - {message}");
        if self.running {
            lum_platform::notify::show(title, message, ms, None);
        }
    }
}

/// Wakes the UI thread from any thread. Until eframe has made its context there is nothing to wake: the first
/// frame drains whatever arrived before it.
#[derive(Clone, Default)]
pub struct Waker(Arc<OnceLock<egui::Context>>);

impl Waker {
    pub fn set(&self, ctx: &egui::Context) {
        let _ = self.0.set(ctx.clone());
    }

    pub fn wake(&self) {
        if let Some(ctx) = self.0.get() {
            ctx.request_repaint();
        }
    }
}

/// Where services post messages: the channel plus a wake-up for the UI thread.
#[derive(Clone)]
pub struct Mailbox {
    tx: Sender<AppMsg>,
    waker: Waker,
}

impl Mailbox {
    pub fn send(&self, message: AppMsg) {
        if self.tx.send(message).is_ok() {
            self.waker.wake();
        }
    }
}

/// How often the UI thread has run, for `--idle-check`: logic passes (woken up) and ui passes (painted).
#[derive(Debug, Default)]
pub struct Passes {
    pub logic: AtomicU64,
    pub ui: AtomicU64,
}

impl Passes {
    pub fn read(&self) -> (u64, u64) {
        (self.logic.load(Ordering::Relaxed), self.ui.load(Ordering::Relaxed))
    }
}

/// The modules, in the order `modules/__init__.py` lists them. Each phase adds the ones it ports.
fn modules() -> Vec<Box<dyn UiModule>> {
    Vec::new()
}

pub fn run(options: Options, started: Instant) -> anyhow::Result<u8> {
    // --smoke and --bench-startup use a throw-away data folder unless LUMINESCENT_HOME picks one
    let throwaway = if (options.smoke || options.bench) && std::env::var_os("LUMINESCENT_HOME").is_none() {
        Some(tempfile::tempdir().context("could not make a temporary data folder")?)
    } else {
        None
    };
    let paths = match &throwaway {
        Some(dir) => Paths::custom(dir.path()),
        None => Paths::from_env(),
    };
    paths.create_dirs().with_context(|| format!("could not make {}", paths.data.display()))?;

    let lock_path = paths.data_file("instance.lock");
    let Some(lock) =
        instance::acquire(&lock_path, Duration::from_secs(3)).context("could not lock the data folder")?
    else {
        return Ok(already_running(&paths, options.smoke));
    };
    let mut app = App::start(paths, &options)?;
    let code = if options.bench {
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        tracing::info!("start to ready: {ms:.1} ms");
        println!("ready in {ms:.1} ms");
        0
    } else if options.smoke {
        smoke::run(&mut app)
    } else if options.headless || !lum_platform::desktop::has_display() {
        if !options.headless {
            tracing::info!("no display: running without windows");
        }
        app.run_headless();
        app.exit_code
    } else {
        app.start_desktop();
        let (back, code) = shell::run(app, options.renderer);
        app = back;
        code
    };
    app.shutdown();
    drop(lock);
    Ok(code)
}

/// A second start hands its request to the first copy and goes away.
fn already_running(paths: &Paths, smoke: bool) -> u8 {
    if smoke {
        println!("SMOKE skipped: already running");
        return 2;
    }
    match CtlEndpoint::for_home(paths).request("overlay", Duration::from_secs(5)) {
        Ok(_) => {
            println!("{APP_NAME} is already running, so it was asked to open.");
            0
        }
        Err(e) => {
            eprintln!("{APP_NAME} is already running but did not answer ({e}).");
            1
        }
    }
}

pub struct App {
    pub paths: Paths,
    pub settings: Settings,
    pub events: EventBus,
    pub actions: ActionRegistry,
    pub usage: Usage,
    pub host: ModuleHost<dyn UiModule>,
    pub endpoint: CtlEndpoint,
    pub ctl_requests: Receiver<CtlRequest>,
    pub safe_mode: bool,
    pub waker: Waker,
    pub passes: Arc<Passes>,
    /// What the process exits with (`--idle-check` sets 1 when over budget).
    pub exit_code: u8,
    /// Something asked to quit; the loop that runs the app stops.
    pub quitting: bool,
    /// How it runs, for reports: "no windows" or the renderer of the hidden main window.
    pub mode: String,
    /// The pixels are drawn by the CPU, whose driver memory then shows up in ours.
    pub software_renderer: bool,
    notifier: AppNotifier,
    clock: Arc<dyn Clock>,
    start_guard: Option<Arc<StartGuard>>,
    runtime: Option<Runtime>,
    /// Where the window rules, the placer and desktop events come from. Nothing until windows start.
    pub desktop: Desktop,
    pub hotkeys: HotkeyManager,
    pub themes: ThemeStore,
    pub themes_ui: lum_ui::themes::ThemesUi,
    /// New tokens a theme switch made, for the UI thread to install into egui's context.
    restyle: Option<Tokens>,
    pub overlay: MainOverlay,
    /// What the overlay takes from the settings, made again when they change.
    pub overlay_config: OverlayConfig,
    /// When the overlay was asked to open, until its first frame is drawn.
    open_asked: Option<Instant>,
    /// How long the last opens took from the request to the first frame, in ms (newest last).
    pub open_times: Vec<f64>,
    /// When the overlay was last rebuilt after a page failed (at most every few seconds).
    last_repair: Option<Instant>,
    tray: Option<Tray>,
    keep_awake: Option<KeepAwake>,
    mailbox: Mailbox,
    messages: Receiver<AppMsg>,
    flush_at: Option<Instant>,
    shut_down: bool,
}

impl App {
    fn start(paths: Paths, options: &Options) -> anyhow::Result<App> {
        let settings = Settings::load(&paths);
        let log_file = settings.get_bool("general.log_to_file", true).then(|| paths.log_file());
        if let Err(e) = logging::init(log_file.as_deref()) {
            eprintln!("could not open the log file: {e}");
        }
        let kind = if cfg!(debug_assertions) { "dev build" } else { "release" };
        tracing::info!(
            "=== starting {VERSION} (pid {}, {kind}, data in {}) ===",
            std::process::id(),
            paths.home.display()
        );
        guard::install_panic_hook(paths.crash_log());
        let now = ZonedDateTime::now().strftime("%Y-%m-%d %H:%M:%S").to_string();
        guard::mark_start_in_crash_log(&paths.crash_log(), VERSION, &now);

        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let forced =
            options.safe_mode || std::env::var_os("LUMINESCENT_SAFE_MODE").is_some_and(|v| !v.is_empty());
        let start_guard = (!options.smoke)
            .then(|| Arc::new(StartGuard::begin(&paths.start_file(), clock.timestamp(), forced)));
        let safe_mode = start_guard.as_ref().map_or(forced, |g| g.safe_mode());
        if safe_mode {
            match &start_guard {
                Some(g) if g.failed_starts >= 2 => {
                    tracing::warn!("safe mode: the last {} starts did not get going", g.failed_starts)
                }
                _ => tracing::warn!("safe mode: asked for"),
            }
        }

        let (messages_tx, messages) = crossbeam_channel::unbounded();
        let (ctl_tx, ctl_requests) = crossbeam_channel::unbounded();
        let endpoint = CtlEndpoint::for_home(&paths);
        let waker = Waker::default();
        let mailbox = Mailbox { tx: messages_tx, waker: waker.clone() };
        let mut app = App {
            usage: Usage::new(&paths, clock.clone()),
            host: ModuleHost::new(modules()),
            events: EventBus::new(),
            actions: ActionRegistry::new(),
            notifier: AppNotifier::default(),
            endpoint,
            ctl_requests,
            safe_mode,
            waker,
            passes: Arc::new(Passes::default()),
            exit_code: 0,
            quitting: false,
            mode: "no windows".to_owned(),
            software_renderer: false,
            clock,
            start_guard,
            runtime: None,
            desktop: Desktop::none(),
            hotkeys: HotkeyManager::dry(),
            themes: ThemeStore::load(&paths, settings.raw_saved()),
            themes_ui: lum_ui::themes::ThemesUi::new(
                paths.data_file("picker.json"),
                paths.data_file("colours.json"),
            ),
            restyle: None,
            overlay: MainOverlay::default(),
            overlay_config: OverlayConfig::from_settings(&settings, safe_mode),
            open_asked: None,
            open_times: Vec::new(),
            last_repair: None,
            tray: None,
            keep_awake: None,
            mailbox: mailbox.clone(),
            messages,
            flush_at: None,
            shut_down: false,
            settings,
            paths,
        };
        app.with_modules(|host, cx, actions| host.start_enabled(cx, actions));
        app.start_services(mailbox, ctl_tx, options)?;
        let running = app.host.running();
        let running = if running.is_empty() { "no modules running".to_owned() } else { running.join(", ") };
        tracing::info!("ready: {running}");
        Ok(app)
    }

    fn start_services(
        &mut self,
        messages: Mailbox,
        ctl: Sender<CtlRequest>,
        options: &Options,
    ) -> anyhow::Result<()> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("luminescent-service")
            .enable_all()
            .build()
            .context("could not start the service threads")?;
        {
            let _inside = runtime.enter();
            match self.endpoint.listen() {
                Ok(listener) => {
                    let waker = self.waker.clone();
                    runtime.spawn(ipc::serve(listener, ctl, Arc::new(move || waker.wake())));
                }
                Err(e) => tracing::error!("could not open the ctl socket {}: {e}", self.endpoint.describe()),
            }
        }
        let quit = messages.clone();
        runtime.spawn(async move {
            lum_platform::signals::quit_requested().await;
            quit.send(AppMsg::Quit("asked to by the system"));
        });

        if let Some(secs) = options.idle_check {
            runtime.spawn(idle::check(Duration::from_secs(secs), self.passes.clone(), messages.clone()));
        }
        if !options.smoke && !options.bench {
            let started = messages.clone();
            runtime.spawn(async move {
                tokio::time::sleep(Duration::from_millis(1500)).await;
                started.send(AppMsg::Emit { name: "luminescent.started".into(), text: VERSION.into() });
            });
            if let Some(start_guard) = self.start_guard.clone() {
                runtime.spawn(async move {
                    // 20 s in without a crash: the start is fine
                    tokio::time::sleep(Duration::from_secs(20)).await;
                    start_guard.ok();
                });
            }
            // a test copy with its own LUMINESCENT_HOME makes no weekly backups, as in Python
            if self.settings.get_bool("general.auto_backup", true) && !self.paths.custom_home {
                runtime.spawn(weekly_backups(self.paths.clone(), self.clock.clone()));
            }
        }
        self.runtime = Some(runtime);
        Ok(())
    }

    /// Sets the desktop up for our windows (Hyprland's rules and the placer) and starts listening to it.
    /// Call it before the first window opens.
    pub fn start_desktop(&mut self) {
        self.desktop = Desktop::start(&[lum_ui::widgets::dim::COVER_PREFIX, lum_ui::overlay::COVER_PREFIX]);
        tracing::info!("windows placed by {}", self.desktop.describe());
        let waker = self.waker.clone();
        self.hotkeys = HotkeyManager::start(&self.desktop, Arc::new(move || waker.wake()));
        if let Some(runtime) = &self.runtime {
            let mailbox = self.mailbox.clone();
            self.desktop.listen(runtime.handle(), move |event| mailbox.send(AppMsg::Desktop(event)));
        }
        self.notifier.running = true;
        self.notifier.desktop = self.settings.get_bool("general.tray_messages", true);
        self.overlay_config = OverlayConfig::from_settings(&self.settings, self.safe_mode);
        self.register_hotkeys();
        self.register_theme_hotkeys();
        let mailbox = self.mailbox.clone();
        self.keep_awake =
            Some(KeepAwake::start(Arc::new(move |on, err| mailbox.send(AppMsg::KeepAwake(on, err)))));
        let mailbox = self.mailbox.clone();
        let sink: tray::Sink = Arc::new(move |event| mailbox.send(AppMsg::Tray(event)));
        match Tray::start(self.tooltip(), tray::sparkle_icons(self.accent()), self.tray_menu(), sink) {
            Ok(t) => self.tray = Some(t),
            // the palette, the hotkeys and `luminescent ctl` still work without a tray (R16)
            Err(e) => tracing::warn!("no tray icon: {e}"),
        }
    }

    /// The launcher's own hotkeys, with the Python labels.
    fn register_hotkeys(&mut self) {
        let mut binds: Vec<(String, String)> = [
            ("general.settings_hotkey", "settings"),
            ("ui.overlay_hotkey", "overlay"),
            ("ui.palette_hotkey", "command palette"),
            ("ui.theme_cycle_hotkey", "next theme"),
            ("ui.theme_prev_hotkey", "previous theme"),
            ("general.keep_awake_hotkey", "keep awake"),
        ]
        .into_iter()
        .map(|(key, label)| (key.to_owned(), label.to_owned()))
        .collect();
        for (page, _) in lum_ui::overlay::pages::NAV_SECTIONS {
            binds.push((lum_ui::overlay::pages::hotkey_setting(page), format!("overlay {page}")));
        }
        for (key, label) in binds {
            let combo = self.settings.get_str(&key).unwrap_or_default().to_owned();
            if let Err(e) =
                self.hotkeys.register("launcher", &combo, &label, &label, hotkeys::Options::default())
                && !combo.is_empty()
            {
                self.hotkey_refused(&label, &e);
            }
        }
    }

    /// The keys themes have of their own (theme editor → General).
    fn register_theme_hotkeys(&mut self) {
        self.hotkeys.unregister_all("themes");
        for (id, name, combo) in self.themes.hotkeys() {
            let label = format!("theme {name}");
            if let Err(e) = self.hotkeys.register("themes", &combo, &id, &label, hotkeys::Options::default())
            {
                self.notifier.notify(
                    APP_NAME,
                    &format!("The key for the theme {name} is not registered: {e}"),
                    4000,
                );
            }
        }
    }

    /// The tokens of the active theme, with Text size, Hover effects and Performance mode from the settings.
    pub fn tokens(&self) -> Tokens {
        let (palette, sections) = self.themes.active_look();
        Tokens::of_theme(&palette, &sections, &self.settings)
    }

    fn accent(&self) -> [u8; 3] {
        let c = self.themes.active().palette().get(lum_theme::Role::Primary);
        [c.r, c.g, c.b]
    }

    /// Makes the active theme's look the one on screen: new tokens for egui (the UI thread installs them before
    /// the next frame is drawn, so everything changes in one frame) and the tray icon in the new accent.
    pub fn restyle(&mut self) {
        self.restyle = Some(self.tokens());
        if let Some(tray) = &self.tray {
            tray.set_icons(tray::sparkle_icons(self.accent()));
        }
        self.waker.wake();
    }

    /// What the UI thread installs, once, after a theme or a look setting changed.
    pub fn take_restyle(&mut self) -> Option<Tokens> {
        self.restyle.take()
    }

    /// Switches to a theme. The restyling, the settings profile and the events follow from what the store says.
    pub fn apply_theme(&mut self, id: &str) {
        if self.themes.apply(id) {
            self.theme_events();
        }
    }

    fn theme_events(&mut self) {
        for event in self.themes.take_events() {
            match event {
                ThemeEvent::Applied(id) => self.theme_applied(&id),
                ThemeEvent::Changed => self.register_theme_hotkeys(),
            }
        }
    }

    fn theme_applied(&mut self, id: &str) {
        let Some(t) = self.themes.get(id) else { return };
        let (name, profile) = (t.name().to_owned(), t.profile().to_owned());
        tracing::info!("theme applied: {name}");
        self.emit("theme.changed", &name);
        // a theme picks its settings profile; none picked means the default profile
        let want = if profile.is_empty() { "default" } else { profile.as_str() };
        if self.settings.active_profile() != want {
            match self.settings.switch_profile(want, true) {
                Ok(_) => {
                    let shown = self.settings.profile_name(&self.settings.active_profile());
                    self.notifier.notify(APP_NAME, &format!("Settings profile: {shown}"), 2500);
                }
                Err(e) => tracing::warn!("could not switch to the settings profile {want}: {e}"),
            }
        }
        self.restyle();
    }

    /// The theme after the active one (`step` -1: before it) in its folder.
    pub fn next_theme(&mut self, step: i64) {
        let Some(t) = self.themes.next_theme(step) else { return };
        let (id, name) = (t.id().to_owned(), t.name().to_owned());
        self.apply_theme(&id);
        self.notifier.notify(APP_NAME, &format!("Theme: {name}"), 1500);
    }

    /// Back to the theme that was on before this one (press again to swap back).
    pub fn last_theme(&mut self) {
        let Some(t) = self.themes.previous().filter(|t| t.id() != self.themes.active_id()) else {
            self.notifier.notify(APP_NAME, "There is no theme to go back to yet.", 2500);
            return;
        };
        let (id, name) = (t.id().to_owned(), t.name().to_owned());
        self.apply_theme(&id);
        self.notifier.notify(APP_NAME, &format!("Theme: {name}"), 1500);
    }

    fn theme_key(&mut self, id: &str) {
        let Some(t) = self.themes.get(id).filter(|t| t.id() != self.themes.active_id()) else { return };
        let name = t.name().to_owned();
        self.apply_theme(id);
        self.notifier.notify(APP_NAME, &format!("Theme: {name}"), 1500);
    }

    fn hotkey_refused(&self, label: &str, error: &str) {
        let mut chars = label.chars();
        let label: String = chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default();
        self.notifier.notify(APP_NAME, &format!("{label} hotkey not registered: {error}"), 4000);
    }

    fn tooltip(&self) -> String {
        let mut lines = vec![APP_NAME.to_owned()];
        if self.keep_awake.as_ref().is_some_and(KeepAwake::is_on) {
            lines.push("Keep awake is on".to_owned());
        }
        tray::tooltip_text(&lines)
    }

    /// The tray menu, from Settings → General → Tray menu. Entries for parts that come in later phases are
    /// left out until then.
    pub(crate) fn tray_menu(&self) -> Vec<MenuItem> {
        let shown = self.settings.get_str_list("general.tray_items");
        let has = |item: &str| shown.iter().any(|s| s == item);
        let pretty = |key: &str| {
            hotkeys::pretty(self.settings.get_str(key).unwrap_or_default(), hotkeys::Os::current())
        };
        let safe = if self.safe_mode { "   (safe mode)" } else { "" };
        let mut menu = vec![MenuItem::Label(format!("{APP_NAME} {VERSION}{safe}"))];
        if has("open") {
            menu.push(MenuItem::action(
                "open",
                format!("✦  Open {APP_NAME}   ({})", pretty("ui.overlay_hotkey")),
            ));
        }
        if has("palette") {
            menu.push(MenuItem::action(
                "palette",
                format!("⌕  Command palette   ({})", pretty("ui.palette_hotkey")),
            ));
        }
        menu.push(MenuItem::Separator);
        if has("keepawake") {
            let combo = self.settings.get_str("general.keep_awake_hotkey").unwrap_or_default();
            let hint = if combo.is_empty() {
                String::new()
            } else {
                format!("   ({})", pretty("general.keep_awake_hotkey"))
            };
            let checked = self.keep_awake.as_ref().is_some_and(KeepAwake::is_on);
            menu.push(MenuItem::Check {
                id: "keepawake".into(),
                text: format!("☕  Keep awake{hint}"),
                checked,
            });
        }
        if has("reset") {
            menu.push(MenuItem::action("reset", "⌨  Reset stuck keys"));
        }
        if has("log") {
            menu.push(MenuItem::action("log", "📄  Open log file"));
        }
        if has("quit") || shown.is_empty() {
            menu.push(MenuItem::Separator);
            menu.push(MenuItem::action("quit", "⏻  Quit everything"));
        }
        menu
    }

    fn refresh_tray(&self) {
        if let Some(t) = &self.tray {
            t.set_menu(self.tray_menu());
            t.set_tooltip(self.tooltip());
        }
    }

    /// Opens the overlay on the page it starts on, or hides it when it is open (the overlay hotkey and a tray
    /// click toggle it).
    pub fn toggle_overlay(&mut self) {
        if self.overlay.is_open() {
            self.hide_overlay();
        } else {
            let page = self.overlay_config.start_page();
            self.open_overlay(&page);
        }
    }

    /// Shows the overlay on `page`, or switches to it when the overlay is up already.
    pub fn open_overlay(&mut self, page: &str) {
        if self.overlay.is_open() {
            self.overlay.show_page(page);
        } else {
            self.open_asked = Some(Instant::now());
            let screens = lum_ui::window::screens();
            let host = MainOverlay::host_for(&screens, self.overlay_config.on_cursor_screen);
            self.overlay.open(page, screens, host);
            let current = self.overlay.current().to_owned();
            self.emit("overlay.opened", &current);
        }
        self.waker.wake();
    }

    pub fn hide_overlay(&mut self) {
        self.overlay.hide();
        self.waker.wake();
    }

    /// The overlay finished going away: it remembers the page it was on.
    fn overlay_closed(&mut self) {
        self.emit("overlay.closed", "");
        let page = self.overlay.current().to_owned();
        if self.settings.get_str("ui.last_page") != Some(page.as_str()) {
            self.settings.set("ui.last_page", json!(page));
            if let Err(e) = self.settings.save_quiet() {
                tracing::warn!("could not save the page the overlay was on: {e}");
            }
            self.overlay_config.last_page = page;
        }
    }

    fn opened_in(&mut self, asked: Instant) {
        let ms = asked.elapsed().as_secs_f64() * 1000.0;
        tracing::info!("overlay open: first frame {ms:.1} ms after asking");
        self.open_times.push(ms);
        if self.open_times.len() > 50 {
            self.open_times.remove(0);
        }
    }

    /// A page failed while drawing: its state goes and is built again, at most every few seconds.
    fn repair_page(&mut self, key: &str) {
        if self.last_repair.is_some_and(|at| at.elapsed() < Duration::from_secs(3)) {
            return;
        }
        self.last_repair = Some(Instant::now());
        tracing::warn!("repairing the overlay (was on {key})");
        if key == "themes" {
            self.themes_ui.reset_page();
        }
        self.notifier.toast("Luminescent error", "The overlay was rebuilt to recover.", 6000);
    }

    fn tray_event(&mut self, event: TrayEvent) {
        match event {
            TrayEvent::Activated => self.toggle_overlay(),
            TrayEvent::Menu(id) => match id.as_str() {
                "open" => {
                    let page = self.overlay_config.start_page();
                    self.open_overlay(&page);
                }
                "palette" => tracing::info!("the command palette comes in phase 5"),
                "keepawake" => self.toggle_keep_awake(),
                "reset" => self.hotkeys.reset_keys(),
                "log" => {
                    if let Err(e) = lum_platform::runner::open_path(&self.paths.log_file().to_string_lossy())
                    {
                        tracing::warn!("could not open the log: {e}");
                    }
                }
                "quit" => {
                    tracing::info!("quitting: asked to from the tray");
                    self.quitting = true;
                }
                other => tracing::debug!("tray: {other}"),
            },
        }
    }

    fn toggle_keep_awake(&mut self) {
        if let Some(k) = &mut self.keep_awake {
            k.toggle("");
        }
    }

    pub fn with_modules<T>(
        &mut self,
        f: impl FnOnce(&mut ModuleHost<dyn UiModule>, &mut ModuleCx<'_>, &mut ActionRegistry) -> T,
    ) -> T {
        let mut cx = ModuleCx {
            settings: &mut self.settings,
            events: &self.events,
            notifier: &self.notifier,
            safe_mode: self.safe_mode,
        };
        f(&mut self.host, &mut cx, &mut self.actions)
    }

    /// The loop without windows: waits on the channels until something asks to quit.
    pub fn run_headless(&mut self) {
        while !self.quitting {
            let flush = match self.next_flush() {
                Some(at) => crossbeam_channel::at(at),
                None => crossbeam_channel::never(),
            };
            select! {
                recv(self.ctl_requests) -> request => match request {
                    Ok(request) => self.answer(request),
                    Err(_) => break,
                },
                recv(self.messages) -> message => match message {
                    Ok(message) => self.handle(message),
                    Err(_) => break,
                },
                recv(flush) -> _ => {}
            }
            self.flush_if_due();
        }
    }

    /// Answers every request and message waiting, without blocking, and flushes usage when it is due. The UI
    /// thread calls it once each time it wakes. Gives back when it wants to run again (the next usage flush).
    pub fn poll(&mut self) -> Option<Instant> {
        while !self.quitting {
            if let Ok(request) = self.ctl_requests.try_recv() {
                self.answer(request);
            } else if let Ok(message) = self.messages.try_recv() {
                self.handle(message);
            } else {
                break;
            }
        }
        for event in self.hotkeys.poll() {
            self.hotkey(event);
        }
        self.theme_events();
        self.flush_if_due();
        self.next_flush()
    }

    fn hotkey(&mut self, event: HotkeyEvent) {
        match event {
            HotkeyEvent::Fired { owner, id, label, combo } => {
                if combo != "esc" && !combo.ends_with("+esc") {
                    let what = if label.is_empty() { owner.clone() } else { format!("{owner}: {label}") };
                    self.usage.record("hotkeys", &combo, &what);
                }
                tracing::debug!("hotkey {combo}: {owner} {id}");
                if owner == "launcher" {
                    match id.as_str() {
                        "overlay" => self.toggle_overlay(),
                        "settings" => self.open_overlay("settings"),
                        "keep awake" => self.toggle_keep_awake(),
                        "command palette" => tracing::info!("the command palette comes in phase 5"),
                        "next theme" => self.next_theme(1),
                        "previous theme" => self.next_theme(-1),
                        other => {
                            if let Some(page) = other.strip_prefix("overlay ") {
                                self.open_overlay(page);
                            }
                        }
                    }
                } else if owner == "themes" {
                    self.theme_key(&id);
                }
            }
            HotkeyEvent::Refused { owner, combo, reason } => {
                tracing::warn!("{owner}: {combo} was refused: {reason}");
                if owner == "launcher" {
                    self.hotkey_refused(&combo, &reason);
                } else if owner == "themes" {
                    let name =
                        self.themes.hotkeys().into_iter().find(|(_, _, c)| hotkeys::normalize(c) == combo);
                    let name = name.map_or(combo.clone(), |(_, name, _)| name);
                    self.notifier.notify(
                        APP_NAME,
                        &format!("The key for the theme {name} is not registered: {reason}"),
                        4000,
                    );
                }
            }
            other => tracing::debug!("hotkey: {other:?}"),
        }
    }

    fn next_flush(&mut self) -> Option<Instant> {
        if self.usage.needs_flush() && self.flush_at.is_none() {
            self.flush_at = Some(Instant::now() + Duration::from_millis(usage::FLUSH_AFTER_MS));
        }
        self.flush_at
    }

    fn flush_if_due(&mut self) {
        if self.flush_at.is_some_and(|at| Instant::now() >= at) {
            self.flush_at = None;
            self.usage.flush();
        }
    }

    fn answer(&mut self, request: CtlRequest) {
        let (answer, quit) = self.handle_ctl(&request.line);
        let _ = request.reply.send(answer);
        if quit {
            tracing::info!("quitting: asked to with ctl quit");
            self.quitting = true;
        }
    }

    fn handle(&mut self, message: AppMsg) {
        match message {
            AppMsg::Quit(why) => {
                tracing::info!("quitting: {why}");
                self.quitting = true;
            }
            AppMsg::Emit { name, text } => self.emit(&name, &text),
            AppMsg::IdleReport(mut numbers) => {
                numbers.software_renderer = self.software_renderer;
                numbers.overlay_ms = self.open_times.last().copied();
                let text = numbers.report(&self.mode);
                println!("{text}");
                tracing::info!("{}", text.lines().next().unwrap_or_default());
                self.exit_code = u8::from(!numbers.ok());
                self.quitting = true;
            }
            AppMsg::IdleOverlay(open) => {
                // only with windows: without them nothing would draw it, so it could not go away again
                if self.notifier.running {
                    if open { self.open_overlay("themes") } else { self.hide_overlay() }
                }
            }
            AppMsg::Desktop(event) => {
                tracing::debug!("desktop: {event:?}");
                self.hotkeys.desktop_event(&event);
            }
            AppMsg::Tray(event) => self.tray_event(event),
            AppMsg::ThemeFile(job, path) => {
                let out = self.with_themes_ui(|ui, store, env| ui.file_chosen(&job, path, store, env));
                self.themes_out(out);
            }
            AppMsg::KeepAwake(on, error) => {
                if let Some(k) = &mut self.keep_awake {
                    k.reported(on);
                }
                if let Some(e) = error {
                    self.notifier.notify("Keep awake", &format!("Could not keep the PC awake: {e}"), 4000);
                }
                self.refresh_tray();
            }
        }
    }

    /// Draws the overlay, the Themes windows and every running module's windows for this frame. A module that
    /// panics while drawing counts a fault, and one that keeps failing is switched off for the session.
    pub fn show_modules(&mut self, ctx: &egui::Context) {
        if self.overlay.is_visible() {
            self.show_overlay(ctx);
        }
        if self.themes_ui.has_windows() {
            let out = self.with_themes_ui(|ui, store, env| ui.show_windows(ctx, store, env));
            self.themes_out(out);
        }
        let App { host, settings, .. } = self;
        let mut cx = lum_ui::UiCx::new(ctx, settings);
        let failed = host.each_running("drawing", |module| module.show(&mut cx));
        for key in failed {
            if let Some(text) = self.with_modules(|host, cx, actions| host.report_fault(key, cx, actions)) {
                self.notifier.toast("Luminescent error", &text, 6000);
            }
        }
    }

    fn show_overlay(&mut self, ctx: &egui::Context) {
        let now = ZonedDateTime::now();
        let data = lum_ui::overlay::ChromeData {
            time: now.strftime("%H:%M").to_string(),
            date: now.strftime("%A %d %B").to_string().to_uppercase().replace(" 0", " "),
            hour: now.hour().clamp(0, 23) as u8,
            ..Default::default()
        };
        let mut themes_out = lum_ui::themes::Out::default();
        let out = {
            let App { overlay, overlay_config, themes_ui, themes, settings, hotkeys, .. } = self;
            with_env(settings, hotkeys, |env| {
                let mut page = |ui: &mut egui::Ui, slot: &PageSlot<'_>| -> PageFrame {
                    match slot.key {
                        "themes" => {
                            let out = themes_ui.page_ui(ui, slot.on_desktop, themes, env);
                            themes_out.files.extend(out.files);
                            themes_out.notify.extend(out.notify);
                            PageFrame { busy: themes_ui.busy() }
                        }
                        _ if slot.extra_screen.is_some() => PageFrame::default(),
                        key => {
                            lum_ui::overlay::pages::placeholder(ui, key);
                            PageFrame::default()
                        }
                    }
                };
                overlay.show(ctx, overlay_config, &data, &mut page)
            })
        };
        self.themes_out(themes_out);
        if out.first_frame
            && let Some(asked) = self.open_asked.take()
        {
            self.opened_in(asked);
        }
        if let Some(key) = out.repair {
            self.repair_page(&key);
        }
        if let Some(list) = out.chrome {
            self.settings.set("ui.chrome_items", serde_json::Value::Array(list.clone()));
            if let Err(e) = self.settings.save_quiet() {
                tracing::warn!("could not save the overlay's layout: {e}");
            }
            self.overlay_config.chrome_items = list;
        }
        if out.closed {
            self.overlay_closed();
        } else if self.overlay.is_open() {
            // the clock changes once a minute; running timers in the chips every second
            let secs = if data.ticks_every_second() { 1 } else { 60 - now.second().clamp(0, 59) as u64 };
            ctx.request_repaint_after(Duration::from_secs(secs));
        }
    }

    fn with_themes_ui<R>(
        &mut self,
        f: impl FnOnce(&mut lum_ui::themes::ThemesUi, &mut ThemeStore, &lum_ui::themes::Env<'_>) -> R,
    ) -> R {
        let App { themes_ui, themes, settings, hotkeys, .. } = self;
        with_env(settings, hotkeys, |env| f(themes_ui, themes, env))
    }

    /// File dialogs the Themes window wants (they answer through the mailbox) and its desktop notices.
    fn themes_out(&mut self, out: lum_ui::themes::Out) {
        for ask in out.files {
            let mailbox = self.mailbox.clone();
            let job = ask.job;
            let done = move |path| mailbox.send(AppMsg::ThemeFile(job, path));
            if ask.save {
                lum_platform::dialogs::save_file(ask.request, done);
            } else {
                lum_platform::dialogs::open_file(ask.request, done);
            }
        }
        for (title, text) in out.notify {
            self.notifier.notify(&title, &text, 4000);
        }
        if let Some(suspended) = out.suspend_keys {
            self.hotkeys.set_suspended(suspended);
        }
    }

    pub fn emit(&mut self, name: &str, text: &str) {
        self.events.emit(name, text);
        let event = lum_core::events::Event { name: name.to_owned(), text: text.to_owned() };
        self.with_modules(|host, cx, actions| host.dispatch(&event, cx, actions));
    }

    /// Answers one ctl line. The flag says whether the app should quit.
    pub fn handle_ctl(&mut self, line: &str) -> (String, bool) {
        let command = match ctl::parse(line) {
            Ok(command) => command,
            Err(e) => return (format!("error: {e}"), false),
        };
        let answer = match command {
            CtlCommand::Ping => "pong".to_owned(),
            CtlCommand::Status => self.status(),
            CtlCommand::Overlay(page) => {
                tracing::info!("asked to open: {line}");
                let page = page.map(|p| p.to_lowercase()).unwrap_or_else(|| self.overlay_config.start_page());
                if lum_ui::overlay::pages::title(&page).is_none() {
                    return (format!("error: there is no page called {page}"), false);
                }
                self.open_overlay(&page);
                "ok".to_owned()
            }
            CtlCommand::Settings(_) => {
                tracing::info!("asked to open: {line}");
                self.open_overlay("settings");
                "ok".to_owned()
            }
            CtlCommand::Palette => {
                tracing::info!("asked to open: {line} (that comes in phase 5)");
                "ok: the palette comes in a later build".to_owned()
            }
            CtlCommand::Action(id) => self.run_action(&id),
            CtlCommand::Theme(which) => self.ctl_theme(&which),
            CtlCommand::Quit => return ("bye".to_owned(), true),
        };
        (answer, false)
    }

    fn ctl_theme(&mut self, which: &str) -> String {
        match which.to_lowercase().as_str() {
            "next" => self.next_theme(1),
            "previous" | "prev" => self.next_theme(-1),
            "back" => self.last_theme(),
            _ => {
                let found = self
                    .themes
                    .themes()
                    .iter()
                    .find(|t| t.id() == which || t.name().eq_ignore_ascii_case(which));
                let Some(id) = found.map(|t| t.id().to_owned()) else {
                    return format!("error: there is no theme called {which}");
                };
                self.apply_theme(&id);
            }
        }
        format!("ok: {}", self.themes.active().name())
    }

    fn status(&self) -> String {
        let errors: serde_json::Map<String, serde_json::Value> =
            self.host.errors().into_iter().map(|(k, e)| (k.to_owned(), e.into())).collect();
        json!({
            "version": VERSION,
            "pid": std::process::id(),
            "home": self.paths.home.display().to_string(),
            "safe_mode": self.safe_mode,
            "theme": self.themes.active().name(),
            "modules": self.host.running(),
            "module_errors": errors,
            "hotkey_latency_p99_ms": self.hotkeys.latency_p99_ms(),
            "overlay_open_ms": open_times(&self.open_times),
        })
        .to_string()
    }

    /// Runs `"<owner>:<action>"` (Dashboard Tools tab, command palette, ctl), with the Python wording.
    pub fn run_action(&mut self, id: &str) -> String {
        let (owner, _) = id.split_once(':').unwrap_or((id, ""));
        let title = self.host.title(owner).unwrap_or(owner);
        if self.actions.find(id).is_err() && self.host.keys().contains(&owner) && !self.host.is_running(owner)
        {
            return format!("error: {title} is turned off. Turn it on in Settings → {title}.");
        }
        match self.actions.run(id) {
            Ok(done) => {
                let label = done.action.label.clone();
                self.usage.record("tools", id, &format!("{title}: {label}"));
                self.usage.activity("tool", &label, None, None);
                "ok".to_owned()
            }
            Err(e) => format!("error: {e}"),
        }
    }

    pub fn shutdown(&mut self) {
        if std::mem::replace(&mut self.shut_down, true) {
            return;
        }
        tracing::info!("shutting down");
        self.usage.flush();
        self.themes.flush();
        self.with_modules(|host, cx, actions| host.stop_all(cx, actions));
        if let Some(start_guard) = &self.start_guard {
            start_guard.ok();
        }
        self.hotkeys.shutdown();
        if let Some(k) = &self.keep_awake {
            k.shutdown();
        }
        if let Some(t) = &self.tray {
            t.stop();
        }
        self.desktop.shutdown();
        // dropping the runtime drops the listener, which removes the socket file
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_timeout(Duration::from_secs(1));
        }
    }
}

/// What the Themes page and its windows need to know about the rest of the app.
fn with_env<R>(
    settings: &Settings,
    hotkeys: &HotkeyManager,
    f: impl FnOnce(&lum_ui::themes::Env<'_>) -> R,
) -> R {
    let profile_name = |id: &str| settings.profile_name(id);
    let key_owner =
        |combo: &str| hotkeys.owner_of(combo).filter(|owner| *owner != "themes").map(str::to_owned);
    let env = lum_ui::themes::Env {
        profile_name: &profile_name,
        profiles: settings
            .profiles()
            .into_iter()
            .filter(|p| p.id != "default")
            .map(|p| (p.id, p.name))
            .collect(),
        key_owner: &key_owner,
        toast_style: settings.get_str("general.toast_style").unwrap_or("glow").to_owned(),
        hyprland: lum_platform::desktop::places_windows_by_rule(),
        caelestia: false,
    };
    f(&env)
}

/// The overlay's open times for `ctl status`: the last one, the middle one and the slowest, in ms.
fn open_times(times: &[f64]) -> serde_json::Value {
    if times.is_empty() {
        return serde_json::Value::Null;
    }
    let mut sorted = times.to_vec();
    sorted.sort_by(f64::total_cmp);
    let round = |ms: f64| (ms * 10.0).round() / 10.0;
    json!({
        "count": times.len(),
        "last": round(times[times.len() - 1]),
        "median": round(sorted[sorted.len() / 2]),
        "max": round(sorted[sorted.len() - 1]),
    })
}

/// Once a week: zip the setup into data/backups and keep the newest five. Checked 20 s after the start and
/// every 6 hours after that.
async fn weekly_backups(paths: Paths, clock: Arc<dyn Clock>) {
    tokio::time::sleep(Duration::from_secs(20)).await;
    loop {
        let (paths, clock) = (paths.clone(), clock.clone());
        let made =
            tokio::task::spawn_blocking(move || backup::weekly(&paths, clock.now(), clock.timestamp())).await;
        match made {
            Ok(Err(e)) => tracing::error!("weekly backup failed: {e}"),
            Err(e) => tracing::error!("weekly backup failed: {e}"),
            Ok(Ok(_)) => {}
        }
        tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
    }
}
