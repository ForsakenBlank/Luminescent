//! JSON text the way Python's `json.dump` writes it.
//!
//! The Python app and the Rust app share every data file, and people diff, sync and back these files up. Writing
//! them byte for byte like Python does (same separators, same indentation, `\uXXXX` escapes for non-ASCII text,
//! floats as Python's `repr`) means switching between the two apps never shows up as a whole-file change.

use serde_json::{Map, Number, Value};

/// How a file is written. Python's defaults are `ensure_ascii=True` and no indentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    pub indent: Option<usize>,
    pub ensure_ascii: bool,
}

impl Style {
    /// `json.dump(data, f)`: one line, `", "` and `": "` between items.
    pub const COMPACT: Style = Style { indent: None, ensure_ascii: true };

    /// `json.dump(data, f, indent=n)`.
    pub const fn indented(n: usize) -> Style {
        Style { indent: Some(n), ensure_ascii: true }
    }
}

pub fn to_string(value: &Value, style: Style) -> String {
    let mut out = String::new();
    write_value(&mut out, value, style, 0);
    out
}

fn write_value(out: &mut String, value: &Value, style: Style, depth: usize) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(n) => write_number(out, n),
        Value::String(s) => write_string(out, s, style.ensure_ascii),
        Value::Array(items) => write_array(out, items, style, depth),
        Value::Object(map) => write_object(out, map, style, depth),
    }
}

fn write_array(out: &mut String, items: &[Value], style: Style, depth: usize) {
    if items.is_empty() {
        out.push_str("[]");
        return;
    }
    out.push('[');
    for (i, item) in items.iter().enumerate() {
        separator(out, i, style, depth + 1);
        write_value(out, item, style, depth + 1);
    }
    closing(out, style, depth);
    out.push(']');
}

fn write_object(out: &mut String, map: &Map<String, Value>, style: Style, depth: usize) {
    if map.is_empty() {
        out.push_str("{}");
        return;
    }
    out.push('{');
    for (i, (key, item)) in map.iter().enumerate() {
        separator(out, i, style, depth + 1);
        write_string(out, key, style.ensure_ascii);
        out.push_str(": ");
        write_value(out, item, style, depth + 1);
    }
    closing(out, style, depth);
    out.push('}');
}

// With an indent Python puts "," at the end of a line; without one it writes ", " between items.
fn separator(out: &mut String, index: usize, style: Style, depth: usize) {
    match style.indent {
        Some(width) => {
            if index > 0 {
                out.push(',');
            }
            newline(out, width * depth);
        }
        None if index > 0 => out.push_str(", "),
        None => {}
    }
}

fn closing(out: &mut String, style: Style, depth: usize) {
    if let Some(width) = style.indent {
        newline(out, width * depth);
    }
}

fn newline(out: &mut String, spaces: usize) {
    out.push('\n');
    out.extend(std::iter::repeat_n(' ', spaces));
}

fn write_number(out: &mut String, n: &Number) {
    if let Some(f) = n.as_f64().filter(|_| n.is_f64()) {
        out.push_str(&float_repr(f));
    } else {
        out.push_str(&n.to_string());
    }
}

/// Python's `repr(float)`: the shortest text that reads back as the same number, with `.0` on whole numbers and
/// an exponent (at least two digits, with its sign) only below 1e-4 or from 1e16 up.
pub fn float_repr(f: f64) -> String {
    if f.is_nan() {
        return "NaN".to_owned();
    }
    if f.is_infinite() {
        return if f > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }
    // `{:e}` gives the shortest round-trip digits, as "d.ddde±x"
    let sci = format!("{f:e}");
    let (mantissa, exp) = sci.split_once('e').expect("{:e} always has an exponent");
    let exp: i32 = exp.parse().expect("{:e} exponent is a number");
    let (sign, mantissa) = match mantissa.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", mantissa),
    };
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();

    if !(-5 < exp && exp < 16) {
        let exp_sign = if exp < 0 { '-' } else { '+' };
        return format!("{sign}{mantissa}e{exp_sign}{:02}", exp.abs());
    }
    let point = exp + 1;
    let body = if point <= 0 {
        format!("0.{}{digits}", "0".repeat(point.unsigned_abs() as usize))
    } else if point as usize >= digits.len() {
        format!("{digits}{}.0", "0".repeat(point as usize - digits.len()))
    } else {
        let (whole, frac) = digits.split_at(point as usize);
        format!("{whole}.{frac}")
    };
    format!("{sign}{body}")
}

fn write_string(out: &mut String, s: &str, ensure_ascii: bool) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => push_escape(out, c as u16),
            // with ensure_ascii Python escapes everything outside space..tilde, DEL included
            c if ensure_ascii && !(' '..='~').contains(&c) => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    push_escape(out, *unit);
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn push_escape(out: &mut String, unit: u16) {
    out.push_str(&format!("\\u{unit:04x}"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn floats_print_like_python() {
        let cases = [
            (1.0, "1.0"),
            (0.1, "0.1"),
            (-2.5, "-2.5"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (1e16, "1e+16"),
            (1.5e16, "1.5e+16"),
            (1234567890123456.0, "1234567890123456.0"),
            (123.456, "123.456"),
            (1795444200.0, "1795444200.0"),
            (1e-300, "1e-300"),
            (-0.0, "-0.0"),
            (5e-324, "5e-324"),
            (0.30000000000000004, "0.30000000000000004"),
        ];
        for (f, text) in cases {
            assert_eq!(float_repr(f), text, "{f}");
        }
    }

    #[test]
    fn compact_and_indented() {
        let v = json!({"a": [1, 2.0, {}], "b": {"c": null, "d": []}, "e": true});
        assert_eq!(
            to_string(&v, Style::COMPACT),
            r#"{"a": [1, 2.0, {}], "b": {"c": null, "d": []}, "e": true}"#
        );
        assert_eq!(
            to_string(&v, Style::indented(2)),
            "{\n  \"a\": [\n    1,\n    2.0,\n    {}\n  ],\n  \"b\": {\n    \"c\": null,\n    \"d\": []\n  },\n  \"e\": true\n}"
        );
    }

    #[test]
    fn strings_escape_like_python() {
        let v = json!("é → 😀 \u{7f} \u{1} \"q\" \\ \t");
        assert_eq!(
            to_string(&v, Style::COMPACT),
            r#""\u00e9 \u2192 \ud83d\ude00 \u007f \u0001 \"q\" \\ \t""#
        );
        let loose = Style { indent: None, ensure_ascii: false };
        assert_eq!(to_string(&v, loose), "\"é → 😀 \u{7f} \\u0001 \\\"q\\\" \\\\ \\t\"");
    }
}
