//! A program's end of keycap.
//!
//! - [`Connection`]: the socket, one message per packet, `Hello` checked.
//! - [`KeycapSource`]: this desktop's `chordkit::KeymapSource`. It lays chordkit's conventions
//!   for our desktop, the desktop's own bindings ([`system_bindings`]) and the person's keymap
//!   over each other, and tells the program when the person's keymap changes. Without a
//!   daemon it reads the keymap file itself, so a program still honours the person's keymap.
//! - [`Registered`]: an app's actions told to keycap so Settings can list them. Informational:
//!   resolution stays in each app, through chordkit.
//!
//! No keymap noun is defined here; every action, chord and keymap is chordkit's.

mod connection;
mod fallback;
mod fetch;
mod register;
mod source;
mod system;
#[cfg(test)]
mod testing;
mod watch;

pub use connection::{ClientError, Connection, Heard};
pub use keycap_protocol::{Registration, SOCKET_PATH};
pub use register::Registered;
pub use source::{KeycapSource, Layer, Origin};
pub use system::system_bindings;
