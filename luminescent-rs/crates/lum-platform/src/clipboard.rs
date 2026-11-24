//! The clipboard: text, pictures and lists of files, read and set from a hotkey.
//!
//! On Wayland a program may only touch the clipboard while one of its windows has the keyboard, which is
//! rarely true for a background tool; the data-control protocol has no such limit, so that is what is used
//! there (Hyprland has it). These calls can wait on another program, so keep them off the UI thread.

use arboard::{Clipboard, ImageData};
use std::borrow::Cow;
use std::path::{Path, PathBuf};

/// A picture, straight RGBA.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

/// What is on the clipboard right now; any of the three may be there at once.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Contents {
    pub files: Vec<PathBuf>,
    pub picture: Option<Picture>,
    pub text: Option<String>,
}

fn open() -> Result<Clipboard, String> {
    Clipboard::new().map_err(|e| e.to_string())
}

pub fn set_text(text: &str) -> Result<(), String> {
    open()?.set_text(text).map_err(|e| e.to_string())
}

/// A picture, as file managers and chat apps expect it.
pub fn set_picture(picture: &Picture) -> Result<(), String> {
    let image =
        ImageData { width: picture.width, height: picture.height, bytes: Cow::Borrowed(&picture.rgba) };
    open()?.set_image(image).map_err(|e| e.to_string())
}

/// Copied files, as file managers and chat apps expect them.
pub fn set_files(paths: &[impl AsRef<Path>]) -> Result<(), String> {
    open()?.set().file_list(paths).map_err(|e| e.to_string())
}

pub fn text() -> Option<String> {
    open().ok()?.get_text().ok()
}

/// Everything on the clipboard (files, a picture and text).
pub fn contents() -> Contents {
    let Ok(mut c) = open() else { return Contents::default() };
    let files = c.get().file_list().unwrap_or_default();
    let picture = c.get_image().ok().map(|i| Picture { width: i.width, height: i.height, rgba: i.bytes.into_owned() });
    let text = c.get_text().ok();
    Contents { files, picture, text }
}
