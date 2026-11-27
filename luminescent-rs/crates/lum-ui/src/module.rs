//! The drawing side of a module (D12): lum-core's [`Module`] knows nothing about windows, so a module that
//! shows something implements [`UiModule`] as well, and the app's host holds `dyn UiModule`s.
//!
//! A module's actions, hotkeys and the app's answers all reach it through its [`Inbox`]: they may come from
//! any thread, so they queue a command and wake the UI, and the module takes its commands at the top of
//! `show`. What a module wants from the rest of the app (a card, an event, the overlay closed) goes the other
//! way as a [`Request`] through [`Services`].

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use lum_core::actions::Action;
use lum_core::module::Module;
use lum_core::settings::Settings;

use crate::anim::Motion;
use crate::overlay::PageFrame;
use crate::palette::Entry;
use crate::theme::Tokens;

/// What a module gets while it draws.
pub struct UiCx<'a> {
    pub ctx: &'a egui::Context,
    pub settings: &'a Settings,
    pub tokens: Arc<Tokens>,
    pub motion: Arc<Motion>,
}

impl<'a> UiCx<'a> {
    pub fn new(ctx: &'a egui::Context, settings: &'a Settings) -> UiCx<'a> {
        UiCx { ctx, settings, tokens: Tokens::of(ctx), motion: Motion::of(ctx) }
    }
}

/// A hotkey a module owns: the setting inside its section that holds the key, the id it is told with when
/// the key is pressed, and the label the cheat sheet and the activity log use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bind {
    pub setting: &'static str,
    pub id: &'static str,
    pub label: &'static str,
}

/// What a module asks of the app.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    HideOverlay,
    OpenSettings(String),
    Notify {
        title: String,
        text: String,
        ms: u32,
    },
    /// A card that always shows, whatever Show notifications says (reminders, finished timers).
    Toast {
        title: String,
        text: String,
        ms: u32,
    },
    Emit {
        name: String,
        text: String,
    },
    /// Counts in the usage file ("tools", a key and what to call it).
    Usage {
        kind: String,
        key: String,
        label: String,
    },
    /// A line for the Today page's log, with how long it took when that is known.
    Activity {
        kind: String,
        label: String,
        secs: Option<f64>,
    },
    /// The tray's tooltip (running timers) and the chips on the overlay need drawing again.
    StatusChanged,
    /// A to-do item's due date changed: Reminders keeps its reminder in step.
    TodoReminder(lum_core::notes::TodoSync),
    /// Add, replace, remove or switch off a reminder in the list in Settings.
    ReminderEdit(lum_calc::reminder::Edit),
    /// Switch to the theme with this id (a reminder that changes the theme at a set time).
    ApplyTheme(String),
    /// A desktop notification and nothing else: no card of ours, whatever the notification style is.
    SystemNotice {
        title: String,
        text: String,
        ms: u32,
    },
    /// Press a key combination in the window that has the keyboard (Snippets pasting). With
    /// `release_modifiers`, keys the person is still holding are let go of first.
    SendKeys {
        combo: String,
        release_modifiers: bool,
    },
    /// Store a setting quietly, as the Settings page would (the folder picked on the Clipboard page).
    SetSetting {
        key: String,
        value: serde_json::Value,
    },
    /// The active theme gets this colour (`#RRGGBB`) as its accent.
    ThemeAccent(String),
    /// A new theme around this colour, applied straight away.
    ThemeFromColour {
        colour: String,
        light: bool,
    },
}

/// A key a module always wants, whatever the settings say (Doodle's Esc).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedBind {
    pub combo: String,
    pub id: &'static str,
    pub label: &'static str,
    /// The key press also reaches the app that has the keyboard.
    pub non_consuming: bool,
}

/// What happens to a module's hold key (Doodle's "hold Alt to draw").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoldEvent {
    Down,
    Up,
    /// Another key went down while it was held: its name.
    Interrupted(String),
    /// The key can not be a hold key here; the reason.
    Refused(String),
}

type Wake = Arc<dyn Fn() + Send + Sync>;

/// How a module reaches the app from any thread.
#[derive(Clone)]
pub struct Services {
    data_dir: PathBuf,
    requests: Arc<Mutex<Vec<Request>>>,
    wake: Wake,
}

impl std::fmt::Debug for Services {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Services").field("data_dir", &self.data_dir).finish_non_exhaustive()
    }
}

impl Services {
    pub fn new(data_dir: PathBuf, wake: impl Fn() + Send + Sync + 'static) -> Services {
        Services { data_dir, requests: Arc::default(), wake: Arc::new(wake) }
    }

    /// For tests and the widget gallery: nothing to wake.
    pub fn detached(data_dir: PathBuf) -> Services {
        Services::new(data_dir, || {})
    }

