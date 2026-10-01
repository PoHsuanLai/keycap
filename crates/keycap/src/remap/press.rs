//! A key press: what it is (role key, rule, plain key) and what it writes.

use super::action::apply;
use super::batch::Batch;
use super::model::{RemapOut, Remapper};
use crate::event::Logical;
use crate::held::At;
use crate::layout::{Layout, role};
use crate::profile::{Profile, RuleAction, Trigger};
use crate::role::Role;

/// What a pressed key is, under the held roles and the focused profile.
enum Press<'p> {
    Role(Role),
    Rule(&'p RuleAction),
    Plain,
}

impl Remapper {
    /// Handle the press of `at`, a key on a keyboard of `layout`.
    pub(super) fn press(
        mut self,
        profile: &Profile,
        layout: Layout,
        at: At,
    ) -> (Self, Vec<RemapOut>) {
        self.roles.remove(&at);
        let kind = classify(profile, self.held_roles(), layout, at);
        let owner = self.chord_owner();
        let batch = Batch::new(std::mem::take(&mut self.held)).then(|held| held.release(at));
        let batch = match &kind {
            Press::Role(role) => batch.then(|held| held.press(at, vec![out_key(profile, *role)])),
            Press::Rule(action) => apply(batch, action, at, owner),
            Press::Plain => batch.then(|held| held.press(at, vec![Logical(at.1.0)])),
        };
        let (held, outs) = batch.finish();
        let mut next = self.with_held(held);
        if let Press::Role(role) = kind {
            next.roles.insert(at, role);
        }
        (next, outs)
    }

    /// The held non-Shift role key a `WhileHeld` stroke belongs to: the lowest one, if several.
    fn chord_owner(&self) -> Option<At> {
        self.roles
            .iter()
            .filter(|(at, role)| **role != Role::Shift && self.held.is_held(**at))
            .map(|(at, _)| *at)
            .next()
    }
}

fn classify(profile: &Profile, roles: crate::role::Roles, layout: Layout, at: At) -> Press<'_> {
    match role(layout, at.1) {
        Some(role) => Press::Role(role),
        None => profile
            .rules
            .get(&Trigger { roles, key: at.1.0 })
            .map_or(Press::Plain, Press::Rule),
    }
}

fn out_key(profile: &Profile, role: Role) -> Logical {
    Logical(profile.modmap.out(role).key())
}
