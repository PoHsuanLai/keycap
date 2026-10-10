//! What the socket thread allows a client, and when it stops listening or stops complaining.
//! The socket is world-connectable, so every bound here is against a client that is hostile or
//! merely broken. Pure: instants are arguments.

use crate::Mono;
use rustix::io::Errno;
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::time::Duration;

/// Clients connected at once, over everyone.
pub(super) const MAX_CLIENTS: usize = 32;
/// Clients connected at once for one uid. Every app of one person holds a connection for the
/// keymap's changes and another while it has registered its actions, so this is a share for a
/// whole desktop's apps, not for one program.
pub(super) const MAX_CLIENTS_PER_UID: usize = 64;
/// Messages read from one client in one pass over the sockets; the rest wait for the next.
pub(super) const MESSAGES_PER_PASS: usize = 16;
/// Connections accepted in one pass.
pub(super) const ACCEPTS_PER_PASS: usize = 16;
/// How long the listener is left alone after `accept` failed for a reason that will not clear
/// by retrying at once (out of file descriptors).
pub(super) const ACCEPT_PAUSE: Duration = Duration::from_secs(1);
/// How often one uid's misbehaviour is logged.
pub(super) const COMPLAINT_INTERVAL: Duration = Duration::from_secs(60);

/// The two client caps, as data so a table can use small ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Caps {
    pub total: usize,
    pub per_uid: usize,
}

impl Caps {
    pub(super) const SERVICE: Caps = Caps {
        total: MAX_CLIENTS,
        per_uid: MAX_CLIENTS_PER_UID,
    };
}

/// Whether a new connection is let in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Admission {
    Admit,
    /// The daemon is full.
    TooManyClients,
    /// This uid already has its share.
    TooManyForUser,
}

/// Whether a client of `uid` may join `connected` (the uid of every current client).
///
/// The seat owner's connections neither count toward nor are refused by the daemon-wide cap, so
/// other users filling the socket can never lock the owner's desktop out; the owner keeps only
/// its per-uid share.
pub(super) fn admit(
    connected: impl IntoIterator<Item = u32>,
    uid: u32,
    owner: Option<u32>,
    caps: Caps,
) -> Admission {
    let (others, mine) = connected
        .into_iter()
        .fold((0usize, 0usize), |(others, mine), other| {
            (
                others + usize::from(Some(other) != owner),
                mine + usize::from(other == uid),
            )
        });
    if Some(uid) != owner && others >= caps.total {
        Admission::TooManyClients
    } else if mine >= caps.per_uid {
        Admission::TooManyForUser
    } else {
        Admission::Admit
    }
}

/// What a failed `accept` means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AcceptFailure {
    /// No connection is waiting: the loop is done for this pass.
    NothingPending,
    /// Interrupted: ask again.
    Retry,
    /// The connection stays pending, so polling would return at once and spin: leave the
    /// listener alone for a while.
    Pause,
}

pub(super) fn accept_failure(e: Errno) -> AcceptFailure {
    match e {
        Errno::AGAIN => AcceptFailure::NothingPending,
        Errno::INTR => AcceptFailure::Retry,
        _ => AcceptFailure::Pause,
    }
}

/// Whether the listener is polled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Intake {
    Open,
    /// Not polled until this instant.
    PausedUntil(Mono),
}

impl Intake {
    /// The listener, left alone from `now` for `ACCEPT_PAUSE`.
    pub(super) fn paused_at(now: Mono) -> Self {
        Intake::PausedUntil(now.after(ACCEPT_PAUSE))
    }

    /// The state at `now`: a pause that has run out is over.
    pub(super) fn at(self, now: Mono) -> Self {
        match self {
            Intake::PausedUntil(until) if until > now => self,
            _ => Intake::Open,
        }
    }

    /// How long the thread may sleep: `ceiling`, or less when the pause ends sooner.
    pub(super) fn sleep(self, now: Mono, ceiling: Duration) -> Duration {
        match self.at(now) {
            Intake::PausedUntil(until) => until.since(now).min(ceiling),
            Intake::Open => ceiling,
        }
    }
}

/// Whether a complaint is written to the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Voice {
    Speak,
    Silent,
}

/// The last time each uid was complained about, so one client cannot fill the log.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct Complaints {
    last: BTreeMap<u32, Mono>,
}

