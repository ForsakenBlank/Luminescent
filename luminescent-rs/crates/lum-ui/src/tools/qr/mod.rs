//! QR Code: the text or link on the clipboard as a QR code you can scan with a phone. The text box under the
//! code can be changed and the code follows; Copy image puts it on the clipboard as a picture, Save as writes a
//! PNG or an SVG. The code itself comes from `lum_core::qr`.

use std::path::PathBuf;
use std::time::Duration;

use egui::{Color32, Id, Rect, Role, Sense, Stroke, UiBuilder, WidgetInfo, pos2, vec2};
use lum_core::actions::Action;
use lum_core::module::{Module, ModuleCx, StartError};
use lum_core::qr::{self, Qr, QrError};
use lum_platform::{clipboard, dialogs};

use crate::dialog::{AskDialog, DialogWindow};
use crate::panel::{FloatingPanel, Geometry, centred_on};
use crate::text::Text;
use crate::theme::{Tokens, Tone};
use crate::tools::edit::cursor_screen_rect;
use crate::tools::{dialog_id, dialog_window};
use crate::widgets::{Button, TextArea};
use crate::{Bind, Inbox, Services, UiCx, UiModule};

const SIZE: (f32, f32) = (380.0, 540.0);
const MIN_SIZE: (f32, f32) = (340.0, 460.0);
/// How long after the last keystroke the code is made again.
const TYPING_PAUSE: Duration = Duration::from_millis(250);
/// Pixels to a module in the picture that is copied or saved.
const PICTURE_SCALE: usize = 10;

enum Cmd {
    /// The hotkey or the action: shows the panel with what is on the clipboard, or hides it.
    Show,
    FromClipboard,
    ClipboardText(String),
    CopyImage,
    SaveAs,
    Copied(Result<(), String>),
    Saved(Result<PathBuf, String>),
}

pub struct QrModule {
    inbox: Inbox<Cmd>,
    running: bool,
    panel: FloatingPanel,
    placed: bool,
    text: String,
    code: Result<Qr, QrError>,
    /// When the text last changed and the code has not been made from it yet.
    typed_at: Option<f64>,
    info: Option<String>,
    dialog: Option<DialogWindow>,
}

impl std::fmt::Debug for QrModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QrModule").field("running", &self.running).finish_non_exhaustive()
    }
}

impl QrModule {
    pub fn new(services: &Services) -> QrModule {
        let geometry = Geometry { x: 100.0, y: 100.0, w: SIZE.0, h: SIZE.1 };
        let mut panel = FloatingPanel::new("qr", "QR Code", geometry)
            .min_size(vec2(MIN_SIZE.0, MIN_SIZE.1))
            .drag_needs_edit_mode(false);
        panel.opacity = 0.97;
        QrModule {
            inbox: Inbox::new(services),
            running: false,
            panel,
            placed: false,
            text: String::new(),
            code: Err(QrError::Empty),
            typed_at: None,
            info: None,
            dialog: None,
        }
    }

    fn render(&mut self) {
        self.typed_at = None;
        self.code = Qr::make(&self.text);
        self.info = None;
    }

    /// What the line under the code says.
    fn line(&self) -> String {
        if let Some(info) = &self.info {
            return info.clone();
        }
        match &self.code {
            Ok(_) => qr::describe(&self.text),
            Err(e) => e.to_string(),
        }
    }

