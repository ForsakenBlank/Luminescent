//! The automation engine inside the app: started with `data/automation.json`, Settings → Automation as its
//! `Config`, the event bus for the event triggers, and a `Host` that hands what nodes ask for to the main thread.
//! What waits on the system (speech, volume, power, the clipboard) goes to a short thread of its own, which
//! answers the node through the engine's handle.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lum_automation::{Answer, Automation, Config, Engine, Handle, Host, Level, Out, Reply, SystemClock};
use lum_core::module::Notifier;
use lum_platform::{clipboard, control, hotkeys, runner, sounds};
use lum_ui::Request;
use lum_ui::dialog::{AskDialog, DialogWindow, Icon, Outcome};
use serde_json::json;

use super::{App, AppMsg, Mailbox};

/// How long a "the internet is there" answer is trusted (the trigger's help says every 10 seconds).
const ONLINE_EVERY: Duration = Duration::from_secs(10);

/// The engine's way into the app and the desktop. `send` only posts to the main thread; the readings run on the
/// engine's worker threads, never the UI thread.
pub(super) struct AppHost {
    mailbox: Mailbox,
    idle: Mutex<lum_platform::system::IdleClock>,
    cpu: Mutex<lum_perf::levels::CpuMeter>,
    online: Mutex<(Option<bool>, Option<Instant>)>,
    keep_awake: Arc<AtomicBool>,
    /// Settings → Dashboard & Scripts → Terminal, for commands shown in a terminal.
    terminal: Arc<Mutex<String>>,
}

impl Host for AppHost {
    fn send(&self, out: Out) {
        self.mailbox.send(AppMsg::Automation(out));
    }

    fn clipboard_text(&self) -> String {
        clipboard::text().unwrap_or_default()
    }

    fn shell_command(&self, shell: &str, line: &str) -> std::process::Command {
        control::shell_command(shell, line)
    }

    fn run_in_terminal(
        &self,
        cwd: &Path,
        lines: &[String],
        shell: &str,
        stop_on_error: bool,
        marker: &Path,
    ) -> Result<std::path::PathBuf, String> {
        let terminal = self.terminal.lock().map(|t| t.clone()).unwrap_or_default();
        control::script_in_terminal(cwd, lines, shell, stop_on_error, marker, "", &terminal)
    }

    fn close_program(&self, name: &str, force: bool) -> bool {
        control::close_program(name, force)
    }

    fn process_names(&self) -> std::collections::BTreeSet<String> {
        lum_platform::system::process_names()
    }

    fn idle_seconds(&self) -> f64 {
        self.idle.lock().map(|mut c| c.seconds()).unwrap_or(0.0)
    }

    fn focused_window(&self) -> (String, String) {
        control::focused_window()
    }

    fn online(&self) -> Option<bool> {
        let mut known = self.online.lock().ok()?;
        if known.1.is_none_or(|at| at.elapsed() >= ONLINE_EVERY) {
            *known = (Some(control::online()), Some(Instant::now()));
        }
        known.0
    }

    fn level(&self, metric: &str) -> Level {
        let read = match metric {
            "cpu" => self.cpu.lock().ok().and_then(|mut m| m.percent()),
            "ram" => lum_perf::levels::ram_percent(),
            "battery" => lum_perf::levels::battery_percent(),
            // temperatures and GPU load come with the sampler (phase 11)
            _ => return Level::NotYet,
        };
        read.map_or(Level::Missing, Level::Value)
    }

    fn keep_awake_on(&self) -> bool {
        self.keep_awake.load(Ordering::Relaxed)
    }

    fn volume(&self) -> Option<(f64, bool)> {
        control::volume()
    }
}

