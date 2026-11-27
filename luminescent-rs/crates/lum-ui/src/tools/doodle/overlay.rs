//! One screen's doodle overlay: its canvas, the stroke under way, the text box, and how it is drawn and
//! answers the mouse while drawing is on.

use egui::{
    Color32, ColorImage, CursorIcon, FontId, Id, Pos2, Rect, Shape, Stroke, StrokeKind, TextureHandle,
    TextureOptions, Ui, pos2, vec2,
};
use lum_screen::doodle::mark::{self, P};
use lum_screen::doodle::strip::{self, Part};
use lum_screen::doodle::{Canvas, Mark, Settings, ShapeKind, Tool};
use lum_screen::{IRect, Point};

use crate::fonts::Weight;
use crate::theme::{Tokens, alpha, colour};
use crate::window::Screen;

/// What a stroke under way is.
#[derive(Debug, Clone, PartialEq)]
enum Live {
    /// Painted on the canvas as the mouse moves.
    Pen,
    Highlight(Vec<P>),
    Shape(ShapeKind),
}

#[derive(Debug, Clone)]
struct Underway {
    live: Live,
    start: P,
    last: P,
    erase: bool,
}

/// The text tool's box: typed into straight on the drawing.
#[derive(Debug, Clone)]
pub struct TextBox {
    at: Pos2,
    pub text: String,
    focused: bool,
}

/// What an overlay asks the module to do after a frame.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    /// A stroke began here: one step in the shared undo history.
    Record,
    /// A finished mark to record and add (a shape, the highlighter, text).
    Add(Mark),
    Tool(Tool),
    ToggleFill,
    Close,
    Size(i32),
    TextOpened,
    /// The text box closed; the mark when there was text to keep.
    TextClosed(Option<Mark>),
    MaskMenuKey,
}

/// How the module is set while this frame is drawn.
#[derive(Debug)]
pub struct Look<'a> {
    pub active: bool,
    pub show_bg: bool,
    pub settings: &'a Settings,
    pub tool: Tool,
    pub fill: bool,
    /// Seconds on the app's clock (the size label goes after a second).
    pub now: f64,
}

pub struct Overlay {
    pub screen: Screen,
    pub primary: bool,
    /// What a bar keeps at the top of this screen; the strip goes below it.
    top: f32,
    pub canvas: Canvas,
    texture: Option<TextureHandle>,
    stroke: Option<Underway>,
    pub text: Option<TextBox>,
    /// "8 px" next to the mouse after a wheel turn, until this time.
    label: Option<(Pos2, f64)>,
    hover: Option<Part>,
}

impl std::fmt::Debug for Overlay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Overlay").field("screen", &self.screen.name).field("canvas", &self.canvas).finish()
    }
}

impl Overlay {
    pub fn new(screen: Screen, primary: bool) -> Overlay {
        let (top, _) = screen.insets();
        let size = screen.rect.size();
        // Windows lists screens in real pixels already; elsewhere the canvas is the layout size times the scale
        let (w, h, scale) = if cfg!(windows) {
            ((size.x / screen.scale).round() as u32, (size.y / screen.scale).round() as u32, screen.scale)
        } else {
            (size.x.round() as u32, size.y.round() as u32, screen.scale)
        };
        let canvas = Canvas::new(w, h, scale);
        Overlay {
            screen,
            primary,
            top,
            canvas,
            texture: None,
            stroke: None,
            text: None,
            label: None,
            hover: None,
        }
    }

    pub fn is_drawing(&self) -> bool {
        self.stroke.is_some()
    }

    /// Drawing stopped: nothing half done stays behind.
    pub fn stop(&mut self) {
        if self.stroke.take().is_some() {
            self.canvas.end();
        }
        self.hover = None;
        self.label = None;
    }

    /// Uploads what changed on the canvas since the last frame.
    fn sync_texture(&mut self, ctx: &egui::Context) -> egui::TextureId {
        let dirty = self.canvas.take_dirty();
        let size = [self.canvas.width() as usize, self.canvas.height() as usize];
        match (&mut self.texture, dirty) {
            (None, _) => {
                let image = ColorImage::from_rgba_premultiplied(size, self.canvas.pixmap().data());
                let name = format!("doodle {}", self.screen.name);
                self.texture.insert(ctx.load_texture(name, image, TextureOptions::LINEAR)).id()
            }
            (Some(texture), Some(area)) => {
                let image = ColorImage::from_rgba_premultiplied(
                    [area.w as usize, area.h as usize],
                    &self.canvas.region(area),
                );
                texture.set_partial([area.x as usize, area.y as usize], image, TextureOptions::LINEAR);
                texture.id()
            }
            (Some(texture), None) => texture.id(),
        }
    }