    fn show_panel(&mut self) {
        if self.panel.is_open() {
            self.panel.close();
            return;
        }
        if !self.placed {
            let screen =
                cursor_screen_rect().unwrap_or(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0)));
            let at = centred_on(vec2(SIZE.0, SIZE.1), screen);
            self.panel.geometry = Geometry { x: at.x, y: at.y, w: SIZE.0, h: SIZE.1 };
            self.placed = true;
        }
        self.panel.open();
        self.inbox.push(Cmd::FromClipboard);
    }

    fn on_thread(&self, name: &str, work: impl FnOnce() + Send + 'static) {
        if let Err(e) = std::thread::Builder::new().name(name.to_owned()).spawn(work) {
            tracing::warn!("could not start the {name} thread: {e}");
        }
    }

    fn take_commands(&mut self) {
        for cmd in self.inbox.take() {
            match cmd {
                Cmd::Show => self.show_panel(),
                Cmd::FromClipboard => {
                    let inbox = self.inbox.clone();
                    self.on_thread("qr-clipboard", move || {
                        let text = clipboard::text().unwrap_or_default().trim().to_owned();
                        inbox.push(Cmd::ClipboardText(text));
                    });
                }
                Cmd::ClipboardText(text) => {
                    self.text = text;
                    self.render();
                }
                Cmd::CopyImage => self.copy_image(),
                Cmd::SaveAs => self.save_as(),
                Cmd::Copied(Ok(())) => self.info = Some("QR code copied as a picture.".into()),
                Cmd::Copied(Err(e)) => self.info = Some(format!("Could not copy the picture: {e}")),
                Cmd::Saved(Ok(path)) => self.info = Some(format!("Saved {}", path.display())),
                Cmd::Saved(Err(e)) => {
                    let dialog = AskDialog::error(dialog_id("qr-save"), "Save QR code", &e);
                    self.dialog = Some(dialog_window(dialog));
                }
            }
        }
    }

    fn copy_image(&mut self) {
        let Ok(code) = &self.code else { return };
        let (code, inbox) = (code.clone(), self.inbox.clone());
        self.on_thread("qr-copy", move || {
            let (side, rgba) = code.rgba(PICTURE_SCALE);
            let picture = clipboard::Picture { width: side, height: side, rgba };
            inbox.push(Cmd::Copied(clipboard::set_picture(&picture)));
        });
    }

    fn save_as(&mut self) {
        let Ok(code) = &self.code else { return };
        let (code, inbox) = (code.clone(), self.inbox.clone());
        let pictures = lum_core::paths::user_home().join("Pictures");
        let folder = if pictures.is_dir() { pictures } else { lum_core::paths::user_home() };
        let request = dialogs::Request::new("Save the QR code")
            .folder(Some(folder))
            .file_name("qr-code.png")
            .filter("PNG image", &["png"])
            .filter("SVG", &["svg"]);
        dialogs::save_file(request, move |path| {
            let Some(path) = path else { return };
            inbox.push(Cmd::Saved(write_code(&code, path)));
        });
    }

    fn draw_panel(&mut self, cx: &mut UiCx<'_>) {
        let line = self.line();
        let now = cx.ctx.input(|i| i.time);
        let mut edited = false;
        let mut wanted = None;
        let (text, code) = (&mut self.text, &self.code);
        let response = self.panel.show(cx.ctx, |ui| {
            let rect = ui.max_rect();
            let inner = Rect::from_min_max(rect.min + vec2(16.0, 14.0), rect.max - vec2(16.0, 16.0));
            let row = Rect::from_min_max(pos2(inner.min.x, inner.max.y - 32.0), inner.max);
            let box_ = Rect::from_min_max(
                pos2(inner.min.x, row.min.y - 10.0 - 70.0),
                pos2(inner.max.x, row.min.y - 10.0),
            );
            let info = Rect::from_min_max(
                pos2(inner.min.x, box_.min.y - 10.0 - 30.0),
                pos2(inner.max.x, box_.min.y - 10.0),
            );
            let area = Rect::from_min_max(inner.min, pos2(inner.max.x, info.min.y - 10.0));
            paint_code(ui, area, code.as_ref().ok());

            let mut at = ui.new_child(UiBuilder::new().max_rect(info));
            at.add(Text::new(line.clone()).px(11.0).tone(Tone::Muted).wrap());

            let mut field = ui.new_child(UiBuilder::new().max_rect(box_));
            let edit = field.add(TextArea::new(text).hint("Text or a link to turn into a QR code").rows(3));
            edited = edit.changed();

            let mut buttons = ui.new_child(
                UiBuilder::new().max_rect(row).layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            buttons.spacing_mut().item_spacing.x = 8.0;
            if buttons.add(Button::ghost("From clipboard")).clicked() {
                wanted = Some(Cmd::FromClipboard);
            }
            buttons.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if ui.add(Button::new("Save as…")).clicked() {
                    wanted = Some(Cmd::SaveAs);
                }
                if ui.add(Button::primary("Copy image")).clicked() {
                    wanted = Some(Cmd::CopyImage);
                }
            });
        });
        if edited {
            self.typed_at = Some(now);
            self.info = None;
        }
        if let Some(cmd) = wanted {
            self.inbox.push(cmd);
        }
        if let Some(at) = self.typed_at {
            let waited = Duration::from_secs_f64((now - at).max(0.0));
            if waited >= TYPING_PAUSE {
                self.render();
            } else {
                cx.ctx.request_repaint_after(TYPING_PAUSE - waited);
            }
        }
        if response.closed || response.escape {
            self.panel.close();
        }
    }
}

