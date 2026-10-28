//! A Caps Lock the user did not ask for. When a rule takes the Caps Lock key (Caps Lock as the
//! input-method switch, say) pressing it no longer toggles the lock, so a lock the desktop has on
//! leaves letters in capitals with no way out. That happens when a keyboard was raw for a while
//! (a switch of remapper, a restart) or when the desktop syncs a stale lock onto a new virtual
//! keyboard.
//!
//! The rule: while the Caps Lock key is remapped, the lock is on only if keycap wrote the Caps
//! Lock press that turned it on (Shift+Caps Lock, say). keycap follows the lock from the LEDs
//! the desktop sets on the virtual keyboard (and, until the desktop has said anything, from the
//! lamp of a keyboard read as it is grabbed). When the lock is on and nothing keycap wrote asked
//! for it, keycap taps Caps Lock before the next key typed. The tap waits for a key because the
//! desktop may not have opened a freshly created virtual keyboard yet, and events written before
//! that are lost; a key typed proves it has. Pure; the reads and the writes belong to the reader.

use super::leds::Lamp;
use crate::{Edge, Key, Layout, Logical, OutEvent, Physical, Profile, RemapInput, Trigger, role};

/// Whether a rule of `profile` takes the Caps Lock key pressed alone, so pressing it no longer
/// toggles the lock. Without one it reaches the desktop as Caps Lock.
pub(crate) fn remaps_caps(profile: &Profile) -> bool {
    Trigger::parse("capslock")
        .ok()
        .is_some_and(|trigger| profile.rules.contains_key(&trigger))
}

/// What keycap knows of the Caps Lock lock and what the user asked of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CapsFix {
    /// Whether the desktop has set the LEDs of the virtual keyboard: from then on its word is
    /// the one kept, and a grab read no longer counts. Until then the lock is nothing yet, or
    /// the lamp of a keyboard read as it was grabbed.
    desktop_reported: bool,
    /// The lock as last reported, or as keycap's own writes are about to make it.
    lock: Lamp,
    /// The lock the user asked for through what keycap wrote: on after a Caps Lock press that
    /// turned it on, off again after one that turned it off. Off until then.
    want: Lamp,
}

impl Default for CapsFix {
    fn default() -> Self {
        Self {
            desktop_reported: false,
            lock: Lamp::Off,
            want: Lamp::Off,
        }
    }
}

impl CapsFix {
    /// The state after a keyboard was grabbed with its Caps Lock lamp `lamp`. Only until the
    /// desktop reports its LEDs: after that its word is the one kept.
    pub(crate) fn grabbed(self, lamp: Lamp) -> CapsFix {
        if self.desktop_reported {
            self
        } else {
            CapsFix { lock: lamp, ..self }
        }
    }

    /// The state after the desktop set the Caps Lock LED of the virtual keyboard to `lamp`.
    pub(crate) fn reported(self, lamp: Lamp) -> CapsFix {
        CapsFix {
            desktop_reported: true,
            lock: lamp,
            ..self
        }
    }

    /// The state after keycap wrote `batch` to the virtual keyboard: each Caps Lock press in it
    /// toggles the lock, and the lock it leaves is the one the user asked for.
    pub(crate) fn wrote(self, batch: &[OutEvent]) -> CapsFix {
        let caps = caps_lock().map(Logical);
        batch
            .iter()
            .filter(|e| matches!(e, OutEvent::Key { key, edge: Edge::Press } if Some(*key) == caps))
            .fold(self, |state, _| {
                let lock = flipped(state.lock);
                CapsFix {
                    lock,
                    want: lock,
                    ..state
                }
            })
    }

    /// Whether the lock is on although the user did not ask for it and the Caps Lock key cannot
    /// turn it off (`remapped`). The tap is written before the next typed key.
    pub(crate) fn stray(self, remapped: bool) -> bool {
        self.lock == Lamp::On && self.want == Lamp::Off && remapped
    }

    /// The state before `input` goes to the remapper, `remapped` being whether the Caps Lock
    /// key is taken by a rule, and the tap to write ahead of it: `pressed` for a key press,
    /// nothing for anything else.
    pub(crate) fn before(self, input: &RemapInput, remapped: bool) -> (CapsFix, Vec<OutEvent>) {
        match input {
            RemapInput::Key {
                layout,
                key: pressed,
                edge: Edge::Press,
                ..
            } => self.pressed(*layout, *pressed, remapped),
            _ => (self, Vec::new()),
        }
    }

    /// The state after `pressed` was pressed on a keyboard of `layout` while the Caps Lock key
    /// does `key`, and the tap to write before it. A modifier alone does not count (Shift
    /// pressed to type a capital proves nothing yet, and the tap comes before the letter it is
    /// for), nor does Caps Lock itself: Shift+Caps Lock is how the user sets the lock.
    pub(crate) fn pressed(
        self,
        layout: Layout,
        pressed: Physical,
        remapped: bool,
    ) -> (CapsFix, Vec<OutEvent>) {
        let typed = role(layout, pressed).is_none() && Some(pressed.0) != caps_lock();
        match (self.stray(remapped), typed) {
            (true, true) => (
                CapsFix {
                    lock: Lamp::Off,
                    ..self
                },
                caps_tap(),
            ),
            _ => (self, Vec::new()),
        }
    }
}

const fn flipped(lamp: Lamp) -> Lamp {
    match lamp {
        Lamp::Off => Lamp::On,
        Lamp::On => Lamp::Off,
    }
}

fn caps_lock() -> Option<Key> {
    Key::named("capslock")
}

/// Caps Lock pressed and released, each in its own frame.
fn caps_tap() -> Vec<OutEvent> {
    caps_lock().map_or_else(Vec::new, |key| {
        let key = |edge| OutEvent::Key {
            key: Logical(key),
            edge,
        };
        vec![
            key(Edge::Press),
            OutEvent::Sync,
            key(Edge::Release),
            OutEvent::Sync,
        ]
    })
}
