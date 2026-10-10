//! The messages each side sends.

use serde::{Deserialize, Serialize};

use crate::keymap::{KeymapState, Registration};
use crate::vocab::{ProfileName, Status, Warning};

/// A desktop to keycap. Only the seat owner's messages that change keycap are honoured; the
/// keymap messages (protocol 2) are open to every client, because they only read or inform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum ToDaemon {
    /// The focused window's profile; the whole desktop-facing contract.
    Focus { profile: ProfileName },
    /// The seat owner's settings as TOML, read leniently by the daemon.
    UserConfig { toml: String },
    /// Ends a pause caused by the escape chord.
    Resume,
    /// Send the person's keymap now and again whenever it changes (protocol 2). A client that
    /// never sends this never hears `Keymap`.
    WatchKeymap,
    /// Tell keycap what an app handles (protocol 2). Kept until the connection closes; a later
    /// registration of the same app on the same connection replaces the earlier one.
    Register(Registration),
    /// Ask for every registered app's actions (protocol 2), answered with `Registered`.
    ListRegistered,
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
    /// The person's keymap (protocol 2): first when asked with `WatchKeymap`, then on each change.
    Keymap(KeymapState),
    /// Every registered app's actions (protocol 2), answering `ListRegistered`.
    Registered(Vec<Registration>),
}
