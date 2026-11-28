//! The text rules every node shares: `{tokens}`, comparing, numbers written the Python way, times and days.

use std::sync::OnceLock;

use jiff::civil::DateTime;
use regex::Regex;
use serde_json::{Map, Value};

/// What a pulse may carry, in characters.
pub const MAX_TEXT: usize = 200_000;

fn token_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\{(input|date|time|now|clipboard|newline|var:[^{}\n]+)\}")
            .expect("a fixed pattern that compiles")
    })
}

/// Fills in `{input}`, `{var:name}`, `{date}`, `{time}`, `{now}`, `{clipboard}` and `{newline}`. Other braces are
/// left alone, so shell and awk code is safe. The clipboard is only read when the text asks for it.
pub fn expand(
    text: &str,
    input: &str,
    now: DateTime,
    vars: &Map<String, Value>,
    clipboard: &mut dyn FnMut() -> String,
) -> String {
    if !text.contains('{') {
        return text.to_owned();
    }
    token_re()
        .replace_all(text, |caps: &regex::Captures<'_>| match &caps[1] {
            "input" => input.to_owned(),
            "newline" => "\n".to_owned(),
            "date" => now.strftime("%Y-%m-%d").to_string(),
            "time" => now.strftime("%H:%M").to_string(),
            "now" => now.strftime("%Y-%m-%d %H:%M:%S").to_string(),
            "clipboard" => clipboard(),
            var => vars.get(var[4..].trim()).map(py_str).unwrap_or_default(),
        })
        .into_owned()
}

/// The If node's test. Text is compared ignoring case and the spaces around it; the four number tests read both
/// sides as numbers (a comma counts as the decimal point) and are false when either is not one.
pub fn compare(a: &str, op: &str, b: &str) -> bool {
    if matches!(op, "gt" | "lt" | "ge" | "le") {
        let (Some(x), Some(y)) = (parse_number(a), parse_number(b)) else { return false };
        return match op {
            "gt" => x > y,
            "lt" => x < y,
            "ge" => x >= y,
            _ => x <= y,
        };
    }
    let (al, bl) = (a.trim().to_lowercase(), b.trim().to_lowercase());
    match op {
        "equals" => al == bl,
        "not_equals" => al != bl,
        "contains" => al.contains(&bl),
        "starts" => al.starts_with(&bl),
        "ends" => al.ends_with(&bl),
        "regex" => fancy_regex::Regex::new(b).is_ok_and(|re| re.is_match(a).unwrap_or(false)),
        "empty" => a.trim().is_empty(),
        _ => !a.trim().is_empty(),
    }
}

/// Python's `float(text.strip().replace(",", "."))`.
pub fn parse_number(text: &str) -> Option<f64> {
    py_float(&text.replace(',', "."))
}

/// A number the way the Maths and Number nodes write it: whole numbers without a decimal point, others with six
/// significant digits (`str(int(x))` or `f"{x:.6g}"`).
pub fn fmt_number(x: f64) -> String {
    if x.is_finite() && x.fract() == 0.0 {
        let whole = format!("{x:.0}");
        return if whole == "-0" { "0".into() } else { whole };
    }
    format_g(x, 6)
}

/// C's `%g`: fixed or scientific notation, whichever is shorter for that exponent, trailing zeros dropped.
pub fn format_g(x: f64, precision: usize) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf".into() } else { "-inf".into() };
    }
    let p = precision.max(1);
    let sci = format!("{x:.*e}", p - 1);
    let (mantissa, exp) = sci.split_once('e').expect("Rust's {:e} always writes an exponent");
    let exp: i32 = exp.parse().expect("the exponent is a whole number");
    if exp < -4 || exp >= p as i32 {
        let mantissa = strip_zeros(mantissa);
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{mantissa}e{sign}{:02}", exp.abs())
    } else {
        let decimals = (p as i32 - 1 - exp).max(0) as usize;
        strip_zeros(&format!("{x:.decimals$}")).to_owned()
    }
}

fn strip_zeros(s: &str) -> &str {
    if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.') } else { s }
}

/// What Python's `str()` makes of a field value: a number field holds `5` or `2.5`, a box holds text.
pub fn py_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Number(n) => match n.as_f64() {
            Some(f) if n.is_f64() => lum_core::pyjson::float_repr(f),
            _ => n.to_string(),
        },
        other => lum_core::pyjson::to_string(other, lum_core::pyjson::Style::COMPACT),
    }
}

