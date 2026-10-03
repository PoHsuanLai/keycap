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

impl OutMod {
    /// Every written modifier, in declaration order.
    pub const ALL: [OutMod; 4] = [OutMod::Ctrl, OutMod::Alt, OutMod::Super, OutMod::Shift];

    /// The written modifier called `name` in a profile's `then` chord.
    pub fn named(name: &str) -> Option<OutMod> {
        Self::ALL.into_iter().find(|m| m.name() == name)
    }

    /// The name used in a profile's `then` chord.
    pub const fn name(self) -> &'static str {
        match self {
            OutMod::Ctrl => "ctrl",
            OutMod::Alt => "alt",
            OutMod::Super => "super",
            OutMod::Shift => "shift",
        }
    }

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
