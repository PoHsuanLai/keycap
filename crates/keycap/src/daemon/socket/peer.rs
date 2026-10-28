//! One connection of the socket thread: who it is, what it has shown itself to be, how fast it
//! may steer, what it missed, and sending to it.

use super::limits::Pace;
use super::seat::{Peer, peer_of};
use crate::daemon::inbox::ClientId;
use keycap_protocol::{self as protocol, KeymapState, MAX_PACKET, Status, ToClient};
use latchkey::guard::Origin;
use rustix::fd::{AsFd, OwnedFd};
use rustix::io::Errno;
use rustix::net::SendFlags;
use std::collections::BTreeSet;

/// What a connection has shown itself to be. The shell's connections steer keycap; a
/// connection that only reads the keymap or registers actions is an observer for the seat even
/// when it runs as the seat owner's uid, so its coming and going never resets the reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Role {
    /// Nothing sent yet; counted with the owners, as every connection was before.
    Undecided,
    /// Has sent a request to the reader.
    Steering,
    /// Has used only the keymap messages.
    KeymapOnly,
}

/// A state change a client did not hear because its queue was full. It is sent again, in its
/// latest form, once there is room.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Owed {
    Status,
    Keymap,
}

pub(super) struct Client {
    pub(super) id: ClientId,
    pub(super) fd: OwnedFd,
    /// The uid the kernel recorded when the client connected.
    pub(super) uid: u32,
    /// What the client showed itself to be when it connected.
    pub(super) origin: Origin,
    pub(super) role: Role,
    /// How many requests to the reader it has made lately.
    pub(super) pace: Pace,
    /// What it missed.
    pub(super) owed: BTreeSet<Owed>,
}

/// What sending one message came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Sent {
    Delivered,
    /// The client's queue is full: the message was dropped and the client is still connected.
    Behind,
    /// The message does not fit one packet, so no client could read it: it was not sent, and
    /// the client is still connected.
    TooLarge,
    Gone,
}

impl Sent {
    /// Whether the client is still connected.
    pub(super) fn alive(self) -> bool {
        self != Sent::Gone
    }
}

impl Client {
    pub(super) fn new(id: ClientId, fd: OwnedFd, uid: u32, origin: Origin, pace: Pace) -> Self {
        Self {
            id,
            fd,
            uid,
            origin,
            role: Role::Undecided,
            pace,
            owed: BTreeSet::new(),
        }
    }

    /// What this client may do while `active` has the seat.
    pub(super) fn peer(&self, active: Option<u32>) -> Peer {
        peer_of(self.uid, active, self.origin.claim())
    }

    /// Sends the latest of everything this client missed, keeping what still does not go.
    pub(super) fn settle(&mut self, status: &Status, keymap: &KeymapState) -> bool {
        for owed in self.owed.clone() {
            let message = match owed {
                Owed::Status => ToClient::Status(status.clone()),
                Owed::Keymap => ToClient::Keymap(keymap.clone()),
            };
            match send(self, &message) {
                Sent::Gone => return false,
                Sent::Delivered | Sent::TooLarge => {
                    self.owed.remove(&owed);
                }
                Sent::Behind => {}
            }
        }
        true
    }
}

/// Sends one message. A full queue drops the message only (`Behind`); a message too large for
/// one packet is not sent; nothing but a client that is really gone ends its connection.
pub(super) fn send(client: &Client, message: &ToClient) -> Sent {
    let bytes = protocol::encode(message);
    if bytes.len() > MAX_PACKET {
        return Sent::TooLarge;
    }
    match rustix::net::send(
        client.fd.as_fd(),
        &bytes,
        SendFlags::DONTWAIT | SendFlags::NOSIGNAL,
    ) {
        Ok(_) => Sent::Delivered,
        Err(Errno::AGAIN) => Sent::Behind,
        Err(Errno::MSGSIZE) => Sent::TooLarge,
        Err(_) => Sent::Gone,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keycap_protocol::Mode;
    use rustix::net::{AddressFamily, RecvFlags, SocketFlags, SocketType, socketpair};

    fn pair() -> (Client, OwnedFd) {
        let (ours, theirs) = socketpair(
            AddressFamily::UNIX,
            SocketType::SEQPACKET,
            SocketFlags::NONBLOCK | SocketFlags::CLOEXEC,
            None,
        )
        .unwrap();
        let now = crate::Mono::default();
        (
            Client::new(ClientId(0), ours, 1000, Origin::Other, Pace::starting(now)),
            theirs,
        )
    }

    fn status(mode: Mode) -> Status {
        Status {
            mode,
            keyboards: Vec::new(),
        }
    }

    /// Reads and drops everything queued for `fd`; how many messages there were.
    fn read_all(fd: &OwnedFd) -> usize {
        let mut buf = vec![0u8; MAX_PACKET];
        std::iter::from_fn(|| rustix::net::recv(fd, &mut buf, RecvFlags::DONTWAIT).ok()).count()
    }

    #[test]
    fn a_missed_status_is_sent_again_in_its_latest_form_once_there_is_room() {
        let (mut client, theirs) = pair();
        let running = ToClient::Status(status(Mode::Running));
        let queued = std::iter::from_fn(|| match send(&client, &running) {
            Sent::Delivered => Some(()),
            _ => None,
        })
        .count();
        assert!(queued > 0, "the queue took some");
        let paused = status(Mode::Paused);
        assert_eq!(
            send(&client, &ToClient::Status(paused.clone())),
            Sent::Behind
        );
        client.owed.insert(Owed::Status);

        assert!(client.settle(&paused, &KeymapState::default()));
        assert!(client.owed.contains(&Owed::Status), "still no room");
        assert_eq!(read_all(&theirs), queued);

        assert!(client.settle(&paused, &KeymapState::default()));
        assert!(client.owed.is_empty(), "delivered");
        let mut buf = vec![0u8; MAX_PACKET];
        let (n, _) = rustix::net::recv(&theirs, &mut buf, RecvFlags::DONTWAIT).unwrap();
        assert_eq!(
            protocol::decode::<ToClient>(&buf[..n]).unwrap(),
            ToClient::Status(paused)
        );
    }

    #[test]
    fn a_message_too_large_for_one_packet_is_not_sent_and_does_not_cost_the_connection() {
        let (client, theirs) = pair();
        let huge = ToClient::Keymap(KeymapState {
            overrides: "x".repeat(MAX_PACKET),
            problems: Vec::new(),
        });
        assert_eq!(send(&client, &huge), Sent::TooLarge);
        assert!(Sent::TooLarge.alive());
        assert_eq!(read_all(&theirs), 0);
    }

    #[test]
    fn a_closed_peer_is_gone() {
        let (client, theirs) = pair();
        drop(theirs);
        assert_eq!(
            send(&client, &ToClient::Status(status(Mode::Running))),
            Sent::Gone
        );
    }
}
