//! Unit, temperature and currency conversions ("5 km to mi") and date sums ("today + 45 days"), as
//! `core/convert.py` does them. Pints and gallons are UK ones (`uspt` / `usgal` for US); dates are day first
//! (`25/12/2026`) or ISO (`2026-12-25`).

use std::collections::HashMap;
use std::sync::LazyLock;

use jiff::Span;
use jiff::civil::{Date, Weekday};
use regex::{Captures, Regex};

use crate::{Error, Num};

type Table = &'static [(&'static str, f64)];

const LENGTH: Table = &[
    ("mm", 0.001),
    ("millimetre", 0.001),
    ("millimetres", 0.001),
    ("cm", 0.01),
    ("centimetre", 0.01),
    ("centimetres", 0.01),
    ("m", 1.0),
    ("metre", 1.0),
    ("metres", 1.0),
    ("meter", 1.0),
    ("meters", 1.0),
    ("km", 1000.0),
    ("kilometre", 1000.0),
    ("kilometres", 1000.0),
    ("in", 0.0254),
    ("inch", 0.0254),
    ("inches", 0.0254),
    ("\"", 0.0254),
    ("ft", 0.3048),
    ("foot", 0.3048),
    ("feet", 0.3048),
    ("yd", 0.9144),
    ("yard", 0.9144),
    ("yards", 0.9144),
    ("mi", 1609.344),
    ("mile", 1609.344),
    ("miles", 1609.344),
    ("nmi", 1852.0),
    ("um", 1e-6),
    ("nm", 1e-9),
];
const MASS: Table = &[
    ("mg", 1e-6),
    ("g", 0.001),
    ("gram", 0.001),
    ("grams", 0.001),
    ("kg", 1.0),
    ("kilo", 1.0),
    ("kilos", 1.0),
    ("kilogram", 1.0),
    ("kilograms", 1.0),
    ("t", 1000.0),
    ("tonne", 1000.0),
    ("tonnes", 1000.0),
    ("oz", 0.028349523125),
    ("ounce", 0.028349523125),
    ("ounces", 0.028349523125),
    ("lb", 0.45359237),
    ("lbs", 0.45359237),
    ("pound", 0.45359237),
    ("pounds", 0.45359237),
    ("st", 6.35029318),
    ("stone", 6.35029318),
];
const TIME: Table = &[
    ("ms", 0.001),
    ("s", 1.0),
    ("sec", 1.0),
    ("secs", 1.0),
    ("second", 1.0),
    ("seconds", 1.0),
    ("min", 60.0),
    ("mins", 60.0),
    ("minute", 60.0),
    ("minutes", 60.0),
    ("h", 3600.0),
    ("hr", 3600.0),
    ("hrs", 3600.0),
    ("hour", 3600.0),
    ("hours", 3600.0),
    ("day", 86400.0),
    ("days", 86400.0),
    ("week", 604800.0),
    ("weeks", 604800.0),
    ("wk", 604800.0),
    ("month", 2629746.0),
    ("months", 2629746.0),
    ("year", 31556952.0),
    ("years", 31556952.0),
    ("yr", 31556952.0),
];
const DATA: Table = &[
    ("bit", 0.125),
    ("bits", 0.125),
    ("b", 1.0),
    ("byte", 1.0),
    ("bytes", 1.0),
    ("kb", 1e3),
    ("mb", 1e6),
    ("gb", 1e9),
    ("tb", 1e12),
    ("pb", 1e15),
    ("kib", 1024.0),
    ("mib", 1_048_576.0),
    ("gib", 1_073_741_824.0),
    ("tib", 1_099_511_627_776.0),
];
const VOLUME: Table = &[
    ("ml", 0.001),
    ("cl", 0.01),
    ("dl", 0.1),
    ("l", 1.0),
    ("litre", 1.0),
    ("litres", 1.0),
    ("liter", 1.0),
    ("liters", 1.0),
    ("tsp", 0.00492892),
    ("tbsp", 0.0147868),
    ("cup", 0.236588),
    ("cups", 0.236588),
    ("floz", 0.0295735),
    ("pt", 0.568261),
    ("pint", 0.568261),
    ("pints", 0.568261),
    ("gal", 4.54609),
    ("gallon", 4.54609),
    ("gallons", 4.54609),
    ("uspt", 0.473176),
    ("usgal", 3.785411784),
    ("m3", 1000.0),
    ("cm3", 0.001),
];
const AREA: Table = &[
    ("mm2", 1e-6),
    ("cm2", 1e-4),
    ("m2", 1.0),
    ("km2", 1e6),
    ("ha", 1e4),
    ("hectare", 1e4),
    ("hectares", 1e4),
    ("acre", 4046.8564224),
    ("acres", 4046.8564224),
    ("ft2", 0.09290304),
    ("sqft", 0.09290304),
    ("in2", 0.00064516),
    ("mi2", 2589988.110336),
];
const SPEED: Table = &[
    ("m/s", 1.0),
    ("mps", 1.0),
    ("km/h", 1.0 / 3.6),
    ("kmh", 1.0 / 3.6),
    ("kph", 1.0 / 3.6),
    ("mph", 0.44704),
    ("kn", 0.514444),
    ("knot", 0.514444),
    ("knots", 0.514444),
    ("ft/s", 0.3048),
];
const ENERGY: Table =
    &[("j", 1.0), ("kj", 1000.0), ("cal", 4.184), ("kcal", 4184.0), ("wh", 3600.0), ("kwh", 3.6e6)];
