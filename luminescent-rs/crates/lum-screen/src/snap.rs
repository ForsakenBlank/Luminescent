//! The window snapper's geometry: where each spot is inside a screen's usable area, the gaps, and what
//! pressing the same direction again moves on to.

use std::collections::HashMap;

/// A spot as fractions of the usable screen: x, y, width, height.
pub type Fractions = (f64, f64, f64, f64);

/// Every spot, its label and where it is, in the order the palette lists them.
pub const SPOTS: [(&str, &str, Fractions); 15] = [
    ("left", "Left half", (0.0, 0.0, 0.5, 1.0)),
    ("right", "Right half", (0.5, 0.0, 0.5, 1.0)),
    ("top", "Top half", (0.0, 0.0, 1.0, 0.5)),
    ("bottom", "Bottom half", (0.0, 0.5, 1.0, 0.5)),
    ("left_third", "Left third", (0.0, 0.0, 1.0 / 3.0, 1.0)),
    ("centre_third", "Centre third", (1.0 / 3.0, 0.0, 1.0 / 3.0, 1.0)),
    ("right_third", "Right third", (2.0 / 3.0, 0.0, 1.0 / 3.0, 1.0)),
    ("left_two_thirds", "Left two thirds", (0.0, 0.0, 2.0 / 3.0, 1.0)),
    ("right_two_thirds", "Right two thirds", (1.0 / 3.0, 0.0, 2.0 / 3.0, 1.0)),
    ("top_left", "Top-left quarter", (0.0, 0.0, 0.5, 0.5)),
    ("top_right", "Top-right quarter", (0.5, 0.0, 0.5, 0.5)),
    ("bottom_left", "Bottom-left quarter", (0.0, 0.5, 0.5, 0.5)),
    ("bottom_right", "Bottom-right quarter", (0.5, 0.5, 0.5, 0.5)),
    ("maximise", "Fill the screen", (0.0, 0.0, 1.0, 1.0)),
    ("centre", "Centre, large", (0.15, 0.1, 0.7, 0.8)),
];

/// What pressing a direction again moves on to.
pub const CYCLES: [(&str, &[&str]); 4] = [
    ("left", &["left", "left_third", "left_two_thirds"]),
    ("right", &["right", "right_third", "right_two_thirds"]),
    ("up", &["top", "maximise"]),
    ("down", &["bottom", "centre"]),
];

/// The spots with a hotkey of their own (besides the four directions).
pub const SPOT_KEYS: [&str; 5] = ["top_left", "top_right", "bottom_left", "bottom_right", "centre_third"];

pub fn label(spot: &str) -> Option<&'static str> {
    SPOTS.iter().find(|(id, _, _)| *id == spot).map(|(_, label, _)| *label)
}

pub fn cycle(direction: &str) -> Option<&'static [&'static str]> {
    CYCLES.iter().find(|(d, _)| *d == direction).map(|(_, spots)| *spots)
}

/// A rectangle: x, y, width, height.
pub type Area = (i32, i32, i32, i32);

/// A Hyprland monitor's area in layout pixels, minus what bars and docks reserve. `width` and `height` are
/// the mode in real pixels; `reserved` is left, top, right, bottom.
pub fn usable_area(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f64,
    transform: u32,
    reserved: [i32; 4],
) -> Area {
    let scale = if scale > 0.0 { scale } else { 1.0 };
    // int() in Python cuts towards zero; the sizes are never negative
    let (mut w, mut h) = ((f64::from(width) / scale) as i32, (f64::from(height) / scale) as i32);
    if transform % 2 == 1 {
        std::mem::swap(&mut w, &mut h);
    }
    let [left, top, right, bottom] = reserved;
    (x + left, y + top, w - left - right, h - top - bottom)
}

/// Where `spot` is inside `area`, with `gap` pixels around it (half a gap between two snapped windows).
pub fn spot_rect(area: Area, spot: &str, gap: i32) -> Option<Area> {
    let (_, _, (fx, fy, fw, fh)) = *SPOTS.iter().find(|(id, _, _)| *id == spot)?;
    let (ax, ay, aw, ah) = area;
    let at = |base: i32, size: i32, f: f64| base + (f64::from(size) * f).round_ties_even() as i32;
    let (x0, y0) = (at(ax, aw, fx), at(ay, ah, fy));
    let (x1, y1) = (at(ax, aw, fx + fw), at(ay, ah, fy + fh));
    let (g, half) = (gap, gap / 2);
    let left = if fx == 0.0 { g } else { half };
    let top = if fy == 0.0 { g } else { half };
    let right = if (fx + fw - 1.0).abs() < 1e-6 { g } else { half };
    let bottom = if (fy + fh - 1.0).abs() < 1e-6 { g } else { half };
    Some((x0 + left, y0 + top, (x1 - x0 - left - right).max(50), (y1 - y0 - top - bottom).max(50)))
}

