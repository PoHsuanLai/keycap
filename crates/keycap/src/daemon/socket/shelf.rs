//! What the socket thread keeps for the keymap: the person's keymap as last settled, who asked
//! to hear it change, and the actions each connection registered. Pure: replies are returned,
//! the server sends them.

use crate::daemon::inbox::ClientId;
use crate::keymap::{Reading, settle};
use chordkit::{AppAction, AppId, DefaultChord};
use keycap_protocol::{KeymapState, RegisteredAction, Registration, ToClient, ToDaemon};
use std::collections::{BTreeMap, BTreeSet};

/// Apps one connection may register at once; the rest are ignored.
pub(super) const MAX_APPS_PER_CLIENT: usize = 16;

/// A keymap message a client sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Call {
    Watch,
    Register(Registration),
    List,
}

impl Call {
    /// The keymap call `message` is, or the message back when it is something else.
    pub(super) fn of(message: ToDaemon) -> Result<Call, ToDaemon> {
        match message {
            ToDaemon::WatchKeymap => Ok(Call::Watch),
            ToDaemon::Register(registration) => Ok(Call::Register(registration)),
            ToDaemon::ListRegistered => Ok(Call::List),
            other => Err(other),
        }
    }
}

/// One message for one client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Reply {
    pub to: ClientId,
    pub message: ToClient,
}

#[derive(Debug, Default)]
pub(super) struct Shelf {
    state: KeymapState,
    watchers: BTreeSet<ClientId>,
    registered: BTreeMap<ClientId, Vec<Registration>>,
}

impl Shelf {
    /// A shelf holding the state settled from the file as the daemon starts.
    pub(super) fn starting_with(first: Reading) -> Shelf {
        Shelf {
            state: settle(&KeymapState::default(), first),
            ..Shelf::default()
        }
    }

    /// The keymap in force.
    pub(super) fn state(&self) -> &KeymapState {
        &self.state
    }

    /// What `client`'s call answers with.
    pub(super) fn call(&mut self, client: ClientId, call: Call) -> Vec<Reply> {
        match call {
            Call::Watch => {
                self.watchers.insert(client);
                vec![self.keymap_for(client)]
            }
            Call::Register(registration) => {
                self.register(client, registration);
                Vec::new()
            }
            Call::List => vec![Reply {
                to: client,
                message: ToClient::Registered(self.everything()),
            }],
        }
    }

    /// Settles a fresh reading of the file. `None` when nothing changed; otherwise the new state
    /// for every watcher.
    pub(super) fn read(&mut self, reading: Reading) -> Option<Vec<Reply>> {
        let next = settle(&self.state, reading);
        if next == self.state {
            return None;
        }
        self.state = next;
        Some(self.watchers.iter().map(|w| self.keymap_for(*w)).collect())
    }

    /// Forgets every client not in `live`: their watches and registrations end with them.
    pub(super) fn retain(&mut self, live: &[ClientId]) {
        self.watchers.retain(|c| live.contains(c));
        self.registered.retain(|c, _| live.contains(c));
    }

    fn keymap_for(&self, to: ClientId) -> Reply {
        Reply {
            to,
            message: ToClient::Keymap(self.state.clone()),
        }
    }

    fn register(&mut self, client: ClientId, registration: Registration) {
        let Some(clean) = vetted(registration) else {
            return;
        };
        let apps = self.registered.entry(client).or_default();
        match apps.iter_mut().find(|r| r.app == clean.app) {
            Some(existing) => *existing = clean,
            None if apps.len() < MAX_APPS_PER_CLIENT => apps.push(clean),
            None => {}
        }
    }

    /// Every connection's registrations, by app, each app once.
    fn everything(&self) -> Vec<Registration> {
        let mut all: Vec<Registration> = self.registered.values().flatten().cloned().collect();
        all.sort();
        all.dedup();
        all
    }
}

/// The registration with only the actions chordkit would accept, or `None` for a bad app name.
fn vetted(registration: Registration) -> Option<Registration> {
    let app = AppId::new(&registration.app).ok()?;
    let actions = registration
        .actions
        .into_iter()
        .filter(|action| reads(&app, action))
        .collect();
    Some(Registration {
        app: registration.app,
        actions,
    })
}

