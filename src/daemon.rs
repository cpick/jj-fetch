//! Fetching periodically while on AC power.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, SystemTime};

use crate::log::log;
use crate::power::{self, PowerSource};

/// Calls `fetch` every `interval` while on AC power, forever.
///
/// Rather than checking the power source periodically, it waits for IOKit to say that it changed.
/// So off AC power, nothing wakes the CPU. Returns only if it can't watch the power source.
pub fn run(interval: Duration, mut fetch: impl FnMut() + Send + 'static) -> String {
    log!(
        "fetching every {} while on AC power",
        humantime::format_duration(interval)
    );
    let (sender, receiver) = mpsc::channel();
    // Fetch on another thread, leaving this one to wait in its run loop for power notifications.
    thread::spawn(move || {
        fetch_while_on_ac_power(interval, &mut Notifications(receiver), &mut fetch);
    });
    let Err(error) = power::watch(move |source| {
        // Only fails if the fetching thread has gone, which only a panic (aborting) could cause.
        let _ = sender.send(source);
    });
    error
}

/// What waiting for the power source to change came to.
#[derive(Debug, PartialEq)]
pub enum Wait {
    /// The power source changed, to this (or to something that couldn't be read).
    Changed(Result<PowerSource, String>),
    /// The timeout passed first.
    TimedOut,
    /// The power source won't be reported again.
    Closed,
}

/// Reports changes to the power source, and the time.
pub trait PowerChanges {
    /// Waits for the power source to change, for up to `timeout`, or for as long as it takes if
    /// `None`.
    fn wait(&mut self, timeout: Option<Duration>) -> Wait;

    /// The current wall-clock time, which, unlike `Instant` on macOS, keeps counting while the
    /// computer sleeps.
    fn now(&self) -> SystemTime;
}

/// Power source changes, as sent by `power::watch` to a channel.
pub struct Notifications(pub Receiver<Result<PowerSource, String>>);

impl PowerChanges for Notifications {
    fn wait(&mut self, timeout: Option<Duration>) -> Wait {
        let received = match timeout {
            Some(timeout) => self.0.recv_timeout(timeout),
            None => self.0.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match received {
            Ok(source) => Wait::Changed(source),
            Err(RecvTimeoutError::Timeout) => Wait::TimedOut,
            Err(RecvTimeoutError::Disconnected) => Wait::Closed,
        }
    }

    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// Calls `fetch` every `interval` while `power` last said the computer is on AC power, until
/// `power` closes.
///
/// Off AC power, it waits for a change without a timeout, so nothing wakes the CPU until the
/// computer is plugged in.
pub fn fetch_while_on_ac_power(
    interval: Duration,
    power: &mut impl PowerChanges,
    mut fetch: impl FnMut(),
) {
    let mut schedule = Schedule::new(interval);
    let mut source = None;
    loop {
        let timeout = if source == Some(Ok(PowerSource::Ac)) {
            let now = power.now();
            if schedule.is_due(now) {
                schedule.record_fetch(now);
                fetch();
                continue;
            }
            Some(schedule.until_due(now))
        } else {
            None
        };

        match power.wait(timeout) {
            Wait::Changed(new_source) => {
                if source.as_ref() != Some(&new_source) {
                    match &new_source {
                        Ok(PowerSource::Ac) => log!("on AC power: fetching"),
                        Ok(source) => log!("on {source}: not fetching"),
                        Err(error) => {
                            log!("couldn't read the power source, so not fetching: {error}")
                        }
                    }
                }
                source = Some(new_source);
            }
            Wait::TimedOut => {}
            Wait::Closed => return,
        }
    }
}

/// Tracks when fetches are due.
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
        self.until_due(now).is_zero()
    }

    /// How long until a fetch is due, from `now`.
    pub fn until_due(&self, now: SystemTime) -> Duration {
        match self.last_fetch {
            None => Duration::ZERO,
            // An error means the clock went backwards, making the time since unknowable.
            Some(last_fetch) => now
                .duration_since(last_fetch)
                .map_or(Duration::ZERO, |elapsed| {
                    self.interval.saturating_sub(elapsed)
                }),
        }
    }

    /// Records that a fetch started at `at`.
    pub fn record_fetch(&mut self, at: SystemTime) {
        self.last_fetch = Some(at);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    use super::*;

    const INTERVAL: Duration = Duration::from_secs(15 * 60);

    fn at(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000 + secs)
    }

