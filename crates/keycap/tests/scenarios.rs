//! Whole-core scripts against the shipped profiles: what the keyboard sends and what the wire
//! sees.

mod support;

use support::{
    Rig, focus, gone, pc_press, pc_release, press, release, release_all, repeat, resume,
};

#[test]
fn command_c_in_default_is_control_c() {
    Rig::new().expect(vec![
        (press(0, "leftmeta"), "+leftctrl SYN"),
        (press(0, "c"), "+c SYN"),
        (release(0, "c"), "-c SYN"),
        (release(0, "leftmeta"), "-leftctrl SYN"),
    ]);
}

#[test]
fn command_c_in_terminal_is_control_shift_c_and_keeps_control() {
    let mut rig = Rig::new();
    rig.expect(vec![
        (focus("terminal"), ""),
        (press(0, "leftmeta"), "+leftctrl SYN"),
        (press(0, "c"), "+leftshift +c SYN"),
        (release(0, "c"), "-c -leftshift SYN"),
        (release(0, "leftmeta"), "-leftctrl SYN"),
    ]);
    assert!(rig.down().is_empty());
}

#[test]
fn command_left_lifts_control_for_home_and_release_restores_it() {
    Rig::new().expect(vec![
        (press(0, "leftmeta"), "+leftctrl SYN"),
        (press(0, "left"), "-leftctrl +home SYN"),
        (release(0, "left"), "-home +leftctrl SYN"),
        (release(0, "leftmeta"), "-leftctrl SYN"),
    ]);
}

#[test]
fn command_left_released_command_first_never_brings_control_back() {
    Rig::new().expect(vec![
        (press(0, "leftmeta"), "+leftctrl SYN"),
        (press(0, "left"), "-leftctrl +home SYN"),
        (release(0, "leftmeta"), ""),
        (release(0, "left"), "-home SYN"),
    ]);
}

#[test]
fn option_left_lifts_alt_for_control_left() {
    Rig::new().expect(vec![
        (press(0, "leftalt"), "+leftalt SYN"),
        (press(0, "left"), "+f24 -f24 -leftalt +leftctrl +left SYN"),
        (release(0, "left"), "-left -leftctrl +leftalt SYN"),
        (release(0, "leftalt"), "+f24 -f24 -leftalt SYN"),
    ]);
}

#[test]
fn command_tab_keeps_the_switcher_open_until_command_is_released() {
    Rig::new().expect(vec![
        (press(0, "leftmeta"), "+leftctrl SYN"),
        (press(0, "tab"), "-leftctrl +leftalt +tab SYN"),
        (release(0, "tab"), "-tab SYN"),
        (press(0, "tab"), "+tab SYN"),
        (release(0, "tab"), "-tab SYN"),
        (release(0, "leftmeta"), "-leftalt SYN"),
    ]);
}

#[test]
fn command_shift_tab_reverses_without_taking_shift_from_the_held_key() {
    let mut rig = Rig::new();
    rig.expect(vec![
        (press(0, "leftshift"), "+leftshift SYN"),
        (press(0, "leftmeta"), "+leftctrl SYN"),
        (press(0, "tab"), "-leftctrl +leftalt +tab SYN"),
        (release(0, "tab"), "-tab SYN"),
        (release(0, "leftshift"), "-leftshift SYN"),
        (press(0, "tab"), "+tab SYN"),
        (release(0, "tab"), "-tab SYN"),
        (release(0, "leftmeta"), "-leftalt SYN"),
    ]);
    assert!(rig.down().is_empty());
}

#[test]
fn command_left_while_the_switcher_is_open_gives_alt_back_after() {
    Rig::new().expect(vec![
        (press(0, "leftmeta"), "+leftctrl SYN"),
        (press(0, "tab"), "-leftctrl +leftalt +tab SYN"),
        (release(0, "tab"), "-tab SYN"),
        (press(0, "left"), "-leftalt +home SYN"),
        (release(0, "left"), "-home +leftalt SYN"),
        (release(0, "leftmeta"), "+f24 -f24 -leftalt SYN"),
    ]);
}

