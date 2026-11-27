//! Screen Doodle's model: the tools, its settings, the marks a drawing is made of, one canvas per screen,
//! the undo history all screens share and the rules for the hold-to-draw key.

pub mod canvas;
pub mod compose;
pub mod hold;
pub mod mark;
pub mod strip;

pub use canvas::Canvas;
pub use hold::{Effect, Hold};
pub use mark::{Mark, ShapeKind};

use std::collections::VecDeque;

use lum_theme::colour::Rgb;
use serde_json::Value;

/// A tool on the strip: its id (as the settings store it), its glyph and its tooltip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    Pen,
    Highlighter,
    Line,
    Rect,
    Ellipse,
    Arrow,
    Text,
}

impl Tool {
    pub const ALL: [Tool; 7] =
        [Tool::Pen, Tool::Highlighter, Tool::Line, Tool::Rect, Tool::Ellipse, Tool::Arrow, Tool::Text];

    pub fn id(self) -> &'static str {
        match self {
            Tool::Pen => "pen",
            Tool::Highlighter => "hl",
            Tool::Line => "line",
            Tool::Rect => "rect",
            Tool::Ellipse => "ellipse",
            Tool::Arrow => "arrow",
            Tool::Text => "text",
        }
    }

    pub fn glyph(self) -> &'static str {
        match self {
            Tool::Pen => "✎",
            Tool::Highlighter => "▌",
            Tool::Line => "╱",
            Tool::Rect => "▭",
            Tool::Ellipse => "○",
            Tool::Arrow => "→",
            Tool::Text => "T",
        }
    }

    pub fn tooltip(self) -> &'static str {
        match self {
            Tool::Pen => "Pen (Shift-drag: straight line)",
            Tool::Highlighter => "Highlighter",
            Tool::Line => "Line",
            Tool::Rect => "Rectangle",
            Tool::Ellipse => "Circle / ellipse",
            Tool::Arrow => "Arrow",
            Tool::Text => "Text",
        }
    }

    pub fn from_id(id: &str) -> Option<Tool> {
        Tool::ALL.into_iter().find(|t| t.id() == id)
    }

    /// Rectangles and ellipses can be filled.
    pub fn fillable(self) -> bool {
        matches!(self, Tool::Rect | Tool::Ellipse)
    }
}

/// The doodle section of the settings, read once when the module starts.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub hotkey: String,
    pub hold_mode: bool,
    pub solo_only: bool,
    pub solo_delay_ms: u64,
    pub clear_on_release: bool,
    pub clear_hotkey: String,
    pub undo_hotkey: String,
    pub bg_hotkey: String,
    pub exit_hotkey: String,
    pub copy_hotkey: String,
    pub save_copies: bool,
    pub show_tools: bool,
    /// `None`: the tool used last.
    pub start_tool: Option<Tool>,
    pub highlighter_opacity: f32,
    pub text_size: u32,
    pub wheel_size: bool,
    pub pixel_brush: bool,
    pub colour: Rgb,
    pub brush_size: u32,
    pub pixel_size: u32,
    pub bg_opacity: f32,
    pub passive_opacity: f32,
    pub undo_levels: usize,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings::from_section(&Value::Null)
    }
}

impl Settings {
    /// From the `doodle` section (missing keys take Python's defaults).
    pub fn from_section(section: &Value) -> Settings {
        let s = |k: &str, d: &str| section.get(k).and_then(Value::as_str).unwrap_or(d).to_owned();
        let b = |k: &str, d: bool| section.get(k).and_then(Value::as_bool).unwrap_or(d);
        let n = |k: &str, d: i64, lo: i64, hi: i64| {
            section.get(k).and_then(Value::as_i64).unwrap_or(d).min(hi).max(lo)
        };
        let f = |k: &str, d: f64| section.get(k).and_then(Value::as_f64).unwrap_or(d) as f32;
        Settings {
            hotkey: s("hotkey", "alt"),
            hold_mode: s("mode", "hold") == "hold",
            solo_only: b("solo_only", true),
            solo_delay_ms: n("solo_delay_ms", 120, 0, 1000) as u64,
            clear_on_release: b("clear_on_release", false),
            clear_hotkey: s("clear_hotkey", "alt+x"),
            undo_hotkey: s("undo_hotkey", "alt+f"),
            bg_hotkey: s("bg_hotkey", "alt+g"),
            exit_hotkey: s("exit_hotkey", "alt+shift+q"),
            copy_hotkey: s("copy_hotkey", "alt+c"),
            save_copies: b("save_copies", false),
            show_tools: b("show_tools", true),
            start_tool: Tool::from_id(&s("start_tool", "last")),
            highlighter_opacity: f("highlighter_opacity", 0.35),
            text_size: n("text_size", 28, 8, 200) as u32,
            wheel_size: b("wheel_size", true),
            pixel_brush: s("brush_style", "smooth") == "pixel",
            colour: Rgb::parse(&s("colour", "#FF0000")).unwrap_or(Rgb::new(255, 0, 0)),
            brush_size: n("brush_size", 4, 1, 60) as u32,
            pixel_size: n("pixel_size", 4, 1, 60) as u32,
            bg_opacity: f("bg_opacity", 1.0),
            passive_opacity: f("passive_opacity", 0.01),
            undo_levels: n("undo_levels", 30, 1, 200) as usize,
        }
    }