/// A box an automation put up, and where its answer goes.
pub(super) struct AutoDialog {
    reply: Reply,
    kind: DialogKind,
    window: DialogWindow,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DialogKind {
    Message,
    Ask,
    Confirm,
}

/// Settings → Automation, as the engine reads it. Safe mode pauses every automation.
pub(super) fn config(settings: &lum_core::settings::Settings, safe_mode: bool) -> Config {
    Config {
        enabled: settings.get_bool("automation.enabled", true) && !safe_mode,
        max_rate: settings.get_f64("automation.max_rate").unwrap_or(200.0).max(1.0) as u32,
        shell: settings.get_str("automation.shell").unwrap_or(control::default_shell()).to_owned(),
        command_timeout: settings.get_f64("automation.command_timeout").unwrap_or(0.0).max(0.0) as u64,
    }
}

impl App {
    /// Starts the engine on its thread. With no workspace on, it sleeps until something changes.
    pub(super) fn start_automation(&mut self, mailbox: Mailbox) {
        let path = self.paths.data_file("automation.json");
        let saved = lum_core::store::load_json(&path);
        *self.auto_terminal.lock().unwrap_or_else(|e| e.into_inner()) =
            self.settings.get_str("dashboard.terminal").unwrap_or_default().to_owned();
        let host = Arc::new(AppHost {
            mailbox,
            idle: Mutex::new(lum_platform::system::IdleClock::new()),
            cpu: Mutex::new(lum_perf::levels::CpuMeter::new()),
            online: Mutex::new((None, None)),
            keep_awake: self.auto_keep_awake.clone(),
            terminal: self.auto_terminal.clone(),
        });
        let config = config(&self.settings, self.safe_mode);
        let engine = Engine::new(saved, Arc::new(SystemClock::default()), host, config);
        let waker = self.waker.clone();
        let events = self.events.subscribe();
        let auto = Automation::start_with_events(engine, path, move || waker.wake(), events);
        self.automation_ui.set_engine(auto.handle());
        self.automation = Some(auto);
    }

    /// The lists the page's choice fields show that only the app knows, by the Python function's name.
    pub(super) fn automation_choices(
        &self,
    ) -> std::collections::HashMap<&'static str, Vec<(String, String)>> {
        let mut out = std::collections::HashMap::new();
        let actions = self
            .actions
            .all()
            .iter()
            .map(|r| {
                let title = self.host.title(&r.owner).unwrap_or(&r.owner);
                (r.full_id(), format!("{title}: {}", r.action.label))
            })
            .collect();
        out.insert("_lum_actions", actions);
        out.insert(
            "_pages",
            lum_ui::overlay::pages::all_pages().map(|(k, t)| ((*k).to_owned(), (*t).to_owned())).collect(),
        );
        let several = self.dashboard.board_count() > 1;
        let mut items = Vec::new();
        for board in self.dashboard.boards() {
            for tab in board.tabs() {
                for item in tab.items().filter(|i| matches!(i.kind(), "item" | "group")) {
                    let head = if several { format!("{} › ", board.name()) } else { String::new() };
                    items.push((item.id().to_owned(), format!("{head}{} › {}", tab.title(), item.name())));
                }
            }
        }
        out.insert("_items", items);
        out.insert(
            "_boards",
            self.dashboard.boards().map(|b| (b.id().to_owned(), b.name().to_owned())).collect(),
        );
        let mut themes = vec![
            ("next".to_owned(), "The next theme".to_owned()),
            ("previous".to_owned(), "The previous theme".to_owned()),
        ];
        themes.extend(self.themes.themes().iter().map(|t| (t.id().to_owned(), t.name().to_owned())));
        out.insert("_themes", themes);
        out.insert("_notes", self.host.running_refs().flat_map(|m| m.note_choices()).collect());
        out
    }

