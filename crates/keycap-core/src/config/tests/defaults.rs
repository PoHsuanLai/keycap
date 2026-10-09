use super::*;
use crate::profile::{Hold, ModMap, Profile, RuleAction};
use crate::role::OutMod;

#[test]
fn defaults_toml_parses_with_zero_warnings() {
    let (layer, warnings) = builtin();
    assert_eq!(warnings, vec![]);
    let (_, resolve_warnings) = resolve(&[layer]);
    assert_eq!(resolve_warnings, vec![]);
}

#[test]
fn defaults_resolve_spot_checks() {
    let profiles = builtin_profiles();
    let default = profiles.resolve(&name("default"));
    let terminal = profiles.resolve(&name("terminal"));

    let mac = ModMap {
        command: OutMod::Ctrl,
        option: OutMod::Alt,
        control: OutMod::Super,
    };
    assert_eq!(default.modmap, mac);
    assert_eq!(
        terminal.modmap,
        ModMap {
            control: OutMod::Ctrl,
            ..mac
        }
    );

    // (profile, trigger, expected action); None = no rule, the key passes through.
    let ghostty = profiles.resolve(&name("ghostty"));
    let cases: [(&Profile, &str, Option<RuleAction>); 20] = [
        (default, "command+left", Some(map("home"))),
        (default, "command+c", None),
        (
            default,
            "command+tab",
            Some(RuleAction::Map(stroke("alt+tab"), Hold::WhileHeld)),
        ),
        (
            default,
            "command+backspace",
            Some(RuleAction::Sequence(vec![
                stroke("shift+home"),
                stroke("delete"),
            ])),
        ),
        (default, "control+a", Some(map("home"))),
        (terminal, "command+c", Some(map("ctrl+shift+c"))),
        (terminal, "option+left", Some(map("ctrl+left"))),
        (terminal, "command+left", Some(map("home"))),
        (terminal, "control+a", Some(map("ctrl+a"))),
        (terminal, "control+e", Some(map("ctrl+e"))),
        (terminal, "command+d", Some(map("ctrl+shift+d"))),
        (terminal, "command+z", Some(map("ctrl+shift+z"))),
        (terminal, "command+backspace", Some(map("ctrl+u"))),
        (terminal, "command+delete", Some(map("ctrl+k"))),
        (terminal, "command+shift+left", Some(map("ctrl+pageup"))),
        (terminal, "command+q", Some(map("alt+f4"))),
        (ghostty, "command+d", Some(map("ctrl+shift+o"))),
        (ghostty, "command+shift+d", Some(map("ctrl+shift+e"))),
        (ghostty, "command+c", Some(map("ctrl+shift+c"))),
        (ghostty, "command+left", Some(map("home"))),
    ];
    for (i, (profile, when, want)) in cases.iter().enumerate() {
        assert_eq!(rule(profile, when), want.as_ref(), "case {i}: {when}");
    }
}

#[test]
fn defaults_hold_every_documented_default_rule() {
    let profiles = builtin_profiles();
    assert_eq!(profiles.resolve(&name("default")).rules.len(), 25);
    assert_eq!(
        profiles.resolve(&name("terminal")).rules.len(),
        25 + 32,
        "25 letters (all but q), \\ [ ] - = . and Cmd+Delete are new triggers; the rest override"
    );
    assert_eq!(
        profiles.resolve(&name("ghostty")).rules.len(),
        25 + 32 + 2,
        "Cmd+Shift+D and Cmd+Option+I are new; Cmd+D, K and = override the terminal's"
    );
}

#[test]
fn ghostty_inherits_terminal_and_the_modmap() {
    let profiles = builtin_profiles();
    assert_eq!(
        profiles.resolve(&name("ghostty")).modmap,
        profiles.resolve(&name("terminal")).modmap
    );
}

#[test]
fn the_filemanager_profile_overrides_five_rules_and_adds_two() {
    let profiles = builtin_profiles();
    let files = profiles.resolve(&name("filemanager"));
    assert_eq!(files.rules.len(), 25 + 2);
    assert_eq!(
        rule(files, "command+backspace"),
        Some(&map("delete")),
        "Cmd+Backspace trashes, it does not select to the line start"
    );
    assert_eq!(rule(files, "command+left"), Some(&map("alt+left")));
    assert_eq!(files.modmap, profiles.resolve(&name("default")).modmap);
}

#[test]
fn an_unknown_profile_resolves_to_default() {
    let profiles = builtin_profiles();
    assert_eq!(
        profiles.resolve(&name("firefox")),
        profiles.resolve(&name("default"))
    );
    assert_eq!(
        profiles.resolve(&name("firefox")),
        profiles.default_profile()
    );
}

#[test]
fn no_layers_still_has_a_default() {
    let (profiles, warnings) = resolve(&[]);
    assert_eq!(warnings, vec![]);
    assert_eq!(profiles.default_profile().modmap, ModMap::IDENTITY);
    assert!(profiles.default_profile().rules.is_empty());
}
