//! Values & text: nodes that are read through a ◆ wire when another node needs them.

use crate::engine::{Cx, NodeRuntime};
use crate::text::{self, MAX_TEXT, clip_chars, split_lines};

pub struct Text;

impl NodeRuntime for Text {
    fn value(&self, _port: &str, input: &str, cx: &mut Cx<'_>) -> String {
        cx.f("text", input)
    }
}

pub struct Number;

impl NodeRuntime for Number {
    fn value(&self, _port: &str, input: &str, cx: &mut Cx<'_>) -> String {
        text::fmt_number(cx.num("number", input, 0.0))
    }
}

/// Variable: reads a variable (kept between restarts) and shows its value.
pub struct VarGet;

impl VarGet {
    fn show(cx: &mut Cx<'_>) {
        let v = cx.get_var(&cx.raw_str("name"));
        let line = if v.is_empty() { "(empty)".to_owned() } else { format!("= {}", clip_chars(&v, 60)) };
        cx.set_status(line, None);
    }
}

impl NodeRuntime for VarGet {
    fn start(&mut self, cx: &mut Cx<'_>) {
        VarGet::show(cx);
    }

    fn vars_changed(&mut self, cx: &mut Cx<'_>) {
        VarGet::show(cx);
    }

    fn value(&self, _port: &str, _input: &str, cx: &mut Cx<'_>) -> String {
        cx.get_var(&cx.raw_str("name"))
    }
}

/// Join text: A, B and C joined into one text, empty ones left out.
pub struct Join;

impl NodeRuntime for Join {
    fn value(&self, _port: &str, input: &str, cx: &mut Cx<'_>) -> String {
        let parts: Vec<String> =
            ["a", "b", "c"].iter().map(|k| cx.f(k, input)).filter(|p| !p.is_empty()).collect();
        parts.join(&cx.raw_str("sep"))
    }
}

/// Maths: A and B with an operation; empty when either is not a number or the sum has no answer.
pub struct Maths;

impl NodeRuntime for Maths {
    fn value(&self, _port: &str, input: &str, cx: &mut Cx<'_>) -> String {
        let a = text::parse_number(&cx.f("a", input));
        let b = text::parse_number(&cx.f("b", input));
        let (Some(a), Some(b)) = (a, b) else { return String::new() };
        let result = match cx.raw_str("op").as_str() {
            "+" => a + b,
            "-" => a - b,
            "×" => a * b,
            "÷" if b != 0.0 => a / b,
            "mod" if b != 0.0 => python_mod(a, b),
            "min" => a.min(b),
            "max" => a.max(b),
            "^" => {
                // Python raises for 0 to a negative power and for a result too big to hold.
                let r = a.powf(b);
                if (a == 0.0 && b < 0.0) || (r.is_infinite() && a.is_finite() && b.is_finite()) {
                    return String::new();
                }
                r
            }
            _ => return String::new(),
        };
        text::fmt_number(result)
    }
}

/// Python's `%` on floats takes the sign of the divisor.
fn python_mod(a: f64, b: f64) -> f64 {
    let r = a % b;
    if r != 0.0 && (r < 0.0) != (b < 0.0) { r + b } else { r }
}

/// Date / time: now, written your way.
pub struct Now;

impl NodeRuntime for Now {
    fn value(&self, _port: &str, _input: &str, cx: &mut Cx<'_>) -> String {
        let format = match cx.raw_str("format") {
            f if f.is_empty() => "%Y-%m-%d %H:%M".to_owned(),
            f => f,
        };
        let now = cx.local();
        jiff::fmt::strtime::format(format.as_str(), now)
            .unwrap_or_else(|_| now.strftime("%Y-%m-%d %H:%M").to_string())
    }
}

pub struct ClipGet;

impl NodeRuntime for ClipGet {
    fn value(&self, _port: &str, _input: &str, cx: &mut Cx<'_>) -> String {
        cx.clipboard_text()
    }
}

/// Read a file: its text (empty when it can not be read).
pub struct ReadFile;

impl NodeRuntime for ReadFile {
    fn value(&self, _port: &str, input: &str, cx: &mut Cx<'_>) -> String {
        let path = text::expand_user(cx.f("path", input).trim());
        match std::fs::read(path) {
            Ok(bytes) => clip_chars(&String::from_utf8_lossy(&bytes), MAX_TEXT).to_owned(),
            Err(_) => String::new(),
        }
    }
}

pub struct Replace;

impl NodeRuntime for Replace {
    fn value(&self, _port: &str, input: &str, cx: &mut Cx<'_>) -> String {
        let t = cx.f("text", input);
        let find = cx.raw_str("find");
        let with = cx.f("with", input);
        if find.is_empty() {
            return t;
        }
        if cx.flag("regex") {
            return text::regex_replace(&find, &with, &t).unwrap_or(t);
        }
        t.replace(&find, &with)
    }
}

/// Change text: upper or lower case, trimmed, its first or last line, its length...
pub struct Change;

impl NodeRuntime for Change {
    fn value(&self, _port: &str, input: &str, cx: &mut Cx<'_>) -> String {
        let t = cx.f("text", input);
        let lines = split_lines(t.trim());
        match cx.raw_str("op").as_str() {
            "upper" => t.to_uppercase(),
            "lower" => t.to_lowercase(),
            "title" => text::title_case(&t),
            "trim" => t.trim().to_owned(),
            "first" => lines.first().copied().unwrap_or_default().to_owned(),
            "last" => lines.last().copied().unwrap_or_default().to_owned(),
            "length" => t.chars().count().to_string(),
            "lines" => lines.len().to_string(),
            _ => t,
        }
    }
}

pub struct RandomNum;

impl NodeRuntime for RandomNum {
    fn value(&self, _port: &str, input: &str, cx: &mut Cx<'_>) -> String {
        let a = cx.num("min", input, 1.0) as i64;
        let b = cx.num("max", input, 6.0) as i64;
        cx.rng().i64(a.min(b)..=a.max(b)).to_string()
    }
}
