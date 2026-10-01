//! `GrabGate`: a keyboard is grabbed only once no key is held, so no press is orphaned.

mod model;
mod step;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use model::KEYS_DOWN_BYTES;
pub use model::{Decision, GrabGate, KeysDown};
