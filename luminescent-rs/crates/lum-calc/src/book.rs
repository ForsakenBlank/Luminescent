//! The Calculator's data: the history, the pins and the panel's place, kept in `calculator.json` over raw JSON so
//! a field this version does not know survives a save. The sums themselves are worked out here too, because what a
//! sum can use (the last answer, the pins) lives in the book.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use jiff::civil::{Date, DateTime};
use lum_core::pyjson::{self, Style};
use lum_core::store::{self, Writer};
use regex::Regex;
use serde_json::{Map, Value, json};

use crate::convert::{convert, date_math};
use crate::eval::{Scope, evaluate, is_reserved};
use crate::{Error, Num, fmt, pin_name};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Base {
    #[default]
    Dec,
    Hex,
    Bin,
    Oct,
}

impl Base {
    pub const ALL: [Base; 4] = [Base::Dec, Base::Hex, Base::Bin, Base::Oct];

    pub fn key(self) -> &'static str {
        match self {
            Base::Dec => "dec",
            Base::Hex => "hex",
            Base::Bin => "bin",
            Base::Oct => "oct",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Base::Dec => "DEC",
            Base::Hex => "HEX",
            Base::Bin => "BIN",
            Base::Oct => "OCT",
        }
    }

    pub fn from_key(key: &str) -> Base {
        Base::ALL.into_iter().find(|b| b.key() == key).unwrap_or_default()
    }

    pub fn next(self) -> Base {
        let at = Base::ALL.iter().position(|b| *b == self).unwrap_or(0);
        Base::ALL[(at + 1) % Base::ALL.len()]
    }
}

/// The Calculator's settings that change what a sum says.
#[derive(Debug, Clone)]
pub struct Options {
    pub decimals: usize,
    pub thousands: bool,
    pub degrees: bool,
    pub currency: bool,
    pub history_limit: usize,
}

impl Default for Options {
    fn default() -> Options {
        Options { decimals: 10, thousands: true, degrees: true, currency: true, history_limit: 200 }
    }
}

/// A worked-out sum: the number (none for a date), the text to show and the sum as it is stored.
#[derive(Debug, Clone, PartialEq)]
pub struct Computed {
    pub value: Option<Num>,
    pub result: String,
    pub expr: String,
}

/// One line of the history or one pin.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry(pub Map<String, Value>);

impl Entry {
    fn text(&self, key: &str) -> &str {
        self.0.get(key).and_then(Value::as_str).unwrap_or_default()
    }

    pub fn expr(&self) -> &str {
        self.text("expr")
    }

    pub fn result(&self) -> &str {
        self.text("result")
    }

    /// The pin's variable name, empty for a history line.
    pub fn var(&self) -> &str {
        self.text("var")
    }

    pub fn name(&self) -> &str {
        self.text("name")
    }

    pub fn folder(&self) -> &str {
        self.text("folder")
    }

    pub fn colour(&self) -> &str {
        self.text("colour")
    }

    /// "2026-11-23 09:15".
    pub fn time(&self) -> &str {
        self.text("t")
    }

    pub fn value(&self) -> Option<Num> {
        self.0.get("value").and_then(Num::from_json)
    }
}

static PLAIN_RESULT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:-?(?:0[xbo][0-9A-Fa-f ]+|[\d,]*\.?\d+(?:e[+-]?\d+)?))$").expect("the pattern is valid")
});

#[derive(Debug)]
pub struct Book {
    path: PathBuf,
    history: Vec<Entry>,
    pins: Vec<Entry>,
    panel: Map<String, Value>,
    pub options: Options,
    writer: Writer,
    dirty: bool,
}

impl Book {
    pub fn load(path: &Path) -> Book {
        let saved = match store::load_json(path) {
            Some(Value::Object(map)) => map,
            _ => Map::new(),
        };
        let entries = |key: &str, needs: &str| -> Vec<Entry> {
            let list = saved.get(key).and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
            list.iter()
                .filter_map(|v| v.as_object().filter(|o| o.contains_key(needs)))
                .map(|o| Entry(o.clone()))
                .collect()
        };
        let panel = saved.get("panel").and_then(Value::as_object).cloned().unwrap_or_default();
        Book {
            path: path.to_owned(),
            history: entries("history", "expr"),
            pins: entries("pins", "var"),
            panel,
            options: Options::default(),
            writer: Writer::new("calculator-save"),
            dirty: false,
        }
    }

    pub fn history(&self) -> &[Entry] {
        &self.history
    }

    pub fn pins(&self) -> &[Entry] {
        &self.pins
    }

    pub fn panel(&self) -> &Map<String, Value> {
        &self.panel
    }