    pub fn data_file(&self, name: &str) -> PathBuf {
        self.data_dir.join(name)
    }

    pub fn wake(&self) {
        (self.wake)();
    }

    pub fn send(&self, request: Request) {
        if let Ok(mut queue) = self.requests.lock() {
            queue.push(request);
        }
        self.wake();
    }

    /// What modules asked for since the last call (the app answers them once per pass).
    pub fn take(&self) -> Vec<Request> {
        self.requests.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
    }
}

/// Commands for a module, queued from any thread (a hotkey, an action in the palette, a worker that finished).
pub struct Inbox<C> {
    queue: Arc<Mutex<VecDeque<C>>>,
    services: Services,
}

impl<C> Clone for Inbox<C> {
    fn clone(&self) -> Inbox<C> {
        Inbox { queue: self.queue.clone(), services: self.services.clone() }
    }
}

impl<C> std::fmt::Debug for Inbox<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Inbox")
    }
}

impl<C: Send + 'static> Inbox<C> {
    pub fn new(services: &Services) -> Inbox<C> {
        Inbox { queue: Arc::default(), services: services.clone() }
    }

    pub fn push(&self, command: C) {
        if let Ok(mut queue) = self.queue.lock() {
            queue.push_back(command);
        }
        self.services.wake();
    }

    pub fn take(&self) -> Vec<C> {
        self.queue.lock().map(|mut q| q.drain(..).collect()).unwrap_or_default()
    }

    /// An action that queues `make()` when it runs.
    pub fn action(&self, id: &str, label: &str, make: impl Fn() -> C + Send + Sync + 'static) -> Action {
        let inbox = self.clone();
        Action::new(id, label, move || inbox.push(make()))
    }
}

pub trait UiModule: Module {
    /// Draws the module's windows for this frame, each one it has open (a panel, a pop-up). Runs whenever
    /// the app repaints, so it must not do anything slow; a module with nothing open does nothing.
    fn show(&mut self, _cx: &mut UiCx<'_>) {}

    /// The hotkeys this module wants registered while it runs, and what to do when one is pressed.
    fn binds(&self) -> Vec<Bind> {
        Vec::new()
    }

    fn bind_pressed(&mut self, _id: &str) {}

    /// Keys the module wants that are not settings (Esc while drawing).
    fn fixed_binds(&self) -> Vec<FixedBind> {
        Vec::new()
    }

    /// The setting inside its section that holds a key it wants told about while it is held.
    fn hold_setting(&self) -> Option<&'static str> {
        None
    }

    fn hold_event(&mut self, _event: HoldEvent) {}

    /// The key of the overlay page this module draws (`calculator`), when it has one.
    fn page_key(&self) -> Option<&'static str> {
        None
    }

    /// Draws the page into `rect` (the overlay's panel or a dashboard box). Return `busy` while a dialog or a
    /// text box has the keyboard.
    fn page(&mut self, _ui: &mut egui::Ui, _rect: egui::Rect, _cx: &mut UiCx<'_>) -> PageFrame {
        PageFrame::default()
    }

    /// A page that panicked is started again from scratch.
    fn reset_page(&mut self) {}

    /// Entries for the tray menu: the action's id inside this module and its label.
    fn tray_actions(&self) -> Vec<(String, String)> {
        Vec::new()
    }

    /// Menus of the module's own in the tray (the colour picker's Recent colours). An entry whose id starts with
    /// `row:` runs that palette row of the module.
    fn tray_menu(&self) -> Vec<lum_platform::tray::MenuItem> {
        Vec::new()
    }

    /// Lines for the tray's tooltip and the overlay's chips ("Focus: 12:30 left").
    fn status_lines(&self) -> Vec<String> {
        Vec::new()
    }

    /// Chips for the overlay's status strip, each an icon and a line ("⏰", "Fri 14:30  Stretch").
    fn chips(&self) -> Vec<(String, String)> {
        Vec::new()
    }

    /// Rows the palette lists whatever is typed (a calculator pin).
    fn palette_entries(&self) -> Vec<Entry> {
        Vec::new()
    }

    /// Rows that depend on what is typed (a sum, "remind me in 20m").
    fn palette_live(&self, _text: &str) -> Vec<Entry> {
        Vec::new()
    }

    /// Reminders only: sets or drops the reminder of a to-do item. `false` when this module is not Reminders.
    fn todo_reminder(&mut self, _sync: &lum_core::notes::TodoSync) -> bool {
        false
    }

    /// Reminders only: goes off as a test (the Settings editor's "Test this reminder now"). `false` when this
    /// module is not Reminders.
    fn preview_reminder(&mut self, _rem: &serde_json::Value) -> bool {
        false
    }

    /// Runs a row this module made. `false` when the id is not one of its own.
    fn run_palette(&mut self, _id: &str, _services: &Services) -> bool {
        false
    }
}
