//! Theme pictures (a card icon, the background image) as textures, decoded on a thread of their own so a big
//! wallpaper never holds up a frame.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use egui::{ColorImage, Context, TextureHandle, TextureOptions};

/// Pictures are shown at card size, so they are shrunk to this (twice the card, for sharp screens).
const LARGEST: u32 = 480;

enum Slot {
    Loading,
    Ready(TextureHandle),
    Missing,
}

pub struct Pictures {
    slots: HashMap<PathBuf, Slot>,
    tx: Sender<(PathBuf, Option<ColorImage>)>,
    rx: Receiver<(PathBuf, Option<ColorImage>)>,
}

impl std::fmt::Debug for Pictures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pictures").field("loaded", &self.slots.len()).finish()
    }
}

impl Default for Pictures {
    fn default() -> Pictures {
        let (tx, rx) = channel();
        Pictures { slots: HashMap::new(), tx, rx }
    }
}

impl Pictures {
    /// The picture at `path`, once it is loaded; asks for it the first time. `None` while it loads, and for a
    /// file that is gone or not a picture.
    pub fn get(&mut self, ctx: &Context, path: &str) -> Option<&TextureHandle> {
        if path.is_empty() {
            return None;
        }
        let key = PathBuf::from(path);
        while let Ok((done, image)) = self.rx.try_recv() {
            let slot = match image {
                Some(image) => Slot::Ready(ctx.load_texture(
                    format!("theme-picture:{}", done.display()),
                    image,
                    TextureOptions::LINEAR,
                )),
                None => Slot::Missing,
            };
            self.slots.insert(done, slot);
        }
        if !self.slots.contains_key(&key) {
            self.slots.insert(key.clone(), Slot::Loading);
            let (tx, ctx, path) = (self.tx.clone(), ctx.clone(), key.clone());
            let started = std::thread::Builder::new().name("theme-picture".into()).spawn(move || {
                let _ = tx.send((path.clone(), decode(&path)));
                ctx.request_repaint();
            });
            if started.is_err() {
                self.slots.insert(key.clone(), Slot::Missing);
            }
        }
        match self.slots.get(&key) {
            Some(Slot::Ready(texture)) => Some(texture),
            _ => None,
        }
    }

    /// Forgets a picture, so it is read again (the file changed).
    pub fn forget(&mut self, path: &str) {
        self.slots.remove(Path::new(path));
    }
}

fn decode(path: &Path) -> Option<ColorImage> {
    let image = image::open(path).ok()?;
    let image = if image.width() > LARGEST || image.height() > LARGEST {
        image.resize(LARGEST, LARGEST, image::imageops::FilterType::Triangle)
    } else {
        image
    };
    let rgba = image.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some(ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
}
