//! The messages each side sends.

use serde::{Deserialize, Serialize};

use crate::vocab::{ProfileName, Status, Warning};

/// A desktop to keycap. Only the seat owner's messages are honoured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum ToDaemon {
    /// The focused window's profile; the whole desktop-facing contract.
    Focus { profile: ProfileName },
    /// The seat owner's settings as TOML, read leniently by the daemon.
    UserConfig { toml: String },
    /// Ends a pause caused by the escape chord.
    Resume,
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
}