#[test]
fn command_backspace_taps_the_sequence_and_writes_nothing_on_release() {
    Rig::new().expect(vec![
        (press(0, "leftmeta"), "+leftctrl SYN"),
        (
            press(0, "backspace"),
            "-leftctrl +leftshift +home -home -leftshift +delete -delete +leftctrl SYN",
        ),
        (release(0, "backspace"), ""),
        (release(0, "leftmeta"), "-leftctrl SYN"),
    ]);
}

#[test]
fn control_c_in_terminal_stays_control_c() {
    Rig::new().expect(vec![
        (focus("terminal"), ""),
        (press(0, "leftctrl"), "+leftctrl SYN"),
        (press(0, "c"), "+c SYN"),
        (release(0, "c"), "-c SYN"),
        (release(0, "leftctrl"), "-leftctrl SYN"),
    ]);
}

#[test]
fn control_a_in_default_is_home_and_in_terminal_control_a() {
    Rig::new().expect(vec![
        (press(0, "leftctrl"), "+leftmeta SYN"),
        (press(0, "a"), "+f24 -f24 -leftmeta +home SYN"),
        (release(0, "a"), "-home +leftmeta SYN"),
        (release(0, "leftctrl"), "+f24 -f24 -leftmeta SYN"),
        (focus("terminal"), ""),
        (press(0, "leftctrl"), "+leftctrl SYN"),
        (press(0, "a"), "+a SYN"),
        (release(0, "a"), "-a SYN"),
        (release(0, "leftctrl"), "-leftctrl SYN"),
    ]);
}

#[test]
fn command_held_across_a_focus_switch_uses_the_new_profile_for_the_next_key() {
    let mut rig = Rig::new();
    rig.expect(vec![
        (press(0, "leftmeta"), "+leftctrl SYN"),
        (press(0, "c"), "+c SYN"),
        (release(0, "c"), "-c SYN"),
        (focus("terminal"), ""),
        (press(0, "c"), "+leftshift +c SYN"),
        (focus("default"), ""),
        (release(0, "c"), "-c -leftshift SYN"),
        (press(0, "c"), "+c SYN"),
        (release(0, "c"), "-c SYN"),
        (release(0, "leftmeta"), "-leftctrl SYN"),
    ]);
    assert!(rig.down().is_empty());
}

#[test]
fn releasing_command_before_the_mapped_key_keeps_control_until_the_key_goes() {
    Rig::new().expect(vec![
        (focus("terminal"), ""),
        (press(0, "leftmeta"), "+leftctrl SYN"),
        (press(0, "c"), "+leftshift +c SYN"),
        (release(0, "leftmeta"), ""),
        (release(0, "c"), "-c -leftshift -leftctrl SYN"),
    ]);
}

#[test]
fn device_gone_mid_chord_releases_what_it_wrote_and_nothing_more() {
    let mut rig = Rig::new();
    rig.expect(vec![
        (press(0, "leftmeta"), "+leftctrl SYN"),
        (press(0, "left"), "-leftctrl +home SYN"),
        (gone(0), "-home SYN"),
        (release(0, "left"), ""),
        (release(0, "leftmeta"), ""),
    ]);
    assert!(rig.down().is_empty());
}

#[test]
fn device_gone_leaves_the_other_keyboards_keys_alone() {
    let mut rig = Rig::new();
    rig.expect(vec![
        (press(0, "leftmeta"), "+leftctrl SYN"),
        (press(1, "rightmeta"), ""),
        (press(1, "left"), "-leftctrl +home SYN"),
        (gone(1), "-home +leftctrl SYN"),
        (release(0, "leftmeta"), "-leftctrl SYN"),
    ]);
}

#[test]
fn release_all_mid_map_writes_the_release_and_later_releases_are_silent() {
    let mut rig = Rig::new();
    rig.expect(vec![
        (press(0, "leftmeta"), "+leftctrl SYN"),
        (press(0, "left"), "-leftctrl +home SYN"),
        (release_all(), "-home SYN"),
        (release_all(), ""),
        (release(0, "left"), ""),
        (release(0, "leftmeta"), ""),
    ]);
    assert!(rig.down().is_empty());
}