    fn strip_parts(&self, ui: &Ui, area: Rect, look: &Look<'_>) -> Vec<(Part, IRect)> {
        let tokens = Tokens::of(ui.ctx());
        let galley = ui.painter().layout_no_wrap(
            format!("{} PX", look.settings.effective_size()),
            tokens.font(11.0, Weight::Regular, true),
            Color32::WHITE,
        );
        strip::parts(area.width() as i32, self.top as i32, galley.size().x.ceil() as i32 + 16)
    }

    fn strip_visible(&self, look: &Look<'_>) -> bool {
        self.primary && look.active && look.settings.show_tools
    }

    /// Draws the overlay into `area` and handles the mouse; what the module must do is added to `acts`.
    pub fn frame(&mut self, ui: &mut Ui, area: Rect, look: &Look<'_>, acts: &mut Vec<Act>) {
        let parts = if self.strip_visible(look) { self.strip_parts(ui, area, look) } else { Vec::new() };
        if look.active {
            self.input(ui, area, look, &parts, acts);
        }
        let texture = self.sync_texture(ui.ctx());
        let painter = ui.painter().clone();
        if look.show_bg {
            painter.rect_filled(
                area,
                0.0,
                Color32::from_black_alpha((look.settings.bg_opacity.clamp(0.0, 1.0) * 255.0) as u8),
            );
        } else if look.active {
            // a faint tint so you can see that the screen is taking the mouse
            let a = ((look.settings.passive_opacity * 255.0) as u8).max(1);
            painter.rect_filled(area, 0.0, Color32::from_black_alpha(a));
        }
        painter.image(texture, area, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        self.paint_preview(&painter, area, look);
        if self.text.is_some() {
            self.text_box(ui, area, look, acts);
        }
        if !parts.is_empty() {
            self.paint_strip(ui, area, look, &parts);
        }
        self.paint_label(&painter, area, look);
        let pointer = ui.input(|i| i.pointer.hover_pos()).filter(|p| area.contains(*p));
        if look.active
            && self.hover.is_none()
            && self.text.is_none()
            && let Some(p) = pointer
        {
            let ink = colour(look.settings.colour);
            paint_cursor(&painter, p, look.settings.effective_size() as f32, ink);
            ui.ctx().set_cursor_icon(CursorIcon::None);
        }
        let info = format!("Doodle on {}", self.screen.name);
        ui.interact(area, Id::unique(("lum-doodle", &self.screen.name)), egui::Sense::hover())
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, info.clone()));
    }

    fn input(&mut self, ui: &Ui, area: Rect, look: &Look<'_>, parts: &[(Part, IRect)], acts: &mut Vec<Act>) {
        let (hover, left, right, released, shift, wheel) = ui.input(|i| {
            (
                i.pointer.hover_pos().filter(|p| area.contains(*p)),
                i.pointer.button_pressed(egui::PointerButton::Primary),
                i.pointer.button_pressed(egui::PointerButton::Secondary),
                i.pointer.any_released(),
                i.modifiers.shift,
                wheel_notches(&i.events),
            )
        });
        let local = |p: Pos2| -> P { (p.x - area.min.x, p.y - area.min.y) };
        let point = |p: Pos2| Point::new((p.x - area.min.x) as i32, (p.y - area.min.y) as i32);
        let on_strip = hover.and_then(|p| strip::hit(parts, self.top as i32, point(p)));
        if self.stroke.is_none() {
            self.hover = on_strip.flatten();
            if self.hover.is_some() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
        }
        if let (Some(p), true) = (hover, left || right) {
            match on_strip {
                Some(Some(Part::Close)) => acts.push(Act::Close),
                Some(Some(Part::Fill)) => acts.push(Act::ToggleFill),
                Some(Some(Part::Tool(t))) => acts.push(Act::Tool(t)),
                Some(_) => {}
                None => self.press(local(p), left, shift, look, acts),
            }
        } else if let (Some(p), Some(stroke)) = (hover, self.stroke.as_mut()) {
            let cur = local(p);
            if cur != stroke.last {
                match &mut stroke.live {
                    Live::Pen => self.canvas.extend(cur),
                    Live::Highlight(points) => points.push(cur),
                    Live::Shape(_) => {}
                }
                stroke.last = cur;
            }
        }
        if released && let Some(stroke) = self.stroke.take() {
            let end = hover.map_or(stroke.last, local);
            let s = look.settings;
            let width = s.effective_size() as f32 * if stroke.erase { 3.0 } else { 1.0 };
            match stroke.live {
                Live::Pen => self.canvas.end(),
                Live::Highlight(points) => acts.push(Act::Add(Mark::Highlight {
                    points,
                    width: mark::highlight_width(s.effective_size()),
                    colour: s.colour,
                    opacity: s.highlighter_opacity,
                    smooth: !s.pixel_brush,
                })),
                Live::Shape(kind) => acts.push(Act::Add(Mark::Shape {
                    kind,
                    a: stroke.start,
                    b: end,
                    width,
                    colour: s.colour,
                    erase: stroke.erase,
                    fill: look.fill && matches!(kind, ShapeKind::Rect | ShapeKind::Ellipse),
                    smooth: !s.pixel_brush,
                })),
            }
        }
        if wheel != 0
            && look.settings.wheel_size
            && let Some(p) = hover
        {
            acts.push(Act::Size(wheel));
            let size = look.settings.effective_size() as f32;
            self.label = Some((p - area.min.to_vec2() + vec2(size / 2.0 + 14.0, 10.0), look.now + 1.0));
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(1000));
        }
    }

    fn press(&mut self, at: P, left: bool, shift: bool, look: &Look<'_>, acts: &mut Vec<Act>) {
        if let Some(text) = self.text.take() {
            // a click somewhere else keeps the text being typed
            acts.push(Act::TextClosed(self.text_mark(&text, look)));
        }
        if left && look.tool == Tool::Text {
            self.text = Some(TextBox { at: pos2(at.0, at.1), text: String::new(), focused: false });
            acts.push(Act::TextOpened);
            return;
        }
        acts.push(Act::MaskMenuKey);
        acts.push(Act::Record);
        let erase = !left;
        let mut tool = look.tool;
        if erase && matches!(tool, Tool::Highlighter | Tool::Text) {
            // the right button always rubs out
            tool = Tool::Pen;
        }
        let live = match tool {
            Tool::Pen if shift => Live::Shape(ShapeKind::Line),
            Tool::Pen | Tool::Text => Live::Pen,
            Tool::Highlighter => Live::Highlight(vec![at]),
            Tool::Line => Live::Shape(ShapeKind::Line),
            Tool::Rect => Live::Shape(ShapeKind::Rect),
            Tool::Ellipse => Live::Shape(ShapeKind::Ellipse),
            Tool::Arrow => Live::Shape(ShapeKind::Arrow),
        };
        if live == Live::Pen {
            let s = look.settings;
            let size = s.effective_size() as f32;
            let width = if erase { size * 3.0 } else { size };
            let cell = s.pixel_brush.then_some(s.pixel_size.max(1));
            self.canvas.begin_pen(at, width, s.colour, erase, !s.pixel_brush, cell);
        }
        self.stroke = Some(Underway { live, start: at, last: at, erase });
    }

    fn text_mark(&self, text: &TextBox, look: &Look<'_>) -> Option<Mark> {
        let typed = text.text.trim();
        (!typed.is_empty()).then(|| Mark::Text {
            x: text.at.x + 5.0,
            centre_y: text.at.y,
            text: text.text.clone(),
            px: look.settings.text_size as f32,
            colour: look.settings.colour,
        })
    }

    /// Closes the text box, keeping the text or not; gives the mark to add when there is one.
    pub fn close_text(&mut self, keep: bool, look: &Look<'_>) -> Option<Option<Mark>> {
        let text = self.text.take()?;
        Some(if keep { self.text_mark(&text, look) } else { None })
    }

    fn text_box(&mut self, ui: &mut Ui, area: Rect, look: &Look<'_>, acts: &mut Vec<Act>) {
        let Some(text) = self.text.as_mut() else { return };
        let s = look.settings;
        let px = s.text_size as f32;
        let font = FontId::new(px, Tokens::of(ui.ctx()).font(px, Weight::SemiBold, false).family);
        let ink = colour(s.colour);
        let shown = if text.text.is_empty() { "Type…" } else { text.text.as_str() };
        let width = ui.painter().layout_no_wrap(shown.to_owned(), font.clone(), ink).size().x + 24.0;
        let h = lum_screen::font::line_height(px) + 8.0;
        let rect =
            Rect::from_min_size(area.min + vec2(text.at.x, text.at.y - h / 2.0), vec2(width.max(80.0), h));
        let painter = ui.painter();
        painter.rect_filled(rect, 4.0, Color32::from_black_alpha(70));
        let outline = crate::paint::rounded_outline(rect, 4.0);
        let mut closed = outline.clone();
        closed.extend(outline.first().copied());
        painter.extend(Shape::dashed_line(&closed, Stroke::new(1.0, ink), 3.0, 3.0));
        let edit = egui::TextEdit::singleline(&mut text.text)
            .font(font)
            .text_color(ink)
            .hint_text("Type…")
            .frame(egui::Frame::NONE)
            .margin(vec2(4.0, 4.0))
            .desired_width(rect.width());
        let response = ui.put(rect, edit);
        if !text.focused {
            response.request_focus();
            text.focused = true;
        }
        if response.lost_focus() {
            let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
            if let Some(mark) = self.close_text(!escape, look) {
                acts.push(Act::TextClosed(mark));
            }
        }
    }

    fn paint_preview(&self, painter: &egui::Painter, area: Rect, look: &Look<'_>) {
        let Some(stroke) = &self.stroke else { return };
        let s = look.settings;
        let at = |p: P| area.min + vec2(p.0, p.1);
        match &stroke.live {
            Live::Pen => {}
            Live::Highlight(points) => {
                let ink = alpha(colour(s.colour), s.highlighter_opacity.clamp(0.05, 1.0));
                let line: Vec<Pos2> = points.iter().map(|p| at(*p)).collect();
                painter.add(Shape::line(line, Stroke::new(mark::highlight_width(s.effective_size()), ink)));
            }
            Live::Shape(kind) => {
                let width = s.effective_size() as f32 * if stroke.erase { 3.0 } else { 1.0 };
                let ink = if stroke.erase { Color32::from_white_alpha(120) } else { colour(s.colour) };
                let pen = Stroke::new(width, ink);
                let (a, b) = (at(stroke.start), at(stroke.last));
                let fill = if look.fill && matches!(kind, ShapeKind::Rect | ShapeKind::Ellipse) {
                    if stroke.erase { Color32::from_white_alpha(60) } else { colour(s.colour) }
                } else {
                    Color32::TRANSPARENT
                };
                match kind {
                    ShapeKind::Line => {
                        painter.line_segment([a, b], pen);
                    }
                    ShapeKind::Arrow => {
                        painter.line_segment([a, b], pen);
                        for (from, tip) in
                            mark::arrow_head(stroke.start, stroke.last, mark::head_length(width))
                        {
                            painter.line_segment([at(from), at(tip)], pen);
                        }
                    }
                    ShapeKind::Rect => {
                        painter.rect(Rect::from_two_pos(a, b), 0.0, fill, pen, StrokeKind::Middle);
                    }
                    ShapeKind::Ellipse => {
                        let r = Rect::from_two_pos(a, b);
                        painter.add(Shape::ellipse_filled(r.center(), r.size() / 2.0, fill));
                        painter.add(Shape::ellipse_stroke(r.center(), r.size() / 2.0, pen));
                    }
                }
            }
        }
    }

    fn paint_strip(&self, ui: &Ui, area: Rect, look: &Look<'_>, parts: &[(Part, IRect)]) {
        let tokens = Tokens::of(ui.ctx());
        let pal = &tokens.p;
        let painter = ui.painter();
        let to_ui = |r: IRect| {
            Rect::from_min_size(area.min + vec2(r.x as f32, r.y as f32), vec2(r.w as f32, r.h as f32))
        };
        let frame = to_ui(strip::frame(parts, self.top as i32));
        let rad = tokens.r() + 2.0;
        painter.rect(frame, rad, pal.bg_base, Stroke::new(1.0, pal.border), StrokeKind::Inside);
        let small = (tokens.r() - 3.0).max(0.0);
        for (part, r) in parts {
            let b = to_ui(*r);
            let hovered = self.hover == Some(*part);
            let chosen = *part == Part::Tool(look.tool);
            if hovered && !chosen {
                painter.rect_filled(b, small, alpha(pal.text_primary, 28.0 / 255.0));
            }
            let label = match part {
                Part::Tool(t) => {
                    if chosen {
                        painter.rect_filled(b, small, pal.primary_muted);
                    }
                    let ink = if chosen { pal.primary_text } else { pal.text_secondary };
                    painter.text(
                        b.center(),
                        egui::Align2::CENTER_CENTER,
                        t.glyph(),
                        FontId::proportional(14.0),
                        ink,
                    );
                    t.tooltip().to_owned()
                }
                Part::Fill => {
                    // a square that is filled while shapes get filled
                    let ink = if look.fill { pal.primary_text } else { pal.text_secondary };
                    let sq = Rect::from_center_size(b.center(), vec2(14.0, 14.0));
                    let fill = if look.fill { ink } else { Color32::TRANSPARENT };
                    painter.rect(sq, 3.0, fill, Stroke::new(1.6, ink), StrokeKind::Middle);
                    "Fill shapes".to_owned()
                }
                Part::Divider => {
                    let c = b.center();
                    painter.line_segment(
                        [c - vec2(0.0, 10.0), c + vec2(0.0, 10.0)],
                        Stroke::new(1.0, pal.border),
                    );
                    continue;
                }
                Part::Colour => {
                    painter.circle_filled(b.center(), 8.0, colour(look.settings.colour));
                    "Colour".to_owned()
                }
                Part::Size => {
                    let text = format!("{} PX", look.settings.effective_size());
                    painter.text(
                        b.center(),
                        egui::Align2::CENTER_CENTER,
                        &text,
                        tokens.font(11.0, Weight::Regular, true),
                        pal.text_secondary,
                    );
                    text
                }
                Part::Close => {
                    let ink = if hovered { pal.text_primary } else { pal.text_muted };
                    painter.text(
                        b.center(),
                        egui::Align2::CENTER_CENTER,
                        "✕",
                        FontId::proportional(12.0),
                        ink,
                    );
                    "Stop drawing".to_owned()
                }
            };
            ui.interact(
                b,
                Id::unique(("lum-doodle-strip", &self.screen.name, label.as_str())),
                egui::Sense::hover(),
            )
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label.clone()));
        }
    }

    fn paint_label(&mut self, painter: &egui::Painter, area: Rect, look: &Look<'_>) {
        let Some((at, until)) = self.label else { return };
        if look.now >= until {
            self.label = None;
            return;
        }
        let text = format!("{} px", look.settings.effective_size());
        let galley = painter.layout_no_wrap(text, FontId::proportional(13.0), Color32::WHITE);
        let r = Rect::from_min_size(area.min + at.to_vec2(), galley.size() + vec2(12.0, 6.0));
        painter.rect_filled(r, 5.0, Color32::from_black_alpha(170));
        painter.galley(r.min + vec2(6.0, 3.0), galley, Color32::WHITE);
    }
}

