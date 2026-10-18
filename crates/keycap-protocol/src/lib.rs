//! The socket contract between keycap and the desktop that drives it.
//!
//! `/run/keycap/keycap.sock` (`SOCKET_PATH`), `SOCK_SEQPACKET`: one JSON message per packet, so
//! there is no framing to get wrong and a reader never sees half a message. The first packet
//! on every connection is `Hello`, whose `version` a client checks before reading anything
//! else; an incompatible change bumps `PROTOCOL_VERSION` and `MIN_PROTOCOL_VERSION` with it,
//! because nothing is released and nothing older is read.

mod keymap;
mod message;
mod vocab;
mod wire;

#[cfg(test)]
mod tests;

pub use keymap::{KeymapProblem, KeymapState, RegisteredAction, Registration};
pub use message::{KeymapCall, Steer, ToClient, ToDaemon};
pub use vocab::{Grab, Keyboard, KeyboardInfo, Mode, NameError, ProfileName, Status, Warning};
pub use wire::{
    CONFIG_DIR, Compat, KEYMAP_FILE, KEYMAP_MAX_BYTES, MAX_PACKET, MIN_PROTOCOL_VERSION,
    PROTOCOL_VERSION, ProtocolError, SOCKET_PATH, compat, decode, encode,
};
