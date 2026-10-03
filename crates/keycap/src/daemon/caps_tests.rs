//! The pure Caps Lock rule: which LED reports, grabs and writes leave a lock to clear, and which
//! typed key the tap goes before.

use super::caps::{CapsFix, CapsKey, Stray};
use super::leds::Lamp;
use crate::{Edge, Key, Layout, Logical, OutEvent, Physical, builtin, resolve};

/// One thing that happens to the rule.
#[derive(Debug, Clone, Copy)]
enum Step {
    /// A keyboard grabbed with its Caps Lock lamp so.
    Grab(Lamp),
    /// The desktop set the Caps Lock LED of the virtual keyboard.
    Led(Lamp),
    /// keycap wrote a Caps Lock press from a rule (Shift+Caps Lock).
    Wrote,
    /// A key pressed on an Apple keyboard.
    Press(&'static str),
}

/// What a press wrote before itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Out {
    Quiet,
    Tap,
}

fn caps_press() -> Vec<OutEvent> {
    let key = Logical(Key::named("capslock").unwrap());
    let edge = |edge| OutEvent::Key { key, edge };
    vec![
        edge(Edge::Press),
        OutEvent::Sync,
        edge(Edge::Release),
        OutEvent::Sync,
    ]
}

/// What each press of `steps` wrote, with the Caps Lock key doing `key`.
fn run(key: CapsKey, steps: &[Step]) -> Vec<Out> {
    let mut state = CapsFix::default();
    let mut outs = Vec::new();
    for &step in steps {
        state = match step {
            Step::Grab(lamp) => state.grabbed(lamp),
            Step::Led(lamp) => state.reported(lamp),
            Step::Wrote => state.wrote(&caps_press()),
            Step::Press(name) => {
                let pressed = Physical(Key::named(name).unwrap());
                let (next, tap) = state.pressed(Layout::Apple, pressed, key);
                outs.push(match tap.as_slice() {
                    [] => Out::Quiet,
                    tap => {
                        assert_eq!(tap, caps_press(), "a tap is one press and one release");
                        Out::Tap
                    }
                });
                next
            }
        };
    }
    outs
}