    pub fn panel_mut(&mut self) -> &mut Map<String, Value> {
        self.dirty = true;
        &mut self.panel
    }

    pub fn base(&self) -> Base {
        Base::from_key(self.panel.get("base").and_then(Value::as_str).unwrap_or("dec"))
    }

    pub fn set_base(&mut self, base: Base) {
        self.panel.insert("base".into(), json!(base.key()));
        self.reformat();
        self.dirty = true;
    }

    pub fn fmt(&self, value: &Num) -> String {
        fmt(value, self.options.decimals, self.options.thousands, self.base())
    }

    /// A result as text to paste: no thousands separators and no unit.
    pub fn plain(&self, entry: &Entry) -> String {
        match entry.value() {
            Some(value) => fmt(&value, self.options.decimals, false, self.base()).replace(' ', ""),
            None => entry.result().to_owned(),
        }
    }

    /// `ans` (the last number in the history) and every pin.
    pub fn scope(&self) -> Scope {
        let mut variables: HashMap<String, Num> = HashMap::new();
        if let Some(last) = self.history.iter().filter_map(Entry::value).next_back() {
            variables.insert("ans".to_owned(), last);
        }
        for pin in &self.pins {
            variables.insert(pin.var().to_owned(), pin.value().unwrap_or_else(|| Num::from(0)));
        }
        Scope { variables, degrees: self.options.degrees }
    }

    /// `*2` carries on from the last answer.
    pub fn continued(&self, text: &str) -> String {
        let t = text.trim();
        if t.starts_with(['*', '/', '×', '÷', '^', '+']) && !self.history.is_empty() {
            return format!("ans{t}");
        }
        t.to_owned()
    }

    /// Works out what was typed. Dates give no number (`None`); conversions keep their unit in the text.
    pub fn compute(&self, text: &str, rates: &HashMap<String, f64>, today: Date) -> Result<Computed, Error> {
        let expr = self.continued(text);
        if let Some((days, result)) = date_math(&expr, today)? {
            let value = days.map(Num::from);
            let result = match &value {
                Some(v) if self.base() != Base::Dec => format!("{} days", self.fmt(v)),
                _ => result,
            };
            return Ok(Computed { value, result, expr });
        }
        let scope = self.scope();
        let none = HashMap::new();
        let rates = if self.options.currency { rates } else { &none };
        let amount = |e: &str| evaluate(e, &scope);
        if let Some((value, unit)) = convert(&expr, &amount, rates)? {
            let shown = fmt(&value, self.options.decimals, self.options.thousands, Base::Dec);
            return Ok(Computed { result: format!("{shown} {unit}"), value: Some(value), expr });
        }
        let value = evaluate(&expr, &scope)?;
        Ok(Computed { result: self.fmt(&value), value: Some(value), expr })
    }

    pub fn add_history(&mut self, expr: &str, value: Option<&Num>, result: &str, now: DateTime) {
        let mut entry = Map::new();
        entry.insert("expr".into(), json!(expr));
        let result = match (result.is_empty(), value) {
            (true, Some(v)) => self.fmt(v),
            _ => result.to_owned(),
        };
        entry.insert("result".into(), json!(result));
        entry.insert("value".into(), value.map_or(Value::Null, Num::to_json));
        entry.insert("t".into(), json!(now.strftime("%Y-%m-%d %H:%M").to_string()));
        self.history.push(Entry(entry));
        // a limit of 0 keeps everything, as Python's `del history[:-0]` does
        let limit = self.options.history_limit;
        if limit > 0 && self.history.len() > limit {
            self.history.drain(..self.history.len() - limit);
        }
        self.dirty = true;
    }

    /// After the base, decimals or thousands changed: plain numbers are written again, "3.1 mi" keeps its text.
    pub fn reformat(&mut self) {
        let fresh = |book: &Book, list: &[Entry], always: bool| -> Vec<Option<String>> {
            list.iter()
                .map(|e| {
                    let value = e.value()?;
                    (always || PLAIN_RESULT.is_match(e.result())).then(|| book.fmt(&value))
                })
                .collect()
        };
        let history = fresh(self, &self.history, false);
        let pins = fresh(self, &self.pins, true);
        for (entry, text) in self.history.iter_mut().zip(history).chain(self.pins.iter_mut().zip(pins)) {
            if let Some(text) = text {
                entry.0.insert("result".into(), json!(text));
            }
        }
        self.dirty = true;
    }

    pub fn delete_history(&mut self, index: usize) {
        if index < self.history.len() {
            self.history.remove(index);
            self.dirty = true;
        }
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
        self.dirty = true;
    }

