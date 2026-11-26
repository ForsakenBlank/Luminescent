//! The Dashboard's side of the app: running items and groups, their hotkeys, the tray and palette entries, and
//! what the page asks for after a frame (`DashboardStore` and the parts of `Launcher.py` that touch it).

use std::collections::HashMap;
use std::path::Path;
use std::process::Child;
use std::time::{Duration, Instant};

use lum_core::dashboard::{MODULE_ICONS, ToolSource};
use lum_core::module::Notifier;
use lum_platform::hotkeys::{self, Os};
use lum_platform::runner;
use lum_platform::tray::MenuItem;
use lum_ui::dashboard::Out;

use super::App;

/// How long the overlay has to finish hiding before an item it launched starts (as the Python app waits).
const AFTER_HIDING: Duration = Duration::from_millis(150);
/// How often running items are checked while any are left.
const CHECK_EVERY: Duration = Duration::from_millis(1500);

#[derive(Debug)]
struct Due {
    at: Instant,
    id: String,
    args: Option<String>,
}

/// The processes the dashboard started and the runs that wait for their turn (a group's next item).
#[derive(Debug, Default)]
pub struct Running {
    procs: HashMap<String, Vec<Child>>,
    due: Vec<Due>,
    check_at: Option<Instant>,
}

impl Running {
    /// Whether the item has a process left. It is looked at every second and a half, so the dot on the row
    /// goes out a moment after the program ends.
    pub fn is_running(&self, id: &str) -> bool {
        self.procs.contains_key(id)
    }

    /// When something has to happen next (a run that is due, or a look at the running processes).
    pub fn deadline(&self) -> Option<Instant> {
        self.due.iter().map(|d| d.at).chain(self.check_at).min()
    }

    /// Forgets processes that ended; true if any did.
    fn reap(&mut self) -> bool {
        let before: usize = self.procs.values().map(Vec::len).sum();
        for list in self.procs.values_mut() {
            list.retain_mut(|c| matches!(c.try_wait(), Ok(None)));
        }
        self.procs.retain(|_, list| !list.is_empty());
        self.check_at = (!self.procs.is_empty()).then(|| Instant::now() + CHECK_EVERY);
        before != self.procs.values().map(Vec::len).sum::<usize>()
    }
}

impl App {
    /// Each module's actions, for the Tools tab of a new dashboard.
    pub(super) fn tool_sources(&self) -> Vec<ToolSource> {
        let mut sources: Vec<ToolSource> = Vec::new();
        for registered in self.actions.all() {
            let icon = MODULE_ICONS.iter().find(|(k, _)| *k == registered.owner).map_or("", |(_, i)| *i);
            let entry = (registered.action.id.clone(), registered.action.label.clone());
            match sources.iter_mut().find(|s| s.key == registered.owner) {
                Some(s) => s.actions.push(entry),
                None => sources.push(ToolSource {
                    key: registered.owner.clone(),
                    icon: icon.to_owned(),
                    actions: vec![entry],
                }),
            }
        }
        sources
    }

    /// Item hotkeys, for every board whether or not the overlay is open.
    pub(super) fn register_dashboard_hotkeys(&mut self) {
        self.hotkeys.unregister_all("dashboard");
        let mut problems = Vec::new();
        let items: Vec<(String, String, String)> = self
            .dashboard
            .all_tabs()
            .flat_map(|(_, t)| {
                t.items()
                    .filter(|i| i.runnable() && !i.hotkey().is_empty())
                    .map(|i| (i.id().to_owned(), i.name().to_owned(), i.hotkey().to_owned()))
                    .collect::<Vec<_>>()
            })
            .collect();
        for (id, name, combo) in items {
            let label = format!("run {name}");
            if let Err(e) =
                self.hotkeys.register("dashboard", &combo, &id, &label, hotkeys::Options::default())
            {
                problems.push(format!("{name}: {e}"));
            }
        }
        if !problems.is_empty() {
            self.notifier.notify("Dashboard hotkeys", &problems.join("\n"), 4000);
        }
        self.dashboard.take_hotkeys_stale();
        self.refresh_tray();
    }

