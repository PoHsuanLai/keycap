//! `GrabGate`: a keyboard is grabbed only once no key is held, so no press is orphaned.

mod model;
mod step;
#[cfg(test)]
mod tests;

pub use model::{Decision, GrabGate, KEYS_DOWN_BYTES, KeysDown};
