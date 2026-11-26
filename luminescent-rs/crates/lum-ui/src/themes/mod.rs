//! The Themes window: the page of theme cards, the colour dialog it shares with the theme editor, and the
//! editor itself. The overlay has no page switcher until phase 5, so for now this is a floating window.

use std::path::PathBuf;

use egui::Context;
use lum_theme::ThemeStore;

use crate::panel::{FloatingPanel, Geometry};
use crate::picker::{ColourDialog, PickerEvent};
use crate::themes::page::{FileAsk, FileJob, PageCx, PageOut, ThemesPage};

pub mod demo;
pub mod page;
pub mod pictures;
pub mod previews;

/// What the window needs to know about the rest of the app.
pub struct Env<'a> {
    /// The settings profile's name for its id.
    pub profile_name: &'a dyn Fn(&str) -> String,
}

/// What the app has to do after a frame of the window.
#[derive(Debug, Default)]
pub struct Out {
    pub files: Vec<FileAsk>,
    /// Desktop notifications: (title, text).
    pub notify: Vec<(String, String)>,
}

impl Out {
    fn take(&mut self, page: PageOut) {
        self.files.extend(page.files);
        self.notify.extend(page.notify);
    }
}

#[derive(Debug)]
pub struct ThemesUi {
    panel: FloatingPanel,
    page: ThemesPage,
    picker: ColourDialog,
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
        }
    }

    pub fn open(&mut self) {
        self.panel.open();
    }

    pub fn is_open(&self) -> bool {
        self.panel.is_open()
    }

    pub fn picker(&mut self) -> &mut ColourDialog {
        &mut self.picker
    }

    /// Draws whatever is open this frame.
    pub fn show(&mut self, ctx: &Context, store: &mut ThemeStore, env: &Env<'_>) -> Out {
        let mut out = Out::default();
        let near = self.panel.geometry.rect();
        let ThemesUi { panel, page, picker } = self;
        let response = panel.show(ctx, |ui| {
            let mut cx =
                PageCx { store: &mut *store, picker: &mut *picker, near, profile_name: env.profile_name };
            out.take(page.ui(ui, &mut cx));
        });
        // Esc closes the window unless a dialog of the page or the colour dialog has it
        if response.escape && !page.has_dialog() && !picker.is_open() {
            panel.close();
        }
        for event in picker.show(ctx) {
            self.picked(&event, store, env, near, &mut out);
        }
        out
    }

    fn picked(
        &mut self,
        event: &PickerEvent,
        store: &mut ThemeStore,
        env: &Env<'_>,
        near: egui::Rect,
        out: &mut Out,
    ) {
        let mut cx = PageCx { store, picker: &mut self.picker, near, profile_name: env.profile_name };
        self.page.picked(event, &mut cx);
        let _ = out;
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
        let mut page_out = PageOut::default();
        let mut cx = PageCx { store, picker: &mut self.picker, near, profile_name: env.profile_name };
        self.page.file_chosen(job, path, &mut cx, &mut page_out);
        out.take(page_out);
        out
    }
}
