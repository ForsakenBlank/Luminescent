//! How a result is written: thousands separators, decimals, a base (hex, binary, octal) and pin names.

use num_traits::{Signed, ToPrimitive};

use crate::Num;
use crate::book::Base;

/// A result as text (`fmt`): whole numbers in the chosen base, big and tiny numbers in scientific form,
/// everything else with at most `decimals` places and no trailing zeros.
pub fn fmt(value: &Num, decimals: usize, thousands: bool, base: Base) -> String {
    if base != Base::Dec
        && value.is_whole()
        && let Some(text) = in_base(value, thousands, base)
    {
        return text;
    }
    match value {
        Num::Int(i) => grouped(&i.to_string(), thousands),
        Num::Float(f) if f.fract() == 0.0 && f.abs() < 1e15 => grouped(&format!("{}", *f as i64), thousands),
        Num::Float(f) => float_text(*f, decimals, thousands),
    }
}

fn in_base(value: &Num, thousands: bool, base: Base) -> Option<String> {
    let whole = value.to_int().ok()?;
    let magnitude = whole.abs();
    if magnitude >= num_bigint::BigInt::from(1u128 << 64) {
        return None;
    }
    let n = magnitude.to_u64()?;
    let (digits, prefix) = match base {
        Base::Hex => (format!("{n:X}"), "0x"),
        Base::Bin => (format!("{n:b}"), "0b"),
        Base::Oct => (format!("{n:o}"), "0o"),
        Base::Dec => return None,
    };
    let digits = if base == Base::Bin && thousands { nibbles(&digits) } else { digits };
    let sign = if whole.is_negative() { "-" } else { "" };
    Some(format!("{sign}{prefix}{digits}"))
}

/// Binary digits in groups of four from the right: `1010 1111`.
fn nibbles(digits: &str) -> String {
    let bytes = digits.as_bytes();
    let first = bytes.len() % 4;
    let mut groups = Vec::new();
    if first > 0 {
        groups.push(&digits[..first]);
    }
    groups.extend((first..bytes.len()).step_by(4).map(|i| &digits[i..i + 4]));
    groups.join(" ")
}

fn float_text(f: f64, decimals: usize, thousands: bool) -> String {
    let tiny = f != 0.0 && f.abs() < 10f64.powf(-(decimals as f64));
    if f.abs() >= 1e15 || tiny {
        return scientific(f, decimals.min(8));
    }
    let text = format!("{:.*}", decimals, f.abs());
    let (whole, fraction) = text.split_once('.').unwrap_or((&text, ""));
    let mut out = String::new();
    if f.is_sign_negative() {
        out.push('-');
    }
    out.push_str(&grouped(whole, thousands));
    let fraction = fraction.trim_end_matches('0');
    if !fraction.is_empty() {
        out.push('.');
        out.push_str(fraction);
    }
    out
}

/// Python's `f"{x:.8e}"`: the exponent has a sign and at least two digits.
fn scientific(f: f64, places: usize) -> String {
    let text = format!("{f:.places$e}");
    let (mantissa, exponent) = text.split_once('e').unwrap_or((&text, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let sign = if exponent < 0 { '-' } else { '+' };
    format!("{mantissa}e{sign}{:02}", exponent.abs())
}

pub(crate) fn group_digits(n: i64) -> String {
    grouped(&n.to_string(), true)
}

fn grouped(digits: &str, thousands: bool) -> String {
    let (sign, digits) = match digits.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", digits),
    };
    if !thousands {
        return format!("{sign}{digits}");
    }
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    format!("{sign}{out}")
}

/// A pin's name as a variable: lower case words joined by `_`, with `v_` in front when it would start with a
/// digit or be empty.
pub fn pin_name(name: &str) -> String {
    let lower = name.trim().to_lowercase();
    let mut out = String::new();
    let mut gap = false;
    for c in lower.chars() {
        if c.is_alphanumeric() || c == '_' {
            if gap && !out.is_empty() {
                out.push('_');
            }
            gap = false;
            out.push(c);
        } else {
            gap = true;
        }
    }
    let n = out.trim_matches('_').to_owned();
    if n.is_empty() || n.starts_with(|c: char| c.is_ascii_digit()) { format!("v_{n}") } else { n }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nibbles_group_from_the_right() {
        assert_eq!(nibbles("101"), "101");
        assert_eq!(nibbles("111111"), "11 1111");
        assert_eq!(nibbles("10101010"), "1010 1010");
    }

    #[test]
    fn scientific_matches_python() {
        assert_eq!(scientific(1e-11, 8), "1.00000000e-11");
        assert_eq!(scientific(1.5e20, 2), "1.50e+20");
    }
}
