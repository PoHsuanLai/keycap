//! `Trigger` and `Stroke`: the two chord vocabularies, parsed from `+`-joined names.

use thiserror::Error;

use crate::key::Key;
use crate::role::{OutMod, OutMods, Role, Roles};

/// Why a chord string is not a `Trigger` or a `Stroke`.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ChordError {
    #[error("chord is empty")]
    Empty,
    #[error("chord has an empty name (a stray '+')")]
    EmptyName,
    #[error(
        "{0:?} is an output modifier; a `when` chord uses role names (command, option, control, shift)"
    )]
    OutputModifier(String),
    #[error("{0:?} is a role name; a `then` chord uses output names (ctrl, alt, super, shift)")]
    RoleName(String),
    #[error("{0:?} is not a modifier name")]
    UnknownModifier(String),
    #[error("{0:?} appears twice")]
    Duplicate(String),
    #[error("chord ends in the modifier {0:?}; its last name must be a key")]
    MissingKey(String),
    #[error("{0:?} is not a key name")]
    UnknownKey(String),
}

/// What a rule matches: the physical key, with exactly these roles held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Trigger {
    pub roles: Roles,
    pub key: Key,
}

/// What a rule writes: the key, with these modifiers held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Stroke {
    pub mods: OutMods,
    pub key: Key,
}

impl Trigger {
    /// Parses `"command+shift+z"`: role names, then one key name last.
    pub fn parse(text: &str) -> Result<Trigger, ChordError> {
        let (roles, key) = parse_chord(text, trigger_modifier)?;
        Ok(Trigger { roles, key })
    }
}

impl Stroke {
    /// Parses `"ctrl+shift+c"`: output modifier names, then one key name last.
    pub fn parse(text: &str) -> Result<Stroke, ChordError> {
        let (mods, key) = parse_chord(text, stroke_modifier)?;
        Ok(Stroke { mods, key })
    }
}

fn trigger_modifier(name: &str) -> Result<Role, ChordError> {
    Role::named(name).ok_or_else(|| match OutMod::named(name) {
        Some(_) => ChordError::OutputModifier(name.to_owned()),
        None => ChordError::UnknownModifier(name.to_owned()),
    })
}

fn stroke_modifier(name: &str) -> Result<OutMod, ChordError> {
    OutMod::named(name).ok_or_else(|| match Role::named(name) {
        Some(_) => ChordError::RoleName(name.to_owned()),
        None => ChordError::UnknownModifier(name.to_owned()),
    })
}

/// The key is the last name; every name before it is a modifier read by `modifier`, none twice.
fn parse_chord<M, S>(
    text: &str,
    modifier: impl Fn(&str) -> Result<M, ChordError>,
) -> Result<(S, Key), ChordError>
where
    M: Copy + PartialEq,
    S: FromIterator<M>,
{
    if text.trim().is_empty() {
        return Err(ChordError::Empty);
    }
    let names: Vec<&str> = text.split('+').map(str::trim).collect();
    if names.contains(&"") {
        return Err(ChordError::EmptyName);
    }
    let Some((key_name, modifier_names)) = names.split_last() else {
        return Err(ChordError::Empty);
    };
    let key = key_named(key_name)?;
    let mods = modifier_names
        .iter()
        .try_fold(Vec::new(), |mut seen, name| {
            let m = modifier(name)?;
            if seen.contains(&m) {
                return Err(ChordError::Duplicate((*name).to_owned()));
            }
            seen.push(m);
            Ok(seen)
        })?;
    Ok((mods.into_iter().collect(), key))
}

/// A key name, where a modifier name in key position is a chord missing its key.
fn key_named(name: &str) -> Result<Key, ChordError> {
    if Role::named(name).is_some() || OutMod::named(name).is_some() {
        return Err(ChordError::MissingKey(name.to_owned()));
    }
    Key::named(name).ok_or_else(|| ChordError::UnknownKey(name.to_owned()))
}
