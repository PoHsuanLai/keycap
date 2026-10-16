//! How large a configuration keycap will resolve. The seat owner's settings arrive over a
//! socket any program of theirs can write to, and resolving runs on the thread that also feeds
//! the watchdog, so a settings file with thousands of profiles must be turned away before it is
//! resolved, with the previous settings left in force.

use std::collections::{BTreeMap, BTreeSet};

use keycap_protocol::{ProfileName, Warning};

use super::confine::MAX_USER_RULES;
use super::layer::{ConfigLayer, ProfileLayer};
use super::lenient::{dotted, warning};
use super::resolve::merge;

/// Profiles in the folded configuration, `default` included. The built-in defaults have five.
pub const MAX_PROFILES: usize = 64;
/// Steps along one profile's `inherits` chain. A real one is a step or two.
pub const MAX_INHERIT_STEPS: usize = 8;
/// Rules one profile defines itself, after the layers are folded. The built-in `default` has
/// about forty.
pub const MAX_RULES_PER_PROFILE: usize = 512;

/// Which limit a configuration is over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Oversize {
    Profiles {
        found: usize,
    },
    InheritSteps {
        profile: ProfileName,
        found: usize,
    },
    Rules {
        profile: ProfileName,
        found: usize,
    },
    /// The seat owner's layer alone has too many rules.
    UserRules {
        found: usize,
    },
}

impl Oversize {
    /// The refusal as a warning in plain words, naming what to shorten.
    pub fn warning(&self) -> Warning {
        const KEPT: &str = "The earlier settings stay in use.";
        match self {
            Oversize::Profiles { found } => warning(
                "profile",
                format!("there are {found} profiles and at most {MAX_PROFILES} are used. {KEPT}"),
            ),
            Oversize::InheritSteps { profile, found } => warning(
                &dotted(&dotted("profile", profile.as_str()), "inherits"),
                format!(
                    "inherits through {found} profiles and at most {MAX_INHERIT_STEPS} are used. {KEPT}"
                ),
            ),
            Oversize::Rules { profile, found } => warning(
                &dotted("profile", profile.as_str()),
                format!("has {found} rules and at most {MAX_RULES_PER_PROFILE} are used. {KEPT}"),
            ),
            Oversize::UserRules { found } => warning(
                "profile",
                format!(
                    "your own settings have {found} rules and at most {MAX_USER_RULES} are used. {KEPT}"
                ),
            ),
        }
    }
}

/// `Ok` when the layers fold into a configuration within the limits. Cheap: it folds the layers
/// and counts, and follows no chain until the number of profiles is known to be small.
pub fn within_limits(layers: &[ConfigLayer]) -> Result<(), Oversize> {
    let merged = layers.iter().fold(BTreeMap::new(), merge);
    if merged.len() > MAX_PROFILES {
        return Err(Oversize::Profiles {
            found: merged.len(),
        });
    }
    for (name, profile) in &merged {
        let found = profile.rules.len();
        if found > MAX_RULES_PER_PROFILE {
            return Err(Oversize::Rules {
                profile: name.clone(),
                found,
            });
        }
        let found = inherit_steps(name, &merged);
        if found > MAX_INHERIT_STEPS {
            return Err(Oversize::InheritSteps {
                profile: name.clone(),
                found,
            });
        }
    }
    Ok(())
}

/// Steps from `name` along declared parents until the chain ends or comes back to a profile it
/// has seen (a cycle, which resolving reports and breaks).
fn inherit_steps(name: &ProfileName, merged: &BTreeMap<ProfileName, ProfileLayer>) -> usize {
    let mut seen = BTreeSet::new();
    let mut at = Some(name);
    while let Some(n) = at {
        if !seen.insert(n) {
            break;
        }
        at = merged.get(n).and_then(|p| p.inherits.as_ref());
    }
    seen.len().saturating_sub(1)
}
