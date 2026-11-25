//! Our windows and the desktop: the placer that puts them where they should be (a Wayland window can not
//! place itself), and the screens they cover.

use egui::{Context, Id, Rect, pos2, vec2};
use lum_platform::desktop;
use lum_platform::window::{Placement, WindowPlacer};
use std::sync::Arc;

fn placer_id() -> Id {
    Id::unique("lum-window-placer")
}

/// Hands the app's placer to every window drawn with this context.
pub fn install_placer(ctx: &Context, placer: Arc<dyn WindowPlacer>) {
    ctx.data_mut(|d| d.insert_temp(placer_id(), placer));
}

pub fn placer(ctx: &Context) -> Arc<dyn WindowPlacer> {
    ctx.data(|d| d.get_temp::<Arc<dyn WindowPlacer>>(placer_id())).unwrap_or_else(lum_platform::window::none)
}

/// Tells the placer a window was drawn this frame at `at` (desktop coordinates; None when a rule places it).
pub fn shown(ctx: &Context, title: &str, at: Option<egui::Pos2>, focus: bool) {
    let at = at.map(|p| (p.x.round() as i32, p.y.round() as i32));
    placer(ctx).shown(&Placement { title, at, focus });
}

/// One screen, in desktop coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct Screen {
    pub name: String,
    pub rect: Rect,
}

impl Screen {
    pub fn new(name: impl Into<String>, rect: Rect) -> Screen {
        Screen { name: name.into(), rect }
    }
}

/// Every screen, the primary one first. Empty when the system can not say.
pub fn screens() -> Vec<Screen> {
    desktop::monitors()
        .into_iter()
        .map(|m| {
            let rect =
                Rect::from_min_size(pos2(m.x as f32, m.y as f32), vec2(m.width as f32, m.height as f32));
            Screen { name: m.name, rect }
        })
        .collect()
}
