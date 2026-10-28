//! What the socket thread allows a client, and when it stops listening or stops complaining.
//! The socket is world-connectable, so every bound here is against a client that is hostile or
//! merely broken. Pure: instants are arguments.

use crate::Mono;
use latchkey::guard::{ACCEPT_PAUSE, COMPLAINT_INTERVAL, Voice};
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::time::Duration;

/// Clients connected at once, over everyone.
pub(super) const MAX_CLIENTS: usize = 32;
/// Clients connected at once for one uid, not counting those that only use the keymap.
pub(super) const MAX_CLIENTS_PER_UID: usize = 8;
/// Clients that only watch the keymap or register actions, per uid. Every app of one person
/// holds one or two, so this is a share for a whole desktop's apps. They are observers for the
/// seat: they never steer keycap and never count toward `MAX_CLIENTS_PER_UID`.
pub(super) const MAX_KEYMAP_CLIENTS_PER_UID: usize = 64;
/// Messages read from one client in one pass over the sockets; the rest wait for the next.
pub(super) const MESSAGES_PER_PASS: usize = 16;
/// Requests to the reader (focus, settings, resume) one client may make in a second. A desktop
/// sends a focus change per window switch, a handful a second at the most.
pub(super) const STEERING_PER_SECOND: u32 = 100;
/// Connections accepted in one pass.
pub(super) const ACCEPTS_PER_PASS: usize = 16;

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

/// Why a new connection is not let in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Refused {
    /// The daemon is full.
    TooManyClients,
    /// This uid already has its share.
    TooManyForUser,
}

impl Refused {
    /// The line logged against the client.
    pub(super) const fn line(self) -> &'static str {
        match self {
            Refused::TooManyClients => "refused: the daemon is full",
            Refused::TooManyForUser => "refused: too many connections for this user",
        }
    }
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
) -> Result<(), Refused> {
    let (others, mine) = connected
        .into_iter()
        .fold((0usize, 0usize), |(others, mine), other| {
            (
                others + usize::from(Some(other) != owner),
                mine + usize::from(other == uid),
            )
        });
    if Some(uid) != owner && others >= caps.total {
        Err(Refused::TooManyClients)
    } else if mine >= caps.per_uid {
        Err(Refused::TooManyForUser)
    } else {
        Ok(())
    }
}

/// The requests a client made in the second since its window opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Pace {
    opened: Mono,
    count: u32,
}

impl Pace {
    pub(super) fn starting(now: Mono) -> Self {
        Self {
            opened: now,
            count: 0,
        }
    }

    /// Counts one request at `now`; true when the client is over its rate. A second after the
    /// window opened, a new one opens.
    pub(super) fn note(self, now: Mono) -> (Self, bool) {
        let window = if now.since(self.opened) >= Duration::from_secs(1) {
            Self::starting(now)
        } else {
            self
        };
        let count = window.count.saturating_add(1);
        (Self { count, ..window }, count > STEERING_PER_SECOND)
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
    fn a_client_may_make_a_hundred_requests_a_second_and_gets_a_new_second() {
        let t0 = Mono::default();
        let (last, pacing) = (0..STEERING_PER_SECOND)
            .fold((Pace::starting(t0), false), |(pace, _), _| {
                pace.note(t0.after(Duration::from_millis(10)))
            });
        assert!(!pacing, "the hundredth is still inside");
        let (over, pacing) = last.note(t0.after(Duration::from_millis(20)));
        assert!(pacing, "the hundred and first is not");
        let (_, pacing) = over.note(t0.after(Duration::from_secs(1)));
        assert!(!pacing, "a new second opens a new window");
    }

    /// A case's name, the uids already in, the asking uid, and the answer.
    type AdmitCase = (&'static str, &'static [u32], u32, Result<(), Refused>);

    #[test]
    fn admission_table() {
        let cases: &[AdmitCase] = &[
            ("an empty daemon", &[], 1000, Ok(())),
            ("one of mine", &[1000], 1000, Ok(())),
            ("my second is my last", &[1000, 7], 1000, Ok(())),
            (
                "my third",
                &[1000, 1000],
                1000,
                Err(Refused::TooManyForUser),
            ),
            (
                "someone else's third does not count against me",
                &[1000, 1000, 7],
                7,
                Ok(()),
            ),
            ("full", &[1, 2, 3, 4], 5, Err(Refused::TooManyClients)),
            (
                "full of my own: the daemon's limit is named",
                &[1000, 1000, 7, 8],
                1000,
                Err(Refused::TooManyClients),
            ),
            ("one short of full", &[1, 2, 3], 4, Ok(())),
        ];
        for &(name, connected, uid, want) in cases {
            let got = admit(connected.iter().copied(), uid, None, SMALL);
            assert_eq!(got, want, "{name}");
        }
        // The shipped service caps: eight clients of one user fill that user's share.
        let eight = std::iter::repeat_n(1000, 8);
        assert_eq!(
            admit(eight, 1000, None, Caps::SERVICE),
            Err(Refused::TooManyForUser),
            "service caps"
        );
    }

    #[test]
    fn others_filling_the_daemon_never_lock_the_seat_owner_out() {
        let owner = Some(1000);
        let cases: &[AdmitCase] = &[
            (
                "full of strangers, the owner still gets in",
                &[1, 2, 3, 4],
                1000,
                Ok(()),
            ),
            (
                "a stranger is still refused",
                &[1, 2, 3, 4],
                5,
                Err(Refused::TooManyClients),
            ),
            (
                "the owner's own clients leave room for others",
                &[1000, 1000, 1, 2],
                3,
                Ok(()),
            ),
            (
                "the owner keeps only its per-uid share",
                &[1000, 1000],
                1000,
                Err(Refused::TooManyForUser),
            ),
        ];
        for &(name, connected, uid, want) in cases {
            let got = admit(connected.iter().copied(), uid, owner, SMALL);
            assert_eq!(got, want, "{name}");
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