const POWER: Table = &[("w", 1.0), ("kw", 1000.0), ("hp", 745.699872)];
const PRESSURE: Table =
    &[("pa", 1.0), ("kpa", 1000.0), ("bar", 1e5), ("mbar", 100.0), ("psi", 6894.757293), ("atm", 101325.0)];
const ANGLE: Table = &[
    ("deg", 0.017453292519943295),
    ("degree", 0.017453292519943295),
    ("degrees", 0.017453292519943295),
    ("rad", 1.0),
    ("radian", 1.0),
    ("radians", 1.0),
];

const DIMENSIONS: &[(&str, Table)] = &[
    ("length", LENGTH),
    ("mass", MASS),
    ("time", TIME),
    ("data", DATA),
    ("volume", VOLUME),
    ("area", AREA),
    ("speed", SPEED),
    ("energy", ENERGY),
    ("power", POWER),
    ("pressure", PRESSURE),
    ("angle", ANGLE),
];

/// Every unit word: its dimension and how many of the dimension's base unit it is.
pub static UNITS: LazyLock<HashMap<&'static str, (&'static str, f64)>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    for (dimension, table) in DIMENSIONS {
        for (alias, factor) in *table {
            map.insert(*alias, (*dimension, *factor));
        }
    }
    map
});

/// A unit word's dimension and factor, for tests and the unit list.
pub fn unit(word: &str) -> Option<(&'static str, f64)> {
    UNITS.get(word).copied()
}

pub fn unit_count() -> usize {
    UNITS.len()
}

const TEMPERATURES: &[(&str, char)] = &[
    ("c", 'c'),
    ("°c", 'c'),
    ("celsius", 'c'),
    ("f", 'f'),
    ("°f", 'f'),
    ("fahrenheit", 'f'),
    ("k", 'k'),
    ("kelvin", 'k'),
];
const SYMBOLS: &[(char, &str)] = &[('$', "usd"), ('€', "eur"), ('£', "gbp"), ('¥', "jpy")];

fn temperature(word: &str) -> Option<char> {
    TEMPERATURES.iter().find(|(w, _)| *w == word).map(|(_, t)| *t)
}

fn to_kelvin(v: f64, unit: char) -> f64 {
    match unit {
        'c' => v + 273.15,
        'f' => (v - 32.0) * 5.0 / 9.0 + 273.15,
        _ => v,
    }
}

