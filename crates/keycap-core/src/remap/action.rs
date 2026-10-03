//! What a matched rule writes: the three `RuleAction`s as transitions of one `Batch`.

use super::batch::Batch;
use crate::event::Logical;
use crate::held::At;
use crate::profile::{Hold, RuleAction, Stroke};
use crate::role::{OutMod, OutMods};

/// Apply `action` for the physical key `at`. `owner` is the held non-Shift role key that keeps a
/// `WhileHeld` stroke's modifiers; without one the stroke behaves as `Restore`.
pub(super) fn apply(batch: Batch, action: &RuleAction, at: At, owner: Option<At>) -> Batch {
    match (action, owner) {
        (RuleAction::Map(stroke, Hold::WhileHeld), Some(owner)) => {
            while_held(batch, *stroke, at, owner)
        }
        (RuleAction::Map(stroke, _), _) => restore(batch, *stroke, at),
        (RuleAction::Sequence(strokes), _) => strokes
            .iter()
            .fold(batch, |batch, stroke| tap(batch, *stroke, at))
            .then(|held| held.unlift(at)),
        (RuleAction::Swallow, _) => batch,
    }
}

/// The stroke is held with `at`. The modifiers it does not want are lifted by `at`, and come
/// back when `at` is released.
fn restore(batch: Batch, stroke: Stroke, at: At) -> Batch {
    batch
        .then(|held| held.lift(at, unwanted(stroke.mods)))
        .then(|held| held.press(at, wants(stroke)))
}

/// The lift and the stroke's extra modifiers belong to `owner`, the role key still held, so the
/// modifiers outlast the physical key; only the stroke's key belongs to `at`.
fn while_held(batch: Batch, stroke: Stroke, at: At, owner: At) -> Batch {
    batch
        .then(|held| held.lift(owner, unwanted(stroke.mods)))
        .then(|held| held.extend(owner, mod_keys(stroke.mods)))
        .then(|held| held.press(at, vec![Logical(stroke.key)]))
}

/// Press and release one stroke; the lifts stay with `at` until the sequence ends.
fn tap(batch: Batch, stroke: Stroke, at: At) -> Batch {
    batch
        .then(|held| held.lift(at, unwanted(stroke.mods)))
        .then(|held| held.press(at, wants(stroke)))
        .then(|held| held.release_record(at))
}

fn mod_keys(mods: OutMods) -> Vec<Logical> {
    mods.iter().map(|m| Logical(m.key())).collect()
}

/// The written modifiers `mods` leaves out, as keys.
fn unwanted(mods: OutMods) -> Vec<Logical> {
    OutMod::ALL
        .into_iter()
        .filter(|m| !mods.contains(*m))
        .map(|m| Logical(m.key()))
        .collect()
}

fn wants(stroke: Stroke) -> Vec<Logical> {
    let mut keys = mod_keys(stroke.mods);
    keys.push(Logical(stroke.key));
    keys
}