/// Which step of its cycle each window is on, so pressing a direction again moves on.
#[derive(Debug, Default)]
pub struct Cycler {
    last: HashMap<String, (String, usize)>,
}

impl Cycler {
    /// The spot a direction press puts this window in: the first of the cycle, or the next one when the
    /// last press for this window was the same direction.
    pub fn step(&mut self, window: &str, direction: &str) -> Option<&'static str> {
        let spots = cycle(direction)?;
        let n = match self.last.get(window) {
            Some((d, n)) if d == direction => (n + 1) % spots.len(),
            _ => 0,
        };
        self.last.insert(window.to_owned(), (direction.to_owned(), n));
        Some(spots[n])
    }

    /// The window went back into tiling: the next press starts over.
    pub fn forget(&mut self, window: &str) {
        self.last.remove(window);
    }
}

/// Windows: the outer rectangle to give `SetWindowPos` so the visible frame lands on `rect`, given the
/// invisible resize borders (left, top, right, bottom) Windows 10 and 11 put around a window.
pub fn with_borders(rect: Area, borders: [i32; 4]) -> Area {
    let (x, y, w, h) = rect;
    let [l, t, r, b] = borders;
    (x - l, y - t, w + l + r, h + t + b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spots_leave_a_full_gap_at_the_edges_and_half_between() {
        let area = (0, 30, 1920, 1050);
        assert_eq!(spot_rect(area, "left", 8), Some((8, 38, 948, 1034)));
        assert_eq!(spot_rect(area, "right", 8), Some((964, 38, 948, 1034)));
        assert_eq!(spot_rect(area, "maximise", 8), Some((8, 38, 1904, 1034)));
        assert_eq!(spot_rect(area, "centre", 0), Some((288, 135, 1344, 840)));
        assert_eq!(spot_rect(area, "left_third", 8), Some((8, 38, 628, 1034)));
        assert_eq!(spot_rect(area, "centre_third", 8), Some((644, 38, 632, 1034)));
        assert_eq!(spot_rect(area, "bottom_right", 10), Some((965, 560, 945, 510)));
        assert_eq!(spot_rect(area, "nowhere", 8), None);
        // never smaller than 50 pixels
        assert_eq!(spot_rect((0, 0, 60, 60), "top_left", 30), Some((30, 30, 50, 50)));
    }

    #[test]
    fn the_usable_area_is_in_layout_pixels_without_the_bars() {
        // a 4K monitor at 1.5 with a 40 px bar on top, to the right of a 1920 one
        assert_eq!(usable_area(1920, 0, 3840, 2160, 1.5, 0, [0, 40, 0, 0]), (1920, 40, 2560, 1400));
        // turned on its side
        assert_eq!(usable_area(0, 0, 1920, 1080, 1.0, 1, [0, 0, 0, 0]), (0, 0, 1080, 1920));
    }

    #[test]
    fn pressing_a_direction_again_steps_through_its_sizes() {
        let mut c = Cycler::default();
        assert_eq!(c.step("0x1", "left"), Some("left"));
        assert_eq!(c.step("0x1", "left"), Some("left_third"));
        assert_eq!(c.step("0x1", "left"), Some("left_two_thirds"));
        assert_eq!(c.step("0x1", "left"), Some("left"));
        // another window starts at the beginning, and so does another direction
        assert_eq!(c.step("0x2", "left"), Some("left"));
        assert_eq!(c.step("0x1", "up"), Some("top"));
        assert_eq!(c.step("0x1", "up"), Some("maximise"));
        c.forget("0x1");
        assert_eq!(c.step("0x1", "up"), Some("top"));
        assert_eq!(c.step("0x1", "sideways"), None);
        assert_eq!(label("centre"), Some("Centre, large"));
    }

    #[test]
    fn windows_borders_are_added_around_the_frame() {
        assert_eq!(with_borders((8, 8, 952, 1034), [7, 0, 7, 7]), (1, 8, 966, 1041));
    }
}
