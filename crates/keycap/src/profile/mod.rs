//! What a chord becomes: triggers, strokes, actions and the profiles that hold them.

mod action;
mod chord;
mod model;
mod profiles;
#[cfg(test)]
mod tests;

pub use action::{Hold, RuleAction};
pub use chord::{ChordError, Stroke, Trigger};
pub use model::{ModMap, Profile};
pub use profiles::Profiles;