impl Complaints {
    /// Notes a complaint about `uid` at `now`: spoken the first time and then once per
    /// `COMPLAINT_INTERVAL`.
    pub(super) fn note(mut self, uid: u32, now: Mono) -> (Self, Voice) {
        self.last
            .retain(|_, at| now.since(*at) < COMPLAINT_INTERVAL);
        let voice = match self.last.entry(uid) {
            Entry::Occupied(_) => Voice::Silent,
            Entry::Vacant(slot) => {
                slot.insert(now);
                Voice::Speak
            }
        };
        (self, voice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMALL: Caps = Caps {
        total: 4,
        per_uid: 2,
    };

    #[test]
    fn admission_table() {
        let cases: &[(&str, &[u32], u32, Admission)] = &[
            ("an empty daemon", &[], 1000, Admission::Admit),
            ("one of mine", &[1000], 1000, Admission::Admit),
            ("my second is my last", &[1000, 7], 1000, Admission::Admit),
            ("my third", &[1000, 1000], 1000, Admission::TooManyForUser),
            (
                "someone else's third does not count against me",
                &[1000, 1000, 7],
                7,
                Admission::Admit,
            ),
            ("full", &[1, 2, 3, 4], 5, Admission::TooManyClients),
            (
                "full of my own: the daemon's limit is named",
                &[1000, 1000, 7, 8],
                1000,
                Admission::TooManyClients,
            ),
            ("one short of full", &[1, 2, 3], 4, Admission::Admit),
        ];
        for &(name, connected, uid, want) in cases {
            let got = admit(connected.iter().copied(), uid, None, SMALL);
            assert_eq!(got, want, "{name}");
        }
        // The shipped service caps: eight clients of one user fill that user's share.
        let eight = std::iter::repeat_n(1000, 8);
        assert_eq!(
            admit(eight, 1000, None, Caps::SERVICE),
            Admission::TooManyForUser,
            "service caps"
        );
    }

    #[test]
    fn others_filling_the_daemon_never_lock_the_seat_owner_out() {
        let owner = Some(1000);
        let cases: &[(&str, &[u32], u32, Admission)] = &[
            (
                "full of strangers, the owner still gets in",
                &[1, 2, 3, 4],
                1000,
                Admission::Admit,
            ),
            (
                "a stranger is still refused",
                &[1, 2, 3, 4],
                5,
                Admission::TooManyClients,
            ),
            (
                "the owner's own clients leave room for others",
                &[1000, 1000, 1, 2],
                3,
                Admission::Admit,
            ),
            (
                "the owner keeps only its per-uid share",
                &[1000, 1000],
                1000,
                Admission::TooManyForUser,
            ),
        ];
        for &(name, connected, uid, want) in cases {
            let got = admit(connected.iter().copied(), uid, owner, SMALL);
            assert_eq!(got, want, "{name}");
        }
    }

    #[test]
    fn accept_failure_table() {
        let cases: &[(&str, Errno, AcceptFailure)] = &[
            (
                "nothing waiting",
                Errno::AGAIN,
                AcceptFailure::NothingPending,
            ),
            ("a signal", Errno::INTR, AcceptFailure::Retry),
            ("out of descriptors", Errno::MFILE, AcceptFailure::Pause),
            (
                "out of descriptors system-wide",
                Errno::NFILE,
                AcceptFailure::Pause,
            ),
            ("out of buffers", Errno::NOBUFS, AcceptFailure::Pause),
            ("out of memory", Errno::NOMEM, AcceptFailure::Pause),
            ("anything else", Errno::IO, AcceptFailure::Pause),
        ];
        for &(name, errno, want) in cases {
            assert_eq!(accept_failure(errno), want, "{name}");
        }
    }

    #[test]
    fn a_paused_listener_wakes_after_a_second_and_never_sleeps_past_it() {
        let t0 = Mono::from_millis(10_000);
        let paused = Intake::paused_at(t0);
        assert_eq!(paused, Intake::PausedUntil(Mono::from_millis(11_000)));
        let ms = Duration::from_millis;
        let cases: &[(&str, Mono, Intake, Duration)] = &[
            (
                "just paused",
                t0,
                Intake::PausedUntil(Mono::from_millis(11_000)),
                ms(500),
            ),
            (
                "near the end: sleep only until it",
                Mono::from_millis(10_800),
                Intake::PausedUntil(Mono::from_millis(11_000)),
                ms(200),
            ),
            (
                "exactly at the end",
                Mono::from_millis(11_000),
                Intake::Open,
                ms(500),
            ),
            (
                "long past it",
                Mono::from_millis(20_000),
                Intake::Open,
                ms(500),
            ),
        ];
        for &(name, now, want, sleep) in cases {
            assert_eq!(paused.at(now), want, "{name}");
            assert_eq!(paused.sleep(now, ms(500)), sleep, "{name}: sleep");
        }
        assert_eq!(Intake::Open.sleep(t0, ms(500)), ms(500));
    }

    #[test]
    fn a_uid_is_complained_about_once_a_minute() {
        let at = Mono::from_millis;
        let steps: &[(&str, u32, Mono, Voice)] = &[
            ("first complaint", 1000, at(0), Voice::Speak),
            ("the same uid at once", 1000, at(1), Voice::Silent),
            ("another uid is its own count", 1001, at(2), Voice::Speak),
            (
                "the same uid just before the minute",
                1000,
                at(59_999),
                Voice::Silent,
            ),
            ("the same uid at the minute", 1000, at(60_000), Voice::Speak),
            ("and silent again after", 1000, at(60_001), Voice::Silent),
        ];
        let mut complaints = Complaints::default();
        for &(name, uid, now, want) in steps {
            let (next, voice) = complaints.note(uid, now);
            assert_eq!(voice, want, "{name}");
            complaints = next;
        }
    }

    #[test]
    fn old_complaints_are_forgotten() {
        let mut complaints = Complaints::default();
        for uid in 0..100 {
            complaints = complaints.note(uid, Mono::from_millis(0)).0;
        }
        assert_eq!(complaints.last.len(), 100);
        let (complaints, _) = complaints.note(1000, Mono::from_millis(61_000));
        assert_eq!(complaints.last.len(), 1, "only the fresh one is kept");
    }
}
