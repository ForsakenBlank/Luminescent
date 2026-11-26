//! Read-only views over the dashboard's JSON.

use serde_json::{Map, Value};

use super::{BOX_W, PAGE_H};

pub(super) fn str_of<'a>(map: &'a Map<String, Value>, key: &str) -> &'a str {
    map.get(key).and_then(Value::as_str).unwrap_or("")
}

/// A number the way Python's `int()` reads it: integers, floats (cut towards zero) and numeric text.
pub(super) fn int_of(map: &Map<String, Value>, key: &str, default: i64) -> i64 {
    match map.get(key) {
        Some(Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)).unwrap_or(default),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(default),
        _ => default,
    }
}

pub(super) fn float_of(map: &Map<String, Value>, key: &str, default: f64) -> f64 {
    match map.get(key) {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(default),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(default),
        _ => default,
    }
}

pub(super) fn bool_of(map: &Map<String, Value>, key: &str, default: bool) -> bool {
    match map.get(key) {
        Some(Value::Bool(b)) => *b,
        Some(Value::Null) | None => default,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ItemView<'a>(pub &'a Map<String, Value>);

impl<'a> ItemView<'a> {
    pub fn id(&self) -> &'a str {
        str_of(self.0, "id")
    }

    pub fn kind(&self) -> &'a str {
        match str_of(self.0, "kind") {
            "" => "item",
            kind => kind,
        }
    }

    pub fn name(&self) -> &'a str {
        str_of(self.0, "name")
    }

    pub fn path(&self) -> &'a str {
        str_of(self.0, "path")
    }

    pub fn args(&self) -> &'a str {
        str_of(self.0, "args")
    }

    pub fn hotkey(&self) -> &'a str {
        str_of(self.0, "hotkey")
    }

    pub fn action(&self) -> &'a str {
        str_of(self.0, "action")
    }

    pub fn cwd(&self) -> &'a str {
        str_of(self.0, "cwd")
    }

    pub fn admin(&self) -> bool {
        bool_of(self.0, "admin", false)
    }

    pub fn ask_args(&self) -> bool {
        bool_of(self.0, "ask_args", false)
    }

    pub fn terminal(&self) -> bool {
        bool_of(self.0, "terminal", false)
    }

    /// The ids a group runs, in order.
    pub fn group(&self) -> Vec<&'a str> {
        self.0
            .get("group")
            .and_then(Value::as_array)
            .map_or_else(Vec::new, |a| a.iter().filter_map(Value::as_str).collect())
    }

    pub fn runnable(&self) -> bool {
        super::runnable(self.kind())
    }

    /// What the search box looks in.
    pub fn search_text(&self) -> String {
        format!("{} {} {}", self.name(), self.path(), self.action()).to_lowercase()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TabView<'a>(pub &'a Map<String, Value>);

impl<'a> TabView<'a> {
    pub fn id(&self) -> &'a str {
        str_of(self.0, "id")
    }

    /// "items", "page" or "picture".
    pub fn kind(&self) -> &'a str {
        match str_of(self.0, "kind") {
            "" => "items",
            kind => kind,
        }
    }

    pub fn title(&self) -> &'a str {
        str_of(self.0, "title")
    }

    pub fn x(&self) -> i64 {
        int_of(self.0, "x", 16)
    }

    pub fn y(&self) -> i64 {
        int_of(self.0, "y", 16)
    }

    pub fn w(&self) -> i64 {
        int_of(self.0, "w", BOX_W)
    }

    pub fn h(&self) -> i64 {
        int_of(self.0, "h", PAGE_H)
    }

    pub fn collapsed(&self) -> bool {
        bool_of(self.0, "collapsed", false)
    }

    /// The screen the box is on; empty is the screen the overlay opens on.
    pub fn screen(&self) -> &'a str {
        str_of(self.0, "screen")
    }

    pub fn page(&self) -> &'a str {
        str_of(self.0, "page")
    }

    pub fn state(&self) -> Option<&'a Map<String, Value>> {
        self.0.get("state").and_then(Value::as_object)
    }

    pub fn items(&self) -> impl Iterator<Item = ItemView<'a>> + use<'a> {
        self.0
            .get("items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_object().map(ItemView))
    }

    pub fn item_count(&self) -> usize {
        self.0.get("items").and_then(Value::as_array).map_or(0, Vec::len)
    }

    /// How many items run; the number in a box's header.
    pub fn runnable_count(&self) -> usize {
        self.items().filter(ItemView::runnable).count()
    }

    pub fn picture_path(&self) -> &'a str {
        str_of(self.0, "path")
    }

    pub fn frame(&self) -> &'a str {
        match str_of(self.0, "frame") {
            "" => "panel",
            frame => frame,
        }
    }

    pub fn fit(&self) -> &'a str {
        match str_of(self.0, "fit") {
            "" => "fit",
            fit => fit,
        }
    }

    pub fn radius(&self) -> i64 {
        int_of(self.0, "radius", 12)
    }

    pub fn opacity(&self) -> f64 {
        float_of(self.0, "opacity", 1.0)
    }

    pub fn title_bar(&self) -> bool {
        bool_of(self.0, "title_bar", false)
    }

    pub fn border_accent(&self) -> bool {
        bool_of(self.0, "border_accent", true)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BoardView<'a>(pub &'a Map<String, Value>);

impl<'a> BoardView<'a> {
    pub fn id(&self) -> &'a str {
        str_of(self.0, "id")
    }

    pub fn name(&self) -> &'a str {
        str_of(self.0, "name")
    }

    pub fn tabs(&self) -> impl Iterator<Item = TabView<'a>> + use<'a> {
        self.0
            .get("tabs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_object().map(TabView))
    }
}
