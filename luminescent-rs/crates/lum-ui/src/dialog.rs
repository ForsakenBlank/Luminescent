//! Dialogs in Luminescent's own look (`RoundedDialog`, `_AskDialog`): rounded corners a little larger than
//! the theme's radius, a header with the sparkle, a mono caps title and ✕, and the buttons on the right.
//!
//! They are modal layers inside the window that opens them, drawn each frame while open. The caller keeps the
//! dialog value and asks [`AskDialog::show`] each frame until it gives an answer.

use egui::text::{CCursor, CCursorRange};
use egui::{
    Align, Context, Frame, Id, Key, Layout, Modal, Rect, ScrollArea, Shape, Stroke, StrokeKind, Ui,
    UiBuilder, vec2,
};

use crate::anim::{self, Ease, Kind};
use crate::fonts::Weight;
use crate::paint;
use crate::text::{Text, caps};
use crate::theme::{DANGER, Role, Tokens, alpha};
use crate::widgets::Combo;
use crate::widgets::button::{Button, IconButton, Variant};
use crate::widgets::input::TextInput;
use crate::widgets::when::{self, WhenState};

/// The round badge left of the message, as `_AskDialog.GLYPHS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Icon {
    #[default]
    None,
    Information,
    Warning,
    Critical,
    Question,
}

impl Icon {
    fn glyph(self) -> Option<&'static str> {
        match self {
            Icon::None => None,
            Icon::Information => Some("✦"),
            Icon::Warning => Some("!"),
            Icon::Critical => Some("✕"),
            Icon::Question => Some("?"),
        }
    }
}

/// What happened to a dialog this frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Still open.
    Open,
    /// A button that accepts was pressed (its label), or Enter on the default one.
    Accepted(String),
    /// Cancel, ✕ or Esc.
    Cancelled,
}

/// Lays out a rounded dialog: the frame, the header and a body with the usual margins. `width` is the width of
/// the whole dialog. Gives back what `body` returned and whether ✕ or Esc asked to close it.
pub fn rounded<R>(
    ctx: &Context,
    id: Id,
    title: &str,
    width: f32,
    body: impl FnOnce(&mut Ui) -> R,
) -> (R, bool) {
    let tokens = Tokens::of(ctx);
    let fade = anim::value_from(ctx, id.with("fade"), 0.0, 1.0, Kind::Panels, 150.0, Ease::Out);
    let mut close = false;
    let modal = Modal::new(id).frame(Frame::NONE).backdrop_color(alpha(tokens.scrim(), 0.35 * fade)).show(
        ctx,
        |ui| {
            ui.multiply_opacity(fade);
            ui.set_width(width);
            let background = ui.painter().add(Shape::Noop);
            let top = ui.cursor().min;
            // the header
            let header = Rect::from_min_size(top, vec2(width, 40.0));
            ui.allocate_rect(header, egui::Sense::hover());
            let painter = ui.painter();
            paint::sparkle(painter, header.left_center() + vec2(22.0, 0.0), 12.0, tokens.p.primary);
            let title_galley = painter.layout_job(caps(title).job(&tokens));
            painter.galley(
                egui::pos2(header.min.x + 36.0, header.center().y - title_galley.size().y / 2.0),
                title_galley,
                tokens.p.text_primary,
            );
            let close_rect =
                Rect::from_center_size(egui::pos2(header.max.x - 21.0, header.center().y), vec2(26.0, 26.0));
            if ui.put(close_rect, IconButton::close().tooltip("Close (Esc)")).clicked() {
                close = true;
            }
            ui.painter().hline(header.x_range(), header.max.y - 0.5, Stroke::new(1.0, tokens.p.border));
            // the body, 16 px in from the sides, 14 from the header, 16 from the bottom. Its height is bounded
            // by the screen: a row laid out right to left centres itself in it
            let bottom_limit = ui.ctx().content_rect().max.y.max(header.max.y + 100.0);
            let body_rect = Rect::from_min_max(
                egui::pos2(top.x + 16.0, header.max.y + 14.0),
                egui::pos2(top.x + width - 16.0, bottom_limit),
            );
            let mut child =
                ui.new_child(UiBuilder::new().max_rect(body_rect).layout(Layout::top_down(Align::Min)));
            child.spacing_mut().item_spacing.y = 10.0;
            let inner = body(&mut child);
            let bottom = child.min_rect().max.y + 16.0;
            let frame = Rect::from_min_max(top, egui::pos2(top.x + width, bottom));
            ui.allocate_rect(
                Rect::from_min_max(egui::pos2(top.x, header.max.y), frame.max),
                egui::Sense::hover(),
            );
            let radius = tokens.r() + 4.0;
            ui.painter().set(
                background,
                Shape::Vec(vec![
                    paint::fill_shape(ui.ctx(), frame, radius, &tokens, Role::BgBase, 1.0),
                    Shape::rect_stroke(frame, radius, Stroke::new(1.0, tokens.p.border), StrokeKind::Inside),
                ]),
            );
            if ui.input(|i| i.key_pressed(Key::Escape)) {
                close = true;
            }
            inner
        },
    );
    if close {
        anim::forget(ctx, id.with("fade"));
    }
    (modal.inner, close)
}

