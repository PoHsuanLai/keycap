//! What the reader still has to look for after the messages of a pass. Pure.
//!
//! A keyboard plugged in during a burst of device events (a hub flapping) is only seen if the
//! kernel's event queue did not overflow, so the reader also looks at every node again from
//! time to time. Many `Appeared` events of one burst cost one look.

use crate::Mono;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

/// How often every node is looked at again with nothing having said so.
const SAFETY_EVERY: Duration = Duration::from_secs(30);

/// The nodes to look at.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) enum Lookout {
    #[default]
    Nothing,
    /// These nodes appeared.
    Paths(BTreeSet<PathBuf>),
    /// Any node may have come or gone.
    Everything,
}

impl Lookout {
    /// `path` appeared.
    pub(super) fn appeared(self, path: PathBuf) -> Self {
        match self {
            Lookout::Nothing => Lookout::Paths(BTreeSet::from([path])),
            Lookout::Paths(mut paths) => {
                paths.insert(path);
                Lookout::Paths(paths)
            }
            Lookout::Everything => Lookout::Everything,
        }
    }

    /// Events were lost, or the safety look is due.
    pub(super) fn everything(self) -> Self {
        Lookout::Everything
    }

    /// Whether the node at `path` is to be looked at.
    pub(super) fn wants(&self, path: &std::path::Path) -> bool {
        match self {
            Lookout::Nothing => false,
            Lookout::Paths(paths) => paths.contains(path),
            Lookout::Everything => true,
        }
    }
}

/// When the next look at every node is due even if nothing said so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SafetyLook {
    next: Mono,
}

impl SafetyLook {
    /// The first one is `SAFETY_EVERY` after `now`.
    pub(super) fn after(now: Mono) -> Self {
        Self {
            next: now.after(SAFETY_EVERY),
        }
    }

    /// How long until it is due; zero when it is.
    pub(super) fn until(self, now: Mono) -> Duration {
        if self.next > now {
            self.next.since(now)
        } else {
            Duration::ZERO
        }
    }

    /// Whether it is due.
    pub(super) fn due(self, now: Mono) -> bool {
        self.until(now) == Duration::ZERO
    }

    /// Done at `now`: the next is `SAFETY_EVERY` later.
    pub(super) fn done(self, now: Mono) -> Self {
        Self::after(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn a_burst_of_appearances_is_one_look_at_those_nodes_and_everything_wins() {
        let burst = Lookout::Nothing
            .appeared(p("/dev/input/event3"))
            .appeared(p("/dev/input/event4"))
            .appeared(p("/dev/input/event3"));
        assert_eq!(
            burst,
            Lookout::Paths(BTreeSet::from([
                p("/dev/input/event3"),
                p("/dev/input/event4")
            ]))
        );
        assert!(burst.wants(&p("/dev/input/event4")));
        assert!(!burst.wants(&p("/dev/input/event9")));
        let all = burst.everything().appeared(p("/dev/input/event5"));
        assert_eq!(all, Lookout::Everything);
        assert!(all.wants(&p("/dev/input/event9")));
        assert!(!Lookout::Nothing.wants(&p("/dev/input/event3")));
    }

    #[test]
    fn the_safety_look_is_due_every_thirty_seconds() {
        let t0 = Mono::default();
        let look = SafetyLook::after(t0);
        assert!(!look.due(t0.after(Duration::from_secs(29))));
        assert_eq!(
            look.until(t0.after(Duration::from_secs(20))),
            Duration::from_secs(10)
        );
        let late = t0.after(Duration::from_secs(31));
        assert!(look.due(late));
        let next = look.done(late);
        assert!(!next.due(late.after(Duration::from_secs(29))));
        assert!(next.due(late.after(Duration::from_secs(30))));
    }
}
