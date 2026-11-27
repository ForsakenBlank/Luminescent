//! Snippets (the Clipboard page and the Ctrl+B grid): a folder of files, text and pictures that paste with one
//! click. The pop-up, the overlay page and a dashboard box all show the same folder through `view.rs`; the rules
//! for what a file is, the order, favourites and colours live in `lum_core::snippets`.
//!
//! Listing a folder, making thumbnails, copying, pasting, saving the clipboard and the Trash all happen off the
//! UI thread and report back through the module's inbox.

mod thumbs;
mod view;

use std::path::{Path, PathBuf};
use std::time::Duration;

use egui::{Rect, Ui, vec2};
use lum_core::actions::Action;
use lum_core::module::{Module, ModuleCx, StartError};
use lum_core::settings::Settings;
use lum_core::snippets::{self, Listing, Meta, Saving};
use lum_core::store::Writer;
use lum_platform::{clipboard, dialogs};
use lum_theme::Rgb;

use crate::dialog::{AskDialog, DialogWindow, Outcome};
use crate::overlay::PageFrame;
use crate::panel::{FloatingPanel, Geometry, centred_on};
use crate::picker::{ColourDialog, Picked, PickerKind};
use crate::tools::edit::cursor_screen_rect;
use crate::tools::{dialog_id, dialog_window};
use crate::widgets::DimOverlay;
use crate::window::Screen;
use crate::{Bind, Inbox, Request, Services, UiCx, UiModule};
use thumbs::{Made, Thumbs};
use view::{Browser, Event, Look, NoFolder};

/// Python waits this long before pasting, so focus is back in the window you were using and the pop-up is gone.
const PASTE_AFTER: Duration = Duration::from_millis(220);
const ENTER_AFTER: Duration = Duration::from_millis(340);
const REOPEN_AFTER: Duration = Duration::from_millis(450);
const PAGE_COLUMNS: usize = 5;
const NARROW: f32 = 640.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    Popup,
    Page,
}

enum Cmd {
    /// The hotkey: opens the pop-up, or closes it when it is open.
    Open,
    Reopen(PathBuf),
    SaveAndTell,
    Loaded {
        place: Place,
        generation: u64,
        listing: Listing,
    },
    Thumb(Made),
    Saved {
        tell: bool,
        result: Result<Vec<PathBuf>, String>,
    },
    Trashed(Result<(), String>),
}

#[derive(Debug)]
enum Ask {
    Rename(PathBuf),
    Delete(PathBuf),
    NewFolder(Place),
    MakeFolder,
    Nothing,
}

struct Popup {
    panel: FloatingPanel,
    browser: Browser,
}

struct PageState {
    browser: Browser,
    /// The pass the page was last drawn in, to notice it coming back. `None` before the first time.
    pass: Option<u64>,
}

pub struct SnippetsModule {
    services: Services,
    inbox: Inbox<Cmd>,
    running: bool,
    base: PathBuf,
    name_limit: usize,
    meta: Meta,
    writer: Option<Writer>,
    popup: Popup,
    page: PageState,
    thumbs: Thumbs,
    dialog: Option<(DialogWindow, Ask)>,
    colour: Option<ColourDialog>,
    colour_for: Option<PathBuf>,
    screens: Vec<Screen>,
    auto_paste: bool,
    placeholders: bool,
}

impl std::fmt::Debug for SnippetsModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SnippetsModule").field("running", &self.running).finish_non_exhaustive()
    }
}

fn number(settings: &Settings, key: &str, default: i64) -> i64 {
    settings.get_i64(&format!("snippets.{key}")).unwrap_or(default)
}

