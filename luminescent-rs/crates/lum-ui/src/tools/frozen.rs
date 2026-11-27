//! The screens frozen for the colour picker, the ruler and text grab: every monitor is grabbed once, on a
//! worker thread, and each picture is shown full size on its own monitor in a window of its own, at the
//! monitor's real pixels. The tool paints on top of it and reads the mouse and keys from it.
//!
//! On Hyprland a window can not place itself, so each window is titled per monitor and a cover rule (one
//! prefix per tool, registered when the app starts) opens it full size on its monitor. Windows places it
//! itself, in real pixels. Where there are no separate windows (tests, the gallery) the screens are drawn into
//! the one window at their desktop rectangles.

use std::time::Instant;

use egui::{
    Color32, ColorImage, Context, Id, Pos2, Rect, Response, Sense, Stroke, StrokeKind, TextureHandle,
    TextureOptions, Ui, ViewportBuilder, ViewportId, pos2, vec2,
};
use lum_platform::capture::Frame;
use lum_platform::desktop::{self, APP_ID};
use lum_screen::{IRect, Point};

use crate::fonts::Weight;
use crate::theme::Tokens;
use crate::window::{self, Screen};

/// The cover prefixes, one per tool, so two tools never share a window title. The app registers each with the
/// desktop when it starts (`Desktop::start`).
pub const COLOUR_PICKER: &str = "Luminescent colour picker";
pub const RULER: &str = "Luminescent ruler";
pub const TEXT_GRAB: &str = "Luminescent text grab";
pub const PREFIXES: [&str; 3] = [COLOUR_PICKER, RULER, TEXT_GRAB];

/// One monitor as it was when the tool started.
#[derive(Debug, Clone)]
pub struct Shot {
    pub screen: Screen,
    pub frame: Frame,
}

/// Every screen, grabbed on a worker thread; `done` gets them (or why none could be grabbed) on that thread.
pub fn grab(ctx: &Context, done: impl FnOnce(Result<Vec<Shot>, String>) + Send + 'static) {
    let screens = window::screens_in(ctx);
    let capture = window::capture(ctx);
    let spawned = std::thread::Builder::new().name("screen-grab".into()).spawn(move || {
        let monitors: Vec<_> = screens.iter().map(Screen::monitor).collect();
        let frames = capture.grab(&monitors);
        let mut shots = Vec::new();
        let mut first_error = None;
        for (screen, frame) in screens.into_iter().zip(frames) {
            match frame {
                Ok(frame) if !frame.is_empty() => shots.push(Shot { screen, frame }),
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!("could not grab {}: {e}", screen.name);
                    first_error.get_or_insert(e);
                }
            }
        }
        if shots.is_empty() {
            done(Err(first_error.unwrap_or_else(|| "there is no screen to grab".to_owned())));
        } else {
            done(Ok(shots));
        }
    });
    if let Err(e) = spawned {
        tracing::warn!("could not start grabbing the screen: {e}");
    }
}

/// What a tool knows about the screen it is painting this frame.
#[derive(Debug)]
pub struct ScreenCx<'a> {
    pub index: usize,
    pub shot: &'a Shot,
    /// Where the screen is in the window being drawn (the whole window, or its desktop rectangle in tests).
    pub area: Rect,
    /// The first screen in the list, where hints show before the mouse arrives.
    pub primary: bool,
    /// This screen gets the keys this frame (the one with the keyboard, or the mouse in tests).
    pub has_keys: bool,
    /// The frozen picture, to draw part of it again (text grab's selection, undimmed).
    pub texture: egui::TextureId,
}

