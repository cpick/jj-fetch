//! Timestamped logging to standard error.

/// Writes a line to standard error, prefixed with the current UTC time.
///
/// Each call writes its whole message at once, so lines from concurrent callers don't interleave.
macro_rules! log {
    ($($arg:tt)*) => {
        eprintln!(
            "{} {}",
            humantime::format_rfc3339_seconds(std::time::SystemTime::now()),
            format_args!($($arg)*),
        )
    };
}

pub(crate) use log;
