//! Asking the daemon for the person's keymap, and listening for it to change.

use crate::connection::{ClientError, Connection};
use keycap_protocol::{KeymapCall, KeymapState, ToClient, ToDaemon};
use std::path::Path;
use std::time::Duration;

/// How long the daemon has to answer. It answers from memory, so this is generous.
pub(crate) const WAIT: Duration = Duration::from_secs(2);

/// A connection that has asked to hear the keymap.
pub(crate) fn subscribe(socket: &Path) -> Result<Connection, ClientError> {
    let connection = Connection::open(socket, WAIT)?;
    connection.send(&ToDaemon::Keymap(KeymapCall::Watch))?;
    Ok(connection)
}

/// The keymap as the daemon holds it now.
pub(crate) fn keymap(socket: &Path) -> Result<KeymapState, ClientError> {
    let connection = subscribe(socket)?;
    connection.await_message(WAIT, |message| match message {
        ToClient::Keymap(state) => Some(state),
        _ => None,
    })
}
