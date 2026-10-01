//! The data of the escape-chord detector.

use std::collections::BTreeSet;

use crate::event::{DeviceId, Physical};

/// Which of left Shift, right Shift and Esc are down, and on which keyboard.
///
/// Only the three chord keys are tracked. A key is per `(DeviceId, Physical)` so a keyboard that
/// vanishes takes its keys with it. When the grabs are given back the releases never reach the
/// remapper, so the owner starts over with `Escape::new()` on resume and on release-all.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Escape {
    pub(super) down: BTreeSet<(DeviceId, Physical)>,
}

/// What a key event did to the chord.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EscapeChord {
    /// Esc was pressed while both Shift keys were down.
    Fired,
    /// Anything else.
    Quiet,
}