/// Wheel notches this frame, up positive. A touchpad's small scrolls still count as one.
fn wheel_notches(events: &[egui::Event]) -> i32 {
    let mut lines = 0.0;
    for e in events {
        if let egui::Event::MouseWheel { unit, delta, .. } = e {
            lines += match unit {
                egui::MouseWheelUnit::Line => delta.y,
                egui::MouseWheelUnit::Page => delta.y * 3.0,
                egui::MouseWheelUnit::Point => delta.y / 50.0,
            };
        }
    }
    let steps = lines.trunc() as i32;
    if steps == 0 && lines != 0.0 { lines.signum() as i32 } else { steps }
}

/// The brush as the mouse pointer: a circle of the real size with a small cross in the middle.
fn paint_cursor(painter: &egui::Painter, at: Pos2, diameter: f32, ink: Color32) {
    let r = diameter.max(3.0) / 2.0;
    painter.circle_stroke(at, r, Stroke::new(3.0, Color32::from_black_alpha(160)));
    painter.circle_stroke(at, r, Stroke::new(1.4, ink));
    let cross = Stroke::new(1.0, Color32::from_white_alpha(200));
    painter.line_segment([at - vec2(2.0, 0.0), at + vec2(2.0, 0.0)], cross);
    painter.line_segment([at - vec2(0.0, 2.0), at + vec2(0.0, 2.0)], cross);
}