    fn minutes(minutes: u64) -> Duration {
        Duration::from_secs(minutes * 60)
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

    #[test]
    fn counts_down_until_due() {
        let mut schedule = Schedule::new(INTERVAL);
        assert_eq!(schedule.until_due(at(0)), Duration::ZERO);

        schedule.record_fetch(at(100));

        assert_eq!(schedule.until_due(at(100)), INTERVAL);
        assert_eq!(schedule.until_due(at(160)), INTERVAL - minutes(1));
        assert_eq!(
            schedule.until_due(at(100 + INTERVAL.as_secs())),
            Duration::ZERO
        );
        assert_eq!(schedule.until_due(at(100_000)), Duration::ZERO);
        assert_eq!(schedule.until_due(at(99)), Duration::ZERO);
    }

    /// Plays back scripted power source changes in virtual time, recording how the daemon waits.
    struct FakePower {
        clock: Rc<Cell<SystemTime>>,
        /// When each change happens, relative to the start.
        changes: VecDeque<(Duration, Result<PowerSource, String>)>,
        /// When to close, if not already waiting for a change that'll never come.
        end: Duration,
        /// When the daemon waited, and for how long.
        waits: Vec<(Duration, Option<Duration>)>,
    }

    impl FakePower {
        fn new(changes: Vec<(Duration, Result<PowerSource, String>)>, end: Duration) -> Self {
            FakePower {
                clock: Rc::new(Cell::new(at(0))),
                changes: changes.into(),
                end,
                waits: Vec::new(),
            }
        }

        fn elapsed(&self) -> Duration {
            self.clock.get().duration_since(at(0)).unwrap()
        }

        /// Runs the daemon until the script ends, returning when it fetched.
        fn run(&mut self) -> Vec<Duration> {
            let clock = Rc::clone(&self.clock);
            let mut fetches = Vec::new();
            fetch_while_on_ac_power(INTERVAL, self, || {
                fetches.push(clock.get().duration_since(at(0)).unwrap());
            });
            fetches
        }

        /// The timeouts of the waits that started within `range`.
        fn timeouts_within(&self, range: std::ops::Range<Duration>) -> Vec<Option<Duration>> {
            self.waits
                .iter()
                .filter(|(started, _)| range.contains(started))
                .map(|(_, timeout)| *timeout)
                .collect()
        }
    }

    impl PowerChanges for FakePower {
        fn wait(&mut self, timeout: Option<Duration>) -> Wait {
            let now = self.elapsed();
            self.waits.push((now, timeout));
            let deadline = timeout.map(|timeout| now + timeout);
            match self.changes.front() {
                Some((change_at, _)) if deadline.is_none_or(|deadline| *change_at <= deadline) => {
                    let (change_at, source) = self.changes.pop_front().unwrap();
                    self.clock.set(at(0) + change_at.max(now));
                    Wait::Changed(source)
                }
                _ => match deadline {
                    Some(deadline) if deadline <= self.end => {
                        self.clock.set(at(0) + deadline);
                        Wait::TimedOut
                    }
                    _ => Wait::Closed,
                },
            }
        }

        fn now(&self) -> SystemTime {
            self.clock.get()
        }
    }

    #[test]
    fn fetches_immediately_and_then_every_interval_on_ac_power() {
        let mut power = FakePower::new(vec![(minutes(0), Ok(PowerSource::Ac))], minutes(50));

        let fetches = power.run();

        assert_eq!(fetches, [minutes(0), minutes(15), minutes(30), minutes(45)]);
    }

    #[test]
    fn never_sets_a_timer_off_ac_power() {
        let mut power = FakePower::new(
            vec![
                (minutes(0), Ok(PowerSource::Battery)),
                (minutes(20), Ok(PowerSource::Ups)),
                (minutes(40), Err("no idea".to_owned())),
            ],
            minutes(120),
        );

        let fetches = power.run();

        assert_eq!(fetches, []);
        assert_eq!(power.waits.len(), 4, "{:?}", power.waits);
        assert_eq!(power.timeouts_within(minutes(0)..minutes(120)), [None; 4]);
    }

    #[test]
    fn stops_when_unplugged_and_resumes_when_plugged_in() {
        let mut power = FakePower::new(
            vec![
                (minutes(0), Ok(PowerSource::Ac)),
                (minutes(5), Ok(PowerSource::Battery)),
                // Overdue by now, so it fetches as soon as it's plugged in.
                (minutes(40), Ok(PowerSource::Ac)),
                (minutes(50), Ok(PowerSource::Battery)),
            ],
            minutes(120),
        );

        let fetches = power.run();

        assert_eq!(fetches, [minutes(0), minutes(40)]);
        let unplugged = [minutes(5)..minutes(40), minutes(50)..minutes(120)];
        for range in unplugged {
            let timeouts = power.timeouts_within(range.clone());
            assert!(!timeouts.is_empty(), "{range:?}");
            assert!(
                timeouts.iter().all(Option::is_none),
                "{range:?}: {timeouts:?}"
            );
        }
    }

    #[test]
    fn waits_out_the_interval_when_plugged_back_in_soon() {
        let mut power = FakePower::new(
            vec![
                (minutes(0), Ok(PowerSource::Ac)),
                (minutes(5), Ok(PowerSource::Battery)),
                (minutes(6), Ok(PowerSource::Ac)),
            ],
            minutes(40),
        );

        let fetches = power.run();

        assert_eq!(fetches, [minutes(0), minutes(15), minutes(30)]);
    }

    #[test]
    fn notifications_come_from_a_channel() {
        let (sender, receiver) = mpsc::channel();
        let mut notifications = Notifications(receiver);

        sender.send(Ok(PowerSource::Battery)).unwrap();
        assert_eq!(
            notifications.wait(None),
            Wait::Changed(Ok(PowerSource::Battery))
        );
        sender.send(Ok(PowerSource::Ac)).unwrap();
        assert_eq!(
            notifications.wait(Some(Duration::ZERO)),
            Wait::Changed(Ok(PowerSource::Ac))
        );
        assert_eq!(
            notifications.wait(Some(Duration::from_millis(1))),
            Wait::TimedOut
        );
        drop(sender);
        assert_eq!(notifications.wait(None), Wait::Closed);
        assert_eq!(notifications.wait(Some(Duration::ZERO)), Wait::Closed);
    }
}