#[derive(Debug, Clone)]
struct DialogButton {
    label: String,
    variant: Variant,
    accept: bool,
    default: bool,
}

/// A message box or question: the text, an optional input, and a row of buttons.
#[derive(Debug, Clone)]
pub struct AskDialog {
    id: Id,
    title: String,
    text: String,
    icon: Icon,
    buttons: Vec<DialogButton>,
    /// The text box of `ask_text`, with what is typed in it.
    input: Option<String>,
    /// The drop-down of `ask_choice`: the entries and the one picked.
    choice: Option<(Vec<String>, usize)>,
    /// The picker of `ask_datetime`.
    when: Option<WhenState>,
    focused: bool,
}

impl AskDialog {
    fn new(id: Id, title: &str, text: &str, icon: Icon) -> AskDialog {
        AskDialog {
            id,
            title: title.to_owned(),
            text: text.to_owned(),
            icon,
            buttons: Vec::new(),
            input: None,
            choice: None,
            when: None,
            focused: false,
        }
    }

    fn button(mut self, label: &str, variant: Variant, accept: bool, default: bool) -> AskDialog {
        self.buttons.push(DialogButton { label: label.to_owned(), variant, accept, default });
        self
    }

    /// A message with OK.
    pub fn message(id: Id, title: &str, text: &str, icon: Icon) -> AskDialog {
        AskDialog::new(id, title, text, icon).button("OK", Variant::Primary, true, true)
    }

    /// Something went wrong: a message with the warning badge.
    pub fn error(id: Id, title: &str, text: &str) -> AskDialog {
        AskDialog::message(id, title, text, Icon::Warning)
    }

    /// Yes or no. A dangerous yes is red and not the default, so Enter does not delete anything by accident.
    pub fn confirm(id: Id, title: &str, text: &str, yes: &str, no: &str, danger: bool) -> AskDialog {
        AskDialog::new(id, title, text, Icon::Question).button(no, Variant::Ghost, false, danger).button(
            yes,
            if danger { Variant::Danger } else { Variant::Primary },
            true,
            !danger,
        )
    }

    /// Several answers plus Cancel; the last one is the main one.
    pub fn choose(id: Id, title: &str, text: &str, answers: &[&str], cancel: &str) -> AskDialog {
        let mut dialog =
            AskDialog::new(id, title, text, Icon::None).button(cancel, Variant::Ghost, false, false);
        for (i, answer) in answers.iter().enumerate() {
            let last = i + 1 == answers.len();
            dialog = dialog.button(answer, if last { Variant::Primary } else { Variant::Normal }, true, last);
        }
        dialog
    }

    /// A line of text to type, with Cancel and OK.
    pub fn ask_text(id: Id, title: &str, label: &str, default: &str) -> AskDialog {
        let mut dialog = AskDialog::new(id, title, label, Icon::None)
            .button("Cancel", Variant::Ghost, false, false)
            .button("OK", Variant::Primary, true, true);
        dialog.input = Some(default.to_owned());
        dialog
    }

