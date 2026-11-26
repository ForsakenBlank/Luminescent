//! The Themes window: the page of theme cards, the colour dialog it shares with the theme editor, and the
//! editor itself. The overlay has no page switcher until phase 5, so for now this is a floating window.

use std::path::PathBuf;

use egui::Context;
use lum_theme::ThemeStore;

use crate::panel::{FloatingPanel, Geometry};
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
    panel: FloatingPanel,
    page: ThemesPage,
    picker: ColourDialog,
    editor: Option<ThemeEditor>,
}

impl ThemesUi {
    /// `picker_file` is `picker.json`, `eyedropper_file` the screen colour picker's `colours.json`.
    pub fn new(picker_file: PathBuf, eyedropper_file: PathBuf) -> ThemesUi {
        ThemesUi {
            panel: FloatingPanel::new(
                "themes",
                "Themes",
                Geometry { x: 160.0, y: 90.0, w: 1040.0, h: 720.0 },
            )
            .min_size(egui::vec2(640.0, 420.0)),
            page: ThemesPage::new(),
            picker: ColourDialog::new(picker_file, eyedropper_file),
            editor: None,
        }
    }

    pub fn open(&mut self) {
        self.panel.open();
    }

    pub fn is_open(&self) -> bool {
        self.panel.is_open() || self.editor.is_some() || self.picker.is_open()
    }

    pub fn picker(&mut self) -> &mut ColourDialog {
        &mut self.picker
    }

    pub fn editor(&mut self) -> Option<&mut ThemeEditor> {
        self.editor.as_mut()
    }

    /// Opens the editor on a theme, as big as the screen the Themes window is on allows.
    pub fn edit(&mut self, ctx: &Context, store: &ThemeStore, id: &str) {
        if let Some(theme) = store.get(id) {
            let area = work_area(ctx, self.panel.geometry.rect());
            self.editor = Some(ThemeEditor::new(theme, store.recent_changes(id), area));
        }
    }

    /// Draws whatever is open this frame.
    pub fn show(&mut self, ctx: &Context, store: &mut ThemeStore, env: &Env<'_>) -> Out {
        let mut out = Out::default();
        let near = self.panel.geometry.rect();
        let mut edit = None;
        {
            let ThemesUi { panel, page, picker, editor } = self;
            let response = panel.show(ctx, |ui| {
                let mut cx =
                    PageCx { store: &mut *store, picker: &mut *picker, near, profile_name: env.profile_name };
                edit = out.take(page.ui(ui, &mut cx));
            });
            // Esc closes the window unless a dialog, the colour dialog or the editor has it
            if response.escape && !page.has_dialog() && !picker.is_open() && editor.is_none() {
                panel.close();
            }
        }
        if let Some(id) = edit {
            self.edit(ctx, store, &id);
        }
        if let Some(editor) = &mut self.editor {
            let result = editor.show(ctx, store, &mut self.picker, env);
            out.files.extend(result.files);
            out.suspend_keys = result.suspend_keys.or(out.suspend_keys);
            if editor.finished {
                self.editor = None;
            }
        }
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

    /// A file dialog the window asked for came back.
    pub fn file_chosen(
        &mut self,
        job: &FileJob,
        path: Option<PathBuf>,
        store: &mut ThemeStore,
        env: &Env<'_>,
    ) -> Out {
        let mut out = Out::default();
        let near = self.panel.geometry.rect();
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