    /// What the Automation page asked for after a frame.
    pub(super) fn automation_page_out(&mut self) {
        if self.overlay.current() != "automation" {
            self.automation_ui.hidden();
        }
        let out = self.automation_ui.take_out();
        if let Some(on) = out.suspend_keys {
            self.hotkeys.set_suspended(on);
        }
        if out.resume {
            self.toggle_automation_pause();
        }
        if let Some((name, file)) = out.export {
            let request = lum_platform::dialogs::Request::new("Export workspace")
                .folder(std::env::home_dir())
                .file_name(&format!("{name}.lumflow"))
                .filter("Luminescent automation", &["lumflow"])
                .filter("JSON", &["json"]);
            let mailbox = self.mailbox.clone();
            lum_platform::dialogs::save_file(request, move |path| {
                let Some(path) = path else { return };
                // on the dialog's own thread: the file is small, but the UI never waits on the disk
                let text = lum_core::pyjson::to_string(&file, lum_core::pyjson::Style::indented(2));
                if let Err(e) = lum_core::store::save_text(&path, &text) {
                    let text = e.to_string();
                    mailbox.send(AppMsg::Automation(Out::Notify { title: "Export".into(), text, ms: 6000 }));
                }
            });
        }
        if out.import {
            let request = lum_platform::dialogs::Request::new("Import workspace")
                .folder(std::env::home_dir())
                .filter("Luminescent automation", &["lumflow", "json"])
                .filter("All files", &["*"]);
            let mailbox = self.mailbox.clone();
            lum_platform::dialogs::open_file(request, move |path| {
                let Some(path) = path else { return };
                let read = std::fs::read_to_string(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|text| serde_json::from_str(&text).map_err(|e| e.to_string()));
                mailbox.send(AppMsg::AutomationImport(read));
            });
        }
        if let Some(browse) = out.browse {
            let mailbox = self.mailbox.clone();
            let start = std::path::PathBuf::from(runner::expand(&browse.start));
            let folder =
                if start.is_dir() { Some(start) } else { start.parent().map(std::path::Path::to_path_buf) };
            let request = lum_platform::dialogs::Request::new(&browse.title).folder(folder);
            let pick_folder = browse.folder;
            let done = move |path| mailbox.send(AppMsg::AutomationFile(browse, path));
            if pick_folder {
                lum_platform::dialogs::pick_folder(request, done);
            } else {
                lum_platform::dialogs::open_file(request, done);
            }
        }
    }

    pub fn automation(&self) -> Option<&Automation> {
        self.automation.as_ref()
    }

    /// Settings were saved: the engine follows (pause, the safety stop, the shell) and the keys are registered
    /// again.
    pub(super) fn automation_settings(&mut self) {
        *self.auto_terminal.lock().unwrap_or_else(|e| e.into_inner()) =
            self.settings.get_str("dashboard.terminal").unwrap_or_default().to_owned();
        let config = config(&self.settings, self.safe_mode);
        if let Some(auto) = &self.automation {
            auto.with(|e| {
                if *e.config() != config {
                    e.set_config(config);
                }
            });
        }
        self.register_automation_hotkeys();
    }

    /// Pause all automations / Resume automations (palette, tray, the pause hotkey).
    pub(super) fn toggle_automation_pause(&mut self) {
        let paused = !self.settings.get_bool("automation.enabled", true);
        self.settings.set("automation.enabled", json!(paused));
        if let Err(e) = self.settings.save_quiet() {
            tracing::warn!("could not store the automation pause: {e}");
        }
        self.settings_ui.mark_stale();
        self.automation_settings();
        let text = if paused { "Automations running again." } else { "Automations paused." };
        self.notifier.notify("Automation", text, 2000);
        self.refresh_tray();
    }

