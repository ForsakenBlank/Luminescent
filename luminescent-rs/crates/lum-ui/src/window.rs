//! Our windows and the desktop: the placer that puts them where they should be (a Wayland window can not
//! place itself), and the screens they cover.

use egui::{Context, Id, Rect, pos2, vec2};
use lum_platform::capture::Capture;
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

fn capture_id() -> Id {
    Id::unique("lum-screen-capture")
}

/// Hands the tools that freeze the screen something else to take screenshots with (tests use a fake).
pub fn install_capture(ctx: &Context, capture: Arc<dyn Capture>) {
    ctx.data_mut(|d| d.insert_temp(capture_id(), capture));
}

/// What takes the screenshots: the system's own unless something else was installed.
pub fn capture(ctx: &Context) -> Arc<dyn Capture> {
    ctx.data(|d| d.get_temp::<Arc<dyn Capture>>(capture_id())).unwrap_or_else(lum_platform::capture::system)
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
    /// The part bars and docks leave free (Noctalia, Waybar, the taskbar stay above the overlay).
    pub work: Rect,
    /// Real pixels per layout pixel (1.5 on a monitor scaled to 150 %).
    pub scale: f32,
}

impl Screen {
    pub fn new(name: impl Into<String>, rect: Rect) -> Screen {
        Screen { name: name.into(), rect, work: rect, scale: 1.0 }
    }

    pub fn with_scale(mut self, scale: f32) -> Screen {
        self.scale = scale;
        self
    }

    /// The screen as the platform layer describes a monitor (what screenshots are taken of).
    pub fn monitor(&self) -> desktop::Monitor {
        desktop::Monitor {
            name: self.name.clone(),
            x: self.rect.min.x.round() as i32,
            y: self.rect.min.y.round() as i32,
            width: self.rect.width().round() as u32,
            height: self.rect.height().round() as u32,
            scale: self.scale,
            primary: false,
            top_inset: (self.work.min.y - self.rect.min.y).max(0.0) as u32,
            bottom_inset: (self.rect.max.y - self.work.max.y).max(0.0) as u32,
        }
    }

    /// How much the bars take at the top and the bottom.
    pub fn insets(&self) -> (f32, f32) {
        ((self.work.min.y - self.rect.min.y).max(0.0), (self.rect.max.y - self.work.max.y).max(0.0))
    }
}

/// Every screen, the primary one first. Empty when the system can not say.
pub fn screens() -> Vec<Screen> {
    desktop::monitors()
        .into_iter()
        .map(|m| {
            let rect =
                Rect::from_min_size(pos2(m.x as f32, m.y as f32), vec2(m.width as f32, m.height as f32));
            let mut work = rect;
            work.min.y += m.top_inset as f32;
            work.max.y -= m.bottom_inset as f32;
            Screen { name: m.name, rect, work: if work.is_positive() { work } else { rect }, scale: m.scale }
        })
        .collect()
}

fn screens_id() -> Id {
    Id::unique("lum-test-screens")
}

/// Pretends these are the screens (tests: two monitors with different scales inside one window).
pub fn install_screens(ctx: &Context, screens: Vec<Screen>) {
    ctx.data_mut(|d| d.insert_temp(screens_id(), Arc::new(screens)));
}

/// The screens the tools that cover whole screens use: the system's, or the ones a test installed. Never empty.
pub fn screens_in(ctx: &Context) -> Vec<Screen> {
    if let Some(test) = ctx.data(|d| d.get_temp::<Arc<Vec<Screen>>>(screens_id())) {
        return test.as_ref().clone();
    }
    let list = screens();
    if list.is_empty() { vec![fallback_screen(ctx)] } else { list }
}

/// The screen the mouse is on, if the system says where it is.
pub fn cursor_screen(screens: &[Screen]) -> Option<usize> {
    let (x, y) = desktop::cursor()?;
    let at = pos2(x as f32, y as f32);
    screens.iter().position(|s| s.rect.contains(at))
}

/// The screen to use when the system lists none (X11 without Hyprland): the one the main window is on, as big as
/// the system says it is. Where there are no windows of our own (tests), the whole window.
pub fn fallback_screen(ctx: &Context) -> Screen {
    if ctx.embed_viewports() {
        return Screen::new("screen", ctx.content_rect());
    }
    let size = ctx
        .input(|i| i.viewport().monitor_size)
        .filter(|s| s.x > 1.0 && s.y > 1.0)
        .unwrap_or(vec2(1280.0, 800.0));
    Screen::new("screen", Rect::from_min_size(pos2(0.0, 0.0), size))
}