    /// The brush size in use: the pixel size with the pixel brush, else the brush size.
    pub fn effective_size(&self) -> u32 {
        if self.pixel_brush { self.pixel_size.max(1) } else { self.brush_size.max(1) }
    }

    /// The setting the wheel changes (`brush_size` or `pixel_size`).
    pub fn size_key(&self) -> &'static str {
        if self.pixel_brush { "pixel_size" } else { "brush_size" }
    }

    /// Turns the size by `steps` wheel notches, between 1 and 60, and gives the new size.
    pub fn change_size(&mut self, steps: i32) -> u32 {
        let now = self.effective_size() as i32;
        let n = (now + steps).clamp(1, 60) as u32;
        if self.pixel_brush {
            self.pixel_size = n;
        } else {
            self.brush_size = n;
        }
        n
    }

    /// The keys of the doodle's own shortcuts besides the draw key (x, f, g, c, shift, q, esc): pressing one
    /// while the draw key is held is not "using an Alt shortcut".
    pub fn own_keys(&self, normalise: impl Fn(&str) -> String) -> Vec<String> {
        let hold = normalise(&self.hotkey);
        let mut keys = vec!["esc".to_owned()];
        for combo in
            [&self.clear_hotkey, &self.undo_hotkey, &self.bg_hotkey, &self.exit_hotkey, &self.copy_hotkey]
        {
            for part in normalise(combo).split('+') {
                if !part.is_empty() && part != hold && !keys.iter().any(|k| k == part) {
                    keys.push(part.to_owned());
                }
            }
        }
        keys
    }
}

/// One undo history for every screen: Alt+F undoes the last stroke wherever it was drawn. Each step lists the
/// screens it drew on (a stroke: one, Clear: all of them).
#[derive(Debug, Default)]
pub struct History {
    steps: VecDeque<Vec<usize>>,
}

impl History {
    /// Records a step; gives back the screens of steps dropped off the end (their oldest mark can no longer be
    /// undone, so it is baked into the canvas).
    pub fn record(&mut self, screens: Vec<usize>, limit: usize) -> Vec<usize> {
        self.steps.push_back(screens);
        let mut dropped = Vec::new();
        while self.steps.len() > limit.max(1) {
            dropped.extend(self.steps.pop_front().unwrap_or_default());
        }
        dropped
    }

    /// The screens of the last step, taken off the history.
    pub fn undo(&mut self) -> Option<Vec<usize>> {
        self.steps.pop_back()
    }

    pub fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn clear(&mut self) {
        self.steps.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn settings_take_python_defaults_and_limits() {
        let s = Settings::default();
        assert_eq!((s.hotkey.as_str(), s.hold_mode, s.solo_delay_ms, s.undo_levels), ("alt", true, 120, 30));
        assert_eq!(s.colour, Rgb::new(255, 0, 0));
        assert_eq!(s.start_tool, None);
        let s = Settings::from_section(
            &json!({"brush_style": "pixel", "pixel_size": 99, "start_tool": "hl", "mode": "toggle"}),
        );
        assert_eq!(
            (s.effective_size(), s.size_key(), s.start_tool, s.hold_mode),
            (60, "pixel_size", Some(Tool::Highlighter), false)
        );
    }

    #[test]
    fn the_wheel_keeps_the_size_between_1_and_60() {
        let mut s = Settings::default();
        assert_eq!(s.change_size(3), 7);
        assert_eq!(s.change_size(-100), 1);
        assert_eq!(s.change_size(500), 60);
        assert_eq!(s.brush_size, 60);
    }

    #[test]
    fn own_keys_leave_out_the_draw_key() {
        let s = Settings::default();
        let keys = s.own_keys(|c| c.to_lowercase());
        assert_eq!(keys, ["esc", "x", "f", "g", "shift", "q", "c"]);
    }

    #[test]
    fn one_history_for_every_screen() {
        let mut h = History::default();
        assert!(h.record(vec![0], 3).is_empty());
        assert!(h.record(vec![1], 3).is_empty());
        assert!(h.record(vec![0, 1], 3).is_empty());
        // the oldest step (screen 0's stroke) falls off
        assert_eq!(h.record(vec![1], 3), [0]);
        assert_eq!(h.undo(), Some(vec![1]));
        assert_eq!(h.undo(), Some(vec![0, 1]));
        assert_eq!(h.len(), 1);
    }
}
