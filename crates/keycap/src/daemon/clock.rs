//! The daemon's one clock read.

use crate::Mono;
use rustix::time::{ClockId, clock_gettime};

/// Now on `CLOCK_MONOTONIC`.
pub(crate) fn now() -> Mono {
    let t = clock_gettime(ClockId::Monotonic);
    let secs = u64::try_from(t.tv_sec).unwrap_or(0);
    let nanos = u64::try_from(t.tv_nsec).unwrap_or(0);
    Mono(secs.saturating_mul(1_000_000_000).saturating_add(nanos))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_does_not_run_backwards() {
        let a = now();
        let b = now();
        assert!(b >= a, "{a:?} then {b:?}");
        assert!(a > Mono(0), "{a:?} reads a real instant");
    }
}