fn reads(app: &AppId, action: &RegisteredAction) -> bool {
    AppAction::new(&action.id).is_ok_and(|parsed| parsed.app() == *app)
        && action.chord.parse::<DefaultChord>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registration(app: &str, ids: &[(&str, &str)]) -> Registration {
        Registration {
            app: app.to_owned(),
            actions: ids
                .iter()
                .map(|(id, chord)| RegisteredAction {
                    id: (*id).to_owned(),
                    chord: (*chord).to_owned(),
                })
                .collect(),
        }
    }

    fn shelf() -> Shelf {
        Shelf::starting_with(Reading::Text("copy = Primary+Alt+C\n".to_owned()))
    }

    fn listed(shelf: &mut Shelf, who: u64) -> Vec<Registration> {
        let replies = shelf.call(ClientId(who), Call::List);
        let [
            Reply {
                message: ToClient::Registered(all),
                ..
            },
        ] = replies.as_slice()
        else {
            panic!("{replies:?}");
        };
        all.clone()
    }

    #[test]
    fn only_keymap_messages_are_keymap_calls() {
        assert_eq!(Call::of(ToDaemon::WatchKeymap), Ok(Call::Watch));
        assert_eq!(Call::of(ToDaemon::ListRegistered), Ok(Call::List));
        assert_eq!(Call::of(ToDaemon::Resume), Err(ToDaemon::Resume));
    }

    #[test]
    fn watching_answers_with_the_keymap_and_each_change_reaches_every_watcher() {
        let mut shelf = shelf();
        let (a, b) = (ClientId(1), ClientId(2));
        let first = shelf.call(a, Call::Watch);
        assert_eq!(
            first,
            [Reply {
                to: a,
                message: ToClient::Keymap(shelf.state().clone())
            }]
        );
        assert!(!shelf.state().overrides.is_empty());
        shelf.call(b, Call::Watch);

        let told = shelf
            .read(Reading::Text("paste = Primary+Alt+V\n".to_owned()))
            .expect("changed");
        assert_eq!(told.iter().map(|r| r.to).collect::<Vec<_>>(), [a, b]);
        assert_eq!(
            told[0].message,
            ToClient::Keymap(shelf.state().clone()),
            "the new state"
        );
    }

    #[test]
    fn a_re_read_that_changes_nothing_tells_nobody() {
        let mut shelf = shelf();
        shelf.call(ClientId(1), Call::Watch);
        assert_eq!(
            shelf.read(Reading::Text("copy = Primary+Alt+C\n".to_owned())),
            None
        );
    }

    #[test]
    fn a_bad_edit_keeps_the_copy_and_still_tells_watchers_the_problem() {
        let mut shelf = shelf();
        shelf.call(ClientId(1), Call::Watch);
        let before = shelf.state().overrides.clone();
        let told = shelf
            .read(Reading::Text("nonsense\n".to_owned()))
            .expect("a problem is news");
        assert_eq!(shelf.state().overrides, before);
        assert!(matches!(&told[0].message, ToClient::Keymap(s) if !s.problems.is_empty()));
    }

    #[test]
    fn a_connection_that_never_watched_hears_no_change() {
        let mut shelf = shelf();
        shelf.call(ClientId(7), Call::List);
        let told = shelf.read(Reading::Missing).expect("changed");
        assert_eq!(told, []);
    }

    #[test]
    fn registrations_are_listed_for_anyone_and_end_with_their_connection() {
        let mut shelf = shelf();
        let mail = registration("mail", &[("mail.compose", "Primary+Shift+N")]);
        shelf.call(ClientId(1), Call::Register(mail.clone()));
        shelf.call(
            ClientId(2),
            Call::Register(registration("notes", &[("notes.new", "Primary+N")])),
        );
        let apps: Vec<String> = listed(&mut shelf, 3).into_iter().map(|r| r.app).collect();
        assert_eq!(apps, ["mail", "notes"]);

        shelf.retain(&[ClientId(2), ClientId(3)]);
        let apps: Vec<String> = listed(&mut shelf, 3).into_iter().map(|r| r.app).collect();
        assert_eq!(apps, ["notes"], "mail went with its connection");
    }

    #[test]
    fn registering_an_app_again_replaces_it_and_two_windows_list_it_once() {
        let mut shelf = shelf();
        let v1 = registration("mail", &[("mail.compose", "Primary+Shift+N")]);
        let v2 = registration("mail", &[("mail.reply", "Primary+R")]);
        shelf.call(ClientId(1), Call::Register(v1.clone()));
        shelf.call(ClientId(1), Call::Register(v2.clone()));
        assert_eq!(listed(&mut shelf, 9), [v2.clone()]);
        shelf.call(ClientId(2), Call::Register(v2.clone()));
        assert_eq!(listed(&mut shelf, 9), [v2]);
    }

    #[test]
    fn what_chordkit_would_refuse_is_dropped_and_the_number_of_apps_is_bounded() {
        let mut shelf = shelf();
        let mixed = registration(
            "mail",
            &[
                ("mail.compose", "Primary+Shift+N"),
                ("notes.new", "Primary+N"),
                ("mail.bad-chord", "Hyper+Q"),
            ],
        );
        shelf.call(ClientId(1), Call::Register(mixed));
        shelf.call(ClientId(1), Call::Register(registration("Bad App", &[])));
        let got = listed(&mut shelf, 9);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].actions.len(), 1);
        assert_eq!(got[0].actions[0].id, "mail.compose");

        for n in 0..MAX_APPS_PER_CLIENT + 4 {
            shelf.call(
                ClientId(2),
                Call::Register(registration(&format!("app{n}"), &[])),
            );
        }
        assert_eq!(listed(&mut shelf, 9).len(), 1 + MAX_APPS_PER_CLIENT);
    }
}
