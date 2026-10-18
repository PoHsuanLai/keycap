//! Telling keycap what an app handles, so Settings can list it.

use crate::connection::{ClientError, Connection};
use crate::fetch::WAIT;
use crate::source::KeycapSource;
use chordkit::{AppAction, AppId, DefaultChord};
use keycap_protocol::{KeymapCall, RegisteredAction, Registration, ToClient, ToDaemon};

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
        let link = Connection::open(self.socket(), WAIT)?;
        link.send(&ToDaemon::Keymap(KeymapCall::Register(Registration {
            app: app.as_str().to_owned(),
            actions: actions
                .iter()
                .map(|(action, chord)| RegisteredAction {
                    id: action.id().to_owned(),
                    chord: chord.canonical(),
                })
                .collect(),
        })))?;
        Ok(Registered { _link: link })
    }

    /// Every app's actions registered with keycap right now, for a settings screen.
    pub fn registered(&self) -> Result<Vec<Registration>, ClientError> {
        let link = Connection::open(self.socket(), WAIT)?;
        link.send(&ToDaemon::Keymap(KeymapCall::List))?;
        link.await_message(WAIT, |message| match message {
            ToClient::Registered(all) => Some(all),
            _ => None,
        })
    }
}