    /// The pause hotkey and every running Hotkey trigger's key.
    pub(super) fn register_automation_hotkeys(&mut self) {
        self.hotkeys.unregister_all("automation");
        let combo = self.settings.get_str("automation.pause_hotkey").unwrap_or_default().to_owned();
        if !combo.is_empty()
            && let Err(e) = self.hotkeys.register(
                "automation",
                &combo,
                "pause",
                "pause automations",
                hotkeys::Options::default(),
            )
        {
            self.notifier.notify("Automation", &format!("Pause hotkey not registered: {e}"), 4000);
        }
        let Some(auto) = &self.automation else { return };
        let mut problems = Vec::new();
        for n in auto.read(|e| e.hotkey_nodes()) {
            let id = format!("{}/{}", n.workspace, n.node);
            let label = format!("automation {}", n.label);
            let status =
                match self.hotkeys.register("automation", &n.combo, &id, &label, hotkeys::Options::default())
                {
                    Ok(()) => String::new(),
                    Err(e) => {
                        problems.push(format!("{}: {e}", n.label));
                        format!("⚠ {e}")
                    }
                };
            auto.with(|e| e.set_status(&n.workspace, &n.node, &status));
        }
        if !problems.is_empty() {
            self.notifier.notify("Automation hotkeys", &problems.join("\n"), 4000);
        }
    }

    pub(super) fn automation_hotkey(&mut self, id: &str) {
        if id == "pause" {
            self.toggle_automation_pause();
        } else if let (Some((wid, nid)), Some(auto)) = (id.split_once('/'), &self.automation) {
            auto.with(|e| e.hotkey_pressed(wid, nid));
        }
    }

    /// Hyprland refused a key a moment after it was registered.
    pub(super) fn automation_key_refused(&mut self, combo: &str, reason: &str) {
        let pretty = hotkeys::pretty(combo, hotkeys::Os::current());
        self.notifier.notify("Automation hotkeys", &format!("{pretty}: {reason}"), 4000);
        let Some(auto) = &self.automation else { return };
        let normal = hotkeys::normalize(combo);
        for n in auto.read(|e| e.hotkey_nodes()) {
            if hotkeys::normalize(&n.combo) == normal {
                auto.with(|e| e.set_status(&n.workspace, &n.node, &format!("⚠ {reason}")));
            }
        }
    }

    fn answer_node(&self, reply: &Reply, answer: Answer) {
        if let Some(auto) = &self.automation {
            auto.handle().answer(reply, answer);
        }
    }

    /// Something for a worker thread that answers the node when it is done.
    fn answer_later(&self, reply: Reply, work: impl FnOnce() -> Answer + Send + 'static) {
        let Some(handle) = self.automation.as_ref().map(Automation::handle) else { return };
        let spawned = std::thread::Builder::new().name("automation-ask".into()).spawn(move || {
            let answer = work();
            Handle::answer(&handle, &reply, answer);
        });
        if let Err(e) = spawned {
            tracing::warn!("automation: could not start a thread: {e}");
        }
    }