    /// Runs an item: a program, file, folder, link or command, a Luminescent action, or every item of a group.
    pub fn run_dashboard_item(&mut self, iid: &str, args: Option<String>) {
        let Some((_, item)) = self.dashboard.find_item(iid) else { return };
        if !item.runnable() {
            return;
        }
        let (name, path, action, cwd) =
            (item.name().to_owned(), item.path().to_owned(), item.action().to_owned(), item.cwd().to_owned());
        let (kind, ask, admin, terminal) =
            (item.kind().to_owned(), item.ask_args(), item.admin(), item.terminal());
        let (own_args, group) =
            (item.args().to_owned(), item.group().iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
        self.usage.record("items", iid, &name);
        self.usage.activity(
            "run",
            if name.is_empty() { if path.is_empty() { "item" } else { &path } } else { &name },
            None,
            None,
        );
        self.emit("dashboard.item_run", &name);
        if kind == "group" {
            self.run_group(iid, &name, group);
            return;
        }
        if !action.is_empty() {
            let answer = self.run_action(&action);
            if let Some(e) = answer.strip_prefix("error: ") {
                self.notifier.notify(&name, e, 4000);
            }
            return;
        }
        if args.is_none() && ask {
            self.ask_arguments(iid, if name.is_empty() { &path } else { &name }, &own_args);
            return;
        }
        let args = args.unwrap_or(own_args);
        let title = if name.is_empty() { "Dashboard" } else { &name };
        let terminal_setting = self.settings.get_str("dashboard.terminal").unwrap_or_default().to_owned();
        let expanded = runner::expand(&path);
        if terminal && !path.trim().is_empty() && !Path::new(&expanded).exists() && !path.contains("://") {
            let line = format!("{} {args}", path.trim()).trim().to_owned();
            let home = if cwd.is_empty() { "~".to_owned() } else { cwd };
            let result =
                runner::open_terminal(Path::new(&runner::expand(&home)), &line, "", &terminal_setting);
            runner::log_command(&self.paths.data_file("command_log.txt"), &line);
            if let Err(e) = result {
                self.notifier.notify(title, &e, 4000);
            }
            return;
        }
        let options = runner::Options {
            use_terminal: self.settings.get_bool("dashboard.use_terminal", true),
            keep_open: self.settings.get_bool("dashboard.keep_open", true),
            python: self.settings.get_str("dashboard.python_path").unwrap_or_default().to_owned(),
            terminal: String::new(),
            preferred_terminal: terminal_setting,
            cwd,
            admin,
        };
        match runner::launch(&path, &args, &name, &options, &self.paths.data_file("command_log.txt")) {
            Err(e) => self.notifier.notify(title, &e, 4000),
            Ok(Some(child)) => {
                self.dash.procs.entry(iid.to_owned()).or_default().push(child);
                self.dash.check_at.get_or_insert_with(|| Instant::now() + CHECK_EVERY);
                self.waker.wake();
            }
            Ok(None) => {}
        }
    }

    fn run_group(&mut self, own: &str, name: &str, ids: Vec<String>) {
        let ids: Vec<String> = ids.into_iter().filter(|i| i != own).collect();
        if ids.is_empty() {
            let title = if name.is_empty() { "Group" } else { name };
            self.notifier.notify(title, "This group has no items yet (right-click → Edit…).", 4000);
            return;
        }
        let delay = Duration::from_millis(
            self.settings.get_f64("dashboard.group_delay_ms").unwrap_or(400.0).max(0.0) as u64,
        );
        let now = Instant::now();
        for (n, id) in ids.into_iter().enumerate() {
            self.dash.due.push(Due { at: now + delay * n as u32, id, args: None });
        }
        self.waker.wake();
    }

    /// An item that wants its arguments each time asks in a dialog of its own, since the overlay may be closed.
    fn ask_arguments(&mut self, iid: &str, name: &str, default: &str) {
        let dialog = lum_ui::dialog::AskDialog::ask_text(
            egui::Id::unique("lum-ask-args"),
            name,
            &format!("Arguments for {name}:"),
            default,
        );
        let screens = lum_ui::window::screens();
        let screen = lum_ui::window::cursor_screen(&screens)
            .or((!screens.is_empty()).then_some(0))
            .map(|i| screens[i].clone());
        self.ask_args = Some((iid.to_owned(), lum_ui::dialog::DialogWindow::new(dialog, screen)));
        self.waker.wake();
    }

    pub fn stop_dashboard_item(&mut self, iid: &str) {
        let mut stopped = 0;
        for child in self.dash.procs.get_mut(iid).into_iter().flatten() {
            stopped += usize::from(runner::stop(child));
        }
        if stopped > 0 {
            let name = self.dashboard.find_item(iid).map(|(_, i)| i.name().to_owned()).unwrap_or_default();
            self.notifier.notify(if name.is_empty() { "Dashboard" } else { &name }, "Stopped.", 1500);
        }
        self.dash.check_at = Some(Instant::now() + Duration::from_millis(500));
    }

    /// Starts what is due (a group's next item, an item the overlay hid for) and drops finished processes.
    pub(super) fn dashboard_tick(&mut self) {
        let now = Instant::now();
        let (ready, waiting): (Vec<Due>, Vec<Due>) =
            std::mem::take(&mut self.dash.due).into_iter().partition(|d| d.at <= now);
        self.dash.due = waiting;
        for due in ready {
            self.run_dashboard_item(&due.id, due.args);
        }
        if self.dash.check_at.is_some_and(|at| at <= now) && self.dash.reap() {
            self.waker.wake();
        }
        if self.dashboard.take_hotkeys_stale() {
            self.register_dashboard_hotkeys();
        }
    }

    /// The dialog for an item that asks for its arguments.
    pub(super) fn show_ask_args(&mut self, ctx: &egui::Context) {
        let Some((id, dialog)) = &mut self.ask_args else { return };
        match dialog.show(ctx) {
            lum_ui::dialog::Outcome::Open => {}
            lum_ui::dialog::Outcome::Accepted(_) => {
                let (id, text) = (id.clone(), dialog.dialog.text().to_owned());
                self.ask_args = None;
                self.run_dashboard_item(&id, Some(text));
            }
            lum_ui::dialog::Outcome::Cancelled => self.ask_args = None,
        }
    }

    // ── the page ──────────────────────────────────────────────────────────

    /// The settings the page reads each frame.
    pub(super) fn dashboard_env_values(&self) -> (i64, String, String, bool) {
        (
            self.settings.get_f64("dashboard.snap").unwrap_or(8.0) as i64,
            self.settings.get_str("dashboard.tidy_style").unwrap_or("columns").to_owned(),
            self.settings.get_str("dashboard.tidy_order").unwrap_or("position").to_owned(),
            self.settings.get_bool("dashboard.show_icons", true),
        )
    }

    /// What the page decided on the last frame: runs, stops, terminals, settings it changed.
    pub(super) fn dashboard_out(&mut self, out: Out) {
        for (id, args) in out.run {
            self.dash.due.push(Due { at: Instant::now() + AFTER_HIDING, id, args });
        }
        for (id, args) in out.run_now {
            self.run_dashboard_item(&id, args);
        }
        for id in out.stop {
            self.stop_dashboard_item(&id);
        }
        for folder in out.terminal_here {
            let terminal = self.settings.get_str("dashboard.terminal").unwrap_or_default().to_owned();
            if let Err(e) = runner::open_terminal(Path::new(&folder), "", "", &terminal) {
                self.notifier.notify("Terminal", &e, 4000);
            }
        }
        for path in out.reveal {
            if let Err(e) = lum_platform::apps::reveal(&path) {
                self.notifier.notify("Dashboard", &e, 4000);
            }
        }
        for (title, text) in out.notify {
            self.notifier.notify(&title, &text, 2500);
        }
        if !out.settings.is_empty() {
            for (key, value) in out.settings {
                self.settings.set(&key, value);
            }
            if let Err(e) = self.settings.save_quiet() {
                tracing::warn!("could not save the dashboard's settings: {e}");
            }
            self.overlay_config.snap = self.settings.get_f64("dashboard.snap").unwrap_or(8.0).max(1.0) as f32;
        }
        if let Some(on) = out.suspend_keys {
            self.hotkeys.set_suspended(on);
        }
        if let Some(name) = out.switched {
            self.emit("dashboard.switched", &name);
        }
        if let Some(page) = out.open_page {
            self.open_overlay(&page);
        }
        if out.hide_overlay {
            self.hide_overlay();
        }
        if self.dashboard.take_hotkeys_stale() {
            self.register_dashboard_hotkeys();
        }
    }

    // ── palette and tray ───────────────────────────────────────────────────

    /// Every item that runs, for the palette (a Luminescent action is already listed by its module).
    pub(crate) fn dashboard_palette_entries(&self) -> Vec<lum_ui::palette::Entry> {
        let several = self.dashboard.board_count() > 1;
        let mut out = Vec::new();
        for (board, tab) in self.dashboard.all_tabs() {
            for item in tab.items().filter(|i| i.runnable() && i.action().is_empty()) {
                let place = if several {
                    format!("{} › {}", board.name(), tab.title())
                } else {
                    tab.title().to_owned()
                };
                let hint = if item.hotkey().is_empty() {
                    place
                } else {
                    format!("{place} · {}", hotkeys::pretty(item.hotkey(), Os::current()))
                };
                out.push(lum_ui::palette::Entry::new(
                    format!("item:{}", item.id()),
                    item.name(),
                    hint,
                    "item",
                ));
            }
        }
        out
    }

    /// The tray's Dashboard submenu: every board's boxes and their items, then "Edit the dashboard…".
    pub(super) fn dashboard_tray_menu(&self) -> Option<MenuItem> {
        let shown = self.settings.get_str_list("general.tray_items");
        if !self.settings.get_bool("dashboard.show_in_tray", true) || !shown.iter().any(|s| s == "dashboard")
        {
            return None;
        }
        let several = self.dashboard.board_count() > 1;
        let mut items = Vec::new();
        for board in self.dashboard.boards() {
            let mut boxes = Vec::new();
            for tab in board.tabs() {
                let mut rows: Vec<MenuItem> = tab
                    .items()
                    .filter(|i| i.kind() != "separator" && i.kind() != "heading")
                    .map(|i| {
                        let hint = if i.hotkey().is_empty() {
                            String::new()
                        } else {
                            format!("   ({})", hotkeys::pretty(i.hotkey(), Os::current()))
                        };
                        MenuItem::action(&format!("dash:{}", i.id()), format!("{}{hint}", i.name()))
                    })
                    .collect();
                if rows.is_empty() {
                    rows.push(MenuItem::Action {
                        id: "dash-empty".into(),
                        text: "(empty)".into(),
                        enabled: false,
                    });
                }
                boxes.push(MenuItem::Sub { text: tab.title().to_owned(), items: rows });
            }
            if several {
                items.push(MenuItem::Sub { text: board.name().to_owned(), items: boxes });
            } else {
                items.extend(boxes);
            }
        }
        items.push(MenuItem::Separator);
        items.push(MenuItem::action("dash-edit", "Edit the dashboard…"));
        Some(MenuItem::Sub { text: "▦  Dashboard".to_owned(), items })
    }

    // ── apps over the overlay ──────────────────────────────────────────────

    /// An app started while the overlay is up (Super+E, a dashboard item…): it floats on top of the overlay.
    pub(super) fn foreign_opened(&mut self, addr: &str) {
        if !self.overlay.is_visible() || !self.settings.get_bool("ui.apps_over_overlay", true) {
            return;
        }
        let screens = lum_ui::window::screens();
        let Some(screen) = lum_ui::window::cursor_screen(&screens).and_then(|i| screens.get(i)) else {
            return;
        };
        let r = screen.rect;
        self.desktop
            .float_foreign(addr, [r.min.x as i32, r.min.y as i32, r.width() as i32, r.height() as i32]);
        self.floated.push(addr.to_owned());
    }

    /// The overlay is gone: the windows put on top of it go back into the tiling layout.
    pub(super) fn unfloat_foreign(&mut self) {
        for addr in std::mem::take(&mut self.floated) {
            self.desktop.unfloat_foreign(&addr);
        }
    }

    pub(super) fn dashboard_repair(&mut self) {
        self.dashboard_ui.reset();
    }

    /// Saves the dashboard at quit.
    pub(super) fn dashboard_flush(&self) {
        self.dashboard.flush();
    }
}