    /// A drop-down with Cancel and OK (`ask_choice`).
    pub fn ask_choice(id: Id, title: &str, label: &str, options: &[&str]) -> AskDialog {
        let mut dialog = AskDialog::new(id, title, label, Icon::None)
            .button("Cancel", Variant::Ghost, false, false)
            .button("OK", Variant::Primary, true, true);
        dialog.choice = Some((options.iter().map(|o| (*o).to_owned()).collect(), 0));
        dialog
    }

    /// A date and time to pick, starting at `at` (`ask_datetime`).
    pub fn ask_datetime(id: Id, title: &str, label: &str, at: jiff::civil::DateTime) -> AskDialog {
        let mut dialog = AskDialog::new(id, title, label, Icon::None)
            .button("Cancel", Variant::Ghost, false, false)
            .button("OK", Variant::Primary, true, true);
        dialog.when = Some(WhenState::new(at));
        dialog
    }

    pub fn id(&self) -> Id {
        self.id
    }

    /// The entry picked in an `ask_choice` dialog.
    pub fn chosen(&self) -> &str {
        self.choice.as_ref().and_then(|(options, at)| options.get(*at)).map_or("", String::as_str)
    }

    /// The moment picked in an `ask_datetime` dialog.
    pub fn when(&self) -> Option<jiff::civil::DateTime> {
        self.when.as_ref().map(WhenState::value)
    }

    /// What is typed in an `ask_text` dialog.
    pub fn text(&self) -> &str {
        self.input.as_deref().unwrap_or("")
    }

    pub fn show(&mut self, ctx: &Context) -> Outcome {
        let tokens = Tokens::of(ctx);
        let width = if self.text.chars().count() < 400 { 460.0 } else { 600.0 };
        let room = ctx.content_rect().height() * 0.55;
        let mut outcome = Outcome::Open;
        let (_, closed) = rounded(ctx, self.id, &self.title.clone(), width, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                if let Some(glyph) = self.icon.glyph() {
                    let colour = if matches!(self.icon, Icon::Warning | Icon::Critical) {
                        DANGER
                    } else {
                        tokens.p.primary
                    };
                    let (bubble, _) = ui.allocate_exact_size(vec2(30.0, 30.0), egui::Sense::hover());
                    ui.painter().circle_filled(bubble.center(), 15.0, alpha(colour, 0.16));
                    ui.painter().text(
                        bubble.center(),
                        egui::Align2::CENTER_CENTER,
                        glyph,
                        tokens.font(14.0, Weight::SemiBold, false),
                        colour,
                    );
                }
                // long text scrolls instead of being cut off
                ScrollArea::vertical().max_height(room).auto_shrink([false, true]).show(ui, |ui| {
                    ui.add(
                        egui::Label::new(Text::new(&self.text).px(13.5).job(&tokens)).wrap().selectable(true),
                    );
                });
            });
            let enter = if let Some(input) = self.input.as_mut() {
                let input_id = self.id.with("input");
                let response = ui.add(TextInput::new(input).id(input_id).name(self.text.clone()));
                if !self.focused {
                    self.focused = true;
                    response.request_focus();
                    if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), input_id) {
                        let end = CCursor::new(input.chars().count());
                        state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(0), end)));
                        state.store(ui.ctx(), input_id);
                    }
                }
                response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))
            } else {
                ui.input(|i| i.key_pressed(Key::Enter))
            };
            if let Some((options, selected)) = self.choice.as_mut() {
                ui.add(Combo::new(self.id.with("choice"), selected, options).width(width - 32.0));
            }
            if let Some(when) = self.when.as_mut() {
                when::show(ui, when, jiff::ZonedDateTime::now().date());
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    for button in self.buttons.iter().rev() {
                        let clicked = ui
                            .add(Button::new(&button.label).variant(button.variant).min_width(84.0))
                            .clicked();
                        if clicked || (enter && button.default) {
                            outcome = if button.accept {
                                Outcome::Accepted(button.label.clone())
                            } else {
                                Outcome::Cancelled
                            };
                        }
                    }
                });
            });
        });
        if closed {
            outcome = Outcome::Cancelled;
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_order_and_defaults_follow_python() {
        let d = AskDialog::confirm(Id::unique("c"), "Delete", "Delete it?", "Delete", "Keep", true);
        let labels: Vec<_> = d.buttons.iter().map(|b| (b.label.as_str(), b.variant, b.default)).collect();
        assert_eq!(labels, [("Keep", Variant::Ghost, true), ("Delete", Variant::Danger, false)]);
        let d = AskDialog::choose(Id::unique("c"), "Paste", "How?", &["Text", "Image"], "Cancel");
        assert_eq!(d.buttons.last().map(|b| (b.label.as_str(), b.default)), Some(("Image", true)));
        assert_eq!(AskDialog::ask_text(Id::unique("a"), "Rename", "Name", "Notes").text(), "Notes");
    }

    #[test]
    fn a_choice_starts_on_its_first_entry_and_a_date_dialog_on_the_given_moment() {
        let d = AskDialog::ask_choice(Id::unique("t"), "Type", "Choose type:", &["note", "todo"]);
        assert_eq!(d.chosen(), "note");
        let at: jiff::civil::DateTime = "2026-11-24T09:00".parse().expect("a date and time");
        let d = AskDialog::ask_datetime(Id::unique("d"), "Due date", "When?", at);
        assert_eq!(d.when(), Some(at));
        assert_eq!(AskDialog::ask_text(Id::unique("a"), "Rename", "Name", "x").when(), None);
    }

    #[test]
    fn enter_accepts_the_default_and_escape_cancels() {
        let ctx = Context::default();
        crate::setup(&ctx, Tokens::dark());
        let mut dialog = AskDialog::message(Id::unique("m"), "Saved", "All done.", Icon::Information);
        let mut run = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1000.0, 700.0))),
                events,
                ..Default::default()
            };
            let mut seen = Outcome::Open;
            let _ = crate::headless_pass(&ctx, input, |ui| seen = dialog.show(ui.ctx()));
            seen
        };
        assert_eq!(run(Vec::new()), Outcome::Open);
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        };
        assert_eq!(run(vec![key(Key::Enter)]), Outcome::Accepted("OK".to_owned()));
        assert_eq!(run(vec![key(Key::Escape)]), Outcome::Cancelled);
    }
}

