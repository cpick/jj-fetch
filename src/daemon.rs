//! Fetching periodically while on AC power.

use std::time::{Duration, SystemTime};

/// Tracks when fetches are due.
///
/// Uses wall-clock time, which, unlike `Instant` on macOS, keeps counting while the computer
/// sleeps.
pub struct Schedule {
    interval: Duration,
    last_fetch: Option<SystemTime>,
}

impl Schedule {
    pub fn new(interval: Duration) -> Self {
        Schedule {
            interval,
            last_fetch: None,
        }
    }

    /// Whether a fetch is due at `now`.
    pub fn is_due(&self, now: SystemTime) -> bool {
        let _ = now;
        todo!()
    }

    /// Records that a fetch started at `at`.
    pub fn record_fetch(&mut self, at: SystemTime) {
        let _ = at;
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTERVAL: Duration = Duration::from_secs(15 * 60);

    fn at(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000 + secs)
    }

    #[test]
    fn is_due_before_the_first_fetch() {
        assert!(Schedule::new(INTERVAL).is_due(at(0)));
    }

    #[test]
    fn is_due_once_the_interval_has_passed() {
        let mut schedule = Schedule::new(INTERVAL);
        schedule.record_fetch(at(0));

        assert!(!schedule.is_due(at(0)));
        assert!(!schedule.is_due(at(INTERVAL.as_secs() - 1)));
        assert!(schedule.is_due(at(INTERVAL.as_secs())));
        assert!(schedule.is_due(at(10 * INTERVAL.as_secs())));
    }

    #[test]
    fn is_due_after_the_clock_goes_backwards() {
        let mut schedule = Schedule::new(INTERVAL);
        schedule.record_fetch(at(100));

        assert!(schedule.is_due(at(99)));
    }
}
