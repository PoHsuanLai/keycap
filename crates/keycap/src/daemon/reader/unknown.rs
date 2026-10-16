//! Which focused profile names the config does not define, so each is logged once per config
//! generation. Pure: the decision is a value; the reader prints the line.

use crate::Profiles;
use keycap_protocol::ProfileName;
use std::collections::BTreeSet;

/// Names listed per config generation. A client can focus any name it likes, so what is kept
/// and logged is bounded; the rest are counted as one line.
const MAX_LISTED: usize = 64;

/// Whether the "more names" line has been written this generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Overflow {
    #[default]
    Unsaid,
    Said,
}

/// The undefined names already reported since the config last changed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct Unknowns {
    listed: BTreeSet<ProfileName>,
    overflow: Overflow,
}

/// What focusing a profile calls for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Notice {
    /// The profile exists, or its absence was already reported.
    Silent,
    /// The first focus of a name the config does not define.
    Undefined(ProfileName),
    /// More undefined names than are listed; said once.
    TooMany,
}

impl Unknowns {
    /// `name` was focused under `profiles`: the notice it earns and the names reported now.
    pub(super) fn note(mut self, name: &ProfileName, profiles: &Profiles) -> (Self, Notice) {
        if profiles.defines(name) || self.listed.contains(name) {
            return (self, Notice::Silent);
        }
        if self.listed.len() < MAX_LISTED {
            self.listed.insert(name.clone());
            return (self, Notice::Undefined(name.clone()));
        }
        match self.overflow {
            Overflow::Unsaid => (
                Self {
                    overflow: Overflow::Said,
                    ..self
                },
                Notice::TooMany,
            ),
            Overflow::Said => (self, Notice::Silent),
        }
    }

    /// The config changed: every name may be reported again.
    pub(super) fn reset(self) -> Self {
        Self::default()
    }
}

impl Notice {
    /// The log line, if there is one.
    pub(super) fn line(&self) -> Option<String> {
        match self {
            Notice::Silent => None,
            Notice::Undefined(name) => Some(format!(
                "keycap: profile \"{name}\" is not defined; using default"
            )),
            Notice::TooMany => Some(format!(
                "keycap: more than {MAX_LISTED} undefined profile names were focused; not listing the rest"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::settings::{Layers, parse_user};

    fn name(s: &str) -> ProfileName {
        ProfileName::parse(s).unwrap()
    }

    fn builtin() -> Profiles {
        Layers::builtin_only().resolve().0
    }

    /// The built-in profiles plus a user profile called `notes`.
    fn with_notes() -> Profiles {
        let (user, _) = parse_user("[profile.notes]\ninherits = \"default\"\n");
        Layers::builtin_only().with_user(user).resolve().0
    }

    fn line(s: &str) -> Option<String> {
        Some(format!(
            "keycap: profile \"{s}\" is not defined; using default"
        ))
    }

    #[test]
    fn an_undefined_name_is_reported_once_per_config_generation() {
        // Each step is a focus under the profiles named, or a config change (`None`).
        enum Step {
            Focus(&'static str, fn() -> Profiles),
            ConfigChanged,
        }
        type Case = (&'static str, &'static [(Step, Option<&'static str>)]);
        let cases: &[Case] = &[
            (
                "defined profiles are silent, default included",
                &[
                    (Step::Focus("default", builtin), None),
                    (Step::Focus("terminal", builtin), None),
                ],
            ),
            (
                "an undefined name logs the first time only",
                &[
                    (Step::Focus("mystery", builtin), Some("mystery")),
                    (Step::Focus("mystery", builtin), None),
                    (Step::Focus("mystery", builtin), None),
                ],
            ),
            (
                "each name is reported on its own",
                &[
                    (Step::Focus("one", builtin), Some("one")),
                    (Step::Focus("two", builtin), Some("two")),
                    (Step::Focus("one", builtin), None),
                ],
            ),
            (
                "a config change lets it be reported again",
                &[
                    (Step::Focus("mystery", builtin), Some("mystery")),
                    (Step::ConfigChanged, None),
                    (Step::Focus("mystery", builtin), Some("mystery")),
                ],
            ),
            (
                "a name the new config defines is silent",
                &[
                    (Step::Focus("notes", builtin), Some("notes")),
                    (Step::ConfigChanged, None),
                    (Step::Focus("notes", with_notes), None),
                ],
            ),
        ];
        for (case, steps) in cases {
            let mut unknowns = Unknowns::default();
            for (i, (step, want)) in steps.iter().enumerate() {
                let got = match step {
                    Step::Focus(n, profiles) => {
                        let (next, notice) = unknowns.clone().note(&name(n), &profiles());
                        unknowns = next;
                        notice.line()
                    }
                    Step::ConfigChanged => {
                        unknowns = unknowns.clone().reset();
                        None
                    }
                };
                assert_eq!(got, want.and_then(line), "{case}, step {i}");
            }
        }
    }

    #[test]
    fn names_beyond_the_listing_are_said_once_and_the_set_stays_small() {
        let profiles = builtin();
        let (unknowns, notices) = (0..MAX_LISTED + 5).fold(
            (Unknowns::default(), Vec::new()),
            |(unknowns, mut notices), i| {
                let (next, notice) = unknowns.note(&name(&format!("n{i}")), &profiles);
                notices.push(notice);
                (next, notices)
            },
        );
        assert_eq!(unknowns.listed.len(), MAX_LISTED);
        let said: Vec<&Notice> = notices
            .iter()
            .filter(|n| matches!(n, Notice::TooMany))
            .collect();
        assert_eq!(said.len(), 1, "one overflow line in all");
        assert_eq!(
            notices
                .iter()
                .filter(|n| matches!(n, Notice::Undefined(_)))
                .count(),
            MAX_LISTED
        );
        let (again, _) = unknowns.reset().note(&name("n0"), &profiles);
        assert_eq!(
            again.listed.len(),
            1,
            "a config change starts the count over"
        );
    }
}
