//! Bare modifiers: a written Alt or Super with no other key since its press.
//!
//! The compositor sees only the written stream. Option+Left writes `-leftalt +leftctrl +left`,
//! which to a desktop is an Alt tap (the menu bar toggles); Control mapped to Super does the
//! same to the launcher. So a bare Alt or Super that is lifted, or restored by a lift and then
//! released, is preceded by a tap of `NEUTRAL`, which cancels the tap. The tap is press and
//! release in one write and is never recorded, so `Held`'s records stay what the keys wrote.

use super::model::{Bare, Held};
use super::step::key_event;
use crate::event::{Edge, Logical, OutEvent};
use crate::key::Key;

/// The key tapped to cancel a modifier tap. F24 is on no keyboard and rarely bound, so a tap of
/// it does nothing but end the compositor's "was this modifier alone" test.
pub(super) const NEUTRAL: Logical = Logical(Key::F24);

/// The written keys a bare release would turn into a desktop shortcut.
const TAP_SENSITIVE: [Key; 4] = [Key::LEFTALT, Key::RIGHTALT, Key::LEFTMETA, Key::RIGHTMETA];

impl Held {
    /// Write a press of `key`: every bare key is no longer bare, and `key` is bare if it is an
    /// Alt or Super.
    pub(super) fn write_press(&mut self, key: Logical, how: Bare) -> OutEvent {
        self.bare.clear();
        if TAP_SENSITIVE.contains(&key.0) {
            self.bare.insert(key, how);
        }
        key_event(key, Edge::Press)
    }

    /// Write a release of `key`, led by a neutral tap when the key is a stray bare modifier.
    pub(super) fn write_release(&mut self, key: Logical) -> Vec<OutEvent> {
        let stray = self.bare.remove(&key) == Some(Bare::Restored);
        if stray && !self.is_down(NEUTRAL) {
            self.bare.clear();
            return vec![
                key_event(NEUTRAL, Edge::Press),
                key_event(NEUTRAL, Edge::Release),
                key_event(key, Edge::Release),
            ];
        }
        vec![key_event(key, Edge::Release)]
    }

    /// A lift is about to take `key` off the wire: if it is bare, its release is a stray tap.
    pub(super) fn mark_stray(&mut self, key: Logical) {
        if let Some(bare) = self.bare.get_mut(&key) {
            *bare = Bare::Restored;
        }
    }
}