impl SnippetsModule {
    pub fn new(services: &Services) -> SnippetsModule {
        let panel =
            FloatingPanel::new("snippets", "Clipboard", Geometry { x: 100.0, y: 100.0, w: 960.0, h: 560.0 })
                .min_size(vec2(400.0, 300.0))
                .resizable(false)
                .drag_needs_edit_mode(false);
        SnippetsModule {
            services: services.clone(),
            inbox: Inbox::new(services),
            running: false,
            base: PathBuf::new(),
            name_limit: 20,
            meta: Meta::load(Path::new("")),
            writer: None,
            popup: Popup { panel, browser: Browser::new("popup") },
            page: PageState { browser: Browser::new("page"), pass: None },
            thumbs: Thumbs::default(),
            dialog: None,
            colour: None,
            colour_for: None,
            screens: Vec::new(),
            auto_paste: true,
            placeholders: false,
        }
    }

    fn browser_mut(&mut self, place: Place) -> &mut Browser {
        match place {
            Place::Popup => &mut self.popup.browser,
            Place::Page => &mut self.page.browser,
        }
    }

    fn open_dialog(&mut self, dialog: AskDialog, ask: Ask) {
        self.dialog = Some((dialog_window(dialog), ask));
    }

    fn tell_error(&mut self, title: &str, text: &str) {
        self.open_dialog(AskDialog::error(dialog_id("snippets-error"), title, text), Ask::Nothing);
    }

    /// Takes in the settings that decide where the folder is; a new folder starts everything over.
    fn sync(&mut self, settings: &Settings) {
        self.name_limit = number(settings, "name_limit", 20).max(1) as usize;
        let base = settings.get_str("snippets.base_path").map(PathBuf::from).unwrap_or_default();
        if snippets::same(&base, &self.base) && !self.base.as_os_str().is_empty() {
            return;
        }
        self.base = base;
        self.meta = Meta::load(&self.base);
        self.thumbs.clear();
        self.page.browser.reset(&self.base, &self.base);
        self.popup.browser.reset(&self.base, &self.base);
    }

    fn reload_all(&mut self) {
        self.popup.browser.wanted = true;
        self.page.browser.wanted = true;
    }

    /// Tells the disk about a change to the order, favourites or colours, and lists everything again.
    fn meta_changed(&mut self) {
        if let Some(writer) = &self.writer {
            writer.save(self.meta.file(), self.meta.text());
        }
        self.reload_all();
    }

    fn on_thread(&self, name: &str, work: impl FnOnce() + Send + 'static) {
        if let Err(e) = std::thread::Builder::new().name(name.to_owned()).spawn(work) {
            tracing::warn!("could not start the {name} thread: {e}");
        }
    }

    /// Starts the listing a place asked for. An answer for an older one is dropped when it arrives.
    fn serve(&mut self, place: Place) {
        let limit = self.name_limit;
        let meta = self.meta.clone();
        let inbox = self.inbox.clone();
        let browser = self.browser_mut(place);
        if !browser.wanted {
            return;
        }
        browser.wanted = false;
        browser.generation += 1;
        let (generation, path, query) = (browser.generation, browser.path.clone(), browser.query.clone());
        self.on_thread("snippets-list", move || {
            let listing = if query.trim().is_empty() {
                snippets::list_folder(&meta, &path, limit)
            } else {
                snippets::search(&meta, &query, limit)
            };
            inbox.push(Cmd::Loaded { place, generation, listing });
        });
    }

    // ── what the views ask for ─────────────────────────────────────────────

    fn handle(&mut self, place: Place, events: Vec<Event>, ctx: &egui::Context) {
        for event in events {
            self.handle_one(place, event, ctx);
        }
    }

