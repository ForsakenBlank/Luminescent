//! One screen as it looked when a tool froze it: straight RGBA at the screen's real pixels, which on a scaled
//! monitor is more than its size in layout pixels.

use lum_theme::colour::Rgb;

use crate::{IRect, Point};

#[derive(Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    /// Four bytes a pixel, row after row, red first.
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Frame({}x{})", self.width, self.height)
    }
}

impl Frame {
    /// `None` when the bytes do not fill the size.
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Option<Frame> {
        (rgba.len() == width as usize * height as usize * 4).then_some(Frame { width, height, rgba })
    }

    /// One colour all over (tests and the gallery's stand-in screens).
    pub fn filled(width: u32, height: u32, c: Rgb) -> Frame {
        let rgba = [c.r, c.g, c.b, 255].repeat(width as usize * height as usize);
        Frame { width, height, rgba }
    }

    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// The colour of a real pixel; outside the frame there is none.
    pub fn get(&self, x: i64, y: i64) -> Option<Rgb> {
        if x < 0 || y < 0 || x >= i64::from(self.width) || y >= i64::from(self.height) {
            return None;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        Some(Rgb::new(self.rgba[i], self.rgba[i + 1], self.rgba[i + 2]))
    }

    /// Real pixels per layout pixel, from how big the screen is in layout pixels (`width`).
    pub fn ratio(&self, layout_width: f64) -> f64 {
        if layout_width <= 0.0 || self.width == 0 { 1.0 } else { f64::from(self.width) / layout_width }
    }

    /// The colour under a point in layout pixels, clamped to the frame as Python's `pixel()` clamps it.
    pub fn at(&self, p: Point, ratio: f64) -> Rgb {
        let x = ((f64::from(p.x) * ratio) as i64).min(i64::from(self.width) - 1).max(0);
        let y = ((f64::from(p.y) * ratio) as i64).min(i64::from(self.height) - 1).max(0);
        self.get(x, y).unwrap_or(Rgb::BLACK)
    }

    /// A part of the frame in real pixels, cut to the frame's edges.
    pub fn crop(&self, area: IRect) -> Frame {
        let area = area.clipped(&IRect::new(0, 0, self.width as i32, self.height as i32));
        if area.is_empty() {
            return Frame { width: 0, height: 0, rgba: Vec::new() };
        }
        let mut rgba = Vec::with_capacity(area.w as usize * area.h as usize * 4);
        for y in area.y..area.bottom() {
            let start = (y as usize * self.width as usize + area.x as usize) * 4;
            rgba.extend_from_slice(&self.rgba[start..start + area.w as usize * 4]);
        }
        Frame { width: area.w as u32, height: area.h as u32, rgba }
    }

    /// The same picture as an `image` buffer (for scaling and saving).
    pub fn to_image(&self) -> image::RgbaImage {
        image::RgbaImage::from_raw(self.width, self.height, self.rgba.clone())
            .unwrap_or_else(|| image::RgbaImage::new(0, 0))
    }

    /// The picture as PNG bytes (what OCR engines read).
    pub fn png(&self) -> Result<Vec<u8>, String> {
        let mut out = std::io::Cursor::new(Vec::new());
        self.to_image().write_to(&mut out, image::ImageFormat::Png).map_err(|e| e.to_string())?;
        Ok(out.into_inner())
    }

    pub fn from_image(image: image::RgbaImage) -> Frame {
        let (width, height) = image.dimensions();
        Frame { width, height, rgba: image.into_raw() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: u32, h: u32) -> Frame {
        let mut rgba = Vec::new();
        for y in 0..h {
            for x in 0..w {
                rgba.extend_from_slice(&[x as u8, y as u8, 0, 255]);
            }
        }
        Frame::new(w, h, rgba).expect("the right size")
    }

    #[test]
    fn a_layout_point_reads_the_real_pixel_on_a_scaled_screen() {
        // a 1.5x monitor: 200 real pixels for 133 layout ones
        let f = gradient(200, 100);
        let ratio = f.ratio(200.0 / 1.5);
        assert_eq!(f.at(Point::new(10, 4), ratio), Rgb::new(15, 6, 0));
        // clamped at the edges, as Python's pixel() does
        assert_eq!(f.at(Point::new(500, -3), ratio), Rgb::new(199, 0, 0));
    }

    #[test]
    fn crops_stay_inside_the_frame() {
        let f = gradient(20, 10);
        let c = f.crop(IRect::new(15, 5, 10, 10));
        assert_eq!((c.width, c.height), (5, 5));
        assert_eq!(c.get(0, 0), Some(Rgb::new(15, 5, 0)));
        assert!(f.crop(IRect::new(30, 0, 5, 5)).is_empty());
        assert_eq!(Frame::new(2, 2, vec![0; 3]), None);
    }
}
