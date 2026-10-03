//! Monotonic instants as plain nanoseconds.
//!
//! The pure core never reads a clock: every step is handed `now`. A `Mono` is a
//! `CLOCK_MONOTONIC` reading carried as an integer so fixtures are exact.

use std::time::Duration;

/// A point on the monotonic clock, in nanoseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Mono(pub u64);

impl Mono {
    /// An instant `ms` milliseconds after the clock's origin; for fixtures.
    pub const fn from_millis(ms: u64) -> Self {
        Self(ms.saturating_mul(1_000_000))
    }

    /// The time from `earlier` to `self`, zero if `earlier` is later.
    pub fn since(self, earlier: Mono) -> Duration {
        Duration::from_nanos(self.0.saturating_sub(earlier.0))
    }

    /// The instant `d` after `self`, saturating at the end of the clock.
    pub fn after(self, d: Duration) -> Mono {
        let ns = u64::try_from(d.as_nanos()).unwrap_or(u64::MAX);
        Mono(self.0.saturating_add(ns))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn since_saturates_and_after_adds() {
        let a = Mono::from_millis(10);
        let b = Mono::from_millis(35);
        assert_eq!(b.since(a), Duration::from_millis(25));
        assert_eq!(a.since(b), Duration::ZERO);
        assert_eq!(a.after(Duration::from_millis(25)), b);
        assert_eq!(Mono(u64::MAX).after(Duration::from_secs(1)), Mono(u64::MAX));
    }
}
