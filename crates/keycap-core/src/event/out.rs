//! `OutEvent`: one event keycap writes to the virtual keyboard.

use super::{Edge, Logical};

/// An event in the written stream. Every batch ends with `Sync`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutEvent {
    /// A key transition.
    Key { key: Logical, edge: Edge },
    /// `SYN_REPORT`: the batch is complete.
    Sync,
}
