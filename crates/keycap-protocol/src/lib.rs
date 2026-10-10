//! The socket contract between keycap and the desktop that drives it.
//!
//! `/run/keycap/keycap.sock` (`SOCKET_PATH`), `SOCK_SEQPACKET`: one JSON message per packet, so
//! there is no framing to get wrong and a reader never sees half a message. The first packet
//! on every connection is `Hello`, whose `version` a client checks before reading anything
//! else; an incompatible change bumps `PROTOCOL_VERSION`. An addition that old clients never
//! ask for (version 2: the keymap messages) bumps it too, but `MIN_PROTOCOL_VERSION` stays, so a
//! client of either version keeps working against a daemon of either version.

mod keymap;
mod message;
mod vocab;
mod wire;

#[cfg(test)]
mod tests;

pub use keymap::{KeymapProblem, KeymapState, RegisteredAction, Registration};
pub use message::{ToClient, ToDaemon};
pub use vocab::{Grab, Keyboard, KeyboardInfo, Mode, NameError, ProfileName, Status, Warning};
pub use wire::{
    CONFIG_DIR, Compat, KEYMAP_FILE, KEYMAP_MAX_BYTES, KEYMAP_PROTOCOL_VERSION, MAX_PACKET,
    MIN_PROTOCOL_VERSION, PROTOCOL_VERSION, ProtocolError, SOCKET_PATH, compat, decode, encode,
};
