//! Tests of layered configuration, by concept; the helpers here are shared by the files below.

mod defaults;
mod dist;
mod inheritance;
mod layering;
mod lenient;

use keycap_protocol::{ProfileName, Warning};

use super::*;
use crate::profile::{Hold, Profile, Profiles, RuleAction, Stroke, Trigger};

pub(super) fn name(s: &str) -> ProfileName {
    ProfileName::parse(s).expect("test profile names are valid")
}

pub(super) fn trigger(s: &str) -> Trigger {
    Trigger::parse(s).expect("test triggers are valid")
}

pub(super) fn stroke(s: &str) -> Stroke {
    Stroke::parse(s).expect("test strokes are valid")
}

pub(super) fn map(s: &str) -> RuleAction {
    RuleAction::Map(stroke(s), Hold::Restore)
}

pub(super) fn layer(text: &str) -> ConfigLayer {
    let (layer, warnings) = ConfigLayer::parse(text, ConfigSource::System);
    assert_eq!(warnings, vec![], "fixture must parse cleanly: {text}");
    layer
}

pub(super) fn resolved(texts: &[&str]) -> (Profiles, Vec<Warning>) {
    let layers: Vec<ConfigLayer> = texts.iter().map(|t| layer(t)).collect();
    resolve(&layers)
}

pub(super) fn rule<'a>(p: &'a Profile, when: &str) -> Option<&'a RuleAction> {
    p.rules.get(&trigger(when))
}

pub(super) fn keys(warnings: &[Warning]) -> Vec<&str> {
    warnings.iter().map(|w| w.key.as_str()).collect()
}

pub(super) fn builtin_profiles() -> Profiles {
    let (layer, warnings) = builtin();
    assert_eq!(warnings, vec![]);
    resolve(&[layer]).0
}
