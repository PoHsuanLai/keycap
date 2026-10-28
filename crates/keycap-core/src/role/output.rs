//! `OutMod` and `OutMods`: the modifiers keycap writes.

use super::bitset::bitset;
use crate::key::Key;

/// A modifier keycap writes, always as its left-hand key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OutMod {
    Ctrl,
    Alt,
    Super,
    Shift,
}

// The names are the ones a profile's `then` chord uses.
names!(OutMod {
    Ctrl => "ctrl",
    Alt => "alt",
    Super => "super",
    Shift => "shift",
});

impl OutMod {
    /// The left-hand key written for this modifier.
    pub const fn key(self) -> Key {
        match self {
            OutMod::Ctrl => Key::LEFTCTRL,
            OutMod::Alt => Key::LEFTALT,
            OutMod::Super => Key::LEFTMETA,
            OutMod::Shift => Key::LEFTSHIFT,
        }
    }

    pub(super) const fn bit(self) -> u8 {
        1 << self as u8
    }
}

bitset! {
    /// A set of written modifiers.
    OutMods of OutMod
}
