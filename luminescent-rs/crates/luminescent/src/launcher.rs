//! Starting, running and stopping the app: the Rust side of `Launcher.py`.
//!
//! The main thread owns the app state and answers requests; with a display it is eframe's UI thread (see
//! `shell.rs`), without one it waits on the channels itself. Everything that waits (the ctl socket, signals,
//! timers, backups) runs on a small tokio runtime, sends a message and wakes the main thread.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
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
use lum_ui::{Services, Tokens, UiModule};
use serde_json::json;
use tokio::runtime::Runtime;

mod dashboard;
mod settings;
mod tools;

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
    /// A few seconds after the start: get the overlay's textures and glyphs ready.
    Prewarm,
    Desktop(DesktopEvent),
    Tray(TrayEvent),
    /// Keep awake changed (or could not), with the error text.
    KeepAwake(bool, Option<String>),
    /// A file dialog the Themes window asked for was answered (`None` when it was cancelled).
    ThemeFile(lum_ui::themes::page::FileJob, Option<std::path::PathBuf>),
    /// A file dialog the Settings page asked for was answered.
    SettingsFile(lum_ui::settings::FileJob, Option<std::path::PathBuf>),
    /// A backup was written, looked at or restored.
    Backup(settings::BackupDone),
}

/// Notifications go to the log, and once windows run, to Luminescent's own cards or to the desktop, as Settings
/// → General says. Cards wait in `cards` until the UI thread draws its next frame.
#[derive(Debug, Default)]
struct AppNotifier {
    /// The app runs with windows, so there is a desktop to show them on.
    running: bool,
    /// Settings → General → Show notifications.
    shown: bool,
    /// Settings → General → Notification style is "Luminescent" (cards) rather than the desktop's.
    cards_style: bool,
    cards: Mutex<Vec<(String, String, u32)>>,
    waker: Waker,
}

impl AppNotifier {
    fn card(&self, title: &str, message: &str, ms: u32) {
        if let Ok(mut cards) = self.cards.lock() {
            cards.push((title.to_owned(), message.to_owned(), ms));
        }
        self.waker.wake();
    }

    fn take_cards(&self) -> Vec<(String, String, u32)> {
        self.cards.lock().map(|mut c| std::mem::take(&mut *c)).unwrap_or_default()
    }
}

impl Notifier for AppNotifier {
    fn notify(&self, title: &str, message: &str, ms: u32) {
        tracing::info!("notify: {title} - {message}");
        if !self.running || !self.shown {
            return;
        }
        if self.cards_style {
            self.card(title, message, ms);
        } else {
            lum_platform::notify::show(title, message, ms, None);
        }
    }

    /// Always shows (reminders, dose warnings): a desktop notification, so it stays in your notification
    /// history, and a card as well when that is the notification style.
    fn toast(&self, title: &str, message: &str, ms: u32) {
        tracing::info!("toast: {title} - {message}");
        if !self.running {
            return;
        }
        if self.cards_style {
            self.card(title, message, ms);
        }
        lum_platform::notify::show(title, message, ms, None);
    }
}

/// Wakes the UI thread from any thread. Until eframe has made its context there is nothing to wake: the first
/// frame drains whatever arrived before it.
#[derive(Clone, Default)]
pub struct Waker(Arc<OnceLock<egui::Context>>);

