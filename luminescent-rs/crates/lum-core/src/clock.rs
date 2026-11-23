//! The wall clock, behind a trait so tests can fix the time.

use std::sync::Mutex;

use jiff::civil::DateTime;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp, ZonedDateTime};

pub trait Clock: Send + Sync {
    /// Local time, as Python's `datetime.now()`.
    fn now(&self) -> DateTime;

    /// Seconds since 1970, as Python's `time.time()`.
    fn timestamp(&self) -> f64;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime {
        ZonedDateTime::now().datetime()
    }

    fn timestamp(&self) -> f64 {
        Timestamp::now().as_microsecond() as f64 / 1e6
    }
}

/// A clock that stays where it is put. Its local time is read as UTC, like the fixtures (made with TZ=UTC).
#[derive(Debug)]
pub struct FixedClock(Mutex<DateTime>);

impl FixedClock {
    pub fn new(at: DateTime) -> FixedClock {
        FixedClock(Mutex::new(at))
    }

    pub fn set(&self, at: DateTime) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = at;
    }

    pub fn advance(&self, by: SignedDuration) {
        let mut at = self.0.lock().unwrap_or_else(|e| e.into_inner());
        *at = at.checked_add(by).unwrap_or(*at);
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime {
        *self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn timestamp(&self) -> f64 {
        let zoned = self.now().to_zoned_datetime(TimeZone::UTC).expect("every civil time exists in UTC");
        zoned.timestamp().as_millisecond() as f64 / 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_is_utc() {
        let clock = FixedClock::new(DateTime::constant(2026, 11, 23, 14, 30, 0, 0));
        assert_eq!(clock.timestamp(), 1795444200.0);
        clock.advance(SignedDuration::from_secs(90));
        assert_eq!(clock.now().strftime("%H:%M:%S").to_string(), "14:31:30");
    }
}
