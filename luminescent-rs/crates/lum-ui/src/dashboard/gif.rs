//! Animated GIFs for picture boxes. The frames are decoded on a thread of their own and shown by the clock, so a
//! GIF only asks for frames while its box is drawn (the overlay is up).

use std::collections::HashMap;
use std::io::BufReader;
use std::sync::mpsc::{Receiver, Sender, channel};

use egui::{ColorImage, Context, TextureHandle, TextureId, TextureOptions, Vec2};
use image::AnimationDecoder;

/// A GIF longer than this many frames stops there, which keeps a huge one from filling memory.
const MAX_FRAMES: usize = 400;
/// Frames are shrunk to fit this many pixels on the longer side.
const LARGEST: u32 = 1024;
/// A frame with no delay (or a tiny one) is shown this long, as browsers do.
const MIN_DELAY: f64 = 0.02;

type Decoded = Option<Vec<(ColorImage, f64)>>;

enum Slot {
    Loading,
    Playing {
        frames: Vec<(TextureHandle, f64)>,
        total: f64,
    },
    /// Not an animation, or it would not decode: the still picture is shown instead.
    Still,
}

pub struct Gifs {
    slots: HashMap<String, Slot>,
    tx: Sender<(String, Decoded)>,
    rx: Receiver<(String, Decoded)>,
}

impl std::fmt::Debug for Gifs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gifs").field("known", &self.slots.len()).finish()
    }
}

impl Default for Gifs {
    fn default() -> Gifs {
        let (tx, rx) = channel();
        Gifs { slots: HashMap::new(), tx, rx }
    }
}

impl Gifs {
    /// The frame of the GIF at `path` to show now, with its size. `None` while it loads and for a file that is
    /// not an animation, so the caller shows the still picture.
    pub fn frame(&mut self, ctx: &Context, path: &str) -> Option<(TextureId, Vec2)> {
        while let Ok((done, frames)) = self.rx.try_recv() {
            let slot = match frames {
                Some(frames) if frames.len() > 1 => {
                    let total = frames.iter().map(|(_, d)| d).sum();
                    let frames = frames
                        .into_iter()
                        .enumerate()
                        .map(|(n, (image, delay))| {
                            (
                                ctx.load_texture(format!("gif:{done}:{n}"), image, TextureOptions::LINEAR),
                                delay,
                            )
                        })
                        .collect();
                    Slot::Playing { frames, total }
                }
                _ => Slot::Still,
            };
            self.slots.insert(done, slot);
        }
        if !self.slots.contains_key(path) {
            self.slots.insert(path.to_owned(), Slot::Loading);
            let (tx, ctx, key) = (self.tx.clone(), ctx.clone(), path.to_owned());
            let name = key.clone();
            let started = std::thread::Builder::new().name("gif-decode".into()).spawn(move || {
                let frames = decode(&key);
                let _ = tx.send((key, frames));
                ctx.request_repaint();
            });
            if started.is_err() {
                self.slots.insert(name, Slot::Still);
            }
        }
        let Some(Slot::Playing { frames, total }) = self.slots.get(path) else { return None };
        let now = ctx.input(|i| i.time) % total;
        let mut at = 0.0;
        for (texture, delay) in frames {
            if now < at + delay {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64((at + delay - now).max(0.005)));
                return Some((texture.id(), texture.size_vec2()));
            }
            at += delay;
        }
        frames.first().map(|(t, _)| (t.id(), t.size_vec2()))
    }

    /// Reads the file again next time (the box was given another picture).
    pub fn forget(&mut self, path: &str) {
        self.slots.remove(path);
    }
}

fn decode(path: &str) -> Decoded {
    let file = std::fs::File::open(path).ok()?;
    let decoder = image::codecs::gif::GifDecoder::new(BufReader::new(file)).ok()?;
    let mut out = Vec::new();
    for frame in decoder.into_frames().take(MAX_FRAMES) {
        let frame = frame.ok()?;
        let (num, den) = frame.delay().numer_denom_ms();
        let delay = (f64::from(num) / f64::from(den.max(1)) / 1000.0).max(MIN_DELAY);
        let mut buffer = image::DynamicImage::ImageRgba8(frame.into_buffer());
        if buffer.width() > LARGEST || buffer.height() > LARGEST {
            buffer = buffer.resize(LARGEST, LARGEST, image::imageops::FilterType::Triangle);
        }
        let rgba = buffer.to_rgba8();
        out.push((
            ColorImage::from_rgba_unmultiplied(
                [rgba.width() as usize, rgba.height() as usize],
                rgba.as_raw(),
            ),
            delay,
        ));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Delay, Frame, RgbaImage, codecs::gif::GifEncoder};

    fn write_gif(path: &std::path::Path) {
        let frames: Vec<Frame> = [[255, 0, 0, 255], [0, 0, 255, 255]]
            .into_iter()
            .map(|rgba| {
                let image = RgbaImage::from_pixel(32, 24, image::Rgba(rgba));
                Frame::from_parts(image, 0, 0, Delay::from_numer_denom_ms(100, 1))
            })
            .collect();
        GifEncoder::new(std::fs::File::create(path).unwrap()).encode_frames(frames).unwrap();
    }

    fn frame_at(ctx: &Context, gifs: &mut Gifs, path: &str, time: f64) -> Option<(TextureId, Vec2)> {
        let mut got = None;
        let input = egui::RawInput { time: Some(time), ..Default::default() };
        let _ = crate::headless_pass(ctx, input, |ui| got = gifs.frame(ui.ctx(), path));
        got
    }

    #[test]
    fn the_frame_follows_the_clock() {
        let dir = std::env::temp_dir().join(format!("lum-gif-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("blink.gif");
        write_gif(&path);
        let path = path.to_string_lossy().into_owned();
        let ctx = Context::default();
        let mut gifs = Gifs::default();
        assert!(frame_at(&ctx, &mut gifs, &path, 0.0).is_none(), "nothing while it decodes");
        let mut first = None;
        for _ in 0..100 {
            std::thread::sleep(std::time::Duration::from_millis(20));
            first = frame_at(&ctx, &mut gifs, &path, 0.01);
            if first.is_some() {
                break;
            }
        }
        let (first, size) = first.expect("the frames came in");
        assert_eq!(size, Vec2::new(32.0, 24.0));
        let (second, _) = frame_at(&ctx, &mut gifs, &path, 0.15).unwrap();
        let (again, _) = frame_at(&ctx, &mut gifs, &path, 0.21).unwrap();
        assert_ne!(first, second, "the next frame after 100 ms");
        assert_eq!(first, again, "and it loops after 200 ms");
        std::fs::remove_dir_all(dir).ok();
    }
}