    /// Does what a node asked for.
    pub(super) fn automation_out(&mut self, out: Out) {
        match out {
            Out::Notify { title, text, ms } => self.notifier.notify(&title, &text, ms),
            Out::Card { title, text, ms } => {
                tracing::info!("card: {title} - {text}");
                if self.notifier.running {
                    self.notifier.card(&title, &text, ms);
                }
            }
            Out::Toast { title, text, ms } => self.notifier.toast(&title, &text, ms),
            Out::Alert { title, text, level, seconds, sound, reply } => {
                if !sound.is_empty() && sound != "none" {
                    sounds::play(&sound, "", 1.0);
                }
                self.alert(&title, &text, &level, seconds);
                if let Some(reply) = reply {
                    self.auto_alerts.push((title, reply));
                }
            }
            Out::Message { title, text, reply } => {
                let dialog = AskDialog::message(
                    egui::Id::unique(("auto-msg", &reply)),
                    &title,
                    &text,
                    Icon::Information,
                );
                self.auto_dialog(reply, DialogKind::Message, dialog);
            }
            Out::Ask { title, question, default, reply } => {
                let dialog =
                    AskDialog::ask_text(egui::Id::unique(("auto-ask", &reply)), &title, &question, &default);
                self.auto_dialog(reply, DialogKind::Ask, dialog);
            }
            Out::Confirm { title, text, yes, reply } => {
                let dialog = AskDialog::confirm(
                    egui::Id::unique(("auto-yes", &reply)),
                    &title,
                    &text,
                    &yes,
                    "Cancel",
                    true,
                );
                self.auto_dialog(reply, DialogKind::Confirm, dialog);
            }
            Out::RunAction(id) => {
                let answer = self.run_action(&id);
                if let Some(e) = answer.strip_prefix("error: ") {
                    tracing::warn!("automation: {id}: {e}");
                }
            }
            Out::Overlay { mode, page } => match mode.as_str() {
                "close" => self.hide_overlay(),
                "toggle" if self.overlay.is_open() => self.hide_overlay(),
                _ => self.open_overlay(&page),
            },
            Out::Theme(id) => match id.as_str() {
                "next" => self.next_theme(1),
                "previous" => self.next_theme(-1),
                _ => self.apply_theme(&id),
            },
            Out::RunItem(id) => self.run_dashboard_item(&id, None),
            Out::SwitchBoard(id) => {
                if self.dashboard.switch_board(&id) {
                    let name = self.dashboard.active_board().name().to_owned();
                    self.emit("dashboard.switched", &name);
                    self.waker.wake();
                }
            }
            Out::StartTimer { ms, label, reply } => {
                let mut started = false;
                self.host.each_running("starting a timer", |m| started |= m.start_timer(ms, &label));
                let answer = if started {
                    Answer::Done(label)
                } else {
                    Answer::Failed("the Timer module is off".into())
                };
                self.answer_node(&reply, answer);
            }
            Out::OpenNote { id, reply } => {
                let mut shown = false;
                self.host.each_running("opening a note", |m| shown |= m.show_note(&id));
                let answer =
                    if shown { Answer::Done(id) } else { Answer::Failed("the Notes module is off".into()) };
                self.answer_node(&reply, answer);
            }
            Out::Open { target, args, name, reply } => {
                let answer = match self.open_for_automation(&target, &args, &name) {
                    Ok(()) => Answer::Done(target),
                    Err(e) => Answer::Failed(e),
                };
                self.answer_node(&reply, answer);
            }
            Out::Emit { name, text } => self.emit(&name, &text),
            Out::Sound { kind, file } => sounds::play(&kind, &file, 1.0),
            Out::Speak { text, reply } => self.answer_later(reply, move || match control::say(&text) {
                Ok(()) => Answer::Done(String::new()),
                Err(e) => Answer::Failed(e),
            }),
            Out::TypeText(text) => {
                let services = self.services.clone();
                std::thread::spawn(move || match control::type_text(&text) {
                    Ok(true) => {}
                    // nothing here can type: paste it instead
                    Ok(false) => {
                        if let Err(e) = clipboard::set_text(&text) {
                            tracing::warn!("automation: could not copy the text to type: {e}");
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(120));
                        services.send(Request::SendKeys { combo: "ctrl+v".into(), release_modifiers: false });
                    }
                    Err(e) => tracing::warn!("automation: could not type: {e}"),
                });
            }
            Out::PressKeys(combo) => self.hotkeys.send(&combo),
            Out::SetClipboard(text) => {
                std::thread::spawn(move || {
                    if let Err(e) = clipboard::set_text(&text) {
                        tracing::warn!("automation: could not copy: {e}");
                    }
                });
            }
            Out::KeepAwake { on, why, reply } => {
                let answer = match &mut self.keep_awake {
                    Some(k) => {
                        let want = on.unwrap_or(!k.is_on());
                        k.set(want, &why);
                        Answer::Done(if want { "keeping awake" } else { "off" }.into())
                    }
                    None => Answer::Failed("keep awake needs the desktop".into()),
                };
                self.answer_node(&reply, answer);
            }
            Out::Volume { mode, amount, reply } => {
                self.answer_later(reply, move || match control::set_volume(&mode, amount) {
                    Err(e) => Answer::Failed(e),
                    Ok(()) => Answer::Done(match control::volume() {
                        Some((v, muted)) => format!("{v:.0} %{}", if muted { " (muted)" } else { "" }),
                        None => String::new(),
                    }),
                })
            }
            Out::Power { action, reply } => self.answer_later(reply, move || match control::power(&action) {
                Ok(()) => Answer::Done(String::new()),
                Err(e) => Answer::Failed(e),
            }),
            Out::HotkeysChanged => self.register_automation_hotkeys(),
            Out::WorkspacesChanged => self.refresh_tray(),
        }
    }

