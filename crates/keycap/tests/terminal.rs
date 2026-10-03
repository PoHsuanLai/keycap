//! The terminal and Ghostty profiles through the whole core: Command never becomes a bare
//! Ctrl+letter (a control character), physical Control stays Control, and Ghostty's keys.

mod support;

use support::{Rig, focus, press, release};

const LETTERS: [&str; 26] = [
    "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s",
    "t", "u", "v", "w", "x", "y", "z",
];

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

/// Whether exactly Ctrl and `key` are down: the bytes a shell reads as a control character.
fn is_bare_control(down: &[&str], key: &str) -> bool {
    down.len() == 2 && down.contains(&"leftctrl") && down.contains(&key)
}

#[test]
fn command_d_in_a_terminal_is_not_end_of_file() {
    Rig::new().expect(vec![
        (focus("terminal"), ""),
        (press(0, "leftmeta"), "+leftctrl SYN"),
        (press(0, "d"), "+leftshift +d SYN"),
        (release(0, "d"), "-d -leftshift SYN"),
        (release(0, "leftmeta"), "-leftctrl SYN"),
    ]);
}

#[test]
fn command_z_in_a_terminal_is_not_suspend() {
    Rig::new().expect(vec![
        (focus("terminal"), ""),
        (press(0, "leftmeta"), "+leftctrl SYN"),
        (press(0, "z"), "+leftshift +z SYN"),
        (release(0, "z"), "-z -leftshift SYN"),
        (release(0, "leftmeta"), "-leftctrl SYN"),
    ]);
}

#[test]
fn no_command_letter_is_a_bare_control_letter_in_a_terminal_or_ghostty() {
    for profile in ["terminal", "ghostty"] {
        for letter in LETTERS {
            let down = chord(profile, &["leftmeta", letter]);
            assert!(
                !is_bare_control(&down, letter),
                "{profile}: Cmd+{letter} wrote {down:?}"
            );
        }
    }
}

#[test]
fn command_q_still_quits_in_a_terminal() {
    assert_eq!(chord("terminal", &["leftmeta", "q"]), ["f4", "leftalt"]);
}

#[test]
fn control_c_d_and_z_stay_control_in_a_terminal() {
    for key in ["c", "d", "z"] {
        let mut rig = Rig::new();
        rig.expect(vec![
            (focus("terminal"), ""),
            (press(0, "leftctrl"), "+leftctrl SYN"),
            (press(0, key), &format!("+{key} SYN")),
            (release(0, key), &format!("-{key} SYN")),
            (release(0, "leftctrl"), "-leftctrl SYN"),
        ]);
    }
}

#[test]
fn the_terminal_chords_ported_from_the_terminal_conventions() {
    // (keys held before the last one, last key, what is down after the last press)
    type Case = (
        &'static [&'static str],
        &'static str,
        &'static [&'static str],
    );
    const CASES: &[Case] = &[
        (&["leftmeta"], "backspace", &["leftctrl", "u"]),
        (&["leftmeta"], "delete", &["leftctrl", "k"]),
        (&["leftmeta"], "minus", &["leftctrl", "minus"]),
        (&["leftmeta"], "equal", &["leftctrl", "leftshift", "equal"]),
        (
            &["leftmeta"],
            "leftbrace",
            &["leftctrl", "leftshift", "leftbrace"],
        ),
        (
            &["leftmeta"],
            "rightbrace",
            &["leftctrl", "leftshift", "rightbrace"],
        ),
        (
            &["leftmeta"],
            "backslash",
            &["leftctrl", "leftshift", "backslash"],
        ),
        (&["leftmeta"], "dot", &["leftctrl", "c"]),
        (&["leftmeta"], "left", &["home"]),
        (&["leftmeta"], "right", &["end"]),
        (&["leftshift", "leftmeta"], "left", &["leftctrl", "pageup"]),
        (
            &["leftshift", "leftmeta"],
            "right",
            &["leftctrl", "pagedown"],
        ),
        (&["leftalt"], "backspace", &["leftctrl", "w"]),
    ];
    for &(held, last, want) in CASES {
        let keys: Vec<&str> = held.iter().copied().chain([last]).collect();
        assert_eq!(chord("terminal", &keys), sorted(want), "{keys:?}");
    }
}

#[test]
fn option_delete_taps_escape_then_d_in_a_terminal() {
    Rig::new().expect(vec![
        (focus("terminal"), ""),
        (press(0, "leftalt"), "+leftalt SYN"),
        (
            press(0, "delete"),
            "+f24 -f24 -leftalt +esc -esc +d -d +leftalt SYN",
        ),
        (release(0, "delete"), ""),
        (release(0, "leftalt"), "+f24 -f24 -leftalt SYN"),
    ]);
}

#[test]
fn ghostty_command_d_splits_right_and_command_shift_d_splits_down() {
    assert_eq!(
        chord("ghostty", &["leftmeta", "d"]),
        sorted(&["leftctrl", "leftshift", "o"])
    );
    assert_eq!(
        chord("ghostty", &["leftshift", "leftmeta", "d"]),
        sorted(&["leftctrl", "leftshift", "e"])
    );
}

#[test]
fn ghostty_command_k_clears_the_screen_and_command_equal_zooms_in() {
    assert_eq!(
        chord("ghostty", &["leftmeta", "k"]),
        sorted(&["leftctrl", "l"])
    );
    assert_eq!(
        chord("ghostty", &["leftmeta", "equal"]),
        sorted(&["leftctrl", "equal"])
    );
}

#[test]
fn ghostty_command_option_i_opens_the_inspector() {
    assert_eq!(
        chord("ghostty", &["leftmeta", "leftalt", "i"]),
        sorted(&["leftctrl", "leftshift", "i"])
    );
}

#[test]
fn ghostty_keeps_the_terminal_chords_it_does_not_override() {
    assert_eq!(
        chord("ghostty", &["leftmeta", "c"]),
        sorted(&["leftctrl", "leftshift", "c"])
    );
    assert_eq!(chord("ghostty", &["leftmeta", "left"]), ["home"]);
}

#[test]
fn plain_terminal_does_not_get_the_ghostty_keys() {
    assert_eq!(
        chord("terminal", &["leftmeta", "k"]),
        sorted(&["leftctrl", "leftshift", "k"])
    );
}
