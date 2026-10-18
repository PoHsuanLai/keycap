//! The messages each side sends.

use serde::{Deserialize, Serialize};

use crate::keymap::{KeymapState, Registration};
use crate::vocab::{ProfileName, Status, Warning};

/// A desktop to keycap: a request to steer it or a call on the keymap. Only the seat owner's
/// steering is honoured; the keymap calls are open to every client, because they only read or
/// inform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum ToDaemon {
    /// A request that changes keycap.
    Steer(Steer),
    /// A call on the person's keymap.
    Keymap(KeymapCall),
}

/// What the seat owner's shell asks of keycap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum Steer {
    /// The focused window's profile; the whole desktop-facing contract.
    Focus { profile: ProfileName },
    /// The seat owner's settings as TOML, read leniently by the daemon.
    UserConfig { toml: String },
    /// Ends a pause caused by the escape chord.
    Resume,
}

/// What any client may ask about the keymap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum KeymapCall {
    /// Send the person's keymap now and again whenever it changes. A client that never sends
    /// this never hears `Keymap`.
    Watch,
    /// Tell keycap what an app handles. Kept until the connection closes; a later registration
    /// of the same app on the same connection replaces the earlier one.
    Register(Registration),
    /// Ask for every registered app's actions, answered with `Registered`.
    List,
}

/// keycap to a desktop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum ToClient {
    /// First on every connection: the protocol version and the current status.
    Hello { version: u16, status: Status },
    /// The status changed.
    Status(Status),
    /// What the last `UserConfig` got wrong; empty when it was clean.
    ConfigWarnings(Vec<Warning>),
    /// The person's keymap: first when asked with `KeymapCall::Watch`, then on each change.
    Keymap(KeymapState),
    /// Every registered app's actions, answering `KeymapCall::List`.
    Registered(Vec<Registration>),
}
