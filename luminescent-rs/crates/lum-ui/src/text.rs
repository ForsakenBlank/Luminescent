//! Text in the design's type scale, like `widgets.label(text, px, weight, mono, spacing, tone)`.
//!
//! Sizes are the design's pixel sizes and follow the Text size setting. Letter spacing is in em, as in the
//! mock-ups (0.16 is ".16em"): the mono caps labels over boxes and panels use it. Colours come from a tone,
//! so a label restyles with the theme on the next frame.

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, Response, Sense, Ui, Widget, WidgetText};

use crate::fonts::Weight;
use crate::theme::{Tokens, Tone};

#[derive(Debug, Clone)]
#[must_use = "add it with ui.add(...)"]
pub struct Text {
    text: String,
    px: f32,
    weight: Weight,
    mono: bool,
    spacing: f32,
    tone: Tone,
    colour: Option<Color32>,
    wrap: bool,
    elide: bool,
    sense: Sense,
}

impl Text {
    pub fn new(text: impl Into<String>) -> Text {
        Text {
            text: text.into(),
            px: 13.0,
            weight: Weight::Regular,
            mono: false,
            spacing: 0.0,
            tone: Tone::Primary,
            colour: None,
            wrap: false,
            elide: false,
            sense: Sense::hover(),
        }
    }

    pub fn px(mut self, px: f32) -> Text {
        self.px = px;
        self
    }

    pub fn weight(mut self, weight: Weight) -> Text {
        self.weight = weight;
        self
    }

    pub fn medium(self) -> Text {
        self.weight(Weight::Medium)
    }

    pub fn semibold(self) -> Text {
        self.weight(Weight::SemiBold)
    }

    pub fn mono(mut self) -> Text {
        self.mono = true;
        self
    }

    /// Letter spacing in em.
    pub fn spacing(mut self, em: f32) -> Text {
        self.spacing = em;
        self
    }

    pub fn tone(mut self, tone: Tone) -> Text {
        self.tone = tone;
        self
    }

    /// A colour that is not one of the tones (a status colour, text on an accent background).
    pub fn colour(mut self, colour: Color32) -> Text {
        self.colour = Some(colour);
        self
    }

    /// Break onto more lines where the width runs out.
    pub fn wrap(mut self) -> Text {
        self.wrap = true;
        self.elide = false;
        self
    }

    /// One line that ends in "…" when it does not fit, like `ElidedLabel`.
    pub fn elide(mut self) -> Text {
        self.elide = true;
        self.wrap = false;
        self
    }

    pub fn sense(mut self, sense: Sense) -> Text {
        self.sense = sense;
        self
    }

    pub fn format(&self, tokens: &Tokens) -> TextFormat {
        let size = tokens.fs(self.px);
        TextFormat {
            font_id: tokens.font(self.px, self.weight, self.mono),
            extra_letter_spacing: size * self.spacing,
            color: self.colour.unwrap_or_else(|| tokens.tone(self.tone)),
            ..Default::default()
        }
    }

    pub fn job(&self, tokens: &Tokens) -> LayoutJob {
        LayoutJob::single_section(self.text.clone(), self.format(tokens))
    }

    /// Ready for anything that takes egui's `WidgetText` (a button of egui's own, a combo entry).
    pub fn into_widget_text(self, tokens: &Tokens) -> WidgetText {
        WidgetText::LayoutJob(self.job(tokens).into())
    }
}

impl Widget for Text {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::of(ui.ctx());
        let mut label = egui::Label::new(self.job(&tokens)).selectable(false).sense(self.sense);
        label = if self.wrap {
            label.wrap()
        } else if self.elide {
            label.truncate()
        } else {
            label.extend()
        };
        ui.add(label)
    }
}

/// Body text in a tone.
pub fn label(text: impl Into<String>) -> Text {
    Text::new(text)
}

/// The mono caps heading used over panels, boxes and dialogs: "NOTES", "CLIPBOARD".
pub fn caps(text: impl AsRef<str>) -> Text {
    Text::new(text.as_ref().to_uppercase()).px(10.5).semibold().mono().spacing(0.16)
}

/// A small muted line under a control, like `#lumHint`.
pub fn hint(text: impl Into<String>) -> Text {
    Text::new(text).px(12.0).tone(Tone::Muted)
}

/// Monospace figures: counts, hotkeys, times.
pub fn mono(text: impl Into<String>) -> Text {
    Text::new(text).px(12.0).mono()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacing_is_in_em_of_the_scaled_size() {
        let tokens = Tokens::new(crate::Palette::dark(), 1.25);
        let f = caps("notes").format(&tokens);
        assert!((f.font_id.size - 13.125).abs() < 1e-4);
        assert!((f.extra_letter_spacing - 13.125 * 0.16).abs() < 1e-4);
        assert_eq!(caps("notes").job(&tokens).text, "NOTES");
        assert_eq!(hint("x").format(&tokens).color, tokens.p.text_muted);
    }
}