#[test]
fn escape_chord_drains_stops_mapping_and_resume_starts_again() {
    let mut rig = Rig::new();
    rig.expect(vec![
        (press(0, "leftshift"), "+leftshift SYN"),
        (press(0, "rightshift"), ""),
        (press(0, "esc"), "-leftshift SYN; GRABS-OFF; PAUSED"),
        (press(0, "a"), ""),
        (release(0, "a"), ""),
        (release(0, "esc"), ""),
        (release(0, "leftshift"), ""),
        (release(0, "rightshift"), ""),
        (resume(), "RUNNING"),
        (press(0, "a"), "+a SYN"),
        (release(0, "a"), "-a SYN"),
    ]);
    assert!(rig.down().is_empty());
}

#[test]
fn the_escape_chord_fires_again_after_resume() {
    Rig::new().expect(vec![
        (press(0, "leftshift"), "+leftshift SYN"),
        (press(0, "rightshift"), ""),
        (press(0, "esc"), "-leftshift SYN; GRABS-OFF; PAUSED"),
        (resume(), "RUNNING"),
        (press(0, "leftshift"), "+leftshift SYN"),
        (press(0, "rightshift"), ""),
        (press(0, "esc"), "-leftshift SYN; GRABS-OFF; PAUSED"),
    ]);
}

#[test]
fn a_pc_keyboards_alt_acts_as_command_and_its_meta_as_option() {
    Rig::new().expect(vec![
        (pc_press(1, "leftalt"), "+leftctrl SYN"),
        (pc_press(1, "left"), "-leftctrl +home SYN"),
        (pc_release(1, "left"), "-home +leftctrl SYN"),
        (pc_release(1, "leftalt"), "-leftctrl SYN"),
        (pc_press(1, "leftmeta"), "+leftalt SYN"),
        (
            pc_press(1, "left"),
            "+f24 -f24 -leftalt +leftctrl +left SYN",
        ),
        (pc_release(1, "left"), "-left -leftctrl +leftalt SYN"),
        (pc_release(1, "leftmeta"), "+f24 -f24 -leftalt SYN"),
    ]);
}

#[test]
fn the_same_physical_key_means_different_things_on_the_two_layouts() {
    Rig::new().expect(vec![
        (press(0, "leftalt"), "+leftalt SYN"),
        (pc_press(1, "leftalt"), "+leftctrl SYN"),
        (release(0, "leftalt"), "-leftalt SYN"),
        (pc_release(1, "leftalt"), "-leftctrl SYN"),
    ]);
}

#[test]
fn repeats_are_dropped_and_do_not_press() {
    Rig::new().expect(vec![
        (repeat(0, "a"), ""),
        (press(0, "a"), "+a SYN"),
        (repeat(0, "a"), ""),
        (repeat(0, "a"), ""),
        (release(0, "a"), "-a SYN"),
        (repeat(0, "leftmeta"), ""),
    ]);
}

#[test]
fn a_release_of_a_key_pressed_before_the_grab_writes_nothing() {
    Rig::new().expect(vec![(release(0, "a"), ""), (release(0, "leftmeta"), "")]);
}

#[test]
fn both_commands_held_release_control_only_with_the_last() {
    Rig::new().expect(vec![
        (press(0, "leftmeta"), "+leftctrl SYN"),
        (press(0, "rightmeta"), ""),
        (release(0, "leftmeta"), ""),
        (release(0, "rightmeta"), "-leftctrl SYN"),
        (press(0, "leftmeta"), "+leftctrl SYN"),
        (press(0, "rightmeta"), ""),
        (release(0, "rightmeta"), ""),
        (release(0, "leftmeta"), "-leftctrl SYN"),
    ]);
}

#[test]
fn both_commands_held_across_a_lift_keep_control_off_until_both_are_gone() {
    Rig::new().expect(vec![
        (press(0, "leftmeta"), "+leftctrl SYN"),
        (press(0, "rightmeta"), ""),
        (press(0, "left"), "-leftctrl +home SYN"),
        (release(0, "left"), "-home +leftctrl SYN"),
        (release(0, "leftmeta"), ""),
        (release(0, "rightmeta"), "-leftctrl SYN"),
    ]);
}

#[test]
fn an_unmatched_chord_writes_the_key_under_the_modifiers_already_down() {
    Rig::new().expect(vec![
        (press(0, "leftshift"), "+leftshift SYN"),
        (press(0, "a"), "+a SYN"),
        (release(0, "leftshift"), "-leftshift SYN"),
        (release(0, "a"), "-a SYN"),
    ]);
}

