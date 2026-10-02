//! `dist/keycap.plasma.toml` laid over the built-in mapping: the Plasma shortcuts Toshy gave the
//! Mac keyboard, and the Caps Lock input switch.

mod support;

use keycap::{ConfigSource, builtin, resolve};
use support::{Rig, focus, press, release};

const OVERLAY: &str = include_str!("../../../dist/keycap.plasma.toml");

fn plasma_rig() -> Rig {
    let (builtin_layer, builtin_warnings) = builtin();
    assert_eq!(builtin_warnings, vec![]);
    let (overlay, warnings) = keycap::ConfigLayer::parse(OVERLAY, ConfigSource::System);
    assert_eq!(warnings, vec![], "the shipped overlay parses cleanly");
    let (profiles, resolve_warnings) = resolve(&[builtin_layer, overlay]);
    assert_eq!(resolve_warnings, vec![]);
    Rig::with_profiles(profiles)
}

#[test]
fn command_space_opens_krunner() {
    plasma_rig().expect(vec![
        (press(0, "leftmeta"), "+leftctrl SYN"),
        (press(0, "space"), "-leftctrl +leftalt +space SYN"),
        (release(0, "space"), "-space -leftalt +leftctrl SYN"),
        (release(0, "leftmeta"), "-leftctrl SYN"),
    ]);
}

#[test]
fn caps_lock_is_the_input_switch_and_shift_caps_is_caps_lock() {
    let mut rig = plasma_rig();
    rig.expect(vec![
        (press(0, "capslock"), "+leftctrl +space SYN"),
        (release(0, "capslock"), "-space -leftctrl SYN"),
        (press(0, "leftshift"), "+leftshift SYN"),
        (press(0, "capslock"), "-leftshift +capslock SYN"),
        (release(0, "capslock"), "-capslock +leftshift SYN"),
        (release(0, "leftshift"), "-leftshift SYN"),
    ]);
    assert!(rig.down().is_empty());
}

#[test]
fn physical_control_space_reaches_fcitx5_as_control_space() {
    plasma_rig().expect(vec![
        (press(0, "leftctrl"), "+leftmeta SYN"),
        (
            press(0, "space"),
            "+f24 -f24 -leftmeta +leftctrl +space SYN",
        ),
        (release(0, "space"), "-space -leftctrl +leftmeta SYN"),
        (release(0, "leftctrl"), "+f24 -f24 -leftmeta SYN"),
    ]);
}

#[test]
fn command_e_opens_the_emoji_selector_and_command_control_f_goes_fullscreen() {
    plasma_rig().expect(vec![
        (press(0, "leftmeta"), "+leftctrl SYN"),
        (press(0, "e"), "-leftctrl +leftmeta +dot SYN"),
        (release(0, "e"), "-dot -leftmeta +leftctrl SYN"),
        (release(0, "leftmeta"), "-leftctrl SYN"),
    ]);
    let mut rig = plasma_rig();
    rig.feed(press(0, "leftctrl"));
    rig.feed(press(0, "leftmeta"));
    rig.feed(press(0, "f"));
    assert_eq!(rig.down(), ["f", "leftshift", "leftmeta"]);
}

#[test]
fn control_left_switches_desktops_but_stays_control_left_in_a_terminal() {
    let mut rig = plasma_rig();
    rig.feed(press(0, "leftctrl"));
    rig.feed(press(0, "left"));
    assert_eq!(rig.down(), ["leftctrl", "left", "leftmeta"]);

    let mut rig = plasma_rig();
    rig.expect(vec![
        (focus("terminal"), ""),
        (press(0, "leftctrl"), "+leftctrl SYN"),
        (press(0, "left"), "+left SYN"),
        (release(0, "left"), "-left SYN"),
        (release(0, "leftctrl"), "-leftctrl SYN"),
    ]);
}
