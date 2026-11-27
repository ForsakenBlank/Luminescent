//! Thumbnails for picture and video cards, made off the UI thread. A picture is decoded with the `image` crate
//! (turned the right way up by its EXIF tag) and shrunk to fit 720 x 520; a video's first frame comes from
//! `ffmpeg` when it is installed.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use egui::{ColorImage, Context, TextureHandle, TextureOptions, Vec2};
use image::imageops::FilterType;

const FIT: (u32, u32) = (720, 520);

/// What the worker made for one file: `None` when it could not be read.
pub struct Made {
    pub path: PathBuf,
    pub modified: Option<SystemTime>,
    pub image: Option<ColorImage>,
}

fn to_colour_image(image: image::DynamicImage) -> ColorImage {
    let image = if image.width() > FIT.0 || image.height() > FIT.1 {
        image.resize(FIT.0, FIT.1, FilterType::Triangle)
    } else {
        image
    };
    let rgba = image.to_rgba8();
    ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw())
}

fn picture(path: &Path) -> Option<ColorImage> {
    let reader = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?;
    let mut decoder = reader.into_decoder().ok()?;
    let orientation = image::ImageDecoder::orientation(&mut decoder).ok();
    let mut image = image::DynamicImage::from_decoder(decoder).ok()?;
    if let Some(orientation) = orientation {
        image.apply_orientation(orientation);
    }
    Some(to_colour_image(image))
}

fn video(path: &Path) -> Option<ColorImage> {
    let png = lum_platform::frame::video_frame(path)?;
    Some(to_colour_image(image::load_from_memory(&png).ok()?))
}

/// Makes the thumbnails in order, giving each to `deliver` as it is done, until `stop` is set.
pub fn make_all(jobs: Vec<(PathBuf, bool)>, stop: Arc<AtomicBool>, deliver: impl Fn(Made) + Send + 'static) {
    let spawned = std::thread::Builder::new().name("thumbnails".into()).spawn(move || {
        for (path, is_video) in jobs {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            let image = if is_video { video(&path) } else { picture(&path) };
            if image.is_none() {
                tracing::debug!("no thumbnail for {}", path.display());
            }
            if !stop.load(Ordering::Relaxed) {
                deliver(Made { path, modified, image });
            }
        }
    });
    if let Err(e) = spawned {
        tracing::warn!("could not start the thumbnail thread: {e}");
    }
}

pub struct Thumb {
    pub modified: Option<SystemTime>,
    pub texture: TextureHandle,
    pub size: Vec2,
}

impl std::fmt::Debug for Thumb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Thumb").field("size", &self.size).finish_non_exhaustive()
    }
}

/// The thumbnails made so far (the pop-up and the page share them), and the ones being made.
#[derive(Debug, Default)]
pub struct Thumbs {
    made: HashMap<PathBuf, Thumb>,
    /// Asked for and not back yet, or tried and failed (until the file changes).
    asked: HashSet<PathBuf>,
    stop: Option<Arc<AtomicBool>>,
}

impl Thumbs {
    pub fn get(&self, path: &Path) -> Option<&Thumb> {
        self.made.get(path)
    }

    /// Whether this file still needs a thumbnail (none yet, or it changed since).
    pub fn wants(&self, path: &Path, modified: Option<SystemTime>) -> bool {
        match self.made.get(path) {
            Some(t) => t.modified != modified,
            None => !self.asked.contains(path),
        }
    }

    /// Starts making thumbnails for `jobs` that are still wanted; an earlier batch is told to stop.
    pub fn ask(
        &mut self,
        jobs: Vec<(PathBuf, bool, Option<SystemTime>)>,
        deliver: impl Fn(Made) + Send + 'static,
    ) {
        let jobs: Vec<(PathBuf, bool)> =
            jobs.into_iter().filter(|(p, _, m)| self.wants(p, *m)).map(|(p, v, _)| (p, v)).collect();
        if jobs.is_empty() {
            return;
        }
        self.stop();
        for (path, _) in &jobs {
            self.asked.insert(path.clone());
        }
        let stop = Arc::new(AtomicBool::new(false));
        self.stop = Some(stop.clone());
        make_all(jobs, stop, deliver);
    }

    pub fn arrived(&mut self, ctx: &Context, made: Made) {
        let Some(image) = made.image else { return };
        let size = Vec2::new(image.size[0] as f32, image.size[1] as f32);
        let texture =
            ctx.load_texture(format!("snippet {}", made.path.display()), image, TextureOptions::LINEAR);
        self.made.insert(made.path, Thumb { modified: made.modified, texture, size });
    }

    /// Stops the batch being made, and forgets what was asked of it so the next look asks again.
    pub fn stop(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop.store(true, Ordering::Relaxed);
            self.asked.retain(|p| self.made.contains_key(p));
        }
    }

    pub fn clear(&mut self) {
        self.stop();
        self.made.clear();
        self.asked.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_png(path: &Path, w: u32, h: u32) {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([200, 30, 30, 255]));
        img.save(path).expect("the picture is saved");
    }

    #[test]
    fn a_big_picture_is_shrunk_to_fit_and_keeps_its_shape() {
        let dir = tempfile::tempdir().expect("a temp folder");
        let path = dir.path().join("big.png");
        write_png(&path, 1440, 520);
        let image = picture(&path).expect("decoded");
        assert_eq!(image.size, [720, 260]);
    }

    #[test]
    fn a_small_picture_is_left_alone() {
        let dir = tempfile::tempdir().expect("a temp folder");
        let path = dir.path().join("small.png");
        write_png(&path, 40, 30);
        assert_eq!(picture(&path).expect("decoded").size, [40, 30]);
    }

    #[test]
    fn a_file_that_is_not_a_picture_gives_nothing() {
        let dir = tempfile::tempdir().expect("a temp folder");
        let path = dir.path().join("liar.png");
        std::fs::write(&path, b"not a picture").expect("written");
        assert!(picture(&path).is_none());
    }

    #[test]
    fn the_worker_delivers_each_file_and_stops_when_told() {
        let dir = tempfile::tempdir().expect("a temp folder");
        let (a, b) = (dir.path().join("a.png"), dir.path().join("b.png"));
        write_png(&a, 8, 8);
        std::fs::write(&b, b"bad").expect("written");
        let (tx, rx) = std::sync::mpsc::channel();
        make_all(vec![(a.clone(), false), (b.clone(), false)], Arc::new(AtomicBool::new(false)), move |m| {
            let _ = tx.send(m);
        });
        let first = rx.recv_timeout(std::time::Duration::from_secs(5)).expect("the first arrives");
        assert_eq!((first.path, first.image.is_some()), (a, true));
        let second = rx.recv_timeout(std::time::Duration::from_secs(5)).expect("the second arrives");
        assert_eq!((second.path, second.image.is_some()), (b, false));
        let (tx, rx) = std::sync::mpsc::channel();
        make_all(vec![(dir.path().join("a.png"), false)], Arc::new(AtomicBool::new(true)), move |m| {
            let _ = tx.send(m);
        });
        assert!(rx.recv_timeout(std::time::Duration::from_millis(300)).is_err());
    }
}
