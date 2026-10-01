//! The gate's transition.

use super::model::{Decision, GrabGate, KeysDown};

impl GrabGate {
    /// A device just seen, not yet grabbed.
    pub const fn new() -> Self {
        GrabGate::Pending
    }

    /// Check the device against its current key bitmap.
    pub fn step(self, keys_down: KeysDown) -> (Self, Decision) {
        match (self, keys_down.any()) {
            (GrabGate::Pending, false) => (GrabGate::Grabbed, Decision::Grab),
            (GrabGate::Pending, true) => (GrabGate::Pending, Decision::Wait),
            (GrabGate::Grabbed, _) => (GrabGate::Grabbed, Decision::Keep),
        }
    }
}

impl Default for GrabGate {
    fn default() -> Self {
        Self::new()
    }
}
