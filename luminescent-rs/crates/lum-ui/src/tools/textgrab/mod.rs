//! Text Grab (OCR): drag a box around text anywhere on screen and it is copied. The screens are grabbed once
//! when the hotkey is pressed, dimmed, and you drag a rectangle (Esc or right-click cancels). Recognition runs
//! on a worker thread (Tesseract on Linux, Windows' own engine on Windows, see `lum_platform::ocr`).
//!
//! With "Show the text in an editable box" on, the result opens in a small panel first: fix what OCR got
//! wrong, then Copy (Ctrl+Enter). Esc throws it away.

use std::time::Instant;

use egui::{Color32, Context, CursorIcon, Id, Key, Rect, Stroke, StrokeKind, UiBuilder, pos2, vec2};
use lum_core::actions::Action;
use lum_core::module::{Module, ModuleCx, StartError};
use lum_platform::capture::Frame;
use lum_screen::{IRect, Point, ocr};

use crate::panel::{FloatingPanel, Geometry, centred_on};
use crate::text::Text;
use crate::theme::{Tokens, Tone};
use crate::tools::edit::cursor_screen_rect;
use crate::tools::frozen::{self, FrozenScreens, Shot};
use crate::widgets::{Button, TextArea};
use crate::{Bind, Inbox, Request, Services, UiCx, UiModule};

const TITLE: &str = "Text Grab";

/// What reads the text: the system's OCR, or a stand-in in tests.
pub type Recogniser = std::sync::Arc<dyn Fn(&Frame, bool) -> Result<String, String> + Send + Sync>;

enum Cmd {
    Grab(Instant),
    Grabbed(Result<Vec<Shot>, String>, Instant),
    Recognised(Result<String, String>),
}

/// One screen's drag, as each of Python's selector windows kept its own.
#[derive(Debug, Clone, Copy, Default)]
struct Drag {
    a: Option<Point>,
    b: Option<Point>,
}

impl Drag {
    fn selection(&self) -> Option<IRect> {
        Some(IRect::spanning(self.a?, self.b?))
    }
}

struct Selecting {
    screens: FrozenScreens,
    drags: Vec<Drag>,
}

/// The editable box with the recognised text.
struct Review {
    panel: FloatingPanel,
    text: String,
    focused: bool,
}

pub struct TextGrabModule {
    inbox: Inbox<Cmd>,
    services: Services,
    running: bool,
    keep_lines: bool,
    upscale: bool,
    review: bool,
    selecting: Option<Selecting>,
    grabbing: bool,
    /// Recognition is running.
    busy: bool,
    open_review: Option<Review>,
    recogniser: Recogniser,
}

impl std::fmt::Debug for TextGrabModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextGrabModule").field("running", &self.running).finish_non_exhaustive()
    }
}

impl TextGrabModule {
    pub fn new(services: &Services) -> TextGrabModule {
        TextGrabModule {
            inbox: Inbox::new(services),
            services: services.clone(),
            running: false,
            keep_lines: true,
            upscale: true,
            review: false,
            selecting: None,
            grabbing: false,
            busy: false,
            open_review: None,
            recogniser: std::sync::Arc::new(lum_platform::ocr::recognise),
        }
    }

    /// Reads text with `recogniser` instead of the system's OCR (tests).
    pub fn set_recogniser(&mut self, recogniser: Recogniser) {
        self.recogniser = recogniser;
    }

    pub fn is_selecting(&self) -> bool {
        self.selecting.is_some()
    }

    pub fn is_busy(&self) -> bool {
        self.busy
    }

    /// The text in the review panel, while it is open.
    pub fn review_text(&self) -> Option<&str> {
        self.open_review.as_ref().filter(|r| r.panel.is_open()).map(|r| r.text.as_str())
    }

    fn notify(&self, text: &str, ms: u32) {
        self.services.send(Request::Notify { title: TITLE.into(), text: text.into(), ms });
    }

    fn take_commands(&mut self, ctx: &Context) {
        for cmd in self.inbox.take() {
            match cmd {
                Cmd::Grab(asked) => {
                    if self.selecting.is_some() || self.grabbing || self.busy {
                        continue;
                    }
                    self.grabbing = true;
                    let inbox = self.inbox.clone();
                    frozen::grab(ctx, move |shots| inbox.push(Cmd::Grabbed(shots, asked)));
                }
                Cmd::Grabbed(shots, asked) => {
                    self.grabbing = false;
                    match shots {
                        Ok(shots) => {
                            let drags = vec![Drag::default(); shots.len()];
                            let screens = FrozenScreens::new(frozen::TEXT_GRAB, shots, asked);
                            self.selecting = Some(Selecting { screens, drags });
                        }
                        Err(e) => self.notify(&format!("Could not grab the screen: {e}"), 4000),
                    }
                }
                Cmd::Recognised(result) => self.finished(result),
            }
        }
    }

