//! Everything the Calculator and the Timer know that is not drawing: the sum parser, number formats, unit,
//! currency and date conversions, the history and pins file, and the time words people type ("5m", "at 17:30").

mod book;
mod convert;
mod eval;
mod format;
mod funcs;
mod num;
mod rates;
pub mod timeparse;

pub use book::{Base, Book, Computed, Entry, Options};
pub use convert::{convert, date_math, fmt_date, parse_date, unit, unit_count};
pub use eval::{CONSTANTS, FUNCTIONS, MAX_FACT, MAX_POW, Scope, evaluate, is_reserved, prepare};
pub use format::{fmt, pin_name};
pub use num::Num;
pub use rates::{Rates, refresh_rates};

/// Why a sum could not be worked out. `kind` and `message` are the names and texts the Python app's exceptions
/// had, which the fixtures record.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Calc(String),
    #[error("{0}")]
    Convert(String),
    #[error("{0}")]
    ZeroDivision(String),
    #[error("{0}")]
    Value(String),
    #[error("{0}")]
    Overflow(String),
}

impl Error {
    pub fn kind(&self) -> &'static str {
        match self {
            Error::Calc(_) => "CalcError",
            Error::Convert(_) => "ConvertError",
            Error::ZeroDivision(_) => "ZeroDivisionError",
            Error::Value(_) => "ValueError",
            Error::Overflow(_) => "OverflowError",
        }
    }

    /// What the preview line says for this error: the Calculator's own messages, a plain word for dividing by
    /// zero, and "not a sum" for everything else.
    pub fn preview(&self) -> String {
        match self {
            Error::ZeroDivision(_) => "can't divide by 0".to_owned(),
            Error::Calc(m) | Error::Convert(m) => format!("⚠ {m}"),
            _ => "⚠ not a sum".to_owned(),
        }
    }
}