impl ScreenCx<'_> {
    /// Real pixels per layout pixel on this screen.
    pub fn ratio(&self) -> f64 {
        self.shot.frame.ratio(f64::from(self.area.width()))
    }

    /// The screen's size in layout pixels.
    pub fn size(&self) -> (i32, i32) {
        (self.area.width().round() as i32, self.area.height().round() as i32)
    }

    /// A position in the window as a point on the screen.
    pub fn local(&self, p: Pos2) -> Point {
        let v = p - self.area.min;
        Point::new(v.x.floor() as i32, v.y.floor() as i32)
    }

    pub fn to_ui(&self, p: Point) -> Pos2 {
        self.area.min + vec2(p.x as f32, p.y as f32)
    }

    pub fn rect(&self, r: IRect) -> Rect {
        Rect::from_min_size(self.to_ui(Point::new(r.x, r.y)), vec2(r.w as f32, r.h as f32))
    }

    /// The point in desktop coordinates (where the system's mouse pointer is moved to).
    pub fn desktop(&self, p: Point) -> (i32, i32) {
        let origin = self.shot.screen.rect.min;
        let k = self.shot.screen.rect.width() / self.area.width().max(1.0);
        ((origin.x + p.x as f32 * k).round() as i32, (origin.y + p.y as f32 * k).round() as i32)
    }
}

/// The frozen screens of one tool while it is open.
pub struct FrozenScreens {
    tool: &'static str,
    shots: Vec<Shot>,
    textures: Vec<Option<TextureHandle>>,
    /// When the hotkey was pressed, to report how long the screens took to come up.
    requested: Instant,
    reported: bool,
    /// The screen that was last given the keyboard.
    focused: Option<usize>,
}

impl std::fmt::Debug for FrozenScreens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrozenScreens").field("tool", &self.tool).field("screens", &self.shots.len()).finish()
    }
}

impl FrozenScreens {
    /// `tool` is the cover prefix the app registered for this tool ("Luminescent colour picker").
    pub fn new(tool: &'static str, shots: Vec<Shot>, requested: Instant) -> FrozenScreens {
        let textures = shots.iter().map(|_| None).collect();
        FrozenScreens { tool, shots, textures, requested, reported: false, focused: None }
    }

    pub fn shots(&self) -> &[Shot] {
        &self.shots
    }

    /// The screen the desktop point is on, with the point on it in layout pixels.
    pub fn at_desktop(&self, x: i32, y: i32) -> Option<(usize, Point)> {
        let at = pos2(x as f32, y as f32);
        let i = self.shots.iter().position(|s| s.screen.rect.contains(at))?;
        let rect = self.shots[i].screen.rect;
        Some((i, Point::new((at.x - rect.min.x) as i32, (at.y - rect.min.y) as i32)))
    }

    fn texture(&mut self, ctx: &Context, i: usize) -> egui::TextureId {
        let tool = self.tool;
        let shot = &self.shots[i];
        self.textures[i]
            .get_or_insert_with(|| {
                let size = [shot.frame.width as usize, shot.frame.height as usize];
                let image = ColorImage::from_rgba_unmultiplied(size, &shot.frame.rgba);
                ctx.load_texture(format!("{tool} {}", shot.screen.name), image, TextureOptions::NEAREST)
            })
            .id()
    }

