//! A Caps Lock nobody asked for, cleared before the next typed key: the reader's side, on a
//! recording emitter. The LED reports come through `forward_leds`, as the desktop sends them.

use super::tests::Rig;
use crate::Edge;
use crate::Key;
use crate::daemon::events::{EV_KEY, EV_LED, InputEvent};
use crate::daemon::leds::{Lamp, Led, LedEvent};
use crate::daemon::settings::{Layers, parse_user};

/// Caps Lock as the input-method switch, Shift+Caps Lock as the real Caps Lock.
const SWITCH: &str = "[profile.default]\nrules = [\n  { when = \"capslock\", then = \"ctrl+space\" },\n  { when = \"shift+capslock\", then = \"capslock\" },\n]\n";

/// A rig whose user config is `toml`.
fn rig(toml: &str) -> Rig {
    let mut rig = Rig::new();
    let (profiles, warnings) = Layers::builtin_only()
        .with_user(parse_user(toml).0)
        .resolve();
    assert_eq!(warnings, []);
    rig.reader.profiles = profiles;
    rig
}

/// The desktop sets the Caps Lock LED of the virtual keyboard.
fn led(rig: &mut Rig, lamp: Lamp) {
    rig.reader.emitter.queued_leds = vec![LedEvent {
        led: Led::Caps,
        lamp,
    }];
    rig.reader.forward_leds();
}

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

fn taps(rig: &Rig) -> usize {
    written(rig)
        .iter()
        .filter(|&&(c, v)| c == code("capslock") && v == 1)
        .count()
}

#[test]
fn a_lock_the_desktop_reports_after_the_grab_is_tapped_off_before_the_next_letter() {
    let mut rig = rig(SWITCH);
    rig.reader.caps_grabbed(Lamp::Off);
    led(&mut rig, Lamp::On);
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
    assert_eq!(taps(&rig), 1, "one tap");
    led(&mut rig, Lamp::Off);
    rig.key("b", Edge::Release);
    rig.key("c", Edge::Press);
    assert_eq!(taps(&rig), 1, "still one tap once the lock is off");
}

#[test]
fn the_lock_set_with_shift_caps_lock_stays_and_goes_with_it_again() {
    let mut rig = rig(SWITCH);
    rig.reader.caps_grabbed(Lamp::Off);
    let shift_caps = |rig: &mut Rig| {
        rig.key("leftshift", Edge::Press);
        rig.key("capslock", Edge::Press);
        rig.key("capslock", Edge::Release);
        rig.key("leftshift", Edge::Release);
    };
    shift_caps(&mut rig);
    assert_eq!(taps(&rig), 1, "the rule wrote Caps Lock");
    led(&mut rig, Lamp::On);
    rig.key("a", Edge::Press);
    rig.key("a", Edge::Release);
    assert_eq!(taps(&rig), 1, "no tap: the user asked for the lock");

    shift_caps(&mut rig);
    assert_eq!(taps(&rig), 2, "the rule wrote Caps Lock again");
    led(&mut rig, Lamp::Off);
    rig.key("b", Edge::Press);
    assert_eq!(taps(&rig), 2, "off, and nothing more");
}

#[test]
fn a_lock_with_a_caps_lock_key_that_still_toggles_is_left_alone() {
    let mut rig = rig("");
    rig.reader.caps_grabbed(Lamp::On);
    led(&mut rig, Lamp::On);
    rig.key("a", Edge::Press);
    assert_eq!(written(&rig), [(code("a"), 1)]);
}

#[test]
fn the_caps_lock_led_is_still_copied_to_the_keyboards() {
    let mut rig = rig(SWITCH);
    led(&mut rig, Lamp::On);
    assert_eq!(
        rig.reader.lamps.events(),
        [InputEvent {
            kind: EV_LED,
            code: 1,
            value: 1
        }]
    );
}
