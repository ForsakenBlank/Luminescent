//! Numbers with Python's rules, because the calculator's answers (and the history files it wrote) come from
//! Python: whole numbers are as big as they like, `/` always gives a decimal, `//` and `%` follow the divisor,
//! and a decimal that is a whole number still prints without its `.0`.

use std::cmp::Ordering;

use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{FromPrimitive, Signed, ToPrimitive, Zero};
use serde_json::{Number, Value};

use crate::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum Num {
    Int(BigInt),
    Float(f64),
}

impl From<i64> for Num {
    fn from(n: i64) -> Num {
        Num::Int(BigInt::from(n))
    }
}

impl From<f64> for Num {
    fn from(f: f64) -> Num {
        Num::Float(f)
    }
}

const OUT_OF_RANGE: &str = "(34, 'Numerical result out of range')";

impl Num {
    pub fn is_float(&self) -> bool {
        matches!(self, Num::Float(_))
    }

    /// A whole number, written as an int or as a decimal such as `3.0` (`value != int(value)` in Python).
    pub fn is_whole(&self) -> bool {
        match self {
            Num::Int(_) => true,
            Num::Float(f) => f.is_finite() && f.fract() == 0.0,
        }
    }

    pub fn to_f64(&self) -> Result<f64, Error> {
        match self {
            Num::Float(f) => Ok(*f),
            Num::Int(i) => match i.to_f64() {
                Some(f) if f.is_finite() => Ok(f),
                _ => Err(Error::Overflow("int too large to convert to float".into())),
            },
        }
    }

    /// The whole number behind this one (`int(x)`), for `!`, bit sums and `fact`.
    pub fn to_int(&self) -> Result<BigInt, Error> {
        match self {
            Num::Int(i) => Ok(i.clone()),
            Num::Float(f) if f.is_nan() => Err(Error::Value("cannot convert float NaN to integer".into())),
            Num::Float(f) if f.is_infinite() => {
                Err(Error::Overflow("cannot convert float infinity to integer".into()))
            }
            Num::Float(f) => Ok(BigInt::from_f64(f.trunc()).unwrap_or_default()),
        }
    }

    pub fn is_zero(&self) -> bool {
        match self {
            Num::Int(i) => i.is_zero(),
            Num::Float(f) => *f == 0.0,
        }
    }

    pub fn is_negative(&self) -> bool {
        match self {
            Num::Int(i) => i.is_negative(),
            Num::Float(f) => *f < 0.0,
        }
    }

    pub fn abs(&self) -> Num {
        match self {
            Num::Int(i) => Num::Int(i.abs()),
            Num::Float(f) => Num::Float(f.abs()),
        }
    }

    pub fn neg(&self) -> Num {
        match self {
            Num::Int(i) => Num::Int(-i),
            Num::Float(f) => Num::Float(-f),
        }
    }

    pub fn compare(&self, other: &Num) -> Ordering {
        match (self, other) {
            (Num::Int(a), Num::Int(b)) => a.cmp(b),
            _ => {
                let a = self.to_f64().unwrap_or(f64::INFINITY);
                let b = other.to_f64().unwrap_or(f64::INFINITY);
                a.partial_cmp(&b).unwrap_or(Ordering::Equal)
            }
        }
    }

    pub fn add(&self, other: &Num) -> Result<Num, Error> {
        match (self, other) {
            (Num::Int(a), Num::Int(b)) => Ok(Num::Int(a + b)),
            _ => Ok(Num::Float(self.to_f64()? + other.to_f64()?)),
        }
    }

    pub fn sub(&self, other: &Num) -> Result<Num, Error> {
        match (self, other) {
            (Num::Int(a), Num::Int(b)) => Ok(Num::Int(a - b)),
            _ => Ok(Num::Float(self.to_f64()? - other.to_f64()?)),
        }
    }

    pub fn mul(&self, other: &Num) -> Result<Num, Error> {
        match (self, other) {
            (Num::Int(a), Num::Int(b)) => Ok(Num::Int(a * b)),
            _ => Ok(Num::Float(self.to_f64()? * other.to_f64()?)),
        }
    }

    /// `/`: always a decimal.
    pub fn div(&self, other: &Num) -> Result<Num, Error> {
        if let (Num::Int(a), Num::Int(b)) = (self, other) {
            if b.is_zero() {
                return Err(Error::ZeroDivision("division by zero".into()));
            }
            return int_div(a, b).map(Num::Float);
        }
        let b = other.to_f64()?;
        if b == 0.0 {
            return Err(Error::ZeroDivision("float division by zero".into()));
        }
        Ok(Num::Float(self.to_f64()? / b))
    }

    /// `//`: rounds down, so `-7 // 2` is `-4`.
    pub fn floor_div(&self, other: &Num) -> Result<Num, Error> {
        if let (Num::Int(a), Num::Int(b)) = (self, other) {
            if b.is_zero() {
                return Err(Error::ZeroDivision("integer division or modulo by zero".into()));
            }
            return Ok(Num::Int(a.div_floor(b)));
        }
        let (a, b) = (self.to_f64()?, other.to_f64()?);
        if b == 0.0 {
            return Err(Error::ZeroDivision("float floor division by zero".into()));
        }
        let rest = a % b;
        let mut div = (a - rest) / b;
        if rest != 0.0 && (b < 0.0) != (rest < 0.0) {
            div -= 1.0;
        }
        let floored = if div != 0.0 {
            let f = div.floor();
            if div - f > 0.5 { f + 1.0 } else { f }
        } else {
            0.0_f64.copysign(a / b)
        };
        Ok(Num::Float(floored))
    }

