//! The wall clock, behind a trait so tests can fix the time.

use std::sync::Mutex;

use chrono::{Local, NaiveDateTime, TimeZone};

pub trait Clock: Send + Sync {
    /// Local time, as Python's `datetime.now()`.
    fn now(&self) -> NaiveDateTime;

    /// Seconds since 1970, as Python's `time.time()`.
    fn timestamp(&self) -> f64;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> NaiveDateTime {
        Local::now().naive_local()
    }

    fn timestamp(&self) -> f64 {
        let now = Local::now();
        now.timestamp() as f64 + f64::from(now.timestamp_subsec_micros()) / 1e6
    }
}

/// A clock that stays where it is put. Its local time is read as UTC, like the fixtures (made with TZ=UTC).
#[derive(Debug)]
pub struct FixedClock(Mutex<NaiveDateTime>);

impl FixedClock {
    pub fn new(at: NaiveDateTime) -> FixedClock {
        FixedClock(Mutex::new(at))
    }

    pub fn set(&self, at: NaiveDateTime) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = at;
    }

    pub fn advance(&self, by: chrono::Duration) {
        let mut at = self.0.lock().unwrap_or_else(|e| e.into_inner());
        *at += by;
    }
}

impl Clock for FixedClock {
    fn now(&self) -> NaiveDateTime {
        *self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn timestamp(&self) -> f64 {
        chrono::Utc.from_utc_datetime(&self.now()).timestamp_millis() as f64 / 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_is_utc() {
        let at = NaiveDateTime::parse_from_str("2026-11-23 14:30:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let clock = FixedClock::new(at);
        assert_eq!(clock.timestamp(), 1795444200.0);
        clock.advance(chrono::Duration::seconds(90));
        assert_eq!(clock.now().format("%H:%M:%S").to_string(), "14:31:30");
    }
}
