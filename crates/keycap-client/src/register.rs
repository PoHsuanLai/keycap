//! Telling keycap what an app handles, so Settings can list it.

use crate::connection::{ClientError, Connection, Heard};
use crate::fetch::WAIT;
use crate::source::KeycapSource;
use chordkit::{AppAction, AppId, DefaultChord};
use keycap_protocol::{RegisteredAction, Registration, ToClient, ToDaemon};
use std::time::Instant;

/// An app's registration. It lasts as long as this value: keycap forgets the actions when the
/// connection closes, so an app that exits leaves nothing behind.
#[derive(Debug)]
pub struct Registered {
    _link: Connection,
}

impl KeycapSource {
    /// Tells keycap `app`'s actions and their default chords. Informational: each app resolves
    /// its own keys in-process through chordkit, and this changes no binding. Keep the result
    /// alive while the app runs.
    pub fn register(
        &self,
        app: &AppId,
        actions: &[(AppAction, DefaultChord)],
    ) -> Result<Registered, ClientError> {
        let link = Connection::open(self.socket(), WAIT)?.require_keymap()?;
        link.send(&ToDaemon::Register(Registration {
            app: app.as_str().to_owned(),
            actions: actions
                .iter()
                .map(|(action, chord)| RegisteredAction {
                    id: action.id().to_owned(),
                    chord: chord.canonical(),
                })
                .collect(),
        }))?;
        Ok(Registered { _link: link })
    }

    /// Every app's actions registered with keycap right now, for a settings screen.
    pub fn registered(&self) -> Result<Vec<Registration>, ClientError> {
        let link = Connection::open(self.socket(), WAIT)?.require_keymap()?;
        link.send(&ToDaemon::ListRegistered)?;
        let deadline = Instant::now() + WAIT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match link.recv(left)? {
                Heard::Message(ToClient::Registered(all)) => return Ok(all),
                Heard::Message(_) => {}
                Heard::Silence => return Err(ClientError::Silent),
                Heard::Closed => return Err(ClientError::Closed),
            }
        }
    }
}