    fn handle_one(&mut self, place: Place, event: Event, ctx: &egui::Context) {
        match event {
            Event::OpenFolder(path) => self.browser_mut(place).open_folder(&path),
            Event::Up => {}
            Event::Search => self.browser_mut(place).wanted = true,
            Event::Use { path, ctrl } => self.use_clip(place, path, ctrl),
            Event::Copy(path) => self.copy_only(path),
            Event::OpenFile(path) => {
                if let Err(e) = lum_platform::runner::open_path(&path.to_string_lossy()) {
                    tracing::warn!("could not open {}: {e}", path.display());
                }
            }
            Event::Favourite(path, on) => {
                self.meta.set_favourite(&path, on, None);
                self.meta_changed();
            }
            Event::FolderColour(path) => self.ask_colour(path, ctx),
            Event::Rename(path) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let dialog = AskDialog::ask_text(dialog_id("snippets-rename"), "Rename", "New name:", &name);
                self.open_dialog(dialog, Ask::Rename(path));
            }
            Event::Reveal(path) => self.on_thread("snippets-reveal", move || {
                if let Err(e) = lum_platform::apps::reveal(&path.to_string_lossy()) {
                    tracing::warn!("could not show {}: {e}", path.display());
                }
            }),
            Event::Delete(path) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let text = format!("Move \"{name}\" to the Trash?");
                let dialog = AskDialog::confirm(
                    dialog_id("snippets-delete"),
                    "Delete",
                    &text,
                    "Delete",
                    "Cancel",
                    true,
                );
                self.open_dialog(dialog, Ask::Delete(path));
            }
            Event::NewFolder => {
                let dialog =
                    AskDialog::ask_text(dialog_id("snippets-new-folder"), "New folder", "Folder name:", "");
                self.open_dialog(dialog, Ask::NewFolder(place));
            }
            Event::OpenCurrent => {
                let path = self.browser_mut(place).path.clone();
                if path.is_dir()
                    && let Err(e) = lum_platform::runner::open_path(&path.to_string_lossy())
                {
                    tracing::warn!("could not open {}: {e}", path.display());
                }
            }
            Event::SaveClipboard => {
                let folder = self.browser_mut(place).path.clone();
                self.save_clipboard(folder, false);
            }
            Event::DropOnFolder { src, folder } => {
                self.move_clip(&src, &folder);
            }
            Event::DropOnClip { src, target, favourite } => self.drop_on_clip(src, target, favourite),
            Event::Thumbnails(jobs) => {
                let inbox = self.inbox.clone();
                self.thumbs.ask(jobs, move |made| inbox.push(Cmd::Thumb(made)));
            }
        }
    }

    fn move_clip(&mut self, src: &Path, folder: &Path) -> Option<PathBuf> {
        match snippets::move_into(src, folder) {
            Ok(dst) => {
                if !snippets::same(&dst, src) {
                    self.meta.path_moved(src, &dst);
                }
                self.meta_changed();
                Some(dst)
            }
            Err(e) => {
                self.tell_error("Move", &e.to_string());
                None
            }
        }
    }

    fn drop_on_clip(&mut self, src: PathBuf, target: PathBuf, favourite: bool) {
        if !src.exists() {
            return;
        }
        if favourite {
            self.meta.set_favourite(&src, true, Some(&target));
            self.meta_changed();
            return;
        }
        if !target.is_file() {
            return;
        }
        let Some(folder) = target.parent().map(Path::to_owned) else { return };
        let mut src = src;
        if src.parent().is_none_or(|p| !snippets::same(p, &folder)) {
            match self.move_clip(&src, &folder) {
                Some(dst) => src = dst,
                None => return,
            }
        }
        if src.is_file() {
            self.meta.place_before(&src, &target);
        }
        self.meta_changed();
    }

    fn ask_colour(&mut self, folder: PathBuf, ctx: &egui::Context) {
        let current = self
            .meta
            .folder_colour(&folder)
            .and_then(|c| Rgb::parse(&c))
            .unwrap_or_else(|| Rgb::parse("#7B6CF6").unwrap_or_else(|| Rgb::from_floats(0.5, 0.4, 0.9)));
        let near = if self.popup.panel.is_open() {
            self.popup.panel.geometry.rect()
        } else {
            cursor_screen_rect().unwrap_or_else(|| crate::window::fallback_screen(ctx).rect)
        };
        let files = (self.services.data_file("picker.json"), self.services.data_file("colours.json"));
        let dialog = self.colour.get_or_insert_with(|| ColourDialog::new(files.0, files.1));
        dialog.open("snippets-folder", "Folder colour", Picked::Colour(current), false, near);
        self.colour_for = Some(folder);
    }

    fn show_colour(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.colour.as_mut() else { return };
        for event in dialog.show(ctx) {
            if event.target != "snippets-folder" {
                continue;
            }
            if let PickerKind::Done(picked) = event.kind
                && let Some(folder) = self.colour_for.take()
            {
                self.meta.set_folder_colour(&folder, &picked.first().hex());
                self.meta_changed();
            }
        }
    }

    // ── using a snippet ────────────────────────────────────────────────────

    fn use_clip(&mut self, place: Place, path: PathBuf, ctrl: bool) {
        let reopen = (ctrl && place == Place::Popup).then(|| self.popup.browser.path.clone());
        match place {
            Place::Popup => self.popup.panel.close(),
            Place::Page => self.services.send(Request::HideOverlay),
        }
        let (inbox, services) = (self.inbox.clone(), self.services.clone());
        let (paste, placeholders) = self.paste_settings();
        self.on_thread("snippets-paste", move || {
            if let Err(e) = copy_clip(&path, placeholders) {
                services.send(Request::Notify { title: "Snippets".into(), text: e, ms: 4000 });
                return;
            }
            if !paste {
                return;
            }
            // focus needs a moment to go back to the window you were using, and the overlay time to fade
            std::thread::sleep(PASTE_AFTER);
            services.send(Request::SendKeys { combo: "ctrl+v".into(), release_modifiers: true });
            if ctrl {
                std::thread::sleep(ENTER_AFTER - PASTE_AFTER);
                services.send(Request::SendKeys { combo: "enter".into(), release_modifiers: false });
            }
            if let Some(folder) = reopen {
                std::thread::sleep(REOPEN_AFTER - ENTER_AFTER);
                inbox.push(Cmd::Reopen(folder));
            }
        });
    }

    fn paste_settings(&self) -> (bool, bool) {
        (self.auto_paste, self.placeholders)
    }

    fn copy_only(&self, path: PathBuf) {
        let (services, placeholders) = (self.services.clone(), self.placeholders);
        self.on_thread("snippets-copy", move || {
            if let Err(e) = copy_clip(&path, placeholders) {
                services.send(Request::Notify { title: "Snippets".into(), text: e, ms: 4000 });
            }
        });
    }

    /// Saves what is on the clipboard into `folder` (the snippets folder itself for the action, which makes it
    /// first and says what it saved).
    fn save_clipboard(&self, folder: PathBuf, tell: bool) {
        let inbox = self.inbox.clone();
        self.on_thread("snippets-save", move || {
            let result = std::fs::create_dir_all(&folder)
                .map_err(|e| format!("Could not make the snippets folder {}:\n{e}", folder.display()))
                .and_then(|()| {
                    let stamp = jiff::ZonedDateTime::now().strftime("%Y-%m-%d %H-%M-%S").to_string();
                    snippets::save_clipboard(&folder, &read_clipboard(), &stamp)
                });
            inbox.push(Cmd::Saved { tell, result });
        });
    }

    // ── opening and closing the pop-up ─────────────────────────────────────

    fn open_popup(&mut self, ctx: &egui::Context, settings: &Settings, at: Option<PathBuf>) {
        self.meta = Meta::load(&self.base);
        let start = at.filter(|p| p.is_dir()).unwrap_or_else(|| self.base.clone());
        self.popup.browser.reset(&self.base, &start);
        self.screens = crate::window::screens();
        if self.screens.is_empty() {
            self.screens = vec![crate::window::fallback_screen(ctx)];
        }
        let screen = cursor_screen_rect().unwrap_or(self.screens[0].rect);
        let size =
            vec2(number(settings, "window_width", 960) as f32, number(settings, "window_height", 560) as f32);
        let size = vec2(size.x.min(screen.width()), size.y.min(screen.height()));
        let at = centred_on(size, screen);
        self.popup.panel.geometry = Geometry { x: at.x, y: at.y, w: size.x, h: size.y };
        self.popup.panel.opacity = 1.0;
        self.popup.panel.open();
    }

    fn toggle(&mut self, ctx: &egui::Context, settings: &Settings, at: Option<PathBuf>) {
        if self.popup.panel.is_open() {
            self.popup.panel.close();
            return;
        }
        if !self.base.is_dir() {
            let text = format!(
                "The snippets folder does not exist yet:\n{}\n\nMake it now? (Settings → Clipboard (Snippets) picks another one.)",
                self.base.display()
            );
            let dialog =
                AskDialog::confirm(dialog_id("snippets-make"), "Snippets", &text, "Make it", "Cancel", false);
            self.open_dialog(dialog, Ask::MakeFolder);
            return;
        }
        self.open_popup(ctx, settings, at);
    }

    // ── dialogs, commands ──────────────────────────────────────────────────

    fn show_dialog(&mut self, ctx: &egui::Context, settings: &Settings) {
        let Some((window, _)) = self.dialog.as_mut() else { return };
        let outcome = window.show(ctx);
        if outcome == Outcome::Open {
            return;
        }
        let text = window.dialog.text().trim().to_owned();
        let Some((_, ask)) = self.dialog.take() else { return };
        if !matches!(outcome, Outcome::Accepted(_)) {
            return;
        }
        match ask {
            Ask::Nothing => {}
            Ask::MakeFolder => match std::fs::create_dir_all(&self.base) {
                Ok(()) => self.open_popup(ctx, settings, None),
                Err(e) => {
                    self.tell_error("Snippets", &format!("Could not make {}:\n{e}", self.base.display()))
                }
            },
            Ask::Rename(path) => self.rename(&path, &text),
            Ask::Delete(path) => {
                let inbox = self.inbox.clone();
                self.on_thread("snippets-trash", move || {
                    inbox.push(Cmd::Trashed(lum_platform::trash::move_to_trash(&path)))
                });
            }
            Ask::NewFolder(place) => {
                if text.is_empty() {
                    return;
                }
                let at = self.browser_mut(place).path.join(&text);
                match std::fs::create_dir(&at) {
                    Ok(()) => self.reload_all(),
                    Err(e) => self.tell_error("New folder", &e.to_string()),
                }
            }
        }
    }

    fn rename(&mut self, path: &Path, name: &str) {
        let old = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if name.is_empty() || name == old {
            return;
        }
        let Some(dst) = path.parent().map(|p| p.join(name)) else { return };
        if dst.exists() {
            self.tell_error("Rename", &format!("\"{name}\" is there already."));
            return;
        }
        match std::fs::rename(path, &dst) {
            Ok(()) => {
                self.meta.path_moved(path, &dst);
                self.meta_changed();
            }
            Err(e) => {
                self.tell_error("Rename", &e.to_string());
                self.reload_all();
            }
        }
    }

    fn take_commands(&mut self, ctx: &egui::Context, settings: &Settings) {
        for cmd in self.inbox.take() {
            match cmd {
                Cmd::Open => self.toggle(ctx, settings, None),
                Cmd::Reopen(path) => {
                    if !self.popup.panel.is_open() && self.base.is_dir() {
                        self.open_popup(ctx, settings, Some(path));
                    }
                }
                Cmd::SaveAndTell => {
                    let base = self.base.clone();
                    self.save_clipboard(base, true);
                }
                Cmd::Loaded { place, generation, listing } => {
                    let browser = self.browser_mut(place);
                    if browser.generation == generation {
                        browser.listing = Some(listing);
                    }
                }
                Cmd::Thumb(made) => self.thumbs.arrived(ctx, made),
                Cmd::Saved { tell, result } => self.saved(tell, result),
                Cmd::Trashed(result) => {
                    if let Err(e) = result {
                        self.tell_error("Delete", &e);
                    }
                    self.reload_all();
                }
            }
        }
    }

    fn saved(&mut self, tell: bool, result: Result<Vec<PathBuf>, String>) {
        if tell {
            let text = match result {
                Ok(saved) => {
                    let names: Vec<String> = saved
                        .iter()
                        .map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
                        .collect();
                    format!("Saved {}", names.join(", "))
                }
                Err(e) => e,
            };
            self.services.send(Request::Notify { title: "Snippets".into(), text, ms: 4000 });
        } else if let Err(e) = result {
            self.tell_error("Save clipboard", &e);
        }
        self.reload_all();
    }

    /// A listing that arrived: pictures on view ask for their thumbnails from the grid itself.
    fn after(&mut self) {
        self.serve(Place::Popup);
        self.serve(Place::Page);
    }

    fn draw_popup(&mut self, cx: &mut UiCx<'_>) {
        let browser = &mut self.popup.browser;
        let title = browser.title();
        self.popup.panel.set_title("Clipboard", &title);
        let cols = number(cx.settings, "card_cols", 4).clamp(1, 12) as usize;
        let events = std::cell::RefCell::new(Vec::new());
        let thumbs = &self.thumbs;
        let response = self.popup.panel.show(cx.ctx, |ui| {
            let rect = ui.max_rect();
            let look = Look { cols, pad: 16.0, narrow: rect.width() < NARROW };
            view::show(ui, rect, browser, thumbs, look, &mut events.borrow_mut());
        });
        if response.closed || response.escape {
            self.popup.panel.close();
        }
        self.handle(Place::Popup, events.into_inner(), cx.ctx);
        let dim = DimOverlay {
            opacity: cx.settings.get_f64("snippets.overlay_dim").unwrap_or(0.35) as f32,
            click_through: false,
        };
        if dim.show(cx.ctx, &self.screens) {
            self.popup.panel.close();
        }
    }

    fn draw_page(&mut self, ui: &mut Ui, rect: Rect, cx: &mut UiCx<'_>) -> bool {
        let pass = cx.ctx.cumulative_pass_nr();
        let returning = self.page.pass.is_none_or(|last| last + 1 < pass);
        self.page.pass = Some(pass);
        if !self.base.is_dir() {
            match view::no_folder(ui, rect, &self.base) {
                Some(NoFolder::Make) => match std::fs::create_dir_all(&self.base) {
                    Ok(()) => self.reload_all(),
                    Err(e) => {
                        self.tell_error("Snippets", &format!("Could not make {}:\n{e}", self.base.display()))
                    }
                },
                Some(NoFolder::Other) => {
                    let services = self.services.clone();
                    let start = lum_core::paths::user_home();
                    dialogs::pick_folder(
                        dialogs::Request::new("Snippets folder").folder(Some(start)),
                        move |path| {
                            if let Some(path) = path {
                                services.send(Request::SetSetting {
                                    key: "snippets.base_path".into(),
                                    value: serde_json::json!(path.to_string_lossy()),
                                });
                            }
                        },
                    );
                }
                None => {}
            }
            return self.dialog.is_some();
        }
        if returning {
            // coming back to the page: what is on disk now, not what it looked like last time
            self.meta = Meta::load(&self.base);
            self.page.browser.wanted = true;
        }
        let narrow = rect.width() < NARROW;
        let cols = if narrow {
            ((rect.width() - 32.0 + 12.0) / 172.0).floor().clamp(1.0, PAGE_COLUMNS as f32) as usize
        } else {
            PAGE_COLUMNS
        };
        let look = Look { cols, pad: if narrow { 12.0 } else { 24.0 }, narrow };
        let mut events = Vec::new();
        view::show(ui, rect, &mut self.page.browser, &self.thumbs, look, &mut events);
        self.handle(Place::Page, events, cx.ctx);
        self.dialog.is_some() || self.page.browser.dragging()
    }
}

