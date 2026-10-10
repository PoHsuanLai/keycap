//! The `keymap-aware` profile against the `terminal` one through the whole core: a foreign
//! terminal still gets Command as Ctrl+Shift, an app that reads the keymap itself gets Super.

use crate::support::{Rig, focus, press};

/// What is down on the wire, sorted, after `keys` are pressed in order with `profile` focused.
fn chord(profile: &str, keys: &[&str]) -> Vec<&'static str> {
    let mut rig = Rig::new();
    rig.feed(focus(profile));
    keys.iter().for_each(|k| drop(rig.feed(press(0, k))));
    let mut down = rig.down();
    down.sort_unstable();
    down
}

/// `names` sorted, to compare with `chord`.
fn sorted(names: &[&'static str]) -> Vec<&'static str> {
    let mut names = names.to_vec();
    names.sort_unstable();
    names
}

#[test]
fn a_foreign_terminal_still_writes_command_d_as_ctrl_shift_d() {
    assert_eq!(
        chord("terminal", &["leftmeta", "d"]),
        sorted(&["leftctrl", "leftshift", "d"])
    );
}

#[test]
fn a_keymap_aware_app_gets_super_d_and_super_shift_d_unchanged() {
    assert_eq!(
        chord("keymap-aware", &["leftmeta", "d"]),
        sorted(&["leftmeta", "d"])
    );
    assert_eq!(
        chord("keymap-aware", &["leftshift", "leftmeta", "d"]),
        sorted(&["leftmeta", "leftshift", "d"])
    );
}

#[test]
fn the_two_splits_differ_for_a_keymap_aware_app_and_collapse_in_a_terminal() {
    let split = |profile: &str| {
        (
            chord(profile, &["leftmeta", "d"]),
            chord(profile, &["leftshift", "leftmeta", "d"]),
        )
    };
    let (right, down) = split("keymap-aware");
    assert_ne!(right, down);
    let (right, down) = split("terminal");
    assert_eq!(right, down, "the information the terminal profile loses");
}

#[test]
fn a_keymap_aware_app_gets_the_editing_chords_as_typed() {
    for (held, key, want) in [
        ("leftmeta", "left", sorted(&["leftmeta", "left"])),
        ("leftmeta", "backspace", sorted(&["leftmeta", "backspace"])),
        ("leftalt", "left", sorted(&["leftalt", "left"])),
        ("leftctrl", "a", sorted(&["leftctrl", "a"])),
    ] {
        assert_eq!(chord("keymap-aware", &[held, key]), want, "{held}+{key}");
    }
}

#[test]
fn a_keymap_aware_app_keeps_the_desktops_window_switch() {
    assert_eq!(
        chord("keymap-aware", &["leftmeta", "tab"]),
        sorted(&["leftalt", "tab"])
    );
}
