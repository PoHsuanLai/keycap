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

impl Role {
    /// Every role, in declaration order.
    pub const ALL: [Role; 4] = [Role::Command, Role::Option, Role::Control, Role::Shift];

    /// The role called `name` in a profile's `when` chord.
    pub fn named(name: &str) -> Option<Role> {
        Self::ALL.into_iter().find(|r| r.name() == name)
    }

    /// The name used in a profile's `when` chord.
    pub const fn name(self) -> &'static str {
        match self {
            Role::Command => "command",
            Role::Option => "option",
            Role::Control => "control",
            Role::Shift => "shift",
        }
    }

    pub(super) const fn bit(self) -> u8 {
        1 << self as u8
    }
}

bitset! {
    /// A set of roles, such as the ones currently held.
    Roles of Role
}
