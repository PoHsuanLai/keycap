//! Caps Lock left on across a switch of remapper. The desktop keeps the lock state, and a
//! keyboard that was raw for a while may have toggled it. When a rule takes over the Caps Lock
//! key (Caps Lock as the input-method switch, say) the lock can no longer be turned off by
//! pressing it, so letters stay in capitals. keycap clears it once: it notes at the first grab
//! whether the keyboard's Caps Lock lamp is lit, and before the first key typed it taps Caps
//! Lock on the virtual keyboard. The tap waits for a key because the desktop may not have
//! opened a freshly created virtual keyboard yet, and events written before that are lost; a
//! key typed proves it has. Pure; the reads and the writes belong to the reader.

use super::events::{EV_KEY, InputEvent};
use super::leds::Lamp;
use crate::key::Key;
use crate::profile::{Profile, Trigger};
use crate::{Layout, Physical, role};

/// What the active profile does with the Caps Lock key pressed alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CapsKey {
    /// No rule: it reaches the desktop as Caps Lock, which toggles the lock.
    Passes,
    /// A rule takes it, so pressing it no longer toggles the lock.
    Remapped,
}

impl CapsKey {
    /// What `profile` does with a bare Caps Lock press.
    pub(crate) fn in_profile(profile: &Profile) -> CapsKey {
        let bare = Trigger::parse("capslock")
            .ok()
            .and_then(|trigger| profile.rules.get(&trigger));
        match bare {
            Some(_) => CapsKey::Remapped,
            None => CapsKey::Passes,
        }
    }
}

/// Where the one-time check of a lit Caps Lock stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum CapsFix {
    /// No keyboard has been grabbed yet.
    #[default]
    Unchecked,
    /// Caps Lock was lit and is remapped: the tap is written before the next typed key.
    Armed,
    /// Nothing more to do this run.
    Done,
}

impl CapsFix {
    /// The state after the first grab found Caps Lock `lamp` with the key `key`. Later grabs
    /// (a keyboard waking up) change nothing: a lock the user set on purpose stays set.
    pub(crate) fn grabbed(self, lamp: Lamp, key: CapsKey) -> CapsFix {
        match (self, lamp, key) {
            (CapsFix::Unchecked, Lamp::On, CapsKey::Remapped) => CapsFix::Armed,
            (CapsFix::Unchecked, _, _) => CapsFix::Done,
            (other, _, _) => other,
        }
    }

    /// The state after `key` was pressed on a keyboard of `layout`, and the events to write
    /// before it. A modifier alone does not count: Shift pressed to type a capital proves
    /// nothing yet, and the tap comes before the letter it is for.
    pub(crate) fn pressed(self, layout: Layout, key: Physical) -> (CapsFix, Vec<InputEvent>) {
        match self {
            CapsFix::Armed if role(layout, key).is_none() => (CapsFix::Done, caps_tap()),
            other => (other, Vec::new()),
        }
    }
}

/// Caps Lock pressed and released, each in its own frame.
fn caps_tap() -> Vec<InputEvent> {
    let key = |value| InputEvent {
        kind: EV_KEY,
        code: caps_lock_code(),
        value,
    };
    vec![key(1), InputEvent::SYNC, key(0), InputEvent::SYNC]
}

fn caps_lock_code() -> u16 {
    Key::named("capslock").map_or(0, Key::code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{builtin, resolve};

    fn physical(name: &str) -> Physical {
        Physical(Key::named(name).unwrap())
    }

    #[test]
    fn the_built_in_profile_leaves_caps_lock_alone_and_a_rule_takes_it() {
        let profiles = resolve(&[builtin().0]).0;
        assert_eq!(
            CapsKey::in_profile(profiles.default_profile()),
            CapsKey::Passes
        );
        let mine = crate::ConfigLayer::parse(
            "[profile.default]\nrules = [{ when = \"capslock\", then = \"ctrl+space\" }]\n",
            crate::ConfigSource::User,
        )
        .0;
        let profiles = resolve(&[builtin().0, mine]).0;
        assert_eq!(
            CapsKey::in_profile(profiles.default_profile()),
            CapsKey::Remapped
        );
    }

    #[test]
    fn a_shifted_caps_lock_rule_alone_does_not_count_as_taking_the_key() {
        let mine = crate::ConfigLayer::parse(
            "[profile.default]\nrules = [{ when = \"shift+capslock\", then = \"capslock\" }]\n",
            crate::ConfigSource::User,
        )
        .0;
        let profiles = resolve(&[builtin().0, mine]).0;
        assert_eq!(
            CapsKey::in_profile(profiles.default_profile()),
            CapsKey::Passes
        );
    }

    #[test]
    fn only_a_lit_lamp_with_a_remapped_key_arms_the_tap_and_only_at_the_first_grab() {
        use CapsFix::{Armed, Done, Unchecked};
        use CapsKey::{Passes, Remapped};
        use Lamp::{Off, On};
        const CASES: &[(&str, CapsFix, Lamp, CapsKey, CapsFix)] = &[
            ("lit and remapped", Unchecked, On, Remapped, Armed),
            ("lit but the key still toggles", Unchecked, On, Passes, Done),
            ("remapped but not lit", Unchecked, Off, Remapped, Done),
            ("neither", Unchecked, Off, Passes, Done),
            ("a later grab does not arm", Done, On, Remapped, Done),
            (
                "a later grab keeps an armed tap",
                Armed,
                Off,
                Remapped,
                Armed,
            ),
        ];
        for &(name, before, lamp, key, want) in CASES {
            assert_eq!(before.grabbed(lamp, key), want, "{name}");
        }
    }

    #[test]
    fn the_tap_comes_before_the_first_key_that_is_not_a_modifier() {
        let (state, tap) = CapsFix::Armed.pressed(Layout::Apple, physical("leftshift"));
        assert_eq!((state, tap), (CapsFix::Armed, vec![]), "Shift waits");
        let (state, tap) = CapsFix::Armed.pressed(Layout::Apple, physical("leftmeta"));
        assert_eq!((state, tap), (CapsFix::Armed, vec![]), "Command waits");
        let (state, tap) = CapsFix::Armed.pressed(Layout::Apple, physical("a"));
        assert_eq!(state, CapsFix::Done);
        let code = Key::named("capslock").unwrap().code();
        let key = |value| InputEvent {
            kind: EV_KEY,
            code,
            value,
        };
        assert_eq!(
            tap,
            [key(1), InputEvent::SYNC, key(0), InputEvent::SYNC],
            "a tap, written once"
        );
        let (state, tap) = state.pressed(Layout::Apple, physical("b"));
        assert_eq!((state, tap), (CapsFix::Done, vec![]));
    }

    #[test]
    fn nothing_is_written_unless_the_tap_is_armed() {
        for state in [CapsFix::Unchecked, CapsFix::Done] {
            assert_eq!(state.pressed(Layout::Apple, physical("a")), (state, vec![]));
        }
    }
}
