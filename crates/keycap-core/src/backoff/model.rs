//! `Backoff`: the delay to wait next and the cap it doubles up to.

use std::time::Duration;

/// Exponential backoff: the first delay, doubling, capped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    first: Duration,
    next: Duration,
    cap: Duration,
}

impl Backoff {
    /// Device reconnects: 1 s doubling to 30 s.
    pub const DEVICE: Backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(30));

    /// A backoff starting at `first` and doubling up to `cap`.
    pub const fn new(first: Duration, cap: Duration) -> Self {
        Self {
            first,
            next: first,
            cap,
        }
    }

    /// The delay to wait now, and the backoff after it.
    pub fn step(self) -> (Duration, Backoff) {
        let next = self.next.saturating_mul(2).min(self.cap);
        (self.next, Backoff { next, ..self })
    }

    /// Back to the first delay, after a success.
    pub fn reset(self) -> Backoff {
        Backoff {
            next: self.first,
            ..self
        }
    }
}