#[test]
fn caps_lock_rule_table() {
    use CapsKey::{Passes, Remapped};
    use Lamp::{Off, On};
    use Out::{Quiet, Tap};
    use Step::{Grab, Led, Press, Wrote};
    type Case = (&'static str, CapsKey, &'static [Step], &'static [Out]);
    const CASES: &[Case] = &[
        (
            "the race: the lamp is off at the grab, the desktop's LED On comes after",
            Remapped,
            &[Grab(Off), Led(On), Press("a"), Press("b")],
            &[Tap, Quiet],
        ),
        (
            "a lamp lit at the grab, before the desktop said anything",
            Remapped,
            &[Grab(On), Press("a")],
            &[Tap],
        ),
        (
            "keycap restarted while the desktop's lock is on: the new device gets LED On",
            Remapped,
            &[Led(On), Press("a")],
            &[Tap],
        ),
        (
            "modifier-only presses and Caps Lock itself do not take the tap",
            Remapped,
            &[
                Led(On),
                Press("leftshift"),
                Press("leftmeta"),
                Press("leftalt"),
                Press("leftctrl"),
                Press("capslock"),
                Press("a"),
            ],
            &[Quiet, Quiet, Quiet, Quiet, Quiet, Tap],
        ),
        (
            "the user's Shift+Caps Lock, then LED On: left on",
            Remapped,
            &[Grab(Off), Wrote, Led(On), Press("a")],
            &[Quiet],
        ),
        (
            "Shift+Caps Lock again turns it off, and nothing is tapped",
            Remapped,
            &[Wrote, Led(On), Press("a"), Wrote, Led(Off), Press("b")],
            &[Quiet, Quiet],
        ),
        (
            "after the user turned it off, a lock that comes back is stray again",
            Remapped,
            &[Wrote, Led(On), Wrote, Led(Off), Led(On), Press("a")],
            &[Tap],
        ),
        (
            "Shift+Caps Lock on a stray lock turns it off without a tap",
            Remapped,
            &[
                Led(On),
                Press("leftshift"),
                Press("capslock"),
                Wrote,
                Press("a"),
            ],
            &[Quiet, Quiet, Quiet],
        ),
        (
            "a VT switch (LED Off, then On again) keeps the lock the user set",
            Remapped,
            &[Wrote, Led(On), Led(Off), Led(On), Press("a")],
            &[Quiet],
        ),
        (
            "the desktop re-syncs a stray lock after the tap was lost: tapped again",
            Remapped,
            &[Led(On), Press("a"), Led(Off), Led(On), Press("b")],
            &[Tap, Tap],
        ),
        (
            "a tap whose LED Off has not come yet is not repeated",
            Remapped,
            &[Led(On), Press("a"), Press("b"), Press("c")],
            &[Tap, Quiet, Quiet],
        ),
        (
            "LED On while the Caps Lock key still toggles: left on",
            Passes,
            &[Grab(On), Led(On), Press("a")],
            &[Quiet],
        ),
        (
            "LED Off before the key: nothing to clear",
            Remapped,
            &[Grab(On), Led(Off), Press("a")],
            &[Quiet],
        ),
        (
            "a grab read after the desktop spoke does not count",
            Remapped,
            &[Led(Off), Grab(On), Press("a")],
            &[Quiet],
        ),
        (
            "a later grab of a dark keyboard does not hide a stray lock",
            Remapped,
            &[Grab(On), Grab(Off), Led(On), Press("a")],
            &[Tap],
        ),
    ];
    for &(name, key, steps, want) in CASES {
        assert_eq!(run(key, steps), want, "{name}");
    }
}

#[test]
fn stray_is_a_lit_lock_nothing_asked_for_with_a_key_that_cannot_clear_it() {
    let lit = CapsFix::default().reported(Lamp::On);
    assert_eq!(lit.stray(CapsKey::Remapped), Stray::Lit);
    assert_eq!(lit.stray(CapsKey::Passes), Stray::Clear);
    let asked = CapsFix::default().wrote(&caps_press()).reported(Lamp::On);
    assert_eq!(asked.stray(CapsKey::Remapped), Stray::Clear);
    assert_eq!(CapsFix::default().stray(CapsKey::Remapped), Stray::Clear);
}

#[test]
fn only_caps_lock_presses_count_as_asking() {
    let shift = Logical(Key::named("leftshift").unwrap());
    let other = [
        OutEvent::Key {
            key: shift,
            edge: Edge::Press,
        },
        OutEvent::Sync,
    ];
    let state = CapsFix::default().wrote(&other).reported(Lamp::On);
    assert_eq!(
        state.stray(CapsKey::Remapped),
        Stray::Lit,
        "Shift asks nothing"
    );
    let twice = [caps_press(), caps_press()].concat();
    let state = CapsFix::default().wrote(&twice).reported(Lamp::On);
    assert_eq!(
        state.stray(CapsKey::Remapped),
        Stray::Lit,
        "on and off again leaves the user wanting it off"
    );
}

#[test]
fn the_built_in_profile_leaves_caps_lock_alone_and_a_rule_takes_it() {
    let profile = |toml: &str| {
        let mine = crate::ConfigLayer::parse(toml, crate::ConfigSource::User).0;
        CapsKey::in_profile(resolve(&[builtin().0, mine]).0.default_profile())
    };
    assert_eq!(profile(""), CapsKey::Passes);
    assert_eq!(
        profile("[profile.default]\nrules = [{ when = \"capslock\", then = \"ctrl+space\" }]\n"),
        CapsKey::Remapped
    );
    assert_eq!(
        profile(
            "[profile.default]\nrules = [{ when = \"shift+capslock\", then = \"capslock\" }]\n"
        ),
        CapsKey::Passes,
        "a shifted Caps Lock rule alone does not take the key"
    );
}