    /// Open a program, file or URL: started as a dashboard item would be, with the Dashboard's settings.
    fn open_for_automation(&self, target: &str, args: &str, name: &str) -> Result<(), String> {
        let options = runner::Options {
            use_terminal: self.settings.get_bool("dashboard.use_terminal", true),
            keep_open: self.settings.get_bool("dashboard.keep_open", true),
            python: self.settings.get_str("dashboard.python_path").unwrap_or_default().to_owned(),
            terminal: String::new(),
            preferred_terminal: self.settings.get_str("dashboard.terminal").unwrap_or_default().to_owned(),
            cwd: String::new(),
            admin: false,
        };
        runner::launch(target, args, name, &options, &self.paths.data_file("command_log.txt")).map(|_| ())
    }

    fn auto_dialog(&mut self, reply: Reply, kind: DialogKind, dialog: AskDialog) {
        if !self.notifier.running {
            // no windows to ask in: a message counts as read, a question as not answered
            let answer =
                if kind == DialogKind::Message { Answer::Done(String::new()) } else { Answer::Cancelled };
            self.answer_node(&reply, answer);
            return;
        }
        let screens = lum_ui::window::screens();
        let screen = lum_ui::window::cursor_screen(&screens)
            .or((!screens.is_empty()).then_some(0))
            .map(|i| screens[i].clone());
        self.auto_dialogs.push(AutoDialog { reply, kind, window: DialogWindow::new(dialog, screen) });
        self.waker.wake();
    }

    /// The boxes automations put up, drawn with the other messages.
    pub(super) fn show_automation_dialogs(&mut self, ctx: &egui::Context) {
        let mut answered = Vec::new();
        for (i, d) in self.auto_dialogs.iter_mut().enumerate() {
            let answer = match d.window.show(ctx) {
                Outcome::Open => continue,
                Outcome::Accepted(_) if d.kind == DialogKind::Ask => {
                    Answer::Done(d.window.dialog.text().to_owned())
                }
                Outcome::Accepted(_) => Answer::Done(String::new()),
                // closing a message box is all it asks for
                Outcome::Cancelled if d.kind == DialogKind::Message => Answer::Done(String::new()),
                Outcome::Cancelled => Answer::Cancelled,
            };
            answered.push((i, answer));
        }
        for (i, answer) in answered.into_iter().rev() {
            let d = self.auto_dialogs.remove(i);
            self.answer_node(&d.reply, answer);
        }
    }