/// Python's truth test, for the tick boxes (`if self.raw("terminal")`).
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// `16:00` or `9.30` as hours and minutes.
pub fn hm(text: &str) -> Option<(u8, u8)> {
    let t = text.trim();
    let (h, m) = t.split_once([':', '.'])?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(h) || h.len() > 2 || !digits(m) || m.len() != 2 {
        return None;
    }
    let (h, m): (u8, u8) = (h.parse().ok()?, m.parse().ok()?);
    (h <= 23 && m <= 59).then_some((h, m))
}

const DAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

/// "every day", "weekdays", "weekends" or a list like "mon,wed,fri" and "fri-mon", as weekdays (Monday = 0).
/// Anything it can not read means every day.
pub fn days(text: &str) -> Vec<u8> {
    let t = text.trim().to_lowercase();
    match t.as_str() {
        "" | "every day" | "daily" | "all" => return (0..7).collect(),
        "weekdays" | "workdays" => return (0..5).collect(),
        "weekends" | "weekend" => return vec![5, 6],
        _ => {}
    }
    let day = |s: &str| DAYS.iter().position(|d| s.get(..3).unwrap_or(s) == *d).map(|i| i as u8);
    let mut out = [false; 7];
    for part in t.split(|c: char| c == ',' || c.is_whitespace()).filter(|p| !p.is_empty()) {
        if let Some((a, b)) = part.split_once('-') {
            if let (Some(i), Some(j)) = (day(a), day(b)) {
                let mut d = i;
                loop {
                    out[d as usize] = true;
                    if d == j {
                        break;
                    }
                    d = (d + 1) % 7;
                }
            }
        } else if let Some(i) = day(part) {
            out[i as usize] = true;
        }
    }
    let picked: Vec<u8> = (0..7).filter(|&i| out[i as usize]).collect();
    if picked.is_empty() { (0..7).collect() } else { picked }
}

/// How long until something, short: `2d 3h`, `4h 10m`, `12m`, `40s`.
pub fn left(secs: f64) -> String {
    let secs = secs as i64;
    if secs >= 86_400 {
        format!("{}d {}h", secs / 86_400, secs % 86_400 / 3600)
    } else if secs >= 3600 {
        format!("{}h {}m", secs / 3600, secs % 3600 / 60)
    } else if secs >= 60 {
        format!("{}m", secs / 60)
    } else {
        format!("{secs}s")
    }
}

/// Python's `os.path.expanduser`: a leading `~` becomes the home folder.
pub fn expand_user(path: &str) -> std::path::PathBuf {
    let rest = if path == "~" {
        Some("")
    } else {
        path.strip_prefix("~/").or_else(|| if cfg!(windows) { path.strip_prefix("~\\") } else { None })
    };
    match rest {
        Some(rest) => lum_core::paths::user_home().join(rest),
        None => std::path::PathBuf::from(path),
    }
}

/// Python's `float(text)`: spaces around are fine, a comma is not.
pub fn py_float(text: &str) -> Option<f64> {
    let t = text.trim();
    if t.is_empty() || t.starts_with('+') && t[1..].starts_with(['+', '-']) {
        return None;
    }
    t.parse::<f64>().ok()
}

/// The first `n` characters (Python slices text by character, not by byte).
pub fn clip_chars(text: &str, n: usize) -> &str {
    match text.char_indices().nth(n) {
        Some((at, _)) => &text[..at],
        None => text,
    }
}

/// The last `n` characters.
pub fn last_chars(text: &str, n: usize) -> &str {
    let count = text.chars().count();
    if count <= n {
        return text;
    }
    let (at, _) = text.char_indices().nth(count - n).expect("count - n is inside the text");
    &text[at..]
}

/// Python's `str.splitlines()`: every kind of line break, no empty last line.
pub fn split_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let is_break = matches!(
            c,
            '\n' | '\r'
                | '\u{0b}'
                | '\u{0c}'
                | '\u{1c}'
                | '\u{1d}'
                | '\u{1e}'
                | '\u{85}'
                | '\u{2028}'
                | '\u{2029}'
        );
        if !is_break {
            continue;
        }
        lines.push(&text[start..i]);
        start = i + c.len_utf8();
        if c == '\r' && chars.peek().is_some_and(|&(_, n)| n == '\n') {
            chars.next();
            start += 1;
        }
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

/// Python's `str.title()`: a letter after a letter is lower case, any other letter upper case.
pub fn title_case(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut after_letter = false;
    for c in text.chars() {
        if after_letter {
            out.extend(c.to_lowercase());
        } else {
            out.extend(c.to_uppercase());
        }
        after_letter = c.is_alphabetic();
    }
    out
}

