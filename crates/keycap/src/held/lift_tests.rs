use super::*;
use crate::event::{DeviceId, Edge, Logical, OutEvent, Physical};
use crate::key::Key;

const D0: DeviceId = DeviceId(0);
const CMD: At = (D0, Physical(Key::LEFTMETA));
const CMD_RIGHT: At = (D0, Physical(Key::RIGHTMETA));
const LEFT: At = (D0, Physical(Key::from_code(105)));
const CTRL: Logical = Logical(Key::LEFTCTRL);
const ALT: Logical = Logical(Key::LEFTALT);
const HOME: Logical = Logical(Key::from_code(102));

fn up(key: Logical) -> OutEvent {
    OutEvent::Key {
        key,
        edge: Edge::Release,
    }
}

fn down(key: Logical) -> OutEvent {
    OutEvent::Key {
        key,
        edge: Edge::Press,
    }
}

fn with_command() -> Held {
    Held::new().press(CMD, vec![CTRL]).0
}

#[test]
fn lift_releases_a_held_key_and_owner_release_puts_it_back() {
    let (held, out) = with_command().lift(LEFT, vec![CTRL]);
    assert_eq!(out, vec![up(CTRL)]);
    assert!(held.holds(CTRL) && !held.is_down(CTRL));
    let (held, out) = held.press(LEFT, vec![HOME]);
    assert_eq!(out, vec![down(HOME)]);
    let (held, out) = held.release(LEFT);
    assert_eq!(out, vec![up(HOME), down(CTRL)]);
    assert!(held.is_down(CTRL));
}

#[test]
fn lift_ignores_keys_nobody_holds_and_is_idempotent() {
    let (held, out) = with_command().lift(LEFT, vec![ALT, CTRL]);
    assert_eq!(out, vec![up(CTRL)], "ALT is held by nobody");
    let (held, out) = held.lift(LEFT, vec![CTRL]);
    assert!(out.is_empty());
    let (_, out) = held.unlift(LEFT);
    assert_eq!(out, vec![down(CTRL)], "pressed back once");
}

#[test]
fn releasing_a_lifted_keys_holder_writes_nothing_and_drops_the_lift() {
    let (held, _) = with_command().lift(LEFT, vec![CTRL]);
    let (held, out) = held.release(CMD);
    assert!(out.is_empty(), "CTRL is already up");
    assert!(
        held.lifts.is_empty(),
        "the lift of a key nobody holds is dropped"
    );
    let (held, out) = held.release(LEFT);
    assert!(out.is_empty(), "nothing to put back");
    assert!(held.is_empty());
}

#[test]
fn a_key_lifted_by_two_owners_returns_with_the_last() {
    let (held, _) = with_command().lift(LEFT, vec![CTRL]);
    let (held, out) = held.lift(CMD_RIGHT, vec![CTRL]);
    assert!(out.is_empty(), "already up");
    let (held, out) = held.unlift(LEFT);
    assert!(out.is_empty(), "the other owner still keeps it up");
    let (_, out) = held.unlift(CMD_RIGHT);
    assert_eq!(out, vec![down(CTRL)]);
}

#[test]
fn a_key_held_by_another_record_returns_when_the_owner_goes() {
    let (held, _) = with_command().lift(CMD_RIGHT, vec![CTRL]);
    let (held, _) = held.press(LEFT, vec![HOME]);
    let (held, out) = held.release(CMD_RIGHT);
    assert_eq!(out, vec![down(CTRL)], "Command is still held");
    assert!(held.is_down(HOME) && held.is_down(CTRL));
}

#[test]
fn pressing_a_lifted_key_writes_it_and_ends_the_lift() {
    let (held, _) = with_command().lift(LEFT, vec![CTRL]);
    let (held, out) = held.press(CMD_RIGHT, vec![CTRL]);
    assert_eq!(out, vec![down(CTRL)]);
    let (held, out) = held.unlift(LEFT);
    assert!(out.is_empty(), "already down again");
    assert!(held.is_down(CTRL));
}

#[test]
fn extend_presses_only_keys_not_already_down() {
    let (held, out) = with_command().extend(CMD, vec![CTRL, ALT]);
    assert_eq!(out, vec![down(ALT)], "CTRL is the record's own");
    let (held, out) = held.release(CMD);
    assert_eq!(out, vec![up(ALT), up(CTRL)]);
    assert!(held.is_empty());
}

#[test]
fn extend_leaves_keys_another_record_holds_to_that_record() {
    let (held, _) = Held::new()
        .press(CMD_RIGHT, vec![ALT])
        .0
        .press(CMD, vec![CTRL]);
    let (held, out) = held.extend(CMD, vec![ALT]);
    assert!(out.is_empty());
    let (_, out) = held.release(CMD);
    assert_eq!(out, vec![up(CTRL)], "ALT stays with the other record");
}

#[test]
fn drain_releases_lifted_holds_without_pressing_anything_back() {
    let (held, _) = with_command().lift(LEFT, vec![CTRL]);
    let (held, _) = held.press(LEFT, vec![HOME]);
    let (held, out) = held.drain();
    assert_eq!(out, vec![up(HOME)], "CTRL was already up");
    assert!(held.is_empty());
}

#[test]
fn device_gone_restores_keys_other_devices_still_hold() {
    let other = (DeviceId(1), Physical(Key::RIGHTMETA));
    let (held, _) = with_command()
        .press(other, vec![CTRL])
        .0
        .lift(LEFT, vec![CTRL]);
    let (held, _) = held.press(LEFT, vec![HOME]);
    let (held, out) = held.device_gone(D0);
    assert_eq!(
        out,
        vec![up(HOME), down(CTRL)],
        "D0's lift ends and D1's record still holds CTRL"
    );
    assert!(held.is_held(other) && held.is_down(CTRL));
}

#[test]
fn device_gone_does_not_press_back_keys_its_own_records_held() {
    let (held, _) = with_command().lift(LEFT, vec![CTRL]);
    let (held, _) = held.press(LEFT, vec![HOME]);
    let (held, out) = held.device_gone(D0);
    assert_eq!(out, vec![up(HOME)]);
    assert!(held.is_empty());
}
