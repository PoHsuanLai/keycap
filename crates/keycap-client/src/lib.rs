//! A program's end of keycap.
//!
//! - [`Connection`]: the socket, one message per packet, `Hello` checked.
//! - [`KeycapSource`]: this desktop's `chordkit::KeymapSource`. It lays chordkit's conventions
//!   for our desktop, the desktop's own bindings ([`system_bindings`]) and the person's keymap
//!   over each other, and tells the program when the person's keymap changes. Without a
//!   daemon it reads the keymap file itself, so a program still honours the person's keymap.
//! - [`Registered`]: an app's actions told to keycap so Settings can list them. Informational:
//!   resolution stays in each app, through chordkit.
//! - [`handler`], [`handled_actions`]: which standard actions our desktop handles and who does
//!   each (sill, or the compositor itself). detent lists only these; casement's test checks its
//!   own rows against this table.
//!
//! No keymap noun is defined here; every action, chord and keymap is chordkit's.

mod bounded;
mod connection;
mod fallback;
mod fetch;
mod handled;
mod register;
mod source;
mod system;
#[cfg(test)]
mod testing;
mod watch;

pub use bounded::{FileText, read_text};
pub use connection::{ClientError, Connection, Heard};
pub use handled::{Handler, handled_actions, handler};
pub use keycap_protocol::{Registration, SOCKET_PATH};
pub use register::Registered;
pub use source::{KeycapSource, Layer, Origin};
pub use system::system_bindings;
