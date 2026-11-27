//! Edit mode for tools whose panels sit on the desktop: a bar of nine anchors in a window of its own, at the top
//! of the screen with the mouse, that snaps every panel of the tool to a corner or edge.

use egui::{Context, Id, Rect, pos2, vec2};

use crate::widgets::{Anchor, anchor_bar};

const SIZE: egui::Vec2 = vec2(420.0, 44.0);
const TITLE: &str = "Luminescent anchor bar";

/// Draws the bar for this frame; gives back the anchor that was clicked and the screen it is on.
pub fn anchor_window(ctx: &Context, id: &str, hint: &str, screen: Rect) -> Option<Anchor> {
    let at = pos2((screen.center().x - SIZE.x / 2.0).round(), screen.min.y + 24.0);
    let mut chosen = None;
    if ctx.embed_viewports() {
        egui::Area::new(Id::unique(("anchor-bar", id))).order(egui::Order::Foreground).fixed_pos(at).show(
            ctx,
            |ui| {
                chosen = anchor_bar(ui, hint);
            },
        );
        return chosen;
    }
    let title = format!("{TITLE} {id}{}", lum_platform::desktop::marker::NO_ACTIVATE);
    let builder = egui::ViewportBuilder::default()
        .with_title(title.clone())
        .with_app_id(lum_platform::desktop::APP_ID)
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top()
        .with_taskbar(false)
        .with_active(false)
        .with_resizable(false)
        .with_position(at)
        .with_inner_size(SIZE);
    ctx.show_viewport_immediate(egui::ViewportId::from_hash_of(("anchor-bar", id)), builder, |ui, _| {
        chosen = anchor_bar(ui, hint);
    });
    crate::window::shown(ctx, &title, Some(at), false);
    chosen
}

/// The screen the mouse is on, or the first one, when the system lists any.
pub fn cursor_screen_rect() -> Option<Rect> {
    let screens = crate::window::screens();
    let at = crate::window::cursor_screen(&screens).or((!screens.is_empty()).then_some(0))?;
    Some(screens[at].rect)
}

/// Where each of `sizes` goes when snapped to `anchor`: stacked so they do not sit on top of each other.
pub fn snap_all(anchor: Anchor, sizes: &[egui::Vec2], padding: f32, screen: Rect) -> Vec<egui::Pos2> {
    let key = anchor.key().as_bytes();
    let mut offset = 0.0;
    sizes
        .iter()
        .map(|size| {
            let mut pos = anchor.place(*size, padding, screen);
            match key[0] {
                b't' => pos.y += offset,
                b'b' => pos.y -= offset,
                _ => pos.x += if key[1] == b'l' { offset } else { -offset },
            }
            offset += if key[0] != b'm' { size.y + 8.0 } else { size.x + 8.0 };
            pos
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapped_panels_stack_away_from_the_edge_they_hug() {
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 800.0));
        let sizes = [vec2(200.0, 100.0), vec2(200.0, 100.0)];
        let top = snap_all(Anchor::TopRight, &sizes, 12.0, screen);
        assert_eq!(top, vec![pos2(788.0, 12.0), pos2(788.0, 120.0)]);
        let bottom = snap_all(Anchor::BottomLeft, &sizes, 12.0, screen);
        assert_eq!(bottom, vec![pos2(12.0, 688.0), pos2(12.0, 580.0)]);
        let middle = snap_all(Anchor::MiddleLeft, &sizes, 12.0, screen);
        assert_eq!(middle, vec![pos2(12.0, 350.0), pos2(220.0, 350.0)]);
    }
}