    /// Recognises the cut-out part of a screen on a worker thread.
    fn recognise(&mut self, cut: Frame) {
        self.busy = true;
        let (upscale, keep_lines, inbox) = (self.upscale, self.keep_lines, self.inbox.clone());
        let recogniser = self.recogniser.clone();
        let spawned = std::thread::Builder::new().name("ocr".into()).spawn(move || {
            let prepared = ocr::prepare(&cut, upscale);
            inbox.push(Cmd::Recognised(recogniser(&prepared, keep_lines)));
        });
        if let Err(e) = spawned {
            self.busy = false;
            self.notify(&format!("Could not start text recognition: {e}"), 4000);
        }
    }

    fn finished(&mut self, result: Result<String, String>) {
        self.busy = false;
        let text = match result {
            Ok(text) => text,
            Err(e) => {
                self.services.send(Request::Toast { title: TITLE.into(), text: e, ms: 6000 });
                return;
            }
        };
        if text.trim().is_empty() {
            self.notify("No text found in that area.", 2500);
            return;
        }
        if self.review {
            self.open_review = Some(review_panel(text.trim()));
            return;
        }
        crate::tools::clip::copy(text.clone());
        self.notify(&ocr::copied_message(&text), 3000);
    }

    fn draw_screens(&mut self, ctx: &Context) {
        let Some(Selecting { screens, drags }) = self.selecting.as_mut() else { return };
        let mut cancelled = false;
        let mut cut = None;
        screens.show(ctx, |ui, cx, _response| {
            ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
            let tokens = Tokens::of(ui.ctx());
            let drag = &mut drags[cx.index];
            let (hover, press, secondary, released) = ui.input(|i| {
                (
                    i.pointer.hover_pos().filter(|p| cx.area.contains(*p)),
                    i.pointer.primary_pressed(),
                    i.pointer.secondary_pressed(),
                    i.pointer.primary_released(),
                )
            });
            if let Some(p) = hover {
                if secondary {
                    cancelled = true;
                } else if press {
                    (drag.a, drag.b) = (Some(cx.local(p)), Some(cx.local(p)));
                } else if drag.a.is_some() {
                    drag.b = Some(cx.local(p));
                }
            }
            if released && let Some(sel) = drag.selection() {
                *drag = Drag::default();
                if sel.w >= ocr::MIN_SIDE && sel.h >= ocr::MIN_SIDE {
                    let k = cx.ratio();
                    let real = |v: i32| (f64::from(v) * k).round() as i32;
                    cut = Some(cx.shot.frame.crop(IRect::new(
                        real(sel.x),
                        real(sel.y),
                        real(sel.w),
                        real(sel.h),
                    )));
                }
            }
            if cx.has_keys && ui.input(|i| i.key_pressed(Key::Escape)) {
                cancelled = true;
            }

            let painter = ui.painter();
            painter.rect_filled(cx.area, 0.0, Color32::from_black_alpha(110));
            match drag.selection() {
                Some(sel) => {
                    let r = cx.rect(sel);
                    // the selected part shows undimmed
                    let uv = Rect::from_min_max(
                        pos2(
                            (r.min.x - cx.area.min.x) / cx.area.width(),
                            (r.min.y - cx.area.min.y) / cx.area.height(),
                        ),
                        pos2(
                            (r.max.x - cx.area.min.x) / cx.area.width(),
                            (r.max.y - cx.area.min.y) / cx.area.height(),
                        ),
                    );
                    painter.image(cx.texture, r, uv, Color32::WHITE);
                    painter.rect_stroke(r, 0.0, Stroke::new(2.0, tokens.p.primary), StrokeKind::Inside);
                }
                None => {
                    let text = "Drag a box around the text  ·  Esc to cancel";
                    let font = tokens.font(13.0, crate::fonts::Weight::Regular, false);
                    let at = pos2(cx.area.center().x, cx.area.min.y + 40.0);
                    let rect = painter.text(at, egui::Align2::CENTER_TOP, text, font, Color32::WHITE);
                    ui.interact(rect, Id::unique(("lum-text-grab-hint", cx.index)), egui::Sense::hover())
                        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, text));
                }
            }
        });
        if let Some(cut) = cut {
            self.selecting = None;
            self.recognise(cut);
        } else if cancelled {
            self.selecting = None;
        }
    }

    fn draw_review(&mut self, ctx: &Context) {
        let Some(review) = self.open_review.as_mut() else { return };
        if !review.panel.is_open() {
            self.open_review = None;
            return;
        }
        let mut copy = false;
        let mut close = false;
        let (text, focused) = (&mut review.text, &mut review.focused);
        let response = review.panel.show(ctx, |ui| {
            let rect = ui.max_rect();
            let row = Rect::from_min_max(
                pos2(rect.min.x + 12.0, rect.max.y - 44.0),
                pos2(rect.max.x - 12.0, rect.max.y - 12.0),
            );
            let field =
                Rect::from_min_max(rect.min + vec2(10.0, 8.0), pos2(rect.max.x - 10.0, row.min.y - 6.0));
            let rows = ((field.height() - 16.0) / 20.0).floor().max(2.0) as usize;
            let mut box_ = ui.new_child(UiBuilder::new().max_rect(field));
            let edit = box_.add(TextArea::new(text).rows(rows).name("Recognised text"));
            if !*focused {
                edit.request_focus();
                *focused = true;
            }
            let ctrl_enter = ui.input(|i| i.modifiers.command && i.key_pressed(Key::Enter));
            if ctrl_enter {
                copy = true;
            }
            let mut buttons = ui.new_child(
                UiBuilder::new().max_rect(row).layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            buttons.add(Text::new("Ctrl+Enter copies · Esc closes").px(11.0).tone(Tone::Muted));
            buttons.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if ui.add(Button::primary("Copy")).clicked() {
                    copy = true;
                }
                if ui.add(Button::ghost("Cancel")).clicked() {
                    close = true;
                }
            });
        });
        if copy {
            let text = review.text.clone();
            if !text.trim().is_empty() {
                crate::tools::clip::copy(text);
                self.notify("Copied.", 1500);
            }
            close = true;
        }
        if (close || response.escape || response.closed)
            && let Some(r) = self.open_review.as_mut()
        {
            r.panel.close();
        }
    }
}

