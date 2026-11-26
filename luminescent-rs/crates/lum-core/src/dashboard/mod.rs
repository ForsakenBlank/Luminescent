//! The dashboard's data: boards of boxes ("tabs") of items, stored in `dashboard.json` (`ui/dashboard.py`).
//!
//! ```text
//! {"layout": 3, "active": board id, "locked": false, "screens": {screen name: board id},
//!  "boards": [{"id", "name", "tabs": [{"id", "kind", "title", "x", "y", "w", "h", "collapsed", "screen",
//!                                      "page", "state", "items": [{"id", "kind", "name", "path", "args",
//!                                      "hotkey", "action", "cwd", "admin", "ask_args", "terminal", "group"}]}]}]}
//! ```
//!
//! The data stays a JSON document on purpose: the Python app writes the same file, boxes and items of kinds
//! this version does not know must survive a save, and keys keep their order so a save of an untouched file
//! changes nothing. The typed [`TabView`] and [`ItemView`] are for reading it.

mod store;
mod view;

pub use store::{DashboardStore, ToolSource};
pub use view::{BoardView, ItemView, TabView};

pub const GRID: i64 = 8;
pub const BOX_W: i64 = 264;
pub const PAGE_W: i64 = 720;
pub const PAGE_H: i64 = 520;
pub const PAGE_MIN: (i64, i64) = (360, 220);
pub const ROW_H: i64 = 32;
pub const HEAD_ROW_H: i64 = 26;
pub const SEP_ROW_H: i64 = 13;
pub const HEADER_H: i64 = 34;
/// Between boxes when they are tidied.
pub const GAP: i64 = 32;
/// Where the first box sits: below the logo and the page switcher.
pub const ORIGIN: (i64, i64) = (96, 136);
pub const LAYOUT: i64 = 3;
pub const TOOLS_TAB: &str = "tools";
/// Pages that fill the whole screen and so are never a box on the dashboard.
pub const FULL_PAGES: [&str; 2] = ["dashboard", "automation"];

/// A new id: ten hex digits, as the Python app makes them.
pub fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..10].to_owned()
}

/// Items of these kinds run; headings and separators only tidy a box up.
pub fn runnable(kind: &str) -> bool {
    kind == "item" || kind == "group"
}

/// Rounds to the snap grid; a grid of 0 or 1 means pixel by pixel. Python's `round` takes halves to the even
/// number, so this does too.
pub fn snap_to(v: f64, grid: i64) -> i64 {
    if grid > 1 {
        let g = grid as f64;
        ((v / g).round_ties_even() * g) as i64
    } else {
        v.round_ties_even() as i64
    }
}

/// The little type tag in front of a row: APP, DIR, URL, CMD, FILE, PY, SH, GRP, or TOOL for Luminescent's own
/// actions.
pub fn badge_for(kind: &str, action: &str, target_kind: &str, path: &str) -> &'static str {
    if kind == "group" {
        return "GRP";
    }
    if !action.is_empty() {
        return "TOOL";
    }
    match target_kind {
        "program" => {
            let ext = path
                .trim()
                .trim_matches('"')
                .rsplit_once('.')
                .map_or(String::new(), |(_, e)| e.to_lowercase());
            if ["sh", "bash", "zsh", "fish"].contains(&ext.as_str()) { "SH" } else { "APP" }
        }
        "url" => "URL",
        "folder" => "DIR",
        "python" => "PY",
        "file" => "FILE",
        "command" => "CMD",
        _ => "·",
    }
}

/// Each module's icon, which the layout 1 migration strips from the front of Tools tab names.
pub const MODULE_ICONS: &[(&str, &str)] = &[
    ("atlas_sync", "⇄"),
    ("caelestia", "◐"),
    ("calculator", "🧮"),
    ("colorpicker", "🎯"),
    ("doodle", "✏"),
    ("hearing", "🎧"),
    ("keybinds", "⌨"),
    ("media", "♪"),
    ("metadata", "🧹"),
    ("notes", "📝"),
    ("performance", "📊"),
    ("promptlog", "⌨"),
    ("qr", "▦"),
    ("reminders", "⏰"),
    ("ruler", "📏"),
    ("snapper", "▣"),
    ("snippets", "📋"),
    ("syscontrol", "🖥"),
    ("sysmon", "📈"),
    ("textgrab", "🔤"),
    ("timer", "⏱"),
    ("vault", "🔒"),
];
