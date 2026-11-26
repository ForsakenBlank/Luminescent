//! Tidy up: where each box goes when the boxes are lined up (`DashboardPage._arrange`).

use lum_core::dashboard::{BOX_W, GAP, ORIGIN};

/// A box to place: its size now, and whether it is a tab of items (the only kind the grid makes uniform).
#[derive(Debug, Clone, PartialEq)]
pub struct Tile {
    pub id: String,
    pub title: String,
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
    pub is_tab: bool,
}

/// Where a box goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spot {
    pub id: String,
    pub x: i64,
    pub y: i64,
    pub w: i64,
}

#[derive(Debug, Clone, Copy)]
struct Placed {
    x: i64,
    y: i64,
    w: i64,
    h: i64,
}

/// `style` is columns, grid or rows; `order` is position, name or size. `right` is the right edge boxes may not
/// pass.
pub fn arrange(mut boxes: Vec<Tile>, x0: i64, y0: i64, right: i64, style: &str, order: &str) -> Vec<Spot> {
    match order {
        "name" => boxes.sort_by_key(|b| b.title.to_lowercase()),
        "size" => boxes.sort_by_key(|b| -(b.w * b.h)),
        // reading order: row bands from the top, then left to right
        _ => boxes.sort_by_key(|b| (b.y.div_euclid(48), b.x)),
    }
    if style == "rows" {
        return rows(&boxes, x0, y0, right);
    }
    let col = BOX_W + GAP;
    let mut placed: Vec<Placed> = Vec::new();
    let mut out = Vec::new();
    for b in &boxes {
        let w = if style == "grid" && b.is_tab { BOX_W } else { b.w };
        let h = b.h;
        let mut xs: Vec<i64> = if style == "grid" {
            let span = ((w + GAP + col - 1) / col).max(1);
            let slot = span * col - GAP;
            let count = ((right - x0 + GAP).div_euclid(col)).max(1);
            (0..count).map(|i| x0 + i * col).filter(|x| x + slot <= right).collect()
        } else {
            // the left edge, the column grid and the right edge of every box placed so far: neat lines
            let count = ((right - x0).div_euclid(col)).max(1);
            let mut xs: Vec<i64> = vec![x0];
            xs.extend((0..count).map(|i| x0 + i * col));
            xs.extend(placed.iter().map(|r| r.x + r.w + GAP));
            xs.sort_unstable();
            xs.dedup();
            xs.retain(|x| x + w <= right);
            xs
        };
        if xs.is_empty() {
            xs.push(x0);
        }
        let mut best: Option<(i64, i64)> = None;
        for x in xs {
            let y = placed
                .iter()
                .filter(|r| r.x < x + w && r.x + r.w > x)
                .map(|r| r.y + r.h + GAP)
                .max()
                .unwrap_or(y0);
            if best.is_none_or(|b| (y, x) < b) {
                best = Some((y, x));
            }
        }
        let (y, x) = best.unwrap_or((y0, x0));
        placed.push(Placed { x, y, w, h });
        out.push(Spot { id: b.id.clone(), x, y, w });
    }
    out
}

fn rows(boxes: &[Tile], x0: i64, y0: i64, right: i64) -> Vec<Spot> {
    let (mut x, mut y, mut row_h) = (x0, y0, 0);
    let mut out = Vec::new();
    for b in boxes {
        if x > x0 && x + b.w > right {
            x = x0;
            y += row_h + GAP;
            row_h = 0;
        }
        out.push(Spot { id: b.id.clone(), x, y, w: b.w });
        x += b.w + GAP;
        row_h = row_h.max(b.h);
    }
    out
}

/// Where the first box of a screen goes when everything is tidied: the switcher's screen starts below the clock.
pub fn start_of(on_host: bool, chrome_bottom: f32) -> (i64, i64) {
    if on_host {
        let below = lum_core::dashboard::snap_to(f64::from(chrome_bottom) + 28.0, lum_core::dashboard::GRID);
        (ORIGIN.0, ORIGIN.1.max(below))
    } else {
        (ORIGIN.0, 64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(id: &str, x: i64, y: i64, h: i64) -> Tile {
        Tile { id: id.into(), title: id.into(), x, y, w: BOX_W, h, is_tab: true }
    }

    fn spot(spots: &[Spot], id: &str) -> (i64, i64) {
        let s = spots.iter().find(|s| s.id == id).unwrap();
        (s.x, s.y)
    }

    #[test]
    fn columns_stack_under_the_box_above() {
        let boxes = vec![tab("a", 500, 40, 200), tab("b", 10, 40, 100), tab("c", 700, 400, 100)];
        let out = arrange(boxes, 96, 136, 1500, "columns", "position");
        assert_eq!(spot(&out, "b"), (96, 136));
        assert_eq!(spot(&out, "a"), (96 + BOX_W + GAP, 136));
        assert_eq!(spot(&out, "c"), (96 + 2 * (BOX_W + GAP), 136));
    }

    #[test]
    fn a_narrow_screen_stacks_boxes_in_one_column() {
        let boxes = vec![tab("a", 0, 0, 100), tab("b", 0, 200, 100)];
        let out = arrange(boxes, 96, 136, 96 + BOX_W + 10, "columns", "position");
        assert_eq!(spot(&out, "a"), (96, 136));
        assert_eq!(spot(&out, "b"), (96, 136 + 100 + GAP));
    }

    #[test]
    fn rows_wrap_at_the_right_edge() {
        let boxes = vec![tab("a", 0, 0, 100), tab("b", 300, 0, 150), tab("c", 600, 0, 50)];
        let out = arrange(boxes, 0, 0, 2 * BOX_W + GAP + 10, "rows", "position");
        assert_eq!(spot(&out, "b"), (BOX_W + GAP, 0));
        assert_eq!(spot(&out, "c"), (0, 150 + GAP));
    }

    #[test]
    fn the_grid_gives_tabs_one_width() {
        let mut wide = tab("a", 0, 0, 100);
        wide.w = 400;
        let out = arrange(vec![wide], 0, 0, 1000, "grid", "position");
        assert_eq!(out[0].w, BOX_W);
    }

    #[test]
    fn order_by_name_and_size() {
        let boxes = vec![tab("b", 0, 0, 100), tab("a", 0, 0, 300), tab("c", 0, 0, 200)];
        let by_name = arrange(boxes.clone(), 0, 0, 5000, "rows", "name");
        assert_eq!(by_name.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
        let by_size = arrange(boxes, 0, 0, 5000, "rows", "size");
        assert_eq!(by_size.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["a", "c", "b"]);
    }

    #[test]
    fn the_first_box_goes_below_the_clock() {
        assert_eq!(start_of(true, 40.0), (96, 136));
        assert_eq!(start_of(true, 200.0), (96, 232));
        assert_eq!(start_of(false, 200.0), (96, 64));
    }
}
