//! The real icons of items (a program's icon, a file type's, a folder's), looked up and decoded on a thread of
//! their own: finding an icon can walk several icon theme folders, and no frame waits for that.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};

use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use lum_platform::apps::{self, IconSource};
use lum_platform::runner;

const SIZE: u16 = 18;

enum Slot {
    Loading,
    Ready(TextureHandle),
    None,
}

pub struct Icons {
    slots: HashMap<String, Slot>,
    tx: Sender<(String, Option<ColorImage>)>,
    rx: Receiver<(String, Option<ColorImage>)>,
}

impl std::fmt::Debug for Icons {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Icons").field("known", &self.slots.len()).finish()
    }
}

impl Default for Icons {
    fn default() -> Icons {
        let (tx, rx) = channel();
        Icons { slots: HashMap::new(), tx, rx }
    }
}

impl Icons {
    /// The icon for what an item opens, once it is found. `None` while looking, and when there is none (the row
    /// shows its type badge then).
    pub fn get(&mut self, ctx: &Context, target: &str) -> Option<&TextureHandle> {
        while let Ok((done, image)) = self.rx.try_recv() {
            let slot = match image {
                Some(image) => {
                    Slot::Ready(ctx.load_texture(format!("icon:{done}"), image, TextureOptions::LINEAR))
                }
                None => Slot::None,
            };
            self.slots.insert(done, slot);
        }
        if !self.slots.contains_key(target) {
            self.slots.insert(target.to_owned(), Slot::Loading);
            let (tx, ctx, key) = (self.tx.clone(), ctx.clone(), target.to_owned());
            let name = key.clone();
            let started = std::thread::Builder::new().name("item-icon".into()).spawn(move || {
                let image = find(&key);
                let _ = tx.send((key, image));
                ctx.request_repaint();
            });
            if started.is_err() {
                self.slots.insert(name, Slot::None);
            }
        }
        match self.slots.get(target) {
            Some(Slot::Ready(texture)) => Some(texture),
            _ => None,
        }
    }
}

fn find(target: &str) -> Option<ColorImage> {
    let source = apps::icon_for(target, runner::kind_of(target), SIZE * 2)?;
    match source {
        IconSource::File(path) => {
            // the image crate has no SVG; those rows keep their badge
            let image = image::open(path).ok()?;
            let image =
                image.resize(u32::from(SIZE) * 2, u32::from(SIZE) * 2, image::imageops::FilterType::Triangle);
            let rgba = image.to_rgba8();
            Some(ColorImage::from_rgba_unmultiplied(
                [rgba.width() as usize, rgba.height() as usize],
                rgba.as_raw(),
            ))
        }
        IconSource::Pixels { size, rgba } => {
            let size = size as usize;
            (rgba.len() == size * size * 4).then(|| ColorImage::from_rgba_unmultiplied([size, size], &rgba))
        }
    }
}
