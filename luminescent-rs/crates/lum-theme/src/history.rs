//! Undo, redo and "Recently changed" for the theme editor (`ui/colour_tools.py` `History`, and the recent list
//! of `ui/themes_page.py`).

use serde_json::{Map, Value, json};

/// Snapshots of the theme being edited. A burst of small edits (dragging in the colour picker) becomes one
/// step, and the quick tools are always one step.
#[derive(Debug, Clone)]
pub struct History {
    undo_stack: Vec<(String, String)>,
    redo_stack: Vec<(String, String)>,
    now: String,
    limit: usize,
}

/// The JSON text with every object's keys in order, so two snapshots that differ only in key order are equal
/// (Python compares `json.dumps(sort_keys=True)`).
fn canonical(v: &Value) -> String {
    fn sorted(v: &Value) -> Value {
        match v {
            Value::Object(m) => {
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                Value::Object(keys.into_iter().map(|k| (k.clone(), sorted(&m[k]))).collect())
            }
            Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
            other => other.clone(),
        }
    }
    sorted(v).to_string()
}

impl History {
    pub fn new(first: &Value) -> History {
        History { undo_stack: Vec::new(), redo_stack: Vec::new(), now: canonical(first), limit: 80 }
    }

    /// Notes the theme as it is now; false when nothing changed since the last step.
    pub fn record(&mut self, t: &Value, label: &str) -> bool {
        let cur = canonical(t);
        if cur == self.now {
            return false;
        }
        self.undo_stack.push((label.to_owned(), std::mem::replace(&mut self.now, cur)));
        if self.undo_stack.len() > self.limit {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
        true
    }

    pub fn undo(&mut self) -> Option<(String, Value)> {
        let (label, before) = self.undo_stack.pop()?;
        self.redo_stack.push((label.clone(), std::mem::replace(&mut self.now, before.clone())));
        Some((label, serde_json::from_str(&before).ok()?))
    }

    pub fn redo(&mut self) -> Option<(String, Value)> {
        let (label, after) = self.redo_stack.pop()?;
        self.undo_stack.push((label.clone(), std::mem::replace(&mut self.now, after.clone())));
        Some((label, serde_json::from_str(&after).ok()?))
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// What Ctrl+Z would undo.
    pub fn undo_label(&self) -> Option<&str> {
        self.undo_stack.last().map(|(l, _)| l.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo_stack.last().map(|(l, _)| l.as_str())
    }
}

/// One entry of "Recently changed": what a change did, old → new, with a button to put just that one back.
#[derive(Debug, Clone, PartialEq)]
pub struct Recent {
    /// `lum` (the theme's own colours), `cael` (Caelestia's) or `term` (the terminal's).
    pub scope: String,
    pub label: String,
    pub old: Map<String, Value>,
    pub new: Map<String, Value>,
    /// What the colours were before the theme changed them, for the scopes that have a base.
    pub base: Map<String, Value>,
    /// A run of changes to one colour (dragging in the picker) is one entry that keeps the colour from before.
    pub live: bool,
}

pub const RECENT_KEPT: usize = 30;
/// How many entries stay with the theme for next time.
pub const RECENT_SAVED: usize = 12;

impl Recent {
    pub fn from_value(v: &Value) -> Option<Recent> {
        let m = v.as_object()?;
        let map = |k: &str| m.get(k).and_then(Value::as_object).cloned().unwrap_or_default();
        m.get("new")?;
        Some(Recent {
            scope: m.get("scope").and_then(Value::as_str).unwrap_or("lum").to_owned(),
            label: m.get("label").and_then(Value::as_str).unwrap_or("").to_owned(),
            old: map("old"),
            new: map("new"),
            base: map("base"),
            live: false,
        })
    }

    pub fn to_value(&self) -> Value {
        json!({"scope": self.scope, "label": self.label, "old": self.old, "new": self.new, "base": self.base})
    }
}

/// Adds a change to the recent list (newest first), folding a run of live changes to the same colour into one.
pub fn note_change(
    list: &mut Vec<Recent>,
    scope: &str,
    old: Map<String, Value>,
    new: Map<String, Value>,
    label: &str,
    base: Map<String, Value>,
) {
    if let Some(top) = list.first_mut()
        && top.scope == scope
        && new.len() == 1
        && top.live
        && top.new.keys().eq(new.keys())
    {
        top.new = new;
        return;
    }
    let live = new.len() == 1;
    list.insert(0, Recent { scope: scope.to_owned(), label: label.to_owned(), old, new, base, live });
    list.truncate(RECENT_KEPT);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_and_redo_walk_the_steps() {
        let mut h = History::new(&json!({"a": 1, "b": 2}));
        assert!(!h.record(&json!({"b": 2, "a": 1}), "Nothing"), "key order alone is no change");
        assert!(h.record(&json!({"a": 5, "b": 2}), "First"));
        assert!(h.record(&json!({"a": 5, "b": 9}), "Second"));
        assert_eq!(h.undo_label(), Some("Second"));
        assert_eq!(h.undo(), Some(("Second".into(), json!({"a": 5, "b": 2}))));
        assert!(h.can_redo());
        assert_eq!(h.redo(), Some(("Second".into(), json!({"a": 5, "b": 9}))));
        h.undo();
        assert!(h.record(&json!({"a": 7, "b": 2}), "Other"), "a new edit drops the redo steps");
        assert!(!h.can_redo());
    }

    #[test]
    fn only_eighty_steps_are_kept() {
        let mut h = History::new(&json!(0));
        for i in 1..=100 {
            h.record(&json!(i), "n");
        }
        let mut steps = 0;
        while h.undo().is_some() {
            steps += 1;
        }
        assert_eq!(steps, 80);
    }

    #[test]
    fn a_run_of_live_changes_is_one_entry() {
        let one = |old: &str, new: &str| {
            (
                json!({"primary": old}).as_object().cloned().unwrap_or_default(),
                json!({"primary": new}).as_object().cloned().unwrap_or_default(),
            )
        };
        let mut list = Vec::new();
        let (o, n) = one("#111111", "#222222");
        note_change(&mut list, "lum", o, n, "Accent", Map::new());
        let (o, n) = one("#222222", "#333333");
        note_change(&mut list, "lum", o, n, "Accent", Map::new());
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].old["primary"], "#111111");
        assert_eq!(list[0].new["primary"], "#333333");
    }
}