    /// A warning closed: the nodes that put one up with this title carry on.
    pub(super) fn automation_alert_closed(&mut self, title: &str) {
        let (done, kept): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.auto_alerts).into_iter().partition(|(t, _)| t == title);
        self.auto_alerts = kept;
        for (_, reply) in done {
            self.answer_node(&reply, Answer::Done(String::new()));
        }
    }

    /// A `.lumflow` was read: it becomes a new workspace, switched off until you look at it.
    pub(super) fn automation_imported(&mut self, file: Result<serde_json::Value, String>) {
        let Some(auto) = &self.automation else { return };
        let wid = file.and_then(|f| {
            auto.with(|e| e.import_workspace(&f)).ok_or_else(|| "it holds no workspace".to_owned())
        });
        match wid {
            Ok(wid) => {
                self.automation_ui.show_workspace(&wid);
                self.notifier.notify(
                    "Automation",
                    "Imported. It is switched off until you turn it on.",
                    4000,
                );
            }
            Err(e) => self.notifier.notify("Import", &format!("Could not read that file: {e}"), 6000),
        }
    }

    /// ▶ for every Button trigger, and Pause all automations / Resume automations.
    pub(super) fn automation_palette_entries(&self) -> Vec<lum_ui::palette::Entry> {
        use lum_ui::palette::Entry;
        let Some(auto) = &self.automation else { return Vec::new() };
        let (buttons, paused) = auto.read(|e| (e.buttons(), e.paused()));
        let mut out: Vec<Entry> = buttons
            .into_iter()
            .map(|b| {
                let off = if b.running { "" } else { " (off)" };
                let hint = format!("Automation · {}{off}", b.workspace_name);
                Entry::new(
                    format!("auto:{}/{}", b.workspace, b.node),
                    format!("▶ {}", b.label),
                    hint,
                    "automation",
                )
            })
            .collect();
        let label = if paused { "Resume automations" } else { "Pause all automations" };
        out.push(Entry::new("auto:pause", label, "Automation", "automation"));
        out
    }

    pub(super) fn run_automation_row(&mut self, what: &str) {
        if what == "pause" {
            self.toggle_automation_pause();
        } else if let (Some((wid, nid)), Some(auto)) = (what.split_once('/'), &self.automation) {
            auto.with(|e| e.run_node(wid, nid, ""));
        }
    }

    /// The tray's ⚡ Automation submenu: pause, the buttons, each workspace on or off, and the page.
    pub(super) fn automation_tray_menu(&self) -> lum_platform::tray::MenuItem {
        use lum_platform::tray::MenuItem;
        let (buttons, workspaces, paused) = self.automation.as_ref().map_or_else(Default::default, |auto| {
            auto.read(|e| {
                let workspaces: Vec<(String, String, bool)> = lum_automation::doc::workspaces(e.doc())
                    .iter()
                    .map(|w| {
                        let id = lum_automation::doc::str_of(w, "id").to_owned();
                        (id, lum_automation::doc::str_of(w, "name").to_owned(), w["enabled"] == true)
                    })
                    .collect();
                (e.buttons(), workspaces, e.paused())
            })
        });
        let label = if paused { "Resume automations" } else { "Pause all automations" };
        let mut items = vec![MenuItem::action("auto:pause", label)];
        if !buttons.is_empty() {
            items.push(MenuItem::Separator);
            for b in buttons {
                items.push(MenuItem::Action {
                    id: format!("auto:run:{}/{}", b.workspace, b.node),
                    text: format!("▶  {}   ({})", b.label, b.workspace_name),
                    enabled: b.running,
                });
            }
        }
        items.push(MenuItem::Separator);
        for (id, name, on) in workspaces {
            items.push(MenuItem::Check { id: format!("auto:ws:{id}"), text: name, checked: on });
        }
        items.push(MenuItem::Separator);
        items.push(MenuItem::action("auto:page", "Open the Automation page"));
        MenuItem::Sub { text: "⚡  Automation".into(), items }
    }

    pub(super) fn automation_tray(&mut self, what: &str) {
        if what == "pause" {
            self.toggle_automation_pause();
        } else if what == "page" {
            self.open_overlay("automation");
        } else if let Some(row) = what.strip_prefix("run:") {
            self.run_automation_row(row);
        } else if let (Some(wid), Some(auto)) = (what.strip_prefix("ws:"), &self.automation) {
            auto.with(|e| {
                let on = e.workspace(wid).is_some_and(|w| w["enabled"] == true);
                e.set_enabled(wid, !on);
            });
        }
    }

    pub(super) fn stop_automation(&mut self) {
        if let Some(mut auto) = self.automation.take() {
            auto.shutdown();
        }
    }
}
