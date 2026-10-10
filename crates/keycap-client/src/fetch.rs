//! Asking the daemon for the person's keymap, and listening for it to change.

use crate::connection::{ClientError, Connection, Heard};
use keycap_protocol::{KeymapState, ToClient, ToDaemon};
use std::path::Path;
use std::time::{Duration, Instant};

/// How long the daemon has to answer. It answers from memory, so this is generous.
pub(crate) const WAIT: Duration = Duration::from_secs(2);

/// A connection that has asked to hear the keymap.
pub(crate) fn subscribe(socket: &Path) -> Result<Connection, ClientError> {
    let connection = Connection::open(socket, WAIT)?.require_keymap()?;
    connection.send(&ToDaemon::WatchKeymap)?;
    Ok(connection)
}

/// The keymap as the daemon holds it now.
pub(crate) fn keymap(socket: &Path) -> Result<KeymapState, ClientError> {
    let connection = subscribe(socket)?;
    let deadline = Instant::now() + WAIT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match connection.recv(left)? {
            Heard::Message(ToClient::Keymap(state)) => return Ok(state),
            Heard::Message(_) => {}
            Heard::Silence => return Err(ClientError::Silent),
            Heard::Closed => return Err(ClientError::Closed),
        }
        if left.is_zero() {
            return Err(ClientError::Silent);
        }
    }
}
