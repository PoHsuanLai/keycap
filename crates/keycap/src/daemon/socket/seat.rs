//! Who owns the seat, and what that makes a client. The decisions are pure; reading the seat
//! file is the one effect.

use keycap_protocol::ToDaemon;
use std::path::PathBuf;

use crate::daemon::inbox::{ClientId, Request};

/// Where logind records the seat's state.
pub(crate) const SEAT_FILE: &str = "/run/systemd/seats/seat0";

/// What a connected client may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Peer {
    /// Runs as the user at the seat: `Focus`, `UserConfig` and `Resume` are honoured.
    SeatOwner,
    /// Anyone else: hears status, is never obeyed.
    Observer,
}

/// The `ACTIVE_UID=` line of a `seat0` state file; `None` when there is no active session.
pub(crate) fn active_uid(text: &str) -> Option<u32> {
    text.lines()
        .find_map(|line| line.strip_prefix("ACTIVE_UID="))
        .and_then(|value| value.trim().parse().ok())
}

/// A client's standing, from the uid the kernel vouches for and the seat's active uid.
pub(crate) fn peer_of(uid: u32, active_uid: Option<u32>) -> Peer {
    match active_uid {
        Some(active) if active == uid => Peer::SeatOwner,
        _ => Peer::Observer,
    }
}

/// What `peer` asking for `message` becomes: only the seat owner's messages are acted on.
pub(crate) fn request_of(peer: Peer, client: ClientId, message: ToDaemon) -> Option<Request> {
    match (peer, message) {
        (Peer::Observer, _) => None,
        (Peer::SeatOwner, ToDaemon::Focus { profile }) => Some(Request::Focus(profile)),
        (Peer::SeatOwner, ToDaemon::UserConfig { toml }) => {
            Some(Request::UserConfig { client, toml })
        }
        (Peer::SeatOwner, ToDaemon::Resume) => Some(Request::Resume),
    }
}

/// Whether a seat owner has a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Presence {
    Absent,
    Present,
}

/// What a look at the seat concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tenancy {
    Unchanged,
    /// The owner left or another user took the seat: their focus and settings must go.
    Vacated,
}

/// What the socket thread last saw of the seat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Seat {
    active: Option<u32>,
    owner: Presence,
}

impl Seat {
    /// A seat seen for the first time: nothing to vacate yet.
    pub(crate) fn first_sight(active: Option<u32>) -> Self {
        Self {
            active,
            owner: Presence::Absent,
        }
    }

    /// The seat now: `active` is its owner's uid, `owner_connections` how many clients are
    /// that owner. Vacated when the active uid changed or the owner's last connection closed.
    pub(crate) fn look(self, active: Option<u32>, owner_connections: usize) -> (Seat, Tenancy) {
        let owner = match owner_connections {
            0 => Presence::Absent,
            _ => Presence::Present,
        };
        let vacated =
            self.active != active || (self.owner, owner) == (Presence::Present, Presence::Absent);
        let tenancy = if vacated {
            Tenancy::Vacated
        } else {
            Tenancy::Unchanged
        };
        (Seat { active, owner }, tenancy)
    }
}

/// The file the active uid is read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SeatFile(pub PathBuf);

