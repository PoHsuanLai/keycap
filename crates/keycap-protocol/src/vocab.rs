//! The words a message is made of: profile names, keyboards, warnings and daemon status.

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The longest a profile name may be, in bytes.
const MAX_NAME_BYTES: usize = 64;

/// Why a string is not a profile name.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NameError {
    #[error("profile name is empty")]
    Empty,
    #[error("profile name is {0} bytes, over the {MAX_NAME_BYTES} limit")]
    TooLong(usize),
    #[error("profile name has {0:?}, allowed are a-z, 0-9, '_' and '-'")]
    BadChar(char),
}

/// A profile's name, valid by construction: 1 to 64 bytes of `[a-z0-9_-]`. Serde validates on
/// deserialize, so a name that arrives over the socket needs no second check.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProfileName(String);

impl ProfileName {
    /// The name of the profile every other one inherits from.
    pub fn default_profile() -> Self {
        Self("default".to_owned())
    }

    /// Validates `s` once, at the boundary.
    pub fn parse(s: &str) -> Result<Self, NameError> {
        match (s.len(), s.chars().find(|c| !is_name_char(*c))) {
            (0, _) => Err(NameError::Empty),
            (n, _) if n > MAX_NAME_BYTES => Err(NameError::TooLong(n)),
            (_, Some(c)) => Err(NameError::BadChar(c)),
            (_, None) => Ok(Self(s.to_owned())),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_name_char(c: char) -> bool {
    matches!(c, 'a'..='z' | '0'..='9' | '_' | '-')
}

impl TryFrom<String> for ProfileName {
    type Error = NameError;

    fn try_from(s: String) -> Result<Self, NameError> {
        Self::parse(&s)
    }
}

impl From<ProfileName> for String {
    fn from(name: ProfileName) -> Self {
        name.0
    }
}

impl fmt::Display for ProfileName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The identity of a keyboard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyboardInfo {
    pub name: String,
    pub path: String,
    pub vendor: u16,
    pub product: u16,
}

/// A problem found in the user's config, keyed by the dotted config path it concerns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warning {
    pub key: String,
    pub problem: String,
}

/// Whether keycap is remapping or has stood down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Running,
    /// The escape chord paused remapping; `Resume` ends it.
    Paused,
}

/// How keycap holds one keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Grab {
    /// Grabbed and remapped.
    Active,
    /// A key is held, so the grab waits for its release.
    WaitingForRelease,
    /// Another remapper already holds it.
    Conflict,
}

/// One keyboard and how keycap holds it; each keyboard has its own grab, so one can be
/// active while another conflicts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Keyboard {
    pub info: KeyboardInfo,
    pub grab: Grab,
}

/// What keycap is doing, for the desktop to show. No keyboards means none is attached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub mode: Mode,
    pub keyboards: Vec<Keyboard>,
}
