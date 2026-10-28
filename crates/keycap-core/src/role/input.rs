//! `Role` and `Roles`: what a held modifier key means, by position.

use super::bitset::bitset;

/// What a modifier key means on the keyboard that sent it, independent of its keycap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    /// The key beside the space bar.
    Command,
    /// The key next to Command, further from the space bar.
    Option,
    Control,
    Shift,
}

// The names are the ones a profile's `when` chord uses.
names!(Role {
    Command => "command",
    Option => "option",
    Control => "control",
    Shift => "shift",
});

impl Role {
    pub(super) const fn bit(self) -> u8 {
        1 << self as u8
    }
}

bitset! {
    /// A set of roles, such as the ones currently held.
    Roles of Role
}
