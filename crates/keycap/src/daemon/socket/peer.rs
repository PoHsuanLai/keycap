//! One connection of the socket thread: who it is, what it has shown itself to be, how fast it
//! may steer, and sending to it.

use super::limits::Pace;
use crate::daemon::inbox::ClientId;
use keycap_protocol::{self as protocol, ToClient};
use rustix::fd::{AsFd, OwnedFd};
use rustix::io::Errno;
use rustix::net::SendFlags;

/// What a connection has shown itself to be. The seat owner's connections steer keycap; a
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

pub(super) struct Client {
    pub(super) id: ClientId,
    pub(super) fd: OwnedFd,
    /// The uid the kernel recorded when the client connected.
    pub(super) uid: u32,
    pub(super) role: Role,
    /// How many requests to the reader it has made lately.
    pub(super) pace: Pace,
}

/// Whether a client is still connected after something was done to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Link {
    Alive,
    Gone,
}

/// Sends one message; `Gone` if the client is. A full queue drops the message only.
pub(super) fn send(client: &Client, message: &ToClient) -> Link {
    match rustix::net::send(
        client.fd.as_fd(),
        &protocol::encode(message),
        SendFlags::DONTWAIT | SendFlags::NOSIGNAL,
    ) {
        Ok(_) | Err(Errno::AGAIN) => Link::Alive,
        Err(_) => Link::Gone,
    }
}
