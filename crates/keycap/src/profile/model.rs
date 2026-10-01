//! `ModMap` and `Profile`: one flat, fully resolved profile.

use std::collections::BTreeMap;

use super::action::Action;
use super::chord::Trigger;
use crate::role::{OutMod, Role};

/// The modifier each role writes when it is pressed. Shift always writes Shift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModMap {
    pub command: OutMod,
    pub option: OutMod,
    pub control: OutMod,
}

impl ModMap {
    /// Each role writes the modifier it is named after: no remapping.
    pub const IDENTITY: ModMap = ModMap {
        command: OutMod::Super,
        option: OutMod::Alt,
        control: OutMod::Ctrl,
    };

    /// What `role` writes when pressed.
    pub const fn out(self, role: Role) -> OutMod {
        match role {
            Role::Command => self.command,
            Role::Option => self.option,
            Role::Control => self.control,
            Role::Shift => OutMod::Shift,
        }
    }
}

/// A profile with inheritance already applied: every rule it has, inherited ones included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub modmap: ModMap,
    pub rules: BTreeMap<Trigger, Action>,
}