/// Python's `re.sub(pattern, template, text)`. The template takes `\1`, `\g<1>` and `\g<name>`; a pattern or
/// template Python would refuse gives `None`.
pub fn regex_replace(pattern: &str, template: &str, text: &str) -> Option<String> {
    let re = fancy_regex::Regex::new(pattern).ok()?;
    let parts = parse_template(template)?;
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for caps in re.captures_iter(text) {
        let caps = caps.ok()?;
        let whole = caps.get(0).expect("group 0 is always there");
        out.push_str(&text[last..whole.start()]);
        for part in &parts {
            match part {
                Part::Text(t) => out.push_str(t),
                Part::Group(i) => out.push_str(caps.get(*i).map_or("", |m| m.as_str())),
                Part::Named(name) => out.push_str(caps.name(name).map_or("", |m| m.as_str())),
            }
        }
        last = whole.end();
    }
    out.push_str(&text[last..]);
    Some(out)
}

enum Part {
    Text(String),
    Group(usize),
    Named(String),
}

fn parse_template(template: &str) -> Option<Vec<Part>> {
    let mut parts = Vec::new();
    let mut text = String::new();
    let mut chars = template.chars().peekable();
    let flush = |text: &mut String, parts: &mut Vec<Part>| {
        if !text.is_empty() {
            parts.push(Part::Text(std::mem::take(text)));
        }
    };
    while let Some(c) = chars.next() {
        if c != '\\' {
            text.push(c);
            continue;
        }
        match chars.next() {
            None => return None,
            Some(d) if d.is_ascii_digit() => {
                let mut n = d.to_string();
                if chars.peek().is_some_and(char::is_ascii_digit) {
                    n.push(chars.next().expect("peeked"));
                }
                flush(&mut text, &mut parts);
                parts.push(Part::Group(n.parse().ok()?));
            }
            Some('g') => {
                if chars.next() != Some('<') {
                    return None;
                }
                let name: String = chars.by_ref().take_while(|&c| c != '>').collect();
                flush(&mut text, &mut parts);
                parts.push(match name.parse::<usize>() {
                    Ok(i) => Part::Group(i),
                    Err(_) if !name.is_empty() => Part::Named(name),
                    Err(_) => return None,
                });
            }
            Some('n') => text.push('\n'),
            Some('t') => text.push('\t'),
            Some('r') => text.push('\r'),
            Some('\\') => text.push('\\'),
            Some(d) if d.is_ascii_alphabetic() => return None,
            Some(d) => {
                text.push('\\');
                text.push(d);
            }
        }
    }
    flush(&mut text, &mut parts);
    Some(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_read_like_python() {
        assert_eq!(parse_number(" 1,5 "), Some(1.5));
        assert_eq!(parse_number("abc"), None);
        assert_eq!(parse_number(""), None);
        assert_eq!(parse_number("1e3"), Some(1000.0));
        assert_eq!(fmt_number(-0.0), "0");
        assert_eq!(format_g(1234567.0, 6), "1.23457e+06");
        assert_eq!(format_g(0.0001, 6), "0.0001");
        assert_eq!(format_g(f64::INFINITY, 6), "inf");
    }

    #[test]
    fn python_text_helpers() {
        assert_eq!(split_lines("a\r\nb\n\nc\n"), ["a", "b", "", "c"]);
        assert_eq!(split_lines(""), Vec::<&str>::new());
        assert_eq!(title_case("hello wORLD they're"), "Hello World They'Re");
        assert_eq!(clip_chars("héllo", 2), "hé");
        assert_eq!(last_chars("héllo", 3), "llo");
        assert_eq!(left(90_000.0), "1d 1h");
        assert_eq!(left(3700.0), "1h 1m");
        assert_eq!(py_str(&serde_json::json!(2.0)), "2.0");
        assert_eq!(py_str(&serde_json::json!(true)), "True");
    }

    #[test]
    fn regex_replace_takes_python_templates() {
        assert_eq!(regex_replace(r"(\w+)@(\w+)", r"\2 at \1", "me@home").as_deref(), Some("home at me"));
        assert_eq!(regex_replace(r"(?P<x>\d)", r"<\g<x>>", "a1b2").as_deref(), Some("a<1>b<2>"));
        assert_eq!(regex_replace("(", "x", "abc"), None);
        assert_eq!(regex_replace("a", r"\q", "abc"), None);
        assert_eq!(regex_replace("a", "$1", "abc").as_deref(), Some("$1bc"));
    }
}