    /// `%` and `mod`: the answer takes the sign of the divisor, so `-7 mod 3` is `2`.
    pub fn modulo(&self, other: &Num) -> Result<Num, Error> {
        if let (Num::Int(a), Num::Int(b)) = (self, other) {
            if b.is_zero() {
                return Err(Error::ZeroDivision("integer modulo by zero".into()));
            }
            return Ok(Num::Int(a.mod_floor(b)));
        }
        let (a, b) = (self.to_f64()?, other.to_f64()?);
        if b == 0.0 {
            return Err(Error::ZeroDivision("float modulo by zero".into()));
        }
        let mut m = a % b;
        if m != 0.0 {
            if (b < 0.0) != (m < 0.0) {
                m += b;
            }
        } else {
            m = 0.0_f64.copysign(b);
        }
        Ok(Num::Float(m))
    }

    /// `**` without the size limits (the evaluator checks those first).
    pub fn pow(&self, other: &Num) -> Result<Num, Error> {
        if let (Num::Int(a), Num::Int(b)) = (self, other)
            && !b.is_negative()
        {
            let exp = b.to_u32().ok_or_else(|| Error::Overflow("that power is too big".into()))?;
            return Ok(Num::Int(num_traits::pow::Pow::pow(a, exp)));
        }
        let (a, b) = (self.to_f64()?, other.to_f64()?);
        if a == 0.0 && b < 0.0 {
            return Err(Error::ZeroDivision("0.0 cannot be raised to a negative power".into()));
        }
        if a < 0.0 && b.fract() != 0.0 {
            // Python answers with a complex number here, which a calculator has no room for
            return Err(Error::Calc("no finite answer".into()));
        }
        let r = a.powf(b);
        if r.is_infinite() && a.is_finite() && b.is_finite() {
            return Err(Error::Overflow(OUT_OF_RANGE.into()));
        }
        Ok(Num::Float(r))
    }

    /// How many digits `10 ** x` would take: for the "that power is too big" check.
    pub fn log10_abs(&self) -> f64 {
        match self {
            Num::Float(f) => f.abs().log10(),
            Num::Int(i) => match i.abs().to_f64() {
                Some(f) if f.is_finite() => f.log10(),
                _ => i.bits() as f64 * std::f64::consts::LOG10_2,
            },
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            Num::Int(i) => {
                if let Some(n) = i.to_i64() {
                    Value::Number(n.into())
                } else if let Some(n) = i.to_u64() {
                    Value::Number(n.into())
                } else {
                    // JSON here keeps numbers in 64 bits; a bigger answer is written as the nearest decimal
                    i.to_f64().and_then(Number::from_f64).map_or(Value::Null, Value::Number)
                }
            }
            Num::Float(f) => Number::from_f64(*f).map_or(Value::Null, Value::Number),
        }
    }

    /// A number read from a data file; `None` for null and anything that is not a number.
    pub fn from_json(value: &Value) -> Option<Num> {
        let Value::Number(n) = value else { return None };
        if let Some(i) = n.as_i64() {
            Some(Num::from(i))
        } else if let Some(u) = n.as_u64() {
            Some(Num::Int(BigInt::from(u)))
        } else {
            n.as_f64().map(Num::Float)
        }
    }
}

/// Whole-number division as Python does it: correctly rounded, even when both sides are too big for a decimal.
fn int_div(a: &BigInt, b: &BigInt) -> Result<f64, Error> {
    if let (Some(x), Some(y)) = (a.to_i64(), b.to_i64())
        && x.unsigned_abs() < (1 << 53)
        && y.unsigned_abs() < (1 << 53)
    {
        return Ok(x as f64 / y as f64);
    }
    // shift both so the quotient keeps 64 bits, then let the decimal division round once
    let shift = (b.bits() as i64 - a.bits() as i64 + 64).max(0) as usize;
    let (q, r) = (a << shift).div_rem(b);
    let sticky = if r.is_zero() { 0 } else { 1 };
    let q = (q << 1usize) | BigInt::from(sticky);
    let f = q.to_f64().unwrap_or(f64::INFINITY);
    let scaled = f * 0.5_f64.powi(shift as i32 + 1);
    if scaled.is_finite() {
        Ok(scaled)
    } else {
        Err(Error::Overflow("integer division result too large for a float".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(n: i64) -> Num {
        Num::from(n)
    }

    #[test]
    fn division_and_remainders_follow_python() {
        assert_eq!(int(-7).floor_div(&int(2)).unwrap(), int(-4));
        assert_eq!(int(-7).modulo(&int(3)).unwrap(), int(2));
        assert_eq!(int(7).modulo(&int(-3)).unwrap(), int(-2));
        assert_eq!(Num::Float(-7.5).floor_div(&int(2)).unwrap(), Num::Float(-4.0));
        assert_eq!(Num::Float(7.5).modulo(&int(-2)).unwrap(), Num::Float(-0.5));
        assert_eq!(int(10).div(&int(4)).unwrap(), Num::Float(2.5));
        assert!(matches!(int(1).div(&int(0)), Err(Error::ZeroDivision(m)) if m == "division by zero"));
    }

    #[test]
    fn big_whole_numbers_divide_without_overflowing() {
        let big = Num::Int(BigInt::from(10).pow(400));
        let tiny = Num::Int(BigInt::from(10).pow(399));
        assert_eq!(big.div(&tiny).unwrap(), Num::Float(10.0));
        assert!(matches!(big.div(&int(1)), Err(Error::Overflow(_))));
    }

    #[test]
    fn json_numbers_keep_their_kind() {
        assert_eq!(Num::from_json(&serde_json::json!(11400)), Some(int(11400)));
        assert_eq!(Num::from_json(&serde_json::json!(2.5)), Some(Num::Float(2.5)));
        assert_eq!(Num::from_json(&Value::Null), None);
        assert_eq!(int(7).to_json(), serde_json::json!(7));
    }
}
