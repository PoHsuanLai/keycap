//! `resolve`: fold the layers, then flatten inheritance.

use std::collections::BTreeMap;
use std::iter::successors;

use keycap_protocol::{ProfileName, Warning};

use super::layer::{ConfigLayer, ProfileLayer};
use super::lenient::{dotted, warning};
use crate::profile::{ModMap, Profile, Profiles};

/// Folds `layers` (earliest first, later overriding earlier per profile, per modmap field and
/// per trigger), then flattens inheritance: a child's modmap fields and rules lie over its
/// parent's, transitively. `default` inherits nothing; a profile naming an unknown parent, or
/// one that leads back to itself, inherits `default` instead and is reported.
pub fn resolve(layers: &[ConfigLayer]) -> (Profiles, Vec<Warning>) {
    let default_name = ProfileName::default_profile();
    let mut merged = layers.iter().fold(BTreeMap::new(), merge);
    let default_layer = merged.remove(&default_name).unwrap_or_default();
    let default_warning = default_layer.inherits.as_ref().map(|_| {
        warning(
            &inherits_key(&default_name),
            "`default` inherits nothing; ignored",
        )
    });
    merged.insert(
        default_name.clone(),
        ProfileLayer {
            inherits: None,
            ..default_layer
        },
    );

    let (declared, unknown): (BTreeMap<_, _>, Vec<_>) = merged
        .iter()
        .filter(|(name, _)| **name != default_name)
        .map(|(name, p)| declared_parent(name, p, &merged, &default_name))
        .unzip();
    let (parents, cyclic): (BTreeMap<_, _>, Vec<_>) = declared
        .keys()
        .map(|name| acyclic_parent(name, &declared, &default_name))
        .unzip();

    let flat = |name: &ProfileName| flatten(name, &merged, &parents);
    let named = parents.keys().map(|n| (n.clone(), flat(n))).collect();
    let warnings = default_warning
        .into_iter()
        .chain(unknown.into_iter().flatten())
        .chain(cyclic.into_iter().flatten())
        .collect();
    (Profiles::new(flat(&default_name), named), warnings)
}

/// `acc` with `layer`'s profiles laid over what it already holds.
fn merge(
    mut acc: BTreeMap<ProfileName, ProfileLayer>,
    layer: &ConfigLayer,
) -> BTreeMap<ProfileName, ProfileLayer> {
    for (name, later) in &layer.profiles {
        let merged = match acc.remove(name) {
            Some(earlier) => earlier.over(later.clone()),
            None => later.clone(),
        };
        acc.insert(name.clone(), merged);
    }
    acc
}

fn inherits_key(name: &ProfileName) -> String {
    dotted(&dotted("profile", name.as_str()), "inherits")
}

/// The parent `name` declares, or `default` (with a warning) when it names none that exists.
fn declared_parent(
    name: &ProfileName,
    profile: &ProfileLayer,
    merged: &BTreeMap<ProfileName, ProfileLayer>,
    default_name: &ProfileName,
) -> ((ProfileName, ProfileName), Option<Warning>) {
    match &profile.inherits {
        None => ((name.clone(), default_name.clone()), None),
        Some(parent) if merged.contains_key(parent) => ((name.clone(), parent.clone()), None),
        Some(parent) => (
            (name.clone(), default_name.clone()),
            Some(warning(
                &inherits_key(name),
                format!("unknown profile `{parent}`; inheriting `default` instead"),
            )),
        ),
    }
}

/// `name`'s parent, or `default` (with a warning) when following parents leads back to `name`.
fn acyclic_parent(
    name: &ProfileName,
    declared: &BTreeMap<ProfileName, ProfileName>,
    default_name: &ProfileName,
) -> ((ProfileName, ProfileName), Option<Warning>) {
    let leads_back = successors(declared.get(name), |p| declared.get(*p))
        .take(declared.len())
        .any(|p| p == name);
    match declared.get(name) {
        Some(parent) if leads_back => (
            (name.clone(), default_name.clone()),
            Some(warning(
                &inherits_key(name),
                format!(
                    "inheriting `{parent}` leads back to this profile; inheriting `default` instead"
                ),
            )),
        ),
        Some(parent) => ((name.clone(), parent.clone()), None),
        None => ((name.clone(), default_name.clone()), None),
    }
}

/// `name` over its parent over its grandparent, down to `default`.
fn flatten(
    name: &ProfileName,
    merged: &BTreeMap<ProfileName, ProfileLayer>,
    parents: &BTreeMap<ProfileName, ProfileName>,
) -> Profile {
    let lineage: Vec<&ProfileLayer> = successors(Some(name), |n| parents.get(*n))
        .filter_map(|n| merged.get(n))
        .collect();
    lineage.into_iter().rev().fold(
        Profile {
            modmap: ModMap::IDENTITY,
            rules: BTreeMap::new(),
        },
        |below, layer| Profile {
            modmap: layer.modmap.onto(below.modmap),
            rules: below.rules.into_iter().chain(layer.rules.clone()).collect(),
        },
    )
}