fn from_kelvin(v: f64, unit: char) -> f64 {
    match unit {
        'c' => v - 273.15,
        'f' => (v - 273.15) * 9.0 / 5.0 + 32.0,
        _ => v,
    }
}

fn pattern(text: &str) -> Regex {
    Regex::new(text).expect("the pattern is written here and is valid")
}

fn looks_like_currency(word: &str) -> bool {
    word.chars().count() == 3 && word.chars().all(char::is_alphabetic)
}

/// `5 km to mi`, `20 usd in eur`, `$20 to eur`: the amount in the new unit and the unit's label. `None` when the
/// text is not a conversion. `amount` works out what comes before the units, so `2*5 km to mi` works too.
pub fn convert(
    text: &str,
    amount: &dyn Fn(&str) -> Result<Num, Error>,
    rates: &HashMap<String, f64>,
) -> Result<Option<(Num, String)>, Error> {
    static CONVERSION: LazyLock<Regex> = LazyLock::new(|| {
        pattern(
            r#"^(?P<expr>.*?)\s*(?P<src>[a-zA-Z°"$€£¥][\w°/]*)\s+(?:to|in|as|->|=>)\s+(?P<dst>[a-zA-Z°"$€£¥][\w°/]*)$"#,
        )
    });
    let mut t = text.trim().to_owned();
    for (symbol, code) in SYMBOLS {
        let escaped = regex::escape(&symbol.to_string());
        let leading = pattern(&format!(r"^{escaped}\s*([\d.,]+)"));
        t = leading.replace(&t, format!("${{1}} {code}")).into_owned();
        let trailing = pattern(&format!(r"\s{escaped}$"));
        t = trailing.replace(&t, format!(" {code}")).into_owned();
    }
    let Some(m) = CONVERSION.captures(&t) else { return Ok(None) };
    let label = m["dst"].to_owned();
    let swap = |word: &str| -> String {
        let lower = word.to_lowercase();
        let mut chars = lower.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) => {
                SYMBOLS.iter().find(|(s, _)| *s == c).map_or(lower.clone(), |(_, code)| (*code).to_owned())
            }
            _ => lower,
        }
    };
    let (src, dst) = (swap(&m["src"]), swap(&m["dst"]));
    let expr = m["expr"].trim();
    let expr = if expr.is_empty() { "1" } else { expr };
    let known = |u: &str| {
        UNITS.contains_key(u)
            || temperature(u).is_some()
            || (!rates.is_empty() && rates.contains_key(&u.to_uppercase()))
    };
    if !(known(&src) || looks_like_currency(&src)) || !(known(&dst) || looks_like_currency(&dst)) {
        return Ok(None);
    }
    let amount = amount(expr)?;
    if let (Some(a), Some(b)) = (temperature(&src), temperature(&dst)) {
        let value = if a == 'k' && b == 'k' {
            amount
        } else {
            Num::Float(from_kelvin(to_kelvin(amount.to_f64()?, a), b))
        };
        let unit = if b == 'k' { "K".to_owned() } else { format!("°{}", b.to_ascii_uppercase()) };
        return Ok(Some((value, unit)));
    }
    if let (Some((d1, f1)), Some((d2, f2))) = (UNITS.get(src.as_str()), UNITS.get(dst.as_str())) {
        if d1 != d2 {
            return Err(Error::Convert(format!("can't convert {d1} ({src}) to {d2} ({dst})")));
        }
        return Ok(Some((Num::Float(amount.to_f64()? * f1 / f2), label)));
    }
    if looks_like_currency(&src)
        && looks_like_currency(&dst)
        && !UNITS.contains_key(src.as_str())
        && !UNITS.contains_key(dst.as_str())
    {
        if rates.is_empty() {
            return Err(Error::Convert(
                "currency rates are not loaded yet (needs internet once a day)".into(),
            ));
        }
        let (a, b) = (rates.get(&src.to_uppercase()), rates.get(&dst.to_uppercase()));
        let (Some(a), Some(b)) = (a, b) else {
            let missing = if a.is_none() { &src } else { &dst };
            return Err(Error::Convert(format!("unknown currency {}", missing.to_uppercase())));
        };
        return Ok(Some((Num::Float(amount.to_f64()? / a * b), dst.to_uppercase())));
    }
    Err(Error::Convert(format!("can't convert {src} to {dst}")))
}