#[test]
fn single_stroke_rules_write_their_chord() {
    const CASES: &[(&str, &str, &str)] = &[
        ("command+q", "q", "-leftctrl +leftalt +f4 SYN"),
        ("command+space", "space", "-leftctrl +leftmeta +space SYN"),
        ("command+up", "up", "+home SYN"),
    ];
    for &(name, key, want) in CASES {
        let mut rig = Rig::new();
        rig.feed(press(0, "leftmeta"));
        assert_eq!(rig.feed(press(0, key)), want, "{name}");
    }
}

#[test]
fn a_lone_esc_after_resume_is_written_when_no_release_reached_the_remapper() {
    let mut rig = Rig::new();
    rig.expect(vec![
        (press(0, "leftshift"), "+leftshift SYN"),
        (press(0, "rightshift"), ""),
        (press(0, "esc"), "-leftshift SYN; GRABS-OFF; PAUSED"),
        (resume(), "RUNNING"),
        (press(0, "a"), "+a SYN"),
        (release(0, "a"), "-a SYN"),
        (press(0, "esc"), "+esc SYN"),
        (release(0, "esc"), "-esc SYN"),
    ]);
    assert_eq!(rig.status(), keycap::CoreStatus::Running);
}

#[test]
fn release_all_forgets_the_shifts_the_chord_saw() {
    Rig::new().expect(vec![
        (press(0, "leftshift"), "+leftshift SYN"),
        (press(0, "rightshift"), ""),
        (release_all(), "-leftshift SYN"),
        (press(0, "esc"), "+esc SYN"),
    ]);
}

#[test]
fn a_shift_on_a_vanished_keyboard_no_longer_arms_the_chord() {
    Rig::new().expect(vec![
        (press(0, "leftshift"), "+leftshift SYN"),
        (gone(0), "-leftshift SYN"),
        (press(1, "rightshift"), "+leftshift SYN"),
        (press(1, "esc"), "+esc SYN"),
    ]);
}

#[test]
fn the_two_shifts_may_be_on_different_keyboards() {
    Rig::new().expect(vec![
        (press(0, "leftshift"), "+leftshift SYN"),
        (press(1, "rightshift"), ""),
        (press(1, "esc"), "-leftshift SYN; GRABS-OFF; PAUSED"),
    ]);
}

#[test]
fn a_modifier_tap_with_no_lift_is_left_alone() {
    Rig::new().expect(vec![
        (press(0, "leftalt"), "+leftalt SYN"),
        (release(0, "leftalt"), "-leftalt SYN"),
        (press(0, "leftctrl"), "+leftmeta SYN"),
        (release(0, "leftctrl"), "-leftmeta SYN"),
    ]);
}

#[test]
fn option_then_a_plain_key_needs_no_neutral_tap() {
    Rig::new().expect(vec![
        (press(0, "leftalt"), "+leftalt SYN"),
        (press(0, "a"), "+a SYN"),
        (release(0, "a"), "-a SYN"),
        (release(0, "leftalt"), "-leftalt SYN"),
    ]);
}

#[test]
fn a_lift_after_another_key_was_written_needs_no_neutral_tap() {
    Rig::new().expect(vec![
        (press(0, "leftalt"), "+leftalt SYN"),
        (press(0, "a"), "+a SYN"),
        (press(0, "left"), "-leftalt +leftctrl +left SYN"),
        (release(0, "left"), "-left -leftctrl +leftalt SYN"),
        (release(0, "a"), "-a SYN"),
        (release(0, "leftalt"), "+f24 -f24 -leftalt SYN"),
    ]);
}

#[test]
fn command_left_gets_no_neutral_tap_because_a_control_tap_is_harmless() {
    let mut rig = Rig::new();
    rig.expect(vec![
        (press(0, "leftmeta"), "+leftctrl SYN"),
        (press(0, "left"), "-leftctrl +home SYN"),
        (release(0, "left"), "-home +leftctrl SYN"),
        (release(0, "leftmeta"), "-leftctrl SYN"),
    ]);
}
