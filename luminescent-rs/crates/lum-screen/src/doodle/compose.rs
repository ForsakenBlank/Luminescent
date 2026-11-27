//! "Copy the screen with the drawing": every screen side by side as the desktop has them, at the largest
//! scale among them, with each screen's blackboard and drawing on top.

use tiny_skia::{Color, FilterQuality, Pixmap, PixmapPaint, Rect, Transform};

use crate::{Frame, IRect};

/// One screen: where it is on the desktop (layout pixels), what it showed, and what to paint over it. With
/// `drawing` left out the screenshot already has the drawing in it (on Wayland the compositor copies our
/// windows too).
#[derive(Debug)]
pub struct Layer<'a> {
    pub rect: IRect,
    pub shot: Option<&'a Frame>,
    pub drawing: Option<&'a Pixmap>,
    /// The blackboard's opacity, while it is up.
    pub blackboard: Option<f32>,
}

fn to_pixmap(frame: &Frame) -> Option<Pixmap> {
    // the frames are opaque, so straight and premultiplied colours are the same bytes
    let size = tiny_skia::IntSize::from_wh(frame.width, frame.height)?;
    Pixmap::from_vec(frame.rgba.clone(), size)
}

/// The picture, `scale` real pixels to a layout pixel.
pub fn compose(layers: &[Layer<'_>], scale: f32) -> Frame {
    let virt = layers.iter().fold(IRect::default(), |acc, l| acc.union(&l.rect));
    let (w, h) = (
        ((virt.w as f32) * scale).round().max(1.0) as u32,
        ((virt.h as f32) * scale).round().max(1.0) as u32,
    );
    let Some(mut out) = Pixmap::new(w, h) else { return Frame { width: 0, height: 0, rgba: Vec::new() } };
    out.fill(Color::BLACK);
    let paint = PixmapPaint { quality: FilterQuality::Bilinear, ..PixmapPaint::default() };
    for layer in layers {
        let (x, y) = ((layer.rect.x - virt.x) as f32 * scale, (layer.rect.y - virt.y) as f32 * scale);
        let (tw, th) = (layer.rect.w as f32 * scale, layer.rect.h as f32 * scale);
        let place = |pm: &Pixmap| {
            Transform::from_row(tw / pm.width() as f32, 0.0, 0.0, th / pm.height() as f32, x, y)
        };
        if let Some(shot) = layer.shot.and_then(to_pixmap) {
            out.draw_pixmap(0, 0, shot.as_ref(), &paint, place(&shot), None);
        }
        if let Some(alpha) = layer.blackboard
            && let Some(r) = Rect::from_xywh(x, y, tw, th)
        {
            let mut fill = tiny_skia::Paint::default();
            fill.set_color(Color::from_rgba8(0, 0, 0, (alpha.clamp(0.0, 1.0) * 255.0) as u8));
            out.fill_rect(r, &fill, Transform::identity(), None);
        }
        if let Some(drawing) = layer.drawing {
            out.draw_pixmap(0, 0, drawing.as_ref(), &paint, place(drawing), None);
        }
    }
    let rgba = out.pixels().iter().flat_map(|p| {
        let c = p.demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    });
    Frame { width: w, height: h, rgba: rgba.collect() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lum_theme::colour::Rgb;

    #[test]
    fn screens_sit_side_by_side_at_the_largest_scale_with_the_drawing_on_top() {
        let left = Frame::filled(100, 50, Rgb::new(10, 20, 30));
        // a 2x screen: 200 x 100 real pixels for 100 x 50 layout ones
        let right = Frame::filled(200, 100, Rgb::new(200, 100, 0));
        let mut drawing = Pixmap::new(200, 100).expect("a pixmap");
        drawing.fill_rect(
            Rect::from_xywh(0.0, 0.0, 20.0, 20.0).expect("a rect"),
            &{
                let mut p = tiny_skia::Paint::default();
                p.set_color(Color::from_rgba8(255, 0, 0, 255));
                p
            },
            Transform::identity(),
            None,
        );
        let layers = [
            Layer { rect: IRect::new(0, 0, 100, 50), shot: Some(&left), drawing: None, blackboard: None },
            Layer {
                rect: IRect::new(100, 0, 100, 50),
                shot: Some(&right),
                drawing: Some(&drawing),
                blackboard: None,
            },
        ];
        let out = compose(&layers, 2.0);
        assert_eq!((out.width, out.height), (400, 100));
        assert_eq!(out.get(50, 50), Some(Rgb::new(10, 20, 30)));
        assert_eq!(out.get(205, 5), Some(Rgb::new(255, 0, 0)));
        assert_eq!(out.get(300, 50), Some(Rgb::new(200, 100, 0)));
    }

    #[test]
    fn the_blackboard_covers_the_screen_under_the_drawing() {
        let shot = Frame::filled(10, 10, Rgb::WHITE);
        let layers = [Layer {
            rect: IRect::new(0, 0, 10, 10),
            shot: Some(&shot),
            drawing: None,
            blackboard: Some(1.0),
        }];
        assert_eq!(compose(&layers, 1.0).get(5, 5), Some(Rgb::BLACK));
    }
}