    /// Draws every screen for this frame and lets the tool paint on each and read its input. Call it every
    /// frame while the tool is open; the windows close when it is no longer called.
    pub fn show(&mut self, ctx: &Context, mut paint: impl FnMut(&mut Ui, &ScreenCx<'_>, &Response)) {
        let embedded = ctx.embed_viewports();
        let mouse_screen =
            window::cursor_screen(&self.shots.iter().map(|s| s.screen.clone()).collect::<Vec<_>>());
        for i in 0..self.shots.len() {
            let texture = self.texture(ctx, i);
            let screen = self.shots[i].screen.clone();
            let id = Id::unique(("lum-frozen", self.tool, &screen.name));
            if embedded {
                let area = screen.rect;
                let has_keys = ctx.pointer_hover_pos().map_or(i == 0, |p| area.contains(p));
                let shot = &self.shots[i];
                egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(area.min).constrain(false).show(
                    ctx,
                    |ui| {
                        ui.set_clip_rect(area);
                        let response = ui.interact(area, id, Sense::click_and_drag());
                        ui.painter().image(texture, area, full_uv(), Color32::WHITE);
                        let cx = ScreenCx { index: i, shot, area, primary: i == 0, has_keys, texture };
                        paint(ui, &cx, &response);
                    },
                );
                continue;
            }
            let title = desktop::cover_title(self.tool, &screen.name);
            let front = mouse_screen.unwrap_or(0) == i;
            let builder = ViewportBuilder::default()
                .with_title(title.clone())
                .with_app_id(APP_ID)
                .with_decorations(false)
                .with_always_on_top()
                .with_taskbar(false)
                .with_resizable(false)
                .with_active(front)
                .with_position(screen.rect.min)
                .with_inner_size(screen.rect.size());
            let mut hovered = false;
            let mut focused = false;
            let shot = &self.shots[i];
            ctx.show_viewport_immediate(
                ViewportId::from_hash_of(("lum-frozen", self.tool, &screen.name)),
                builder,
                |ui, _| {
                    let area = Rect::from_min_size(Pos2::ZERO, ui.ctx().content_rect().size());
                    hovered = ui.input(|inp| inp.pointer.has_pointer());
                    focused = ui.input(|inp| inp.focused);
                    let response = ui.interact(area, id, Sense::click_and_drag());
                    ui.painter().image(texture, area, full_uv(), Color32::WHITE);
                    let cx = ScreenCx { index: i, shot, area, primary: i == 0, has_keys: focused, texture };
                    paint(ui, &cx, &response);
                },
            );
            let placer = window::placer(ctx);
            window::shown(ctx, &title, None, front && self.focused.is_none());
            let r = screen.rect;
            placer.cover(&title, (r.min.x as i32, r.min.y as i32, r.width() as i32, r.height() as i32));
            // keys go to the screen the mouse is on
            if hovered && !focused && self.focused != Some(i) {
                self.focused = Some(i);
                placer.focus(&title);
            }
        }
        if !self.reported {
            self.reported = true;
            let ms = self.requested.elapsed().as_secs_f64() * 1000.0;
            tracing::info!("{}: frozen screens up {ms:.0} ms after the hotkey", self.tool);
        }
    }
}

fn full_uv() -> Rect {
    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))
}

/// The pill at the top of a frozen screen: the sparkle, then the parts with dots between them, in mono
/// capitals. `top` is its top edge, centred on `centre_x`; it is as wide as its text or `min_w`.
pub fn paint_hint(ui: &Ui, centre_x: f32, top: f32, min_w: f32, parts: &[String]) -> Rect {
    let tokens = Tokens::of(ui.ctx());
    let p = &tokens.p;
    let font = tokens.font(11.0, Weight::Regular, true);
    let mut job = egui::text::LayoutJob::default();
    for (n, part) in parts.iter().enumerate() {
        if n > 0 {
            job.append("  ·  ", 0.0, egui::TextFormat::simple(font.clone(), p.text_muted));
        }
        job.append(part, 0.0, egui::TextFormat::simple(font.clone(), p.text_secondary));
    }
    let galley = ui.painter().layout_job(job);
    let sparkle = 13.0 + 16.0;
    let w = (galley.size().x + sparkle + 40.0).max(min_w);
    let rect = Rect::from_min_size(pos2(centre_x - w / 2.0, top), vec2(w, 32.0));
    ui.painter().rect(rect, 16.0, p.bg_base, Stroke::new(1.0, p.border), StrokeKind::Inside);
    let x = rect.center().x - (galley.size().x + sparkle) / 2.0;
    crate::paint::sparkle(ui.painter(), pos2(x + 6.5, rect.center().y), 13.0, p.primary);
    ui.painter().galley(pos2(x + sparkle, rect.center().y - galley.size().y / 2.0), galley, p.text_secondary);
    rect
}