/// Puts a snippet on the clipboard: a text file's contents as text (with its placeholders filled in when that is
/// switched on), anything else as the file itself.
fn copy_clip(path: &Path, placeholders: bool) -> Result<(), String> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if snippets::is_text(path) {
        let text = snippets::read_text(path).map_err(|e| format!("Could not read {name}: {e}"))?;
        let mut text = text.trim().to_owned();
        if placeholders {
            let before = clipboard::text().unwrap_or_default();
            text = snippets::fill_placeholders(&text, &before, jiff::ZonedDateTime::now().datetime());
        }
        clipboard::set_text(&text).map_err(|e| format!("Could not copy {name}: {e}"))
    } else {
        clipboard::set_files(&[path]).map_err(|e| format!("Could not copy {name}: {e}"))
    }
}

fn read_clipboard() -> Saving {
    let contents = clipboard::contents();
    let picture_png = contents.picture.and_then(|p| {
        let image = image::RgbaImage::from_raw(p.width as u32, p.height as u32, p.rgba)?;
        let mut png = Vec::new();
        image.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).ok()?;
        Some(png)
    });
    Saving { files: contents.files, picture_png, text: contents.text }
}

impl Module for SnippetsModule {
    fn key(&self) -> &'static str {
        "snippets"
    }

    fn title(&self) -> &'static str {
        "Snippets"
    }

    fn start(&mut self, cx: &mut ModuleCx<'_>) -> Result<(), StartError> {
        self.base = PathBuf::new();
        self.auto_paste = cx.settings.get_bool("snippets.auto_paste", true);
        self.placeholders = cx.settings.get_bool("snippets.placeholders", false);
        self.writer = Some(Writer::new("snippets-save"));
        self.sync(cx.settings);
        self.running = true;
        Ok(())
    }

    fn stop(&mut self, _cx: &mut ModuleCx<'_>) {
        self.popup.panel.close();
        self.thumbs.clear();
        self.dialog = None;
        self.colour = None;
        self.running = false;
        self.writer = None;
    }

    fn apply_settings(&mut self, cx: &mut ModuleCx<'_>) -> Result<(), StartError> {
        self.auto_paste = cx.settings.get_bool("snippets.auto_paste", true);
        self.placeholders = cx.settings.get_bool("snippets.placeholders", false);
        self.thumbs.clear();
        self.sync(cx.settings);
        self.reload_all();
        Ok(())
    }

    fn actions(&self) -> Vec<Action> {
        vec![
            self.inbox.action("open", "Open Snippets", || Cmd::Open),
            self.inbox.action("save_clipboard", "Save the clipboard to Snippets", || Cmd::SaveAndTell),
        ]
    }
}

