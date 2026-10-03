//! `ConfigLayer`: one TOML file, parsed but not yet resolved against the others.

use std::collections::BTreeMap;
use std::fmt;

use keycap_protocol::ProfileName;

use crate::profile::{ModMap, RuleAction, Trigger};
use crate::role::OutMod;

/// Where a layer came from, so a warning says whose file to fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    /// `defaults.toml`, compiled in.
    Builtin,
    /// `/etc/keycap/keycap.toml`.
    System,
    /// The seat owner's settings, sent over the socket.
    User,
}

impl fmt::Display for ConfigSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ConfigSource::Builtin => "built-in defaults",
            ConfigSource::System => "system config",
            ConfigSource::User => "user config",
        })
    }
}

/// The parsed form of one file: per profile, what that file says and nothing it leaves out.
/// Later layers override earlier ones per profile, per modmap field and per trigger.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigLayer {
    pub(super) profiles: BTreeMap<ProfileName, ProfileLayer>,
    pub(super) devices: Vec<super::devices::DeviceEntry>,
}

/// One profile as one file wrote it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ProfileLayer {
    pub(super) inherits: Option<ProfileName>,
    pub(super) modmap: PartialModMap,
    pub(super) rules: BTreeMap<Trigger, RuleAction>,
}

/// The modmap fields a file set.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct PartialModMap {
    pub(super) command: Option<OutMod>,
    pub(super) option: Option<OutMod>,
    pub(super) control: Option<OutMod>,
}

impl PartialModMap {
    /// The fields `later` sets, else the ones `self` sets.
    pub(super) fn over(self, later: PartialModMap) -> PartialModMap {
        PartialModMap {
            command: later.command.or(self.command),
            option: later.option.or(self.option),
            control: later.control.or(self.control),
        }
    }

    /// `base` with the fields this sets replaced.
    pub(super) fn onto(self, base: ModMap) -> ModMap {
        ModMap {
            command: self.command.unwrap_or(base.command),
            option: self.option.unwrap_or(base.option),
            control: self.control.unwrap_or(base.control),
        }
    }
}

impl ProfileLayer {
    /// `self` with everything `later` says laid over it.
    pub(super) fn over(self, later: ProfileLayer) -> ProfileLayer {
        ProfileLayer {
            inherits: later.inherits.or(self.inherits),
            modmap: self.modmap.over(later.modmap),
            rules: self.rules.into_iter().chain(later.rules).collect(),
        }
    }
}