fn review_panel(text: &str) -> Review {
    let (w, h) = ocr::review_size(text);
    let screen = cursor_screen_rect().unwrap_or(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0)));
    let at = centred_on(vec2(w, h), screen);
    let mut panel = FloatingPanel::new("textgrab", TITLE, Geometry { x: at.x, y: at.y, w, h })
        .min_size(vec2(320.0, 180.0))
        .drag_needs_edit_mode(false);
    panel.opacity = 0.96;
    panel.set_title(TITLE, "check the text");
    panel.open();
    Review { panel, text: text.to_owned(), focused: false }
}

impl Module for TextGrabModule {
    fn key(&self) -> &'static str {
        "textgrab"
    }

    fn title(&self) -> &'static str {
        TITLE
    }

    fn start(&mut self, cx: &mut ModuleCx<'_>) -> Result<(), StartError> {
        self.keep_lines = cx.settings.get_bool("textgrab.keep_lines", true);
        self.upscale = cx.settings.get_bool("textgrab.upscale", true);
        self.review = cx.settings.get_bool("textgrab.review", false);
        self.running = true;
        Ok(())
    }

    fn stop(&mut self, _cx: &mut ModuleCx<'_>) {
        self.selecting = None;
        self.open_review = None;
        self.running = false;
    }

    fn actions(&self) -> Vec<Action> {
        vec![self.inbox.action("grab", "Grab text from the screen (OCR)", || Cmd::Grab(Instant::now()))]
    }
}

impl UiModule for TextGrabModule {
    fn binds(&self) -> Vec<Bind> {
        vec![Bind { setting: "hotkey", id: "grab", label: "grab text" }]
    }

    fn bind_pressed(&mut self, id: &str) {
        if id == "grab" {
            self.inbox.push(Cmd::Grab(Instant::now()));
        }
    }

    fn show(&mut self, cx: &mut UiCx<'_>) {
        if !self.running {
            return;
        }
        let ctx = cx.ctx.clone();
        self.take_commands(&ctx);
        if self.selecting.is_some() {
            self.draw_screens(&ctx);
        }
        if self.open_review.is_some() {
            self.draw_review(&ctx);
        }
    }
}