impl UiModule for SnippetsModule {
    fn binds(&self) -> Vec<Bind> {
        vec![Bind { setting: "hotkey", id: "open", label: "open snippets" }]
    }

    fn bind_pressed(&mut self, id: &str) {
        if id == "open" {
            self.inbox.push(Cmd::Open);
        }
    }

    fn page_key(&self) -> Option<&'static str> {
        Some("clipboard")
    }

    fn reset_page(&mut self) {
        self.page = PageState { browser: Browser::new("page"), pass: None };
        self.page.browser.reset(&self.base, &self.base);
    }

    fn tray_actions(&self) -> Vec<(String, String)> {
        vec![("open".into(), "Open Snippets".into())]
    }

    fn show(&mut self, cx: &mut UiCx<'_>) {
        if !self.running {
            return;
        }
        self.sync(cx.settings);
        self.take_commands(cx.ctx, cx.settings);
        self.show_dialog(cx.ctx, cx.settings);
        self.show_colour(cx.ctx);
        if self.popup.panel.is_open() {
            self.draw_popup(cx);
        }
        self.after();
    }

    fn page(&mut self, ui: &mut Ui, rect: Rect, cx: &mut UiCx<'_>) -> PageFrame {
        if !self.running {
            return PageFrame::default();
        }
        self.sync(cx.settings);
        self.take_commands(cx.ctx, cx.settings);
        self.show_dialog(cx.ctx, cx.settings);
        self.show_colour(cx.ctx);
        let busy = self.draw_page(ui, rect, cx) || self.colour.as_ref().is_some_and(ColourDialog::is_open);
        self.after();
        PageFrame { busy }
    }
}
