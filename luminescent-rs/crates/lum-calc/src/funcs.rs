//! The calculator's functions: `sqrt`, `sin`, `round`, `fact` and the rest, with Python's `math` answers.

use std::f64::consts::PI;

use num_bigint::BigInt;
use num_traits::{FromPrimitive, One, ToPrimitive};

use crate::eval::MAX_FACT;
use crate::{Error, Num};

fn domain() -> Error {
    Error::Value("math domain error".into())
}

fn range() -> Error {
    Error::Overflow("math range error".into())
}

fn arity(name: &str) -> Error {
    Error::Value(format!("{name}() takes a different number of arguments"))
}

pub fn call(name: &str, args: &[Num], degrees: bool) -> Result<Num, Error> {
    match name {
        "min" | "max" => extreme(name == "max", args),
        "hypot" => hypot(args),
        "round" => match args {
            [x] => round(x, 0),
            [x, n] => round(x, int_arg(n)?),
            _ => Err(arity(name)),
        },
        "abs" | "floor" | "ceil" | "fact" | "factorial" => match args {
            [x] => one_whole(name, x),
            _ => Err(arity(name)),
        },
        _ => match args {
            [x] => Ok(Num::Float(one_decimal(name, x.to_f64()?, degrees)?)),
            _ => Err(arity(name)),
        },
    }
}

fn int_arg(n: &Num) -> Result<i64, Error> {
    n.to_int()?.to_i64().ok_or_else(|| Error::Overflow("Python int too large to convert to C long".into()))
}

fn one_whole(name: &str, x: &Num) -> Result<Num, Error> {
    match name {
        "abs" => Ok(x.abs()),
        "floor" | "ceil" => match x {
            Num::Int(_) => Ok(x.clone()),
            Num::Float(f) => {
                let r = if name == "floor" { f.floor() } else { f.ceil() };
                BigInt::from_f64(r)
                    .map(Num::Int)
                    .ok_or_else(|| Error::Overflow("cannot convert float infinity to integer".into()))
            }
        },
        _ => fact(x),
    }
}

fn fact(x: &Num) -> Result<Num, Error> {
    if !x.is_whole() || x.is_negative() {
        return Err(Error::Calc("fact() needs a whole number ≥ 0".into()));
    }
    let n = x.to_int()?;
    if n > BigInt::from(MAX_FACT) {
        return Err(Error::Calc(format!("fact() is limited to {MAX_FACT}")));
    }
    let n = n.to_u32().unwrap_or(0);
    Ok(Num::Int((2..=n).fold(BigInt::one(), |acc, k| acc * k)))
}

/// `round(x, n)`: whole numbers stay whole, decimals stay decimals (`round(2.5)` is `2.0`, as in the Python app),
/// and a tie goes to the even neighbour.
fn round(x: &Num, digits: i64) -> Result<Num, Error> {
    match x {
        Num::Int(i) if digits >= 0 => Ok(Num::Int(i.clone())),
        Num::Int(i) => {
            let unit = BigInt::from(10).pow(digits.unsigned_abs().min(u64::from(u32::MAX)) as u32);
            Ok(Num::Int(round_half_even(i, &unit)))
        }
        Num::Float(f) if !f.is_finite() => Ok(Num::Float(*f)),
        Num::Float(f) if digits >= 0 => {
            if digits > 330 {
                return Ok(Num::Float(*f));
            }
            // formatting rounds the exact binary value, which is what Python's round does too
            let text = format!("{f:.*}", digits as usize);
            text.parse().map(Num::Float).map_err(|_| Error::Value("could not round".into()))
        }
        Num::Float(f) => {
            let unit = 10f64.powi(i32::try_from(-digits).unwrap_or(i32::MAX));
            let scaled = f / unit;
            let nearest = scaled.round();
            let rounded =
                if (scaled - scaled.trunc()).abs() == 0.5 { 2.0 * (scaled / 2.0).round() } else { nearest };
            Ok(Num::Float(rounded * unit))
        }
    }
}

fn round_half_even(value: &BigInt, unit: &BigInt) -> BigInt {
    use num_integer::Integer;
    let (q, r) = value.div_mod_floor(unit);
    let twice = &r * 2;
    let up = twice > *unit || (twice == *unit && q.is_odd());
    (if up { q + 1 } else { q }) * unit
}

fn extreme(max: bool, args: &[Num]) -> Result<Num, Error> {
    let mut best = args.first().ok_or_else(|| Error::Value("expected at least 1 argument".into()))?;
    for item in &args[1..] {
        let better = if max { item.compare(best).is_gt() } else { item.compare(best).is_lt() };
        if better {
            best = item;
        }
    }
    Ok(best.clone())
}

fn hypot(args: &[Num]) -> Result<Num, Error> {
    let values = args.iter().map(Num::to_f64).collect::<Result<Vec<_>, _>>()?;
    let result = match values.as_slice() {
        [] => 0.0,
        [a] => a.abs(),
        [a, b] => a.hypot(*b),
        many => {
            let top = many.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
            if top == 0.0 {
                0.0
            } else {
                top * many.iter().map(|v| (v / top) * (v / top)).sum::<f64>().sqrt()
            }
        }
    };
    Ok(Num::Float(result))
}

/// The functions that take one number and give a decimal.
fn one_decimal(name: &str, x: f64, degrees: bool) -> Result<f64, Error> {
    let to_radians = |v: f64| if degrees { v * (PI / 180.0) } else { v };
    let from_radians = |v: f64| if degrees { v * (180.0 / PI) } else { v };
    let checked = |ok: bool, v: f64| if ok { Ok(v) } else { Err(domain()) };
    Ok(match name {
        "sqrt" => checked(x >= 0.0, x.sqrt())?,
        "cbrt" => x.abs().powf(1.0 / 3.0).copysign(x),
        "sin" => to_radians(x).sin(),
        "cos" => to_radians(x).cos(),
        "tan" => to_radians(x).tan(),
        "asin" => from_radians(checked(x.abs() <= 1.0, x.asin())?),
        "acos" => from_radians(checked(x.abs() <= 1.0, x.acos())?),
        "atan" => from_radians(x.atan()),
        "ln" => checked(x > 0.0, x.ln())?,
        "log" => checked(x > 0.0, x.log10())?,
        "log2" => checked(x > 0.0, x.log2())?,
        "exp" => {
            let v = x.exp();
            if v.is_infinite() {
                return Err(range());
            }
            v
        }
        other => return Err(Error::Calc(format!("unknown function {other}"))),
    })
}
