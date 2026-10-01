use super::*;
use crate::key::Key;
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
    let cases: [(&Profile, &str, Option<RuleAction>); 10] = [
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
    ];
    for (i, (profile, when, want)) in cases.iter().enumerate() {
        assert_eq!(rule(profile, when), want.as_ref(), "case {i}: {when}");
    }
}

#[test]
fn defaults_hold_every_documented_default_rule() {
    let default_rules = builtin_profiles().resolve(&name("default")).rules.len();
    assert_eq!(default_rules, 21);
    let terminal_rules = builtin_profiles().resolve(&name("terminal")).rules.len();
    assert_eq!(
        terminal_rules,
        21 + 6,
        "6 new triggers; control+a and +e are overrides"
    );
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

#[test]
fn key_names_in_defaults_exist() {
    for k in ["backspace", "delete", "space", "tab", "f4"] {
        assert!(Key::named(k).is_some(), "{k}");
    }
}