impl std::fmt::Debug for Waker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Waker")
    }
}

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
fn modules(services: &Services) -> Vec<Box<dyn UiModule>> {
    vec![
        Box::new(lum_ui::tools::snippets::SnippetsModule::new(services)),
        Box::new(lum_ui::tools::notes::NotesModule::new(services)),
        Box::new(lum_ui::tools::timer::TimerModule::new(services)),
        Box::new(lum_ui::tools::reminders::RemindersModule::new(services)),
        Box::new(lum_ui::tools::calculator::CalculatorModule::new(services)),
        Box::new(lum_ui::tools::colorpicker::ColorPickerModule::new(services)),
        Box::new(lum_ui::tools::textgrab::TextGrabModule::new(services)),
        Box::new(lum_ui::tools::ruler::RulerModule::new(services)),
        Box::new(lum_ui::tools::qr::QrModule::new(services)),
    ]
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
    if let Some(safe) = app.restart {
        tracing::info!("restarting{}", if safe { " in safe mode" } else { "" });
        if let Err(e) = instance::relaunch(safe) {
            eprintln!("Luminescent could not start again: {e}");
            return Ok(1);
        }
    }
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
    /// How the everyday tools reach the app: what they ask for, and a way to wake the UI.
    services: Services,
    /// When the tray's tooltip is made again, while a timer runs.
    tooltip_at: Option<Instant>,
    clock: Arc<dyn Clock>,
    start_guard: Option<Arc<StartGuard>>,
    runtime: Option<Runtime>,
    /// Where the window rules, the placer and desktop events come from. Nothing until windows start.
    pub desktop: Desktop,
    pub hotkeys: HotkeyManager,
    pub themes: ThemeStore,
    pub themes_ui: lum_ui::themes::ThemesUi,
    pub settings_ui: lum_ui::settings::SettingsUi,
    /// Which dashboard item uses which hotkey, as of the last time they were registered.
    dashboard_keys: std::collections::HashMap<String, String>,
    /// The card on screen shows what the Settings page holds, not what is saved (the notification test).
    toast_preview: bool,
    pub dashboard: lum_core::dashboard::DashboardStore,
    pub dashboard_ui: lum_ui::dashboard::DashboardUi,
    /// The Today, Usage and Calendar pages.
    overview: lum_ui::tools::overview::Overview,
    /// The processes items started and the runs that wait for their turn.
    dash: dashboard::Running,
    /// The dialog of an item that asks for its arguments each time it runs.
    ask_args: Option<(String, lum_ui::dialog::DialogWindow)>,
    /// Other programs' windows put on top of the overlay while it is up.
    floated: Vec<String>,
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
    pub palette: lum_ui::palette::Palette,
    pub cheatsheet: lum_ui::cheatsheet::CheatSheet,
    pub toasts: lum_ui::toast::Toasts,
    toast_look: lum_ui::toast::Look,
    pub alerts: lum_ui::alert::Alerts,
    quit_dialog: Option<lum_ui::dialog::DialogWindow>,
    prewarm_due: bool,
    /// Start again once this copy has shut down (`Some(true)`: in safe mode).
    pub restart: Option<bool>,
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
        let services = {
            let waker = waker.clone();
            Services::new(paths.data.clone(), move || waker.wake())
        };
        let mut app = App {
            usage: Usage::new(&paths, clock.clone()),
            host: ModuleHost::new(modules(&services)),
            events: EventBus::new(),
            actions: ActionRegistry::new(),
            notifier: AppNotifier { waker: waker.clone(), ..AppNotifier::default() },
            services,
            tooltip_at: None,
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
            settings_ui: lum_ui::settings::SettingsUi::new(
                paths.data_file("picker.json"),
                paths.data_file("colours.json"),
            ),
            toast_preview: false,
            dashboard_keys: std::collections::HashMap::new(),
            themes_ui: lum_ui::themes::ThemesUi::new(
                paths.data_file("picker.json"),
                paths.data_file("colours.json"),
            ),
            dashboard: lum_core::dashboard::DashboardStore::load(
                &paths.data_file("dashboard.json"),
                &[],
                settings.raw_saved(),
            ),
            dashboard_ui: lum_ui::dashboard::DashboardUi::default(),
            overview: lum_ui::tools::overview::Overview::new(),
            dash: dashboard::Running::default(),
            ask_args: None,
            floated: Vec::new(),
            restyle: None,
            overlay: MainOverlay::default(),
            overlay_config: OverlayConfig::from_settings(&settings, safe_mode),
            open_asked: None,
            open_times: Vec::new(),
            last_repair: None,
            palette: lum_ui::palette::Palette::new(paths.data_file("palette.json")),
            cheatsheet: lum_ui::cheatsheet::CheatSheet::default(),
            toasts: lum_ui::toast::Toasts::default(),
            toast_look: lum_ui::toast::Look::from_settings(&settings),
            alerts: lum_ui::alert::Alerts::default(),
            quit_dialog: None,
            prewarm_due: false,
            restart: None,
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
            let prewarm = messages.clone();
            runtime.spawn(async move {
                // the first open of the overlay without the wait, as Python's prewarm
                tokio::time::sleep(Duration::from_secs(3)).await;
                prewarm.send(AppMsg::Prewarm);
            });
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
        let mut covers = vec![
            lum_ui::widgets::dim::COVER_PREFIX,
            lum_ui::overlay::COVER_PREFIX,
            lum_ui::cheatsheet::COVER_PREFIX,
            lum_ui::dialog::COVER_PREFIX,
        ];
        covers.extend(lum_ui::tools::frozen::PREFIXES);
        self.desktop = Desktop::start(&covers);
        tracing::info!("windows placed by {}", self.desktop.describe());
        let waker = self.waker.clone();
        self.hotkeys = HotkeyManager::start(&self.desktop, Arc::new(move || waker.wake()));
        if let Some(runtime) = &self.runtime {
            let mailbox = self.mailbox.clone();
            self.desktop.listen(runtime.handle(), move |event| mailbox.send(AppMsg::Desktop(event)));
        }
        self.notifier.running = true;
        self.settings_changed();
        self.overlay_config = OverlayConfig::from_settings(&self.settings, self.safe_mode);
        self.register_hotkeys();
        self.register_theme_hotkeys();
        self.register_dashboard_hotkeys();
        self.register_module_hotkeys();
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

    /// What follows the settings (on start and after a profile switch): the overlay, the cards, notifications.
    fn settings_changed(&mut self) {
        self.overlay_config = OverlayConfig::from_settings(&self.settings, self.safe_mode);
        self.toast_look = lum_ui::toast::Look::from_settings(&self.settings);
        self.notifier.shown = self.settings.get_bool("general.tray_messages", true);
        self.notifier.cards_style =
            self.settings.get_str("general.notify_style").unwrap_or("luminescent") == "luminescent";
        self.settings_ui.mark_stale();
    }

    /// The launcher's own hotkeys, with the Python labels.
    fn register_hotkeys(&mut self) {
        let mut binds: Vec<(String, String)> = [
            ("general.force_quit_hotkey", "quit all"),
            ("general.settings_hotkey", "settings"),
            ("ui.overlay_hotkey", "overlay"),
            ("ui.palette_hotkey", "command palette"),
            ("ui.cheatsheet_hotkey", "cheat sheet"),
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
                    self.settings_changed();
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
        lines.extend(self.status_lines());
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
        if let Some(sub) = self.dashboard_tray_menu() {
            menu.push(sub);
        }
        if has("actions") {
            menu.extend(self.module_tray_items());
        }
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
        if has("settings") || shown.is_empty() {
            menu.push(MenuItem::action("settings", "⚙  Settings…"));
        }
        if has("log") {
            menu.push(MenuItem::action("log", "📄  Open log file"));
        }
        if self.safe_mode {
            menu.push(MenuItem::action("restart", "🛟  Safe mode: restart normally"));
        }
        if has("restart") {
            menu.push(MenuItem::action("restart", "🔄  Restart"));
        }
        if has("quit") || shown.is_empty() {
            menu.push(MenuItem::Separator);
            menu.push(MenuItem::action(
                "quit",
                format!("⏻  Quit everything  ({})", pretty("general.force_quit_hotkey")),
            ));
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

    /// Opens the command palette (with text typed on the overlay), or closes it when it is open and nothing
    /// was typed.
    pub fn open_palette(&mut self, text: &str) {
        if self.palette.is_open() && text.is_empty() {
            self.palette.close();
        } else {
            let screens = lum_ui::window::screens();
            let screen = lum_ui::window::cursor_screen(&screens).or((!screens.is_empty()).then_some(0));
            let entries = self.palette_entries();
            self.palette.open(entries, text, screen.map(|i| screens[i].rect));
        }
        self.waker.wake();
    }

    /// Everything the palette can find, in the order the Python app lists it. Dashboard items, notes, colours,
    /// calculator pins and automations join as their phases bring them.
    pub fn palette_entries(&self) -> Vec<lum_ui::palette::Entry> {
        use lum_ui::palette::Entry;
        let mut out = Vec::new();
        for (key, name, _) in &self.overlay_config.nav {
            out.push(Entry::new(format!("page:{key}"), format!("Open {name}"), APP_NAME, "page"));
        }
        for registered in self.actions.all() {
            if !self.host.is_running(&registered.owner) {
                continue;
            }
            let title = self.host.title(&registered.owner).unwrap_or(&registered.owner);
            out.push(Entry::new(
                format!("act:{}", registered.full_id()),
                &registered.action.label,
                title,
                "action",
            ));
        }
        out.extend(self.dashboard_palette_entries());
        out.extend(self.module_palette_entries());
        let active = self.themes.active_id();
        for t in self.themes.themes() {
            let hint = if t.id() == active { "active" } else { "theme" };
            out.push(Entry::new(format!("theme:{}", t.id()), format!("Theme: {}", t.name()), hint, "theme"));
        }
        for s in &self.settings.schema().sections {
            out.push(Entry::new(
                format!("settings:{}", s.key),
                format!("Settings › {}", s.title),
                "settings",
                "settings",
            ));
        }
        for label in LUMINESCENT_ENTRIES {
            out.push(Entry::new(format!("lum:{label}"), *label, APP_NAME, "luminescent"));
        }
        out
    }

    fn run_palette_entry(&mut self, entry: &lum_ui::palette::Entry) {
        if !entry.id.starts_with("calc:") && !entry.id.starts_with("remind:") {
            let group = entry.id.split(':').next().unwrap_or_default();
            self.usage.record("tools", &format!("palette:{group}"), "Command palette");
        }
        let (kind, rest) = entry.id.split_once(':').unwrap_or((entry.id.as_str(), ""));
        match kind {
            "page" => self.open_overlay(rest),
            "act" => {
                let answer = self.run_action(rest);
                if let Some(e) = answer.strip_prefix("error: ") {
                    self.notifier.notify(APP_NAME, e, 4000);
                }
            }
            "theme" => self.apply_theme(rest),
            "item" => self.run_dashboard_item(rest, None),
            "settings" => self.open_settings(rest),
            "lum" => self.run_luminescent(rest),
            "calc" | "remind" | "note" => self.run_module_row(&entry.id),
            other => tracing::warn!("the palette has no way to run {other} entries yet"),
        }
    }

    fn run_luminescent(&mut self, label: &str) {
        match label {
            "Next theme" => self.next_theme(1),
            "Previous theme" => self.next_theme(-1),
            "Back to the theme before (undo a theme switch)" => self.last_theme(),
            "Reset stuck keys" => self.reset_keys(),
            "Keep awake on / off" => self.toggle_keep_awake(),
            // from the palette there is no key to hold
            "Show the hotkey cheat sheet" => self.cheat_sheet("", true),
            "Open the log file" => self.open_log(),
            "Restart Luminescent" => self.restart_app(false),
            "Restart in safe mode (no modules, automations paused)" => self.restart_app(true),
            "Quit Luminescent…" => self.confirm_quit(),
            other => tracing::warn!("palette: {other} is not built yet"),
        }
    }

    /// The cheat sheet's hotkey was pressed (or the palette asked for it, `toggle` then).
    pub(crate) fn cheat_sheet(&mut self, combo: &str, toggle: bool) {
        if self.cheatsheet.is_open() {
            if self.cheatsheet.toggle_mode {
                self.cheatsheet.close();
                self.hotkeys.watch_release(None);
            }
            return;
        }
        let toggle = toggle || self.settings.get_bool("ui.cheatsheet_toggle", false) || combo.is_empty();
        let groups = self.cheat_sheet_groups();
        let screens = lum_ui::window::screens();
        self.cheatsheet.open(groups, screens, toggle);
        if !toggle {
            self.hotkeys.watch_release(Some(combo));
        }
        self.waker.wake();
    }

    /// Every hotkey in the settings, by section, in the order of Settings; switched-off modules are greyed.
    fn cheat_sheet_groups(&self) -> Vec<lum_ui::cheatsheet::Group> {
        let mut out = Vec::new();
        for s in &self.settings.schema().sections {
            let Some(values) = self.settings.section(&s.key) else { continue };
            let hold = s.key == "doodle"
                && values.get("mode").and_then(serde_json::Value::as_str).unwrap_or("hold") == "hold";
            let rows: Vec<(String, String)> = s
                .fields
                .iter()
                .filter(|f| f.kind == lum_core::settings::schema::FieldType::Hotkey)
                .filter_map(|f| {
                    let combo = values.get(&f.key)?.as_str().filter(|c| !c.is_empty())?;
                    let label =
                        if hold && f.label == "Draw hotkey" { "Hold to draw" } else { f.label.as_str() };
                    Some((label.to_owned(), combo.to_owned()))
                })
                .collect();
            if rows.is_empty() {
                continue;
            }
            let enabled = values.get("enabled").and_then(serde_json::Value::as_bool).unwrap_or(true);
            let on = enabled && (!self.host.keys().contains(&s.key.as_str()) || self.host.is_running(&s.key));
            out.push(lum_ui::cheatsheet::Group { name: s.title.clone(), on, rows });
        }
        out
    }

    fn reset_keys(&mut self) {
        self.hotkeys.reset_keys();
        self.notifier.notify(APP_NAME, "Keyboard state reset.", 2000);
    }

    fn open_log(&self) {
        if let Err(e) = lum_platform::runner::open_path(&self.paths.log_file().to_string_lossy()) {
            tracing::warn!("could not open the log: {e}");
        }
    }

    pub fn hide_overlay(&mut self) {
        self.overlay.hide();
        self.waker.wake();
    }

    /// The overlay finished going away: it remembers the page it was on.
    fn overlay_closed(&mut self) {
        self.emit("overlay.closed", "");
        self.dashboard_ui.hidden();
        if self.settings_ui.on_hide() {
            self.hotkeys.set_suspended(false);
        }
        self.unfloat_foreign();
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
        match key {
            "themes" => self.themes_ui.reset_page(),
            "settings" => {
                if self.settings_ui.on_hide() {
                    self.hotkeys.set_suspended(false);
                }
                self.settings_ui.reset_page();
            }
            "dashboard" => self.dashboard_repair(),
            "today" | "stats" | "calendar" => self.overview.reset(key),
            other => self.reset_module_page(other),
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
                "palette" => self.open_palette(""),
                "keepawake" => self.toggle_keep_awake(),
                "reset" => self.reset_keys(),
                "log" => self.open_log(),
                "settings" => self.open_settings(""),
                "restart" => self.restart_app(false),
                "quit" => self.confirm_quit(),
                "dash-edit" => self.open_overlay("dashboard"),
                other => {
                    if let Some(id) = other.strip_prefix("dash:") {
                        self.run_dashboard_item(id, None);
                    } else if let Some(action) = other.strip_prefix("act:") {
                        self.run_action(action);
                    } else if let Some(row) = other.strip_prefix("row:") {
                        self.run_module_row(row);
                    } else {
                        tracing::debug!("tray: {other}");
                    }
                }
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
        self.dashboard_tick();
        self.handle_requests();
        self.flush_if_due();
        let next = match (self.next_flush(), self.dash.deadline()) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        match (next, self.tooltip_deadline()) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
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
                        "quit all" => self.confirm_quit(),
                        "settings" => self.open_settings(""),
                        "keep awake" => self.toggle_keep_awake(),
                        "command palette" => self.open_palette(""),
                        "cheat sheet" => self.cheat_sheet(&combo, false),
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
                } else if owner == "dashboard" {
                    self.run_dashboard_item(&id, None);
                } else if self.host.keys().contains(&owner.as_str()) {
                    self.module_hotkey(&owner, &id);
                }
            }
            HotkeyEvent::Released { .. } => {
                self.cheatsheet.close();
                self.waker.wake();
            }
            HotkeyEvent::ReleaseUnknown { .. } => {
                self.cheatsheet.switch_to_toggle();
                self.waker.wake();
            }
            HotkeyEvent::Refused { owner, combo, reason } => {
                tracing::warn!("{owner}: {combo} was refused: {reason}");
                if owner == "launcher" {
                    self.hotkey_refused(&combo, &reason);
                } else if owner == "dashboard" {
                    self.notifier.notify(
                        "Dashboard hotkeys",
                        &format!("{}: {reason}", hotkeys::pretty(&combo, hotkeys::Os::current())),
                        4000,
                    );
                } else if let Some(title) = self.host.title(&owner) {
                    self.notifier.notify(
                        title,
                        &format!("{}: {reason}", hotkeys::pretty(&combo, hotkeys::Os::current())),
                        4000,
                    );
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

    pub(crate) fn handle(&mut self, message: AppMsg) {
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
            AppMsg::Prewarm => {
                self.prewarm_due = true;
                self.waker.wake();
            }
            AppMsg::IdleOverlay(open) => {
                // only with windows: without them nothing would draw it, so it could not go away again
                if self.notifier.running {
                    if open { self.open_overlay("themes") } else { self.hide_overlay() }
                }
            }
            AppMsg::Desktop(event) => {
                tracing::debug!("desktop: {event:?}");
                if let DesktopEvent::ForeignWindow { addr, .. } = &event {
                    self.foreign_opened(addr);
                }
                self.hotkeys.desktop_event(&event);
            }
            AppMsg::Tray(event) => self.tray_event(event),
            AppMsg::ThemeFile(job, path) => {
                let out = self.with_themes_ui(|ui, store, env| ui.file_chosen(&job, path, store, env));
                self.themes_out(out);
            }
            AppMsg::SettingsFile(job, path) => self.settings_file_chosen(&job, path),
            AppMsg::Backup(done) => self.backup_done(done),
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
            let drawn = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.show_overlay(ctx)));
            if drawn.is_err() {
                // something on the overlay broke: throw all of it away; the next frame builds it again
                tracing::error!("the overlay failed while drawing: building it again");
                self.overlay = MainOverlay::default();
                self.themes_ui.reset_page();
                self.dashboard_repair();
                self.notifier.toast("Luminescent error", "The overlay was rebuilt to recover.", 6000);
            }
        } else if std::mem::take(&mut self.prewarm_due) {
            let screens = lum_ui::window::screens();
            self.overlay.prewarm(ctx, &screens, &self.overlay_config);
        }
        if self.cheatsheet.is_open() && self.cheatsheet.show(ctx) {
            self.hotkeys.watch_release(None);
        }
        self.show_messages(ctx);
        if self.palette.is_open() {
            let host = &self.host;
            let live = |text: &str| host.running_refs().flat_map(|m| m.palette_live(text)).collect();
            if let Some(entry) = self.palette.show(ctx, &live) {
                self.run_palette_entry(&entry);
            }
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

    /// Cards, warnings and the quit question, wherever they come from.
    fn show_messages(&mut self, ctx: &egui::Context) {
        for (title, message, ms) in self.notifier.take_cards() {
            self.toasts.push(&title, &message, ms, &self.toast_look);
        }
        if !self.toasts.is_empty() {
            let screens = lum_ui::window::screens();
            let primary =
                screens.first().map_or_else(|| lum_ui::window::fallback_screen(ctx).rect, |s| s.work);
            let out = self.toasts.show(ctx, &self.toast_look, primary);
            if let Some(text) = out.copy {
                // the clipboard can wait on whoever owns it, so not on this thread
                std::thread::spawn(move || {
                    if let Err(e) = lum_platform::clipboard::set_text(&text) {
                        tracing::warn!("could not copy the notification: {e}");
                    }
                });
            }
        }
        self.restore_toast_look();
        if !self.alerts.is_empty() {
            for title in self.alerts.show(ctx) {
                self.emit("alert.closed", &title);
            }
        }
        self.show_ask_args(ctx);
        if let Some(dialog) = &mut self.quit_dialog {
            match dialog.show(ctx) {
                lum_ui::dialog::Outcome::Open => {}
                lum_ui::dialog::Outcome::Accepted(_) => {
                    tracing::info!("quitting: confirmed");
                    self.quit_dialog = None;
                    self.quitting = true;
                }
                lum_ui::dialog::Outcome::Cancelled => self.quit_dialog = None,
            }
        }
    }

    /// Cards on, as with windows and the default settings, so `--smoke` can draw them without a desktop.
    pub(crate) fn cards_on(&mut self) {
        self.notifier.running = true;
        self.notifier.shown = true;
        self.notifier.cards_style = true;
    }

    /// The big warning in the middle of the screen with the mouse (automation and performance alerts).
    pub fn alert(&mut self, title: &str, message: &str, level: &str, seconds: f64) {
        let screens = lum_ui::window::screens();
        let screen = lum_ui::window::cursor_screen(&screens).map(|i| screens[i].rect);
        self.alerts.show_alert(title, message, level, seconds, screen);
        self.waker.wake();
    }

    /// Tray → Quit everything, the "Quit all" hotkey and the palette ask first.
    pub fn confirm_quit(&mut self) {
        if self.quit_dialog.is_some() {
            return;
        }
        let dialog = lum_ui::dialog::AskDialog::confirm(
            egui::Id::unique("lum-quit"),
            &format!("Quit {APP_NAME}"),
            "Close Luminescent and every module?",
            "Quit",
            "Cancel",
            false,
        );
        let screens = lum_ui::window::screens();
        let screen = lum_ui::window::cursor_screen(&screens)
            .or((!screens.is_empty()).then_some(0))
            .map(|i| screens[i].clone());
        self.quit_dialog = Some(lum_ui::dialog::DialogWindow::new(dialog, screen));
        self.waker.wake();
    }

    /// Shuts down and starts again (in safe mode: no modules, automations paused).
    pub fn restart_app(&mut self, safe: bool) {
        tracing::info!("restarting{}", if safe { " in safe mode" } else { "" });
        self.restart = Some(safe);
        self.quitting = true;
        self.waker.wake();
    }

    fn show_overlay(&mut self, ctx: &egui::Context) {
        let now = ZonedDateTime::now();
        let data = lum_ui::overlay::ChromeData {
            time: now.strftime("%H:%M").to_string(),
            date: now.strftime("%A %d %B").to_string().to_uppercase().replace(" 0", " "),
            hour: now.hour().clamp(0, 23) as u8,
            status: self.host.running_refs().flat_map(|m| m.chips()).collect(),
            timers: self.status_lines().into_iter().take(6).map(|line| ("⏱".to_owned(), line)).collect(),
            ..Default::default()
        };
        let mut themes_out = lum_ui::themes::Out::default();
        let mut settings_result = lum_ui::settings::Out::default();
        let tools = self.tool_sources();
        let (snap, tidy_style, tidy_order, show_icons) = self.dashboard_env_values();
        let today = now.strftime("%Y-%m-%d").to_string();
        // a module's page needs the module for itself, so what Settings shows about modules is made first
        let infos: std::collections::HashMap<&str, lum_ui::settings::ModuleInfo> = self
            .host
            .keys()
            .into_iter()
            .filter_map(|key| Some((key, module_info(&self.host, &self.actions, key)?)))
            .collect();
        let theme_names: Vec<(String, String)> =
            self.themes.themes().iter().map(|t| (t.id().to_owned(), t.name().to_owned())).collect();
        let out = {
            let App {
                overlay,
                overlay_config,
                themes_ui,
                themes,
                settings,
                settings_ui,
                hotkeys,
                dashboard,
                dashboard_ui,
                dashboard_keys,
                dash,
                host,
                safe_mode,
                overview,
                usage,
                paths,
                ..
            } = self;
            EnvData::of(settings).with(hotkeys, |env| {
                let is_running = |id: &str| dash.is_running(id);
                let key_owner = |combo: &str| hotkeys.owner_of(combo).map(str::to_owned);
                let key_user = |combo: &str| other_key_user(dashboard_keys, hotkeys, combo);
                let key_list = || dashboard_key_list(dashboard_keys);
                let module = |key: &str| infos.get(key).cloned();
                let theme_list = || theme_names.clone();
                let settings_env = lum_ui::settings::Env {
                    key_user: &key_user,
                    dashboard_keys: &key_list,
                    module: &module,
                    themes: &theme_list,
                    version: VERSION,
                    today: &today,
                };
                let dashboard_env = lum_ui::dashboard::Env {
                    snap,
                    tidy_style: &tidy_style,
                    tidy_order: &tidy_order,
                    show_icons,
                    safe_mode: *safe_mode,
                    is_running: &is_running,
                    key_owner: &key_owner,
                    tools: &tools,
                };
                let mut page = |ui: &mut egui::Ui, slot: &PageSlot<'_>| -> PageFrame {
                    let mut sub = |ui: &mut egui::Ui, key: &str, rect: egui::Rect| -> PageFrame {
                        match key {
                            "themes" => {
                                let out = themes_ui.page_ui(ui, rect, themes, env);
                                themes_out.files.extend(out.files);
                                themes_out.notify.extend(out.notify);
                                PageFrame { busy: themes_ui.busy() }
                            }
                            "settings" => {
                                let page_out = settings_ui.page_ui(ui, rect, settings, &settings_env);
                                settings_result.files.extend(page_out.files);
                                settings_result.notify.extend(page_out.notify);
                                settings_result.run.extend(page_out.run);
                                settings_result.turn_on.extend(page_out.turn_on);
                                settings_result.suspend_keys =
                                    page_out.suspend_keys.or(settings_result.suspend_keys);
                                settings_result.saved = page_out.saved.or(settings_result.saved.take());
                                settings_result.quit |= page_out.quit;
                                settings_result.restart |= page_out.restart;
                                settings_result.toast_test =
                                    page_out.toast_test.or(settings_result.toast_test.take());
                                settings_result.export_to =
                                    page_out.export_to.or(settings_result.export_to.take());
                                settings_result.check_import =
                                    page_out.check_import.or(settings_result.check_import.take());
                                settings_result.restore = page_out.restore.or(settings_result.restore.take());
                                settings_result.preview_reminder =
                                    page_out.preview_reminder.or(settings_result.preview_reminder.take());
                                PageFrame { busy: settings_ui.busy() }
                            }
                            other if lum_ui::tools::overview::Overview::handles(other) => {
                                let reminders = host.is_running("reminders").then(|| {
                                    settings
                                        .get("reminders.items")
                                        .and_then(|v| v.as_array())
                                        .cloned()
                                        .unwrap_or_default()
                                });
                                let mut page_env = lum_ui::tools::overview::Env {
                                    usage,
                                    today: now.date(),
                                    now: now.datetime(),
                                    reminders: reminders.as_deref(),
                                    atlas_on: settings.get_bool("atlas_sync.enabled", false),
                                    atlas_file: paths.data_file("atlas_sync.json"),
                                };
                                overview.show(other, ui, rect, &mut page_env)
                            }
                            other => {
                                let owner = host
                                    .running_refs()
                                    .find(|m| m.page_key() == Some(other))
                                    .map(|m| m.key());
                                match owner.and_then(|key| host.running_module(key)) {
                                    Some(module) => {
                                        let mut cx = lum_ui::UiCx::new(ctx, settings);
                                        module.page(ui, rect, &mut cx)
                                    }
                                    None => {
                                        lum_ui::overlay::pages::placeholder(ui, other);
                                        PageFrame::default()
                                    }
                                }
                            }
                        }
                    };
                    match slot.key {
                        "dashboard" => dashboard_ui.show(ui, slot, dashboard, &dashboard_env, &mut sub),
                        _ if slot.extra_screen.is_some() => PageFrame::default(),
                        key => sub(ui, key, slot.on_desktop),
                    }
                };
                overlay.show(ctx, overlay_config, &data, &mut page)
            })
        };
        let dashboard_out = self.dashboard_ui.take_out();
        self.dashboard_out(dashboard_out);
        self.settings_out(settings_result);
        self.themes_out(themes_out);
        if let Some(section) = self.overview.take_out().open_settings {
            self.open_settings(&section);
        }
        if out.first_frame
            && let Some(asked) = self.open_asked.take()
        {
            self.opened_in(asked);
        }
        if let Some(text) = out.palette {
            self.open_palette(&text);
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
            CtlCommand::Settings(section) => {
                tracing::info!("asked to open: {line}");
                self.open_settings(section.as_deref().unwrap_or_default());
                "ok".to_owned()
            }
            CtlCommand::Palette => {
                tracing::info!("asked to open: {line}");
                self.open_palette("");
                "ok".to_owned()
            }
            CtlCommand::Action(id) => self.run_action(&id),
            CtlCommand::Theme(which) => self.ctl_theme(&which),
            CtlCommand::Notify(title, text) => {
                self.notifier.notify(&title, &text, 4000);
                "ok".to_owned()
            }
            CtlCommand::Alert(title, text) => {
                self.alert(&title, &text, "warning", 0.0);
                "ok".to_owned()
            }
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
        self.dashboard_flush();
        self.palette.flush();
        self.toasts.clear();
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

/// The palette's own entries (`lum:` ids), with the Python labels. Check for updates and Update now come with
/// the updater in phase 15.
const LUMINESCENT_ENTRIES: &[&str] = &[
    "Next theme",
    "Previous theme",
    "Back to the theme before (undo a theme switch)",
    "Reset stuck keys",
    "Keep awake on / off",
    "Show the hotkey cheat sheet",
    "Open the log file",
    "Restart Luminescent",
    "Restart in safe mode (no modules, automations paused)",
    "Quit Luminescent…",
];

/// What the Themes page and its windows need to know about the rest of the app.
/// What the Themes and Settings windows read about the settings, taken in one go so the page can then hold
/// the settings mutably.
struct EnvData {
    profiles: Vec<lum_core::settings::Profile>,
    toast_style: String,
    hyprland: bool,
}

impl EnvData {
    fn of(settings: &Settings) -> EnvData {
        EnvData {
            profiles: settings.profiles(),
            toast_style: settings.get_str("general.toast_style").unwrap_or("glow").to_owned(),
            hyprland: lum_platform::desktop::places_windows_by_rule(),
        }
    }

    fn with<R>(&self, hotkeys: &HotkeyManager, f: impl FnOnce(&lum_ui::themes::Env<'_>) -> R) -> R {
        // the same answer as Settings::profile_name
        let profile_name = |id: &str| match self.profiles.iter().find(|p| p.id == id) {
            Some(p) if !p.name.is_empty() => p.name.clone(),
            Some(p) => p.id.clone(),
            None => "Default".to_owned(),
        };
        let key_owner =
            |combo: &str| hotkeys.owner_of(combo).filter(|owner| *owner != "themes").map(str::to_owned);
        let env = lum_ui::themes::Env {
            profile_name: &profile_name,
            profiles: self
                .profiles
                .iter()
                .filter(|p| p.id != "default")
                .map(|p| (p.id.clone(), p.name.clone()))
                .collect(),
            key_owner: &key_owner,
            toast_style: self.toast_style.clone(),
            hyprland: self.hyprland,
            caelestia: false,
        };
        f(&env)
    }
}

fn with_env<R>(
    settings: &Settings,
    hotkeys: &HotkeyManager,
    f: impl FnOnce(&lum_ui::themes::Env<'_>) -> R,
) -> R {
    EnvData::of(settings).with(hotkeys, f)
}

/// What other than the Settings page's own fields uses a hotkey: a dashboard item or a theme's key.
fn other_key_user(
    dashboard_keys: &std::collections::HashMap<String, String>,
    hotkeys: &HotkeyManager,
    combo: &str,
) -> Option<String> {
    let normal = hotkeys::keys::normalize(combo);
    if let Some(what) = dashboard_keys.get(&normal) {
        return Some(what.clone());
    }
    (hotkeys.owner_of(&normal) == Some("themes")).then(|| "a theme's own key (Themes)".to_owned())
}

fn dashboard_key_list(dashboard_keys: &std::collections::HashMap<String, String>) -> Vec<(String, String)> {
    let mut keys: Vec<(String, String)> =
        dashboard_keys.iter().map(|(c, w)| (c.clone(), w.clone())).collect();
    keys.sort();
    keys
}

/// A module's section on the Settings page: its buttons while it runs, "Turn it on" while it does not.
fn module_info(
    host: &ModuleHost<dyn UiModule>,
    actions: &ActionRegistry,
    key: &str,
) -> Option<lum_ui::settings::ModuleInfo> {
    if !host.keys().contains(&key) {
        return None;
    }
    Some(lum_ui::settings::ModuleInfo {
        title: host.title(key).unwrap_or(key).to_owned(),
        running: host.is_running(key),
        actions: actions
            .all()
            .iter()
            .filter(|r| r.owner == key)
            .map(|r| (r.full_id(), r.action.label.clone()))
            .collect(),
        error: host.errors().into_iter().find(|(k, _)| *k == key).map(|(_, e)| e),
    })
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
