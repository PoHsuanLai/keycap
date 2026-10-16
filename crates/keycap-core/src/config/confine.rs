//! What the seat owner's own settings may say. They arrive over a socket that a program of the
//! person's can reach, so they are cut down to what a person's shortcuts need: a rule is a
//! chord held with a modifier, and it writes one stroke. Anything that types by itself, on a key
//! pressed alone, belongs to the machine settings, which only an administrator can write.

use keycap_protocol::Warning;

use super::layer::{ConfigLayer, ProfileLayer};
use super::lenient::{dotted, warning};
use super::limits::Oversize;
use crate::profile::{RuleAction, Trigger};

/// Rules a user layer may hold across all its profiles. The shipped session layer has about
/// twenty; a person with a long personal list has a few dozen.
pub const MAX_USER_RULES: usize = 256;

/// Where the rules that are left out can still be set.
const MACHINE_FILE: &str = "/etc/keycap/keycap.toml";

/// Whether the user layer may hold rules that type several keys, or none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sequences {
    /// A rule writes one stroke; a list of strokes, or an empty list, is left out.
    #[default]
    Refuse,
    /// Lists are kept. Only the machine file can say this.
    Allow,
}

impl Sequences {
    /// The choice called `name` in `[user_layer] sequences`.
    pub fn named(name: &str) -> Option<Sequences> {
        match name {
            "refuse" => Some(Sequences::Refuse),
            "allow" => Some(Sequences::Allow),
            _ => None,
        }
    }
}

/// Why a rule of the user layer is left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reason {
    /// The trigger is a key pressed with no modifier held: ordinary typing.
    KeyAlone,
    /// The rule writes a list of strokes, or none.
    StrokeList,
}

impl Reason {
    fn text(self) -> String {
        let what = match self {
            Reason::KeyAlone => "a key pressed on its own",
            Reason::StrokeList => "a shortcut that types several keys, or none",
        };
        format!("{what} can only be set in {MACHINE_FILE}; this rule is left out")
    }
}

/// Why `action` on `trigger` may not stay in the user layer, if it may not.
fn objection(trigger: &Trigger, action: &RuleAction, sequences: Sequences) -> Option<Reason> {
    match (action, sequences) {
        _ if trigger.roles.is_empty() => Some(Reason::KeyAlone),
        (RuleAction::Sequence(_) | RuleAction::Swallow, Sequences::Refuse) => {
            Some(Reason::StrokeList)
        }
        _ => None,
    }
}

/// `command+shift+z`, for a warning.
fn chord_text(trigger: &Trigger) -> String {
    trigger
        .roles
        .iter()
        .map(|role| role.name())
        .chain([trigger.key.name()])
        .collect::<Vec<_>>()
        .join("+")
}

impl ConfigLayer {
    /// This layer, read as the seat owner's settings: a layer with more than
    /// [`MAX_USER_RULES`] rules is refused whole; otherwise each rule that
    /// [`Sequences`] or the modifier requirement forbids is dropped with a warning that names
    /// its chord. The machine file's layer is never passed through this.
    pub fn confined(self, sequences: Sequences) -> Result<(ConfigLayer, Vec<Warning>), Oversize> {
        let found: usize = self.profiles.values().map(|p| p.rules.len()).sum();
        if found > MAX_USER_RULES {
            return Err(Oversize::UserRules { found });
        }
        let mut warnings = Vec::new();
        let profiles = self
            .profiles
            .into_iter()
            .map(|(name, profile)| {
                let path = dotted(&dotted("profile", name.as_str()), "rules");
                let (kept, dropped) = keep_allowed(profile.rules, sequences);
                warnings.extend(dropped.into_iter().map(|(trigger, why)| {
                    warning(
                        &path,
                        format!("\"{}\": {}", chord_text(&trigger), why.text()),
                    )
                }));
                (
                    name,
                    ProfileLayer {
                        rules: kept,
                        ..profile
                    },
                )
            })
            .collect();
        Ok((ConfigLayer { profiles, ..self }, warnings))
    }
}

type Rules = std::collections::BTreeMap<Trigger, RuleAction>;

/// The rules that may stay, and the triggers of those that may not, with why.
fn keep_allowed(rules: Rules, sequences: Sequences) -> (Rules, Vec<(Trigger, Reason)>) {
    let mut kept = Rules::new();
    let mut dropped = Vec::new();
    for (trigger, action) in rules {
        match objection(&trigger, &action, sequences) {
            None => {
                kept.insert(trigger, action);
            }
            Some(why) => dropped.push((trigger, why)),
        }
    }
    (kept, dropped)
}