const DATE: &str = r"(?:today|now|tomorrow|yesterday|\d{4}-\d{1,2}-\d{1,2}|\d{1,2}[/.]\d{1,2}[/.]\d{2,4})";
const SPAN: &str = r"(?P<n>\d+)\s*(?P<u>days?|d|weeks?|wks?|w|months?|mo|years?|yrs?|y)";

#[derive(Clone, Copy, PartialEq)]
enum Rule {
    Shift,
    ShiftBack,
    Diff,
    Until,
    Since,
    Between,
    Show,
}

static DATE_RULES: LazyLock<Vec<(Rule, Regex)>> = LazyLock::new(|| {
    let rules = [
        (Rule::Shift, format!(r"^(?P<a>{DATE})\s*(?P<op>[+-])\s*{SPAN}$")),
        (Rule::Shift, format!(r"^(?:in\s+)?{SPAN}(?:\s+from\s+(?P<a>{DATE}|now))?$")),
        (Rule::ShiftBack, format!(r"^{SPAN}\s+(?:ago|before\s+(?P<a>{DATE}))$")),
        (Rule::Diff, format!(r"^(?P<a>{DATE})\s*-\s*(?P<b>{DATE})$")),
        (Rule::Until, format!(r"^days?\s+(?:until|till|to)\s+(?P<b>{DATE})$")),
        (Rule::Since, format!(r"^days?\s+since\s+(?P<b>{DATE})$")),
        (Rule::Between, format!(r"^days?\s+between\s+(?P<a>{DATE})\s+and\s+(?P<b>{DATE})$")),
        (Rule::Show, r"^(?P<a>today|tomorrow|yesterday)$".to_owned()),
    ];
    rules.into_iter().map(|(rule, text)| (rule, pattern(&format!("(?i){text}")))).collect()
});

fn not_a_date(text: &str) -> Error {
    Error::Convert(format!("not a date: {text}"))
}

/// A date as typed: `today`, `tomorrow`, `yesterday`, `2026-3-7` or `7/3/2026` (two digit years are 20xx).
pub fn parse_date(text: &str, today: Date) -> Result<Date, Error> {
    let s = text.trim().to_lowercase();
    let s = if s.is_empty() { "today".to_owned() } else { s };
    match s.as_str() {
        "today" | "now" => return Ok(today),
        "tomorrow" => return add_days(today, 1),
        "yesterday" => return add_days(today, -1),
        _ => {}
    }
    let whole = |p: &str| p.trim().parse::<i32>().ok();
    let (y, m, d) = if s.contains('-') {
        let parts: Vec<_> = s.split('-').map(whole).collect();
        match parts.as_slice() {
            [Some(y), Some(m), Some(d)] => (*y, *m, *d),
            _ => return Err(not_a_date(&s)),
        }
    } else {
        let parts: Vec<_> = s.split(['/', '.']).map(whole).collect();
        match parts.as_slice() {
            [Some(d), Some(m), Some(y)] => (if *y < 100 { y + 2000 } else { *y }, *m, *d),
            _ => return Err(not_a_date(&s)),
        }
    };
    let valid = (1..=9999).contains(&y) && i8::try_from(m).is_ok() && i8::try_from(d).is_ok();
    valid.then(|| Date::new(y as i16, m as i8, d as i8).ok()).flatten().ok_or_else(|| not_a_date(&s))
}

fn out_of_range() -> Error {
    Error::Overflow("date value out of range".into())
}

fn add_days(d: Date, n: i64) -> Result<Date, Error> {
    d.checked_add(Span::new().try_days(n).map_err(|_| out_of_range())?).map_err(|_| out_of_range())
}

