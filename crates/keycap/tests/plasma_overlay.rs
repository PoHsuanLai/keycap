//! `dist/keycap.plasma.toml` laid over the built-in mapping: the Plasma shortcuts Toshy gave the
//! Mac keyboard.

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
fn command_control_space_opens_the_emoji_selector_and_command_control_f_goes_fullscreen() {
    let mut rig = plasma_rig();
    rig.feed(press(0, "leftctrl"));
    rig.feed(press(0, "leftmeta"));
    rig.feed(press(0, "space"));
    assert_eq!(rig.down(), ["dot", "leftmeta"]);
    let mut rig = plasma_rig();
    rig.feed(press(0, "leftctrl"));
    rig.feed(press(0, "leftmeta"));
    rig.feed(press(0, "f"));
    assert_eq!(rig.down(), ["f", "leftshift", "leftmeta"]);
}

#[test]
fn control_left_and_right_switch_desktops_in_every_profile_including_terminals() {
    for profile in ["default", "terminal", "ghostty", "filemanager"] {
        for (arrow, name) in [("left", "left"), ("right", "right")] {
            let mut rig = plasma_rig();
            rig.feed(focus(profile));
            rig.feed(press(0, "leftctrl"));
            rig.feed(press(0, arrow));
            assert_eq!(
                rig.down(),
                if name == "left" {
                    ["leftctrl", "left", "leftmeta"]
                } else {
                    ["leftctrl", "right", "leftmeta"]
                },
                "{profile}: Ctrl+{name}"
            );
        }
    }
}

#[test]
fn control_left_in_a_terminal_is_ctrl_super_left_with_a_clean_release() {
    plasma_rig().expect(vec![
        (focus("terminal"), ""),
        (press(0, "leftctrl"), "+leftctrl SYN"),
        (press(0, "left"), "+leftmeta +left SYN"),
        (release(0, "left"), "-left -leftmeta SYN"),
        (release(0, "leftctrl"), "-leftctrl SYN"),
    ]);
}

#[test]
fn command_h_hides_in_a_terminal_too() {
    for profile in ["terminal", "ghostty"] {
        let mut rig = plasma_rig();
        rig.feed(focus(profile));
        rig.feed(press(0, "leftmeta"));
        rig.feed(press(0, "h"));
        assert_eq!(rig.down(), ["pagedown", "leftmeta"], "{profile}: Cmd+H");
    }
}

#[test]
fn under_the_plasma_config_no_command_letter_is_a_bare_control_letter_in_a_terminal() {
    for profile in ["terminal", "ghostty"] {
        for letter in "abcdefghijklmnopqrstuvwxyz".chars().map(String::from) {
            let mut rig = plasma_rig();
            rig.feed(focus(profile));
            rig.feed(press(0, "leftmeta"));
            rig.feed(press(0, &letter));
            let down = rig.down();
            let bare = down.len() == 2 && down.contains(&"leftctrl") && down.contains(&&*letter);
            assert!(!bare, "{profile}: Cmd+{letter} wrote {down:?}");
        }
    }
}

/// The Caps Lock block of `docs/personal-config.md`, as a user layer over the built-ins.
#[test]
fn the_documented_personal_caps_lock_block_works() {
    let docs = include_str!("../../../docs/personal-config.md");
    let start = docs.find("```toml\n[profile.default]").expect("the block") + 8;
    let block = &docs[start
        ..docs[start..]
            .find("```")
            .map(|n| start + n)
            .expect("its end")];
    let (builtin_layer, _) = builtin();
    let (user, warnings) = keycap::ConfigLayer::parse(block, ConfigSource::User);
    assert_eq!(warnings, vec![]);
    let mut rig = Rig::with_profiles(resolve(&[builtin_layer, user]).0);
    rig.expect(vec![
        (press(0, "capslock"), "+leftctrl +space SYN"),
        (release(0, "capslock"), "-space -leftctrl SYN"),
        (press(0, "leftshift"), "+leftshift SYN"),
        (press(0, "capslock"), "-leftshift +capslock SYN"),
        (release(0, "capslock"), "-capslock +leftshift SYN"),
        (release(0, "leftshift"), "-leftshift SYN"),
    ]);
}

/// The shipped Plasma file with the personal blocks of `docs/personal-config.md` over it: what
/// `/etc/keycap/keycap.toml` holds on the machine this was written for.
#[test]
fn the_plasma_file_with_the_personal_blocks_does_what_a_mac_user_expects() {
    let docs = include_str!("../../../docs/personal-config.md");
    let fenced = |from: &str| -> String {
        let start = docs.find(from).expect("the block");
        docs[start..start + docs[start..].find("```").expect("its end")].to_owned()
    };
    // The device block, then the Caps Lock rules the docs say to add to the default profile.
    let personal = format!(
        "{}\n{}",
        fenced("[[device]]\nmatch"),
        fenced("[profile.default]\nrules = [\n  { when = \"capslock\"")
    );
    let personal = personal.as_str();
    let (builtin_layer, _) = builtin();
    let (plasma, warnings) = keycap::ConfigLayer::parse(OVERLAY, ConfigSource::System);
    assert_eq!(warnings, vec![]);
    let (mine, warnings) = keycap::ConfigLayer::parse(personal, ConfigSource::System);
    assert_eq!(warnings, vec![]);
    let (profiles, warnings) = resolve(&[builtin_layer, plasma, mine]);
    assert_eq!(warnings, vec![]);

    let mut rig = Rig::with_profiles(profiles);
    rig.expect(vec![
        // Caps Lock switches the input method, Shift+Caps Lock is the real one.
        (press(0, "capslock"), "+leftctrl +space SYN"),
        (release(0, "capslock"), "-space -leftctrl SYN"),
        (press(0, "leftshift"), "+leftshift SYN"),
        (press(0, "capslock"), "-leftshift +capslock SYN"),
        (release(0, "capslock"), "-capslock +leftshift SYN"),
        (release(0, "leftshift"), "-leftshift SYN"),
        // In a terminal: Cmd+D is not EOF, Ctrl+C is still Ctrl+C, Ctrl+Left changes desktop.
        (focus("terminal"), ""),
        (press(0, "leftmeta"), "+leftctrl SYN"),
        (press(0, "d"), "+leftshift +d SYN"),
        (release(0, "d"), "-d -leftshift SYN"),
        (release(0, "leftmeta"), "-leftctrl SYN"),
        (press(0, "leftctrl"), "+leftctrl SYN"),
        (press(0, "c"), "+c SYN"),
        (release(0, "c"), "-c SYN"),
        (press(0, "left"), "+leftmeta +left SYN"),
        (release(0, "left"), "-left -leftmeta SYN"),
        (release(0, "leftctrl"), "-leftctrl SYN"),
        // In Ghostty: Cmd+D splits right.
        (focus("ghostty"), ""),
        (press(0, "leftmeta"), "+leftctrl SYN"),
        (press(0, "d"), "+leftshift +o SYN"),
        (release(0, "d"), "-o -leftshift SYN"),
        (release(0, "leftmeta"), "-leftctrl SYN"),
    ]);
}
