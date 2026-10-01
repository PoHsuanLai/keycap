//! `Profiles`: every flat profile, with `default` always present.

use std::collections::BTreeMap;

use keycap_protocol::ProfileName;

use super::model::Profile;

/// The profiles a desktop may focus. A name nobody defined resolves to `default`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profiles {
    default: Profile,
    named: BTreeMap<ProfileName, Profile>,
}

impl Profiles {
    /// Built only by config resolution, which is what guarantees `default` exists and that
    /// `named` does not repeat it.
    pub(crate) fn new(default: Profile, named: BTreeMap<ProfileName, Profile>) -> Profiles {
        Profiles { default, named }
    }

    /// The profile called `name`, or `default` when there is none.
    pub fn resolve(&self, name: &ProfileName) -> &Profile {
        self.named.get(name).unwrap_or(&self.default)
    }

    /// The profile every other one inherits from.
    pub fn default_profile(&self) -> &Profile {
        &self.default
    }

    /// Whether `name` is a profile the config defines (`default` included).
    pub fn defines(&self, name: &ProfileName) -> bool {
        *name == ProfileName::default_profile() || self.named.contains_key(name)
    }
}
