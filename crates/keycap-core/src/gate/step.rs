//! The gate's check.

use super::model::{Decision, KeysDown};

/// Check a device against its current key bitmap. `past_gate` is whether it was grabbed already;
/// then there is nothing to do and the answer is `None`.
pub fn check_gate(past_gate: bool, keys_down: KeysDown) -> Option<Decision> {
    match (past_gate, keys_down.any()) {
        (true, _) => None,
        (false, false) => Some(Decision::Grab),
        (false, true) => Some(Decision::Wait),
    }
}
