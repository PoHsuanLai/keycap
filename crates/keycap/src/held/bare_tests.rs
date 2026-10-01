use super::bare::NEUTRAL;
use super::*;
use crate::event::{DeviceId, Edge, Logical, OutEvent, Physical};
use crate::key::Key;

const D0: DeviceId = DeviceId(0);
const OPTION: At = (D0, Physical(Key::LEFTALT));
const LEFT: At = (D0, Physical(Key::from_code(105)));
const LETTER: At = (D0, Physical(Key::from_code(30)));
const ALT: Logical = Logical(Key::LEFTALT);
const SUPER: Logical = Logical(Key::LEFTMETA);
const CTRL: Logical = Logical(Key::LEFTCTRL);
const HOME: Logical = Logical(Key::from_code(102));

fn down(key: Logical) -> OutEvent {
    OutEvent::Key {
        key,
        edge: Edge::Press,
    }
}

fn up(key: Logical) -> OutEvent {
    OutEvent::Key {
        key,
        edge: Edge::Release,
    }
}

/// The two events of a neutral tap.
fn tap() -> [OutEvent; 2] {
    [down(NEUTRAL), up(NEUTRAL)]
}

#[test]
fn lifting_a_bare_alt_or_super_taps_the_neutral_key_before_releasing_it() {
    for (name, key) in [("alt", ALT), ("super", SUPER)] {
        let (held, _) = Held::new().press(OPTION, vec![key]);
        let (held, out) = held.lift(LEFT, vec![key]);
        assert_eq!(out, [tap()[0], tap()[1], up(key)], "{name}");
        assert!(!held.is_down(NEUTRAL) && !held.holds(NEUTRAL), "{name}");
    }
}

#[test]
fn lifting_a_modifier_that_is_not_alt_or_super_writes_no_tap() {
    let (held, _) = Held::new().press(OPTION, vec![CTRL]);
    let (_, out) = held.lift(LEFT, vec![CTRL]);
    assert_eq!(out, vec![up(CTRL)]);
}

#[test]
fn lifting_an_alt_after_another_key_was_written_writes_no_tap() {
    let (held, _) = Held::new().press(OPTION, vec![ALT]);
    let (held, _) = held.press(LETTER, vec![HOME]);
    let (_, out) = held.lift(LEFT, vec![ALT]);
    assert_eq!(out, vec![up(ALT)]);
}

#[test]
fn releasing_a_bare_alt_the_person_pressed_writes_no_tap() {
    let (held, _) = Held::new().press(OPTION, vec![ALT]);
    let (_, out) = held.release(OPTION);
    assert_eq!(out, vec![up(ALT)]);
}

#[test]
fn an_alt_put_back_by_a_lift_is_tapped_over_when_released_bare() {
    let (held, _) = Held::new().press(OPTION, vec![ALT]);
    let (held, _) = held.lift(LEFT, vec![ALT]);
    let (held, out) = held.release(LEFT);
    assert_eq!(out, vec![down(ALT)]);
    let (held, out) = held.release(OPTION);
    assert_eq!(out, vec![tap()[0], tap()[1], up(ALT)]);
    assert!(held.is_empty());
}

#[test]
fn a_key_pressed_after_the_put_back_ends_the_bare_state() {
    let (held, _) = Held::new().press(OPTION, vec![ALT]);
    let (held, _) = held.lift(LEFT, vec![ALT]);
    let (held, _) = held.release(LEFT);
    let (held, _) = held.press(LETTER, vec![HOME]);
    let (_, out) = held.release(OPTION);
    assert_eq!(out, vec![up(ALT)]);
}

#[test]
fn one_tap_covers_every_bare_modifier_released_with_it() {
    let (held, _) = Held::new().press(OPTION, vec![ALT, SUPER]);
    let (held, _) = held.lift(LEFT, vec![ALT, SUPER]);
    let (held, _) = held.release(LEFT);
    let (_, out) = held.release(OPTION);
    assert_eq!(out, vec![tap()[0], tap()[1], up(SUPER), up(ALT)]);
}

#[test]
fn drain_with_a_stray_bare_modifier_taps_and_forgets_it() {
    let (held, _) = Held::new().press(OPTION, vec![ALT]);
    let (held, _) = held.lift(LEFT, vec![ALT]);
    let (held, _) = held.unlift(LEFT);
    let (held, out) = held.drain();
    assert_eq!(out, vec![tap()[0], tap()[1], up(ALT)]);
    assert!(held.is_empty() && held.bare.is_empty());
}