/// A dialog with nothing of ours under it (Quit from the tray or a hotkey): a see-through window of its own over
/// the screen with the mouse, with the dialog in the middle.
#[derive(Debug)]
pub struct DialogWindow {
    pub dialog: AskDialog,
    screen: Option<crate::window::Screen>,
    fresh: bool,
}

/// Dialog windows are titled per screen, so on Hyprland a cover rule opens them full size on it.
pub const COVER_PREFIX: &str = "Luminescent dialog";

impl DialogWindow {
    pub fn new(dialog: AskDialog, screen: Option<crate::window::Screen>) -> DialogWindow {
        DialogWindow { dialog, screen, fresh: true }
    }

    pub fn show(&mut self, ctx: &Context) -> Outcome {
        if ctx.embed_viewports() {
            return self.dialog.show(ctx);
        }
        let screen = self.screen.clone().unwrap_or_else(|| crate::window::fallback_screen(ctx));
        let title = lum_platform::desktop::cover_title(COVER_PREFIX, &screen.name);
        let builder = egui::ViewportBuilder::default()
            .with_title(title.clone())
            .with_app_id(lum_platform::desktop::APP_ID)
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_taskbar(false)
            .with_resizable(false)
            .with_active(true)
            .with_position(screen.rect.min)
            .with_inner_size(screen.rect.size());
        let mut outcome = Outcome::Open;
        let dialog = &mut self.dialog;
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of(dialog.id()), builder, |ui, _| {
            outcome = dialog.show(ui.ctx());
        });
        crate::window::shown(ctx, &title, None, true);
        if std::mem::take(&mut self.fresh) {
            crate::window::placer(ctx).focus(&title);
        }
        outcome
    }
}
