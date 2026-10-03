//! A Caps Lock left on, cleared before the first typed key: the reader's side, on a recording
//! emitter.

use super::tests::Rig;
use crate::Edge;
use crate::daemon::caps::{CapsFix, CapsKey};
use crate::daemon::events::{EV_KEY, InputEvent};
use crate::daemon::leds::Lamp;
use crate::key::Key;

/// The `(code, value)` pairs of the key events written so far, in order.
fn written(rig: &Rig) -> Vec<(u16, i32)> {
    rig.reader
        .emitter
        .batches
        .iter()
        .flatten()
        .filter(|e| *e != &InputEvent::SYNC && e.kind == EV_KEY)
        .map(|e| (e.code, e.value))
        .collect()
}

fn code(name: &str) -> u16 {
    Key::named(name).unwrap().code()
}

#[test]
fn a_lit_remapped_caps_lock_is_tapped_once_before_the_first_letter() {
    let mut rig = Rig::new();
    rig.reader.arm_caps_with(Lamp::On, CapsKey::Remapped);
    assert_eq!(rig.reader.caps, CapsFix::Armed);
    assert_eq!(
        written(&rig),
        [],
        "nothing is written before a key is typed"
    );

    rig.key("leftshift", Edge::Press);
    assert_eq!(written(&rig), [(code("leftshift"), 1)], "Shift alone waits");
    rig.key("a", Edge::Press);
    assert_eq!(
        written(&rig),
        [
            (code("leftshift"), 1),
            (code("capslock"), 1),
            (code("capslock"), 0),
            (code("a"), 1)
        ],
        "the tap comes before the letter"
    );
    rig.key("a", Edge::Release);
    rig.key("b", Edge::Press);
    let taps = written(&rig)
        .iter()
        .filter(|(c, _)| *c == code("capslock"))
        .count();
    assert_eq!(taps, 2, "one press and one release, once per run");
    assert_eq!(rig.reader.caps, CapsFix::Done);
}

#[test]
fn a_caps_lock_that_is_off_or_still_toggles_is_left_alone() {
    for (lamp, key) in [
        (Lamp::Off, CapsKey::Remapped),
        (Lamp::On, CapsKey::Passes),
        (Lamp::Off, CapsKey::Passes),
    ] {
        let mut rig = Rig::new();
        rig.reader.arm_caps_with(lamp, key);
        rig.key("a", Edge::Press);
        assert_eq!(written(&rig), [(code("a"), 1)], "{lamp:?} {key:?}");
    }
}

#[test]
fn a_later_grab_does_not_arm_a_second_tap() {
    let mut rig = Rig::new();
    rig.reader.arm_caps_with(Lamp::Off, CapsKey::Remapped);
    rig.reader.arm_caps_with(Lamp::On, CapsKey::Remapped);
    rig.key("a", Edge::Press);
    assert_eq!(written(&rig), [(code("a"), 1)]);
}
