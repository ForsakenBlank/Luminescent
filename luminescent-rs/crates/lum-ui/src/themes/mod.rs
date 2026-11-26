//! The Themes page of the overlay (the theme cards), the colour dialog it shares with the theme editor, and the
//! editor itself. The page draws into the overlay's panel; the editor and the colour dialog are windows of
//! their own, drawn whether the overlay is up or not.

use std::path::PathBuf;

use egui::Context;
use lum_theme::ThemeStore;

use crate::picker::{ColourDialog, PickerEvent};
use crate::themes::editor::ThemeEditor;
use crate::themes::page::{FileAsk, FileJob, PageCx, PageOut, ThemesPage};

pub mod demo;
pub mod editor;
pub mod page;
pub mod pictures;
pub mod previews;

/// Every module with a look of its own in a theme: key, icon and title (the same list as the modules of the
/// Python app, in the same order).
pub const MODULES: &[(&str, &str, &str)] = &[
    ("atlas_sync", "⇄", "Atlas Sync"),
    ("caelestia", "◐", "Caelestia & Glass"),
    ("calculator", "∑", "Calculator"),
    ("hearing", "🎧", "Hearing Guard"),
    ("keybinds", "⌨", "Keybinds"),
    ("media", "♪", "Media Controls"),
    ("metadata", "✂", "Metadata Stripper"),
    ("notes", "📝", "Notes"),
    ("performance", "📊", "Performance"),
    ("promptlog", "⌨", "Prompt Log"),
    ("qr", "▦", "QR Code"),
    ("reminders", "⏰", "Reminders"),
    ("snippets", "📋", "Snippets"),
    ("syscontrol", "🖥", "System Settings"),
    ("sysmon", "📈", "System Monitor"),
    ("textgrab", "🔤", "Text Grab"),
    ("timer", "⏱", "Timer / Stopwatch"),
    ("vault", "🔒", "Ghost Vault"),
];

/// What the window needs to know about the rest of the app.
pub struct Env<'a> {
    /// The settings profile's name for its id.
    pub profile_name: &'a dyn Fn(&str) -> String,
    /// The settings profiles besides the default: (id, name).
    pub profiles: Vec<(String, String)>,
    /// What else uses a hotkey, if anything (themes' own keys excluded).
    pub key_owner: &'a dyn Fn(&str) -> Option<String>,
    /// How notification cards are styled (`general.toast_style`), for the preview.
    pub toast_style: String,
    pub hyprland: bool,
    pub caelestia: bool,
}

/// What the app has to do after a frame of the window.
#[derive(Debug, Default)]
pub struct Out {
    pub files: Vec<FileAsk>,
    /// Desktop notifications: (title, text).
    pub notify: Vec<(String, String)>,
    /// Hotkeys must not fire while the editor records a key (`Some(true)`), and may again afterwards.
    pub suspend_keys: Option<bool>,
}

impl Out {
    fn take(&mut self, page: PageOut) -> Option<String> {
        self.files.extend(page.files);
        self.notify.extend(page.notify);
        page.edit
    }
}

#[derive(Debug)]
pub struct ThemesUi {
    page: ThemesPage,
    picker: ColourDialog,
    editor: Option<ThemeEditor>,
    /// Where the page was drawn last, in desktop coordinates: the editor and dialogs open in front of it.
    near: egui::Rect,
}

impl ThemesUi {
    /// `picker_file` is `picker.json`, `eyedropper_file` the screen colour picker's `colours.json`.
    pub fn new(picker_file: PathBuf, eyedropper_file: PathBuf) -> ThemesUi {
        ThemesUi {
            page: ThemesPage::new(),
            picker: ColourDialog::new(picker_file, eyedropper_file),
            editor: None,
            near: egui::Rect::from_min_size(egui::pos2(160.0, 90.0), egui::vec2(1040.0, 720.0)),
        }
    }

