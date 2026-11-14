//! Fetching periodically while on AC power.

use std::thread;
use std::time::{Duration, SystemTime};

use crate::log::log;
use crate::power::{self, PowerSource};

/// Calls `fetch` every `interval` while on AC power, checking the power source and whether a fetch
/// is due every `check_interval`, forever.
pub fn run(interval: Duration, check_interval: Duration, mut fetch: impl FnMut()) -> ! {
    log!(
        "fetching every {} while on AC power",
        humantime::format_duration(interval)
    );
    let mut schedule = Schedule::new(interval);
    let mut last_source = None;
    loop {
        let source = power::current();
        // Log changes, rather than every check.
        if last_source.as_ref() != Some(&source) {
            match &source {
                Ok(PowerSource::Ac) => log!("on AC power: fetching"),
                Ok(source) => log!("on {source}: not fetching"),
                Err(error) => log!("couldn't read the power source, so not fetching: {error}"),
            }
            last_source = Some(source.clone());
        }

        let now = SystemTime::now();
        if source == Ok(PowerSource::Ac) && schedule.is_due(now) {
            schedule.record_fetch(now);
            fetch();
        }
        thread::sleep(check_interval.min(interval));
    }
}

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
        match self.last_fetch {
            None => true,
            // An error means the clock went backwards, making the time since unknowable.
            Some(last_fetch) => now
                .duration_since(last_fetch)
                .map_or(true, |elapsed| elapsed >= self.interval),
        }
    }

    /// Records that a fetch started at `at`.
    pub fn record_fetch(&mut self, at: SystemTime) {
        self.last_fetch = Some(at);
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
