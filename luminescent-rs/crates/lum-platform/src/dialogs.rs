//! The system's file dialogs: the desktop portal on Linux (so they match the user's desktop and work on
//! Wayland), the common dialogs on Windows.
//!
//! A dialog waits for the person to answer, so it runs on a thread of its own and hands the answer to a
//! callback; the UI thread is never blocked (D30).

use std::path::PathBuf;

/// What a dialog asks: a title, a starting folder and the kinds of file to show.
#[derive(Debug, Clone, Default)]
pub struct Request {
    pub title: String,
    pub folder: Option<PathBuf>,
    /// A name suggested in a save dialog.
    pub file_name: String,
    /// (name, extensions without the dot) pairs, like `("Luminescent themes", ["lumtheme"])`.
    pub filters: Vec<(String, Vec<String>)>,
}

impl Request {
    pub fn new(title: &str) -> Request {
        Request { title: title.to_owned(), ..Request::default() }
    }

    pub fn folder(mut self, folder: Option<PathBuf>) -> Request {
        self.folder = folder;
        self
    }

    pub fn file_name(mut self, name: &str) -> Request {
        self.file_name = name.to_owned();
        self
    }

    pub fn filter(mut self, name: &str, extensions: &[&str]) -> Request {
        self.filters.push((name.to_owned(), extensions.iter().map(|e| (*e).to_owned()).collect()));
        self
    }

    fn build(&self) -> rfd::FileDialog {
        let mut dialog = rfd::FileDialog::new().set_title(&self.title);
        if let Some(folder) = &self.folder {
            dialog = dialog.set_directory(folder);
        }
        if !self.file_name.is_empty() {
            dialog = dialog.set_file_name(&self.file_name);
        }
        for (name, extensions) in &self.filters {
            dialog = dialog.add_filter(name, extensions);
        }
        dialog
    }
}

/// Asks for a file to open. `done` gets the path, or `None` when the dialog was cancelled.
pub fn open_file(request: Request, done: impl FnOnce(Option<PathBuf>) + Send + 'static) {
    spawn(move || request.build().pick_file(), done);
}

/// Asks for a folder.
pub fn pick_folder(request: Request, done: impl FnOnce(Option<PathBuf>) + Send + 'static) {
    spawn(move || request.build().pick_folder(), done);
}

/// Asks where to save a file.
pub fn save_file(request: Request, done: impl FnOnce(Option<PathBuf>) + Send + 'static) {
    spawn(move || request.build().save_file(), done);
}

fn spawn(
    ask: impl FnOnce() -> Option<PathBuf> + Send + 'static,
    done: impl FnOnce(Option<PathBuf>) + Send + 'static,
) {
    let started = std::thread::Builder::new().name("file-dialog".into()).spawn(move || done(ask()));
    if let Err(e) = started {
        tracing::warn!("could not open a file dialog: {e}");
    }
}
