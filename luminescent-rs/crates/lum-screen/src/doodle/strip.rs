//! The tool strip at the top of the primary screen while drawing: the tools, a divider, the fill toggle, the
//! colour, the size and ✕, laid out as Python lays them out.

use super::Tool;
use crate::IRect;

/// A button's side.
pub const BUTTON: i32 = 34;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Tool(Tool),
    Divider,
    Fill,
    Colour,
    Size,
    Close,
}

/// The parts left to right with their rectangles, centred on a screen `screen_w` wide, below `top` (what a
/// bar keeps at the top of the screen). `size_w` is the width of the "N PX" text plus its padding.
pub fn parts(screen_w: i32, top: i32, size_w: i32) -> Vec<(Part, IRect)> {
    let mut widths: Vec<(Part, i32)> = Tool::ALL.iter().map(|t| (Part::Tool(*t), BUTTON)).collect();
    widths.extend([
        (Part::Divider, 13),
        (Part::Fill, BUTTON),
        (Part::Colour, 28),
        (Part::Size, size_w),
        (Part::Close, BUTTON),
    ]);
    let total: i32 = widths.iter().map(|(_, w)| w).sum::<i32>() + 8;
    let mut x = (screen_w - total) / 2 + 4;
    let y = top + 18 + 4;
    widths
        .into_iter()
        .map(|(part, w)| {
            let r = IRect::new(x, y, w, BUTTON);
            x += w;
            (part, r)
        })
        .collect()
}

/// The strip's frame around its parts.
pub fn frame(parts: &[(Part, IRect)], top: i32) -> IRect {
    let (Some(first), Some(last)) = (parts.first(), parts.last()) else { return IRect::default() };
    IRect::new(first.1.x - 4, top + 18, last.1.right() - first.1.x + 8, BUTTON + 8)
}

/// What is under `p`: a part that does something, `Some(None)` for the strip itself (a click there does
/// nothing), `None` off the strip.
pub fn hit(parts: &[(Part, IRect)], top: i32, p: crate::Point) -> Option<Option<Part>> {
    if !frame(parts, top).contains(p) {
        return None;
    }
    let part = parts.iter().find(|(_, r)| r.contains(p)).map(|(part, _)| *part);
    Some(part.filter(|p| matches!(p, Part::Tool(_) | Part::Fill | Part::Close)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Point;

    #[test]
    fn the_strip_is_centred_under_the_bar() {
        let p = parts(1920, 40, 50);
        assert_eq!(p.len(), 12);
        let total = 7 * 34 + 13 + 34 + 28 + 50 + 34 + 8;
        assert_eq!(p[0].1, IRect::new((1920 - total) / 2 + 4, 62, 34, 34));
        let f = frame(&p, 40);
        assert_eq!((f.y, f.h, f.w), (58, 42, total));
        assert_eq!(hit(&p, 40, Point::new(p[1].1.x + 3, 70)), Some(Some(Part::Tool(Tool::Highlighter))));
        assert_eq!(hit(&p, 40, Point::new(p[9].1.x + 3, 70)), Some(None), "the colour does nothing");
        assert_eq!(hit(&p, 40, Point::new(5, 5)), None);
        assert_eq!(hit(&p, 40, Point::new(p[11].1.x + 3, 70)), Some(Some(Part::Close)));
    }
}
