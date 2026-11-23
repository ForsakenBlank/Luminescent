//! Starting, running and stopping the app: the Rust side of `Launcher.py`.
//!
//! The main thread owns the app state and answers requests; it becomes the UI thread in phase 2. Everything
//! that waits (the ctl socket, signals, timers, backups) runs on a small tokio runtime and sends messages here.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use crossbeam_channel::{Receiver, Sender, select};
use jiff::ZonedDateTime;
use lum_core::actions::ActionRegistry;
use lum_core::clock::{Clock, SystemClock};
use lum_core::events::EventBus;
use lum_core::guard::{self, StartGuard};
use lum_core::module::{LogNotifier, Module, ModuleCx, ModuleHost};
use lum_core::settings::Settings;
use lum_core::usage::{self, Usage};
use lum_core::{APP_NAME, Paths, VERSION, backup, logging};
use lum_platform::instance;
use lum_platform::ipc::{self, CtlEndpoint, CtlRequest};
use serde_json::json;
use tokio::runtime::Runtime;

use crate::ctl::{self, CtlCommand};
use crate::smoke;

pub struct Options {
    pub smoke: bool,
    pub bench: bool,
    pub safe_mode: bool,
}

/// What services tell the main thread.
#[derive(Debug)]
pub enum AppMsg {
    Quit(&'static str),
    Emit { name: String, text: String },
}

/// The modules, in the order `modules/__init__.py` lists them. Each phase adds the ones it ports.
fn modules() -> Vec<Box<dyn Module>> {
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
    } else {
        app.run_loop();
        0
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
    pub host: ModuleHost,
    pub endpoint: CtlEndpoint,
    pub ctl_requests: Receiver<CtlRequest>,
    pub safe_mode: bool,
    notifier: LogNotifier,
    clock: Arc<dyn Clock>,
    start_guard: Option<Arc<StartGuard>>,
    runtime: Option<Runtime>,
    messages: Receiver<AppMsg>,
    flush_at: Option<Instant>,
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
        let mut app = App {
            usage: Usage::new(&paths, clock.clone()),
            host: ModuleHost::new(modules()),
            events: EventBus::new(),
            actions: ActionRegistry::new(),
            notifier: LogNotifier,
            endpoint,
            ctl_requests,
            safe_mode,
            clock,
            start_guard,
            runtime: None,
            messages,
            flush_at: None,
            settings,
            paths,
        };
        app.with_modules(|host, cx, actions| host.start_enabled(cx, actions));
        app.start_services(messages_tx, ctl_tx, options)?;
        let running = app.host.running();
        let running = if running.is_empty() { "no modules running".to_owned() } else { running.join(", ") };
        tracing::info!("ready: {running}");
        Ok(app)
    }

    fn start_services(
        &mut self,
        messages: Sender<AppMsg>,
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
                    runtime.spawn(ipc::serve(listener, ctl));
                }
                Err(e) => tracing::error!("could not open the ctl socket {}: {e}", self.endpoint.describe()),
            }
        }
        let quit = messages.clone();
        runtime.spawn(async move {
            lum_platform::signals::quit_requested().await;
            let _ = quit.send(AppMsg::Quit("asked to by the system"));
        });

        if !options.smoke && !options.bench {
            let started = messages.clone();
            runtime.spawn(async move {
                tokio::time::sleep(Duration::from_millis(1500)).await;
                let _ =
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

    pub fn with_modules<T>(
        &mut self,
        f: impl FnOnce(&mut ModuleHost, &mut ModuleCx<'_>, &mut ActionRegistry) -> T,
    ) -> T {
        let mut cx = ModuleCx {
            settings: &mut self.settings,
            events: &self.events,
            notifier: &self.notifier,
            safe_mode: self.safe_mode,
        };
        f(&mut self.host, &mut cx, &mut self.actions)
    }

    fn run_loop(&mut self) {
        loop {
            if self.usage.needs_flush() && self.flush_at.is_none() {
                self.flush_at = Some(Instant::now() + Duration::from_millis(usage::FLUSH_AFTER_MS));
            }
            let flush = match self.flush_at {
                Some(at) => crossbeam_channel::at(at),
                None => crossbeam_channel::never(),
            };
            select! {
                recv(self.ctl_requests) -> request => {
                    let Ok(request) = request else { break };
                    let (answer, quit) = self.handle_ctl(&request.line);
                    let _ = request.reply.send(answer);
                    if quit {
                        tracing::info!("quitting: asked to with ctl quit");
                        break;
                    }
                }
                recv(self.messages) -> message => match message {
                    Ok(AppMsg::Quit(why)) => {
                        tracing::info!("quitting: {why}");
                        break;
                    }
                    Ok(AppMsg::Emit { name, text }) => self.emit(&name, &text),
                    Err(_) => break,
                },
                recv(flush) -> _ => {
                    self.flush_at = None;
                    self.usage.flush();
                }
            }
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
            CtlCommand::Overlay(_) | CtlCommand::Palette | CtlCommand::Settings(_) => {
                tracing::info!("asked to open: {line} (the overlay is not in this build yet)");
                "ok: the overlay is not in this build yet".to_owned()
            }
            CtlCommand::Action(id) => self.run_action(&id),
            CtlCommand::Quit => return ("bye".to_owned(), true),
        };
        (answer, false)
    }

    fn status(&self) -> String {
        let errors: serde_json::Map<String, serde_json::Value> =
            self.host.errors().into_iter().map(|(k, e)| (k.to_owned(), e.into())).collect();
        json!({
            "version": VERSION,
            "pid": std::process::id(),
            "home": self.paths.home.display().to_string(),
            "safe_mode": self.safe_mode,
            "modules": self.host.running(),
            "module_errors": errors,
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
        tracing::info!("shutting down");
        self.usage.flush();
        self.with_modules(|host, cx, actions| host.stop_all(cx, actions));
        if let Some(start_guard) = &self.start_guard {
            start_guard.ok();
        }
        // dropping the runtime drops the listener, which removes the socket file
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_timeout(Duration::from_secs(1));
        }
    }
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