    /// The editor or the colour dialog is open, so there is something to draw even with the overlay hidden.
    pub fn has_windows(&self) -> bool {
        self.editor.is_some() || self.picker.is_open()
    }

    /// Something of the page's own uses the keyboard right now (a dialog, the colour dialog, the editor), so Esc
    /// and typing are not the overlay's.
    pub fn busy(&self) -> bool {
        self.page.has_dialog() || self.has_windows()
    }

    /// Throws the page's state away (it failed while drawing); the next frame builds it afresh.
    pub fn reset_page(&mut self) {
        self.page = ThemesPage::new();
    }

    pub fn picker(&mut self) -> &mut ColourDialog {
        &mut self.picker
    }

    pub fn editor(&mut self) -> Option<&mut ThemeEditor> {
        self.editor.as_mut()
    }

    /// Opens the editor on a theme, as big as the screen the page is on allows.
    pub fn edit(&mut self, ctx: &Context, store: &ThemeStore, id: &str) {
        if let Some(theme) = store.get(id) {
            let area = work_area(ctx, self.near);
            self.editor = Some(ThemeEditor::new(theme, store.recent_changes(id), area));
        }
    }

    /// Draws the page into `ui`, which sits at `on_desktop` on the screen.
    pub fn page_ui(
        &mut self,
        ui: &mut egui::Ui,
        on_desktop: egui::Rect,
        store: &mut ThemeStore,
        env: &Env<'_>,
    ) -> Out {
        self.near = on_desktop;
        let mut out = Out::default();
        let mut cx = PageCx {
            store: &mut *store,
            picker: &mut self.picker,
            near: on_desktop,
            profile_name: env.profile_name,
        };
        if let Some(id) = out.take(self.page.ui(ui, &mut cx)) {
            self.edit(ui.ctx(), store, &id);
        }
        out
    }

    /// Draws the editor and the colour dialog, when they are open.
    pub fn show_windows(&mut self, ctx: &Context, store: &mut ThemeStore, env: &Env<'_>) -> Out {
        let mut out = Out::default();
        if let Some(editor) = &mut self.editor {
            let result = editor.show(ctx, store, &mut self.picker, env);
            out.files.extend(result.files);
            out.suspend_keys = result.suspend_keys.or(out.suspend_keys);
            if editor.finished {
                self.editor = None;
            }
        }
        let near = self.near;
        for event in self.picker.show(ctx) {
            self.picked(&event, store, env, near);
        }
        out
    }

    fn picked(&mut self, event: &PickerEvent, store: &mut ThemeStore, env: &Env<'_>, near: egui::Rect) {
        if self.editor.as_mut().is_some_and(|editor| editor.picked(event)) {
            return;
        }
        let mut cx = PageCx { store, picker: &mut self.picker, near, profile_name: env.profile_name };
        self.page.picked(event, &mut cx);
    }

    /// A file dialog the page or the editor asked for came back.
    pub fn file_chosen(
        &mut self,
        job: &FileJob,
        path: Option<PathBuf>,
        store: &mut ThemeStore,
        env: &Env<'_>,
    ) -> Out {
        let mut out = Out::default();
        let near = self.near;
        if let (FileJob::Browse(field), Some(editor)) = (job, &mut self.editor) {
            editor.file_chosen(field, path, near);
            return out;
        }
        let mut page_out = PageOut::default();
        let mut cx = PageCx { store, picker: &mut self.picker, near, profile_name: env.profile_name };
        self.page.file_chosen(job, path, &mut cx, &mut page_out);
        out.take(page_out);
        out
    }
}

/// The screen a window is on, or the window itself where the system does not say.
fn work_area(ctx: &Context, near: egui::Rect) -> egui::Rect {
    if ctx.embed_viewports() {
        return ctx.content_rect();
    }
    crate::window::screens()
        .into_iter()
        .map(|s| s.rect)
        .find(|r| r.contains(near.center()))
        .unwrap_or_else(|| egui::Rect::from_center_size(near.center(), egui::vec2(1600.0, 1000.0)))
}