/// The code on a white card, as big as the room allows, drawn module by module.
fn paint_code(ui: &mut egui::Ui, area: Rect, code: Option<&Qr>) {
    let tokens = Tokens::of(ui.ctx());
    let response = ui.interact(area, Id::unique("lum-qr-code"), Sense::hover());
    response.widget_info(|| WidgetInfo::labeled(Role::Image, true, "QR code"));
    ui.painter().rect_filled(area, 8.0, Color32::WHITE);
    let Some(code) = code else {
        ui.painter().rect_stroke(area, 8.0, Stroke::new(1.0, tokens.p.border), egui::StrokeKind::Inside);
        return;
    };
    let side = (area.width().min(area.height()) - 16.0).max(40.0);
    let modules = (code.width() + 2 * qr::BORDER) as f32;
    let each = side / modules;
    let origin = area.center() - vec2(side, side) / 2.0;
    let edge = |v: f32| (v * each).round();
    for y in 0..code.width() {
        let mut x = 0;
        while x < code.width() {
            if !code.is_dark(x as isize, y as isize) {
                x += 1;
                continue;
            }
            let start = x;
            while x < code.width() && code.is_dark(x as isize, y as isize) {
                x += 1;
            }
            let (left, right) = (edge((start + qr::BORDER) as f32), edge((x + qr::BORDER) as f32));
            let (top, bottom) = (edge((y + qr::BORDER) as f32), edge((y + qr::BORDER + 1) as f32));
            let run = Rect::from_min_max(origin + vec2(left, top), origin + vec2(right, bottom));
            ui.painter().rect_filled(run, 0.0, Color32::BLACK);
        }
    }
}

/// Writes the code to `path`: an SVG when the name ends in `.svg`, otherwise a PNG (`.png` is added when the
/// name has no extension of that kind).
fn write_code(code: &Qr, mut path: PathBuf) -> Result<PathBuf, String> {
    let is_svg = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg"));
    if is_svg {
        std::fs::write(&path, code.svg(PICTURE_SCALE)).map_err(|e| e.to_string())?;
        return Ok(path);
    }
    if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
        let mut name = path.clone().into_os_string();
        name.push(".png");
        path = PathBuf::from(name);
    }
    let (side, rgba) = code.rgba(PICTURE_SCALE);
    let image =
        image::RgbaImage::from_raw(side as u32, side as u32, rgba).ok_or("could not write the file")?;
    image.save_with_format(&path, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(path)
}

impl Module for QrModule {
    fn key(&self) -> &'static str {
        "qr"
    }

    fn title(&self) -> &'static str {
        "QR Code"
    }

    fn start(&mut self, _cx: &mut ModuleCx<'_>) -> Result<(), StartError> {
        self.running = true;
        Ok(())
    }

    fn stop(&mut self, _cx: &mut ModuleCx<'_>) {
        self.panel.close();
        self.dialog = None;
        self.running = false;
    }

    fn actions(&self) -> Vec<Action> {
        vec![self.inbox.action("clipboard", "QR code of the clipboard", || Cmd::Show)]
    }
}

impl UiModule for QrModule {
    fn binds(&self) -> Vec<Bind> {
        vec![Bind { setting: "hotkey", id: "clipboard", label: "QR code of the clipboard" }]
    }

    fn bind_pressed(&mut self, id: &str) {
        if id == "clipboard" {
            self.inbox.push(Cmd::Show);
        }
    }

    fn show(&mut self, cx: &mut UiCx<'_>) {
        if !self.running {
            return;
        }
        self.take_commands();
        if let Some(dialog) = self.dialog.as_mut()
            && dialog.show(cx.ctx) != crate::dialog::Outcome::Open
        {
            self.dialog = None;
        }
        if self.panel.is_open() {
            self.draw_panel(cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_without_an_extension_gets_png_and_svg_is_kept() {
        let dir = tempfile::tempdir().expect("a temp folder");
        let code = Qr::make("hi").expect("a code");
        let png = write_code(&code, dir.path().join("code")).expect("written");
        assert_eq!(png.file_name().and_then(|n| n.to_str()), Some("code.png"));
        assert!(image::open(&png).is_ok());
        let svg = write_code(&code, dir.path().join("code.SVG")).expect("written");
        assert!(std::fs::read_to_string(svg).expect("read").starts_with("<?xml"));
        let jpg_name = write_code(&code, dir.path().join("code.jpg")).expect("written");
        assert_eq!(jpg_name.file_name().and_then(|n| n.to_str()), Some("code.jpg.png"));
    }
}
