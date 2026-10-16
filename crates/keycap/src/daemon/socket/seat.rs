//! Who owns the seat, and what that makes a client. The decisions are pure; reading the seat
//! file is the one effect.

use keycap_protocol::ToDaemon;
use std::path::PathBuf;

use super::shell::Claim;
use crate::daemon::inbox::{ClientId, Request};

/// Where logind records the seat's state.
pub(crate) const SEAT_FILE: &str = "/run/systemd/seats/seat0";

/// What a connected client may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Peer {
    /// The shell of the user at the seat: `Focus`, `UserConfig` and `Resume` are honoured.
    Steering,
    /// Another program of the user at the seat: hears status, may register actions and may
    /// `Resume`, but is not obeyed for `Focus` or `UserConfig`.
    Plain,
    /// Anyone else: hears status, is never obeyed.
    Observer,
}

/// The `ACTIVE_UID=` line of a `seat0` state file; `None` when there is no active session.
pub(crate) fn active_uid(text: &str) -> Option<u32> {
    text.lines()
        .find_map(|line| line.strip_prefix("ACTIVE_UID="))
        .and_then(|value| value.trim().parse().ok())
}

/// A client's standing, from the uid the kernel vouches for, the seat's active uid and what
/// the client has shown itself to be. The shell counts only for the user whose manager runs it.
pub(super) fn peer_of(uid: u32, active_uid: Option<u32>, claim: Claim) -> Peer {
    match (active_uid, claim) {
        (Some(active), Claim::Shell { user }) if active == uid && user == uid => Peer::Steering,
        (Some(active), _) if active == uid => Peer::Plain,
        _ => Peer::Observer,
    }
}

/// Why a client's message was not acted on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// Only the shell steers: any other client only listens.
    NotSteering,
    /// A keymap message, which the shelf answers and which is never a request to the reader.
    /// The server takes those before it asks for a request, so this is not reached.
    NotARequest,
}

impl Refusal {
    /// The line for the log.
    pub(crate) fn what(self) -> &'static str {
        match self {
            Refusal::NotSteering => "sent a request without being the shell of the seat; dropped",
            Refusal::NotARequest => "sent a keymap message where a request was expected; dropped",
        }
    }
}

/// What `peer` asking for `message` becomes: `Focus` and `UserConfig` only from the shell;
/// `Resume` also from another program of the seat's user, since it only re-grabs.
pub(crate) fn request_of(
    peer: Peer,
    client: ClientId,
    message: ToDaemon,
) -> Result<Request, Refusal> {
    match (peer, message) {
        (Peer::Steering | Peer::Plain, ToDaemon::Resume) => Ok(Request::Resume),
        (Peer::Plain | Peer::Observer, _) => Err(Refusal::NotSteering),
        (Peer::Steering, ToDaemon::Focus { profile }) => Ok(Request::Focus(profile)),
        (Peer::Steering, ToDaemon::UserConfig { toml }) => Ok(Request::UserConfig { client, toml }),
        (
            Peer::Steering,
            ToDaemon::WatchKeymap | ToDaemon::Register(_) | ToDaemon::ListRegistered,
        ) => Err(Refusal::NotARequest),
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
        const SHELL_1000: Claim = Claim::Shell { user: 1000 };
        const CASES: &[(&str, u32, Option<u32>, Claim, Peer)] = &[
            (
                "the user's shell",
                1000,
                Some(1000),
                SHELL_1000,
                Peer::Steering,
            ),
            (
                "another program of the user",
                1000,
                Some(1000),
                Claim::Other,
                Peer::Plain,
            ),
            (
                "a shell under another user's manager",
                1000,
                Some(1000),
                Claim::Shell { user: 1001 },
                Peer::Plain,
            ),
            (
                "another user's shell",
                1001,
                Some(1000),
                Claim::Shell { user: 1001 },
                Peer::Observer,
            ),
            (
                "the shell while another user is active",
                1000,
                Some(1001),
                SHELL_1000,
                Peer::Observer,
            ),
            ("no active user", 1000, None, SHELL_1000, Peer::Observer),
            (
                "root is not special",
                0,
                Some(1000),
                Claim::Other,
                Peer::Observer,
            ),
            (
                "root can own the seat",
                0,
                Some(0),
                Claim::Shell { user: 0 },
                Peer::Steering,
            ),
        ];
        for &(name, uid, active, claim, want) in CASES {
            assert_eq!(peer_of(uid, active, claim), want, "{name}");
        }
    }

    #[test]
    fn only_the_shell_is_obeyed() {
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
                "shell focus",
                Peer::Steering,
                focus(),
                Ok(Request::Focus(profile.clone())),
            ),
            (
                "shell config",
                Peer::Steering,
                config(),
                Ok(Request::UserConfig {
                    client,
                    toml: "x = 1".to_owned(),
                }),
            ),
            (
                "shell resume",
                Peer::Steering,
                ToDaemon::Resume,
                Ok(Request::Resume),
            ),
            (
                "observer focus",
                Peer::Observer,
                focus(),
                Err(Refusal::NotSteering),
            ),
            (
                "observer config",
                Peer::Observer,
                config(),
                Err(Refusal::NotSteering),
            ),
            (
                "observer resume",
                Peer::Observer,
                ToDaemon::Resume,
                Err(Refusal::NotSteering),
            ),
            (
                "plain focus",
                Peer::Plain,
                focus(),
                Err(Refusal::NotSteering),
            ),
            (
                "plain config",
                Peer::Plain,
                config(),
                Err(Refusal::NotSteering),
            ),
            (
                "plain resume",
                Peer::Plain,
                ToDaemon::Resume,
                Ok(Request::Resume),
            ),
            (
                "shell keymap message",
                Peer::Steering,
                ToDaemon::WatchKeymap,
                Err(Refusal::NotARequest),
            ),
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