    /// Pins a history line under a name (the name becomes a variable). Says why when it can not.
    pub fn pin(&mut self, history_index: usize, name: &str) -> Result<(), String> {
        let name = name.trim();
        let Some(line) = self.history.get(history_index).cloned() else { return Ok(()) };
        if name.is_empty() {
            return Ok(());
        }
        let var = pin_name(name);
        if is_reserved(&var) {
            return Err(format!("'{var}' is a built-in name. Pick another."));
        }
        self.pins.retain(|p| p.var() != var);
        let mut pin = Map::new();
        pin.insert("name".into(), json!(name));
        pin.insert("var".into(), json!(var));
        pin.insert("expr".into(), json!(line.expr()));
        pin.insert("result".into(), json!(line.result()));
        pin.insert("value".into(), line.0.get("value").cloned().unwrap_or(json!(0)));
        self.pins.push(Entry(pin));
        self.dirty = true;
        Ok(())
    }

    pub fn rename_pin(&mut self, index: usize, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() || index >= self.pins.len() {
            return Ok(());
        }
        let var = pin_name(name);
        let taken = self.pins.iter().enumerate().any(|(i, p)| p.var() == var && i != index);
        if is_reserved(&var) || taken {
            return Err(format!("'{var}' is already used. Pick another."));
        }
        let pin = &mut self.pins[index].0;
        pin.insert("name".into(), json!(name));
        pin.insert("var".into(), json!(var));
        self.dirty = true;
        Ok(())
    }

    pub fn recalc_pin(&mut self, index: usize) -> Result<(), String> {
        let Some(pin) = self.pins.get(index) else { return Ok(()) };
        let mut scope = self.scope();
        scope.variables.remove(pin.var());
        let var = pin.var().to_owned();
        let value = evaluate(pin.expr(), &scope).map_err(|e| format!("Could not recalculate {var}: {e}"))?;
        let text = self.fmt(&value);
        let pin = &mut self.pins[index].0;
        pin.insert("value".into(), value.to_json());
        pin.insert("result".into(), json!(text));
        self.dirty = true;
        Ok(())
    }

    /// Sets a pin's folder and/or colour (an empty text clears it, as in Python where the keys stay with "").
    pub fn set_pin(&mut self, index: usize, folder: Option<&str>, colour: Option<&str>) {
        let Some(pin) = self.pins.get_mut(index) else { return };
        if let Some(f) = folder {
            pin.0.insert("folder".into(), json!(f));
        }
        if let Some(c) = colour {
            pin.0.insert("colour".into(), json!(c));
        }
        self.dirty = true;
    }

    pub fn unpin(&mut self, index: usize) {
        if index < self.pins.len() {
            self.pins.remove(index);
            self.dirty = true;
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    fn document(&self) -> Value {
        let list =
            |entries: &[Entry]| Value::Array(entries.iter().map(|e| Value::Object(e.0.clone())).collect());
        json!({"history": list(&self.history), "pins": list(&self.pins), "panel": self.panel})
    }

    /// Queues the file for the writer thread (the UI never waits for the disk).
    pub fn save(&mut self) {
        self.dirty = false;
        self.writer.save(&self.path, pyjson::to_string(&self.document(), Style::indented(1)));
    }

    pub fn flush(&mut self) {
        if self.dirty {
            self.save();
        }
        self.writer.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> Date {
        Date::new(2026, 11, 23).unwrap()
    }

    #[test]
    fn a_sum_starting_with_an_operator_carries_on_from_the_last_answer() {
        let dir = tempfile::tempdir().unwrap();
        let mut book = Book::load(&dir.path().join("calculator.json"));
        assert_eq!(book.continued("*2"), "*2");
        let now = today().at(9, 15, 0, 0);
        book.add_history("6*7", Some(&Num::from(42)), "", now);
        assert_eq!(book.continued("*2"), "ans*2");
        let got = book.compute("*2", &HashMap::new(), today()).unwrap();
        assert_eq!(got.result, "84");
        assert_eq!(got.expr, "ans*2");
    }

    #[test]
    fn pins_refuse_built_in_names_and_work_as_variables() {
        let dir = tempfile::tempdir().unwrap();
        let mut book = Book::load(&dir.path().join("calculator.json"));
        book.add_history("950*12", Some(&Num::from(11400)), "", today().at(9, 0, 0, 0));
        assert!(book.pin(0, "pi").is_err());
        book.pin(0, "Rent").unwrap();
        assert_eq!(book.compute("rent/4", &HashMap::new(), today()).unwrap().result, "2,850");
    }
}