/// Months and years move to the same day number, or the month's last day when there is none (31 Jan + 1 month).
fn add_span(d: Date, n: i64, unit: &str) -> Result<Date, Error> {
    let unit = unit.to_lowercase();
    if unit.starts_with('d') {
        return add_days(d, n);
    }
    if unit.starts_with('w') {
        return add_days(d, n.checked_mul(7).ok_or_else(out_of_range)?);
    }
    let months = if unit.starts_with('y') { n.checked_mul(12).ok_or_else(out_of_range)? } else { n };
    let index = i64::from(d.month()) - 1 + months;
    let year = i64::from(d.year()) + index.div_euclid(12);
    let month = index.rem_euclid(12) + 1;
    if !(1..=9999).contains(&year) {
        return Err(Error::Value(format!("year {year} is out of range")));
    }
    let first = Date::new(year as i16, month as i8, 1).map_err(|_| out_of_range())?;
    let day = d.day().min(first.days_in_month());
    Date::new(year as i16, month as i8, day).map_err(|_| out_of_range())
}

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const MONTHS: [&str; 12] =
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

pub fn fmt_date(d: Date) -> String {
    let weekday = match d.weekday() {
        Weekday::Monday => 0,
        Weekday::Tuesday => 1,
        Weekday::Wednesday => 2,
        Weekday::Thursday => 3,
        Weekday::Friday => 4,
        Weekday::Saturday => 5,
        Weekday::Sunday => 6,
    };
    format!("{} {} {} {}", WEEKDAYS[weekday], d.day(), MONTHS[usize::from(d.month() as u8 - 1)], d.year())
}

fn days_text(n: i64) -> String {
    let mut text = format!("{} day{}", crate::format::group_digits(n), if n.abs() == 1 { "" } else { "s" });
    if n.abs() >= 14 {
        text.push_str(&format!(" ({:.1} weeks)", n as f64 / 7.0));
    }
    text
}

/// What a date sum gives: a date (`None`, "Tue 9 Dec 2026") or a gap in days (`Some(45)`, "45 days"). `Ok(None)`
/// when the text is not a date sum.
pub fn date_math(text: &str, today: Date) -> Result<Option<(Option<i64>, String)>, Error> {
    let t = text.trim();
    for (rule, regex) in DATE_RULES.iter() {
        let Some(m) = regex.captures(t) else { continue };
        let group = |name: &str| m.name(name).map(|g| g.as_str());
        let when = |name: &str| parse_date(group(name).unwrap_or("today"), today);
        return match rule {
            Rule::Show => Ok(Some((None, fmt_date(when("a")?)))),
            Rule::Shift | Rule::ShiftBack => {
                let n = span_count(&m)?;
                let n = if *rule == Rule::ShiftBack || group("op") == Some("-") { -n } else { n };
                let shifted = add_span(when("a")?, n, group("u").unwrap_or("d"))?;
                Ok(Some((None, fmt_date(shifted))))
            }
            _ => {
                let n = match rule {
                    Rule::Since => (today - when("b")?).get_days(),
                    Rule::Diff => (when("a")? - when("b")?).get_days(),
                    _ => (when("b")? - when("a")?).get_days(),
                };
                let n = i64::from(n);
                Ok(Some((Some(n), days_text(n))))
            }
        };
    }
    Ok(None)
}

fn span_count(m: &Captures<'_>) -> Result<i64, Error> {
    m["n"].parse().map_err(|_| out_of_range())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_ends_clamp() {
        let d = Date::new(2026, 1, 31).unwrap();
        assert_eq!(fmt_date(add_span(d, 1, "month").unwrap()), "Sat 28 Feb 2026");
        let leap = Date::new(2024, 2, 29).unwrap();
        assert_eq!(fmt_date(add_span(leap, 1, "year").unwrap()), "Fri 28 Feb 2025");
    }
}