impl SeatFile {
    /// The active uid now; a missing or unreadable file means nobody owns the seat.
    pub(crate) fn active_uid(&self) -> Option<u32> {
        std::fs::read_to_string(&self.0)
            .ok()
            .and_then(|text| active_uid(&text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keycap_protocol::ProfileName;

    #[test]
    fn active_uid_table() {
        const CASES: &[(&str, &str, Option<u32>)] = &[
            (
                "typical file",
                "# This is private data.\nIS_SEAT0=1\nACTIVE=c2\nACTIVE_UID=1000\nSESSIONS=c2\nUIDS=1000\n",
                Some(1000),
            ),
            ("no active session", "IS_SEAT0=1\nSESSIONS=\nUIDS=\n", None),
            ("empty file", "", None),
            ("root", "ACTIVE_UID=0\n", Some(0)),
            ("not a number", "ACTIVE_UID=bob\n", None),
            ("negative", "ACTIVE_UID=-1\n", None),
            ("empty value", "ACTIVE_UID=\n", None),
            (
                "a different key that ends the same",
                "OLD_ACTIVE_UID=5\n",
                None,
            ),
            ("indented is not the key", " ACTIVE_UID=7\n", None),
            ("first wins", "ACTIVE_UID=1\nACTIVE_UID=2\n", Some(1)),
            ("trailing space", "ACTIVE_UID=42 \n", Some(42)),
            ("no final newline", "ACTIVE_UID=9", Some(9)),
        ];
        for &(name, text, want) in CASES {
            assert_eq!(active_uid(text), want, "{name}");
        }
    }

    #[test]
    fn peer_table() {
        const CASES: &[(&str, u32, Option<u32>, Peer)] = &[
            ("the active user", 1000, Some(1000), Peer::SeatOwner),
            ("another user", 1001, Some(1000), Peer::Observer),
            ("no active user", 1000, None, Peer::Observer),
            ("root is not special", 0, Some(1000), Peer::Observer),
            ("root can own the seat", 0, Some(0), Peer::SeatOwner),
        ];
        for &(name, uid, active, want) in CASES {
            assert_eq!(peer_of(uid, active), want, "{name}");
        }
    }

    #[test]
    fn only_the_seat_owner_is_obeyed() {
        let client = ClientId(3);
        let profile = ProfileName::parse("terminal").unwrap();
        let focus = || ToDaemon::Focus {
            profile: profile.clone(),
        };
        let config = || ToDaemon::UserConfig {
            toml: "x = 1".to_owned(),
        };
        let cases = [
            (
                "owner focus",
                Peer::SeatOwner,
                focus(),
                Some(Request::Focus(profile.clone())),
            ),
            (
                "owner config",
                Peer::SeatOwner,
                config(),
                Some(Request::UserConfig {
                    client,
                    toml: "x = 1".to_owned(),
                }),
            ),
            (
                "owner resume",
                Peer::SeatOwner,
                ToDaemon::Resume,
                Some(Request::Resume),
            ),
            ("observer focus", Peer::Observer, focus(), None),
            ("observer config", Peer::Observer, config(), None),
            ("observer resume", Peer::Observer, ToDaemon::Resume, None),
        ];
        for (name, peer, message, want) in cases {
            assert_eq!(request_of(peer, client, message), want, "{name}");
        }
    }

    #[test]
    fn the_seat_is_vacated_when_the_owner_leaves_or_the_user_changes() {
        use Tenancy::{Unchanged, Vacated};
        // (name, seat before, active now, owner connections now, tenancy)
        let cases = [
            (
                "first sight, nobody",
                Seat::first_sight(Some(1000)),
                Some(1000),
                0,
                Unchanged,
            ),
            (
                "owner connects",
                Seat::first_sight(Some(1000)),
                Some(1000),
                1,
                Unchanged,
            ),
            (
                "owner stays",
                Seat {
                    active: Some(1000),
                    owner: Presence::Present,
                },
                Some(1000),
                2,
                Unchanged,
            ),
            (
                "last owner connection closes",
                Seat {
                    active: Some(1000),
                    owner: Presence::Present,
                },
                Some(1000),
                0,
                Vacated,
            ),
            (
                "user switch",
                Seat {
                    active: Some(1000),
                    owner: Presence::Present,
                },
                Some(1001),
                0,
                Vacated,
            ),
            (
                "user switch to one already connected",
                Seat {
                    active: Some(1000),
                    owner: Presence::Present,
                },
                Some(1001),
                1,
                Vacated,
            ),
            (
                "session ends",
                Seat {
                    active: Some(1000),
                    owner: Presence::Absent,
                },
                None,
                0,
                Vacated,
            ),
            (
                "an observer's closing is nothing",
                Seat {
                    active: Some(1000),
                    owner: Presence::Absent,
                },
                Some(1000),
                0,
                Unchanged,
            ),
        ];
        for (name, seat, active, owners, want) in cases {
            let (next, tenancy) = seat.look(active, owners);
            assert_eq!(tenancy, want, "{name}");
            assert_eq!(next.active, active, "{name}: remembers the active uid");
        }
        let (after, _) = Seat::first_sight(Some(1)).look(Some(1), 1);
        assert_eq!(after.owner, Presence::Present);
    }
}
