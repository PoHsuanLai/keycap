use std::path::PathBuf;

use keycap_protocol::{ProfileName, Warning};

use super::*;
use crate::key::Key;
use crate::profile::{Action, Hold, ModMap, Profile, Profiles, Stroke, Trigger};
use crate::role::OutMod;

fn name(s: &str) -> ProfileName {
    ProfileName::parse(s).expect("test profile names are valid")
}

fn trigger(s: &str) -> Trigger {
    Trigger::parse(s).expect("test triggers are valid")
}

fn stroke(s: &str) -> Stroke {
    Stroke::parse(s).expect("test strokes are valid")
}

fn map(s: &str) -> Action {
    Action::Map(stroke(s), Hold::Restore)
}

fn layer(text: &str) -> Layer {
    let (layer, warnings) = Layer::parse(text, Source::System);
    assert_eq!(warnings, vec![], "fixture must parse cleanly: {text}");
    layer
}

fn resolved(texts: &[&str]) -> (Profiles, Vec<Warning>) {
    let layers: Vec<Layer> = texts.iter().map(|t| layer(t)).collect();
    resolve(&layers)
}

fn rule<'a>(p: &'a Profile, when: &str) -> Option<&'a Action> {
    p.rules.get(&trigger(when))
}

fn keys(warnings: &[Warning]) -> Vec<&str> {
    warnings.iter().map(|w| w.key.as_str()).collect()
}

fn builtin_profiles() -> Profiles {
    let (layer, warnings) = builtin();
    assert_eq!(warnings, vec![]);
    resolve(&[layer]).0
}

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
    let cases: [(&Profile, &str, Option<Action>); 10] = [
        (default, "command+left", Some(map("home"))),
        (default, "command+c", None),
        (
            default,
            "command+tab",
            Some(Action::Map(stroke("alt+tab"), Hold::WhileHeld)),
        ),
        (
            default,
            "command+backspace",
            Some(Action::Sequence(vec![
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
fn later_layers_override_per_field_and_per_trigger() {
    let base = r#"
        [profile.default]
        modmap = { command = "ctrl", option = "alt", control = "super" }
        rules = [
          { when = "command+left", then = "home" },
          { when = "command+right", then = "end" },
        ]
    "#;
    let over = r#"
        [profile.default]
        modmap = { option = "super" }
        rules = [{ when = "command+left", then = "ctrl+home" }]
    "#;
    let (profiles, warnings) = resolved(&[base, over]);
    assert_eq!(warnings, vec![]);
    let p = profiles.default_profile();
    assert_eq!(
        p.modmap,
        ModMap {
            command: OutMod::Ctrl,
            option: OutMod::Super,
            control: OutMod::Super
        }
    );
    assert_eq!(rule(p, "command+left"), Some(&map("ctrl+home")));
    assert_eq!(
        rule(p, "command+right"),
        Some(&map("end")),
        "untouched trigger stays"
    );

    let (reversed, _) = resolved(&[over, base]);
    assert_eq!(
        rule(reversed.default_profile(), "command+left"),
        Some(&map("home"))
    );
    assert_eq!(reversed.default_profile().modmap.option, OutMod::Alt);
}

#[test]
fn a_later_layer_can_add_a_profile_and_change_an_inherited_one() {
    let base = r#"
        [profile.default]
        rules = [{ when = "command+q", then = "alt+f4" }]
        [profile.terminal]
        rules = [{ when = "command+c", then = "ctrl+shift+c" }]
    "#;
    let over = r#"
        [profile.terminal]
        rules = [{ when = "command+c", then = "ctrl+insert" }]
        [profile.editor]
        inherits = "terminal"
    "#;
    let (profiles, warnings) = resolved(&[base, over]);
    assert_eq!(warnings, vec![]);
    let editor = profiles.resolve(&name("editor"));
    assert_eq!(rule(editor, "command+c"), Some(&map("ctrl+insert")));
    assert_eq!(rule(editor, "command+q"), Some(&map("alt+f4")));
}

#[test]
fn inheritance_flattens_transitively_child_over_parent() {
    let text = r#"
        [profile.default]
        modmap = { command = "ctrl", option = "alt", control = "super" }
        rules = [
          { when = "left", then = "a" },
          { when = "right", then = "b" },
          { when = "up", then = "c" },
        ]
        [profile.mid]
        inherits = "default"
        modmap = { control = "ctrl" }
        rules = [{ when = "right", then = "d" }, { when = "down", then = "e" }]
        [profile.leaf]
        inherits = "mid"
        modmap = { option = "super" }
        rules = [{ when = "up", then = "f" }]
    "#;
    let (profiles, warnings) = resolved(&[text]);
    assert_eq!(warnings, vec![]);
    let leaf = profiles.resolve(&name("leaf"));
    assert_eq!(
        leaf.modmap,
        ModMap {
            command: OutMod::Ctrl,
            option: OutMod::Super,
            control: OutMod::Ctrl
        }
    );
    const CASES: &[(&str, &str)] = &[("left", "a"), ("right", "d"), ("up", "f"), ("down", "e")];
    for &(when, then) in CASES {
        assert_eq!(rule(leaf, when), Some(&map(then)), "{when}");
    }
    assert_eq!(profiles.resolve(&name("mid")).rules.len(), 4);
}

#[test]
fn a_profile_without_inherits_inherits_default() {
    let text = r#"
        [profile.default]
        rules = [{ when = "left", then = "home" }]
        [profile.plain]
        rules = [{ when = "right", then = "end" }]
    "#;
    let (profiles, warnings) = resolved(&[text]);
    assert_eq!(warnings, vec![]);
    assert_eq!(profiles.resolve(&name("plain")).rules.len(), 2);
}

#[test]
fn inheritance_problems_fall_back_to_default_with_one_warning_each() {
    struct Case {
        label: &'static str,
        text: &'static str,
        warned: &'static [&'static str],
        /// The profile checked, and whether it still has default's `left` rule.
        profile: &'static str,
        has_default_rule: bool,
    }
    const CASES: &[Case] = &[
        Case {
            label: "unknown parent",
            text: r#"[profile.a]
                     inherits = "nowhere"
                     rules = [{ when = "up", then = "c" }]"#,
            warned: &["profile.a.inherits"],
            profile: "a",
            has_default_rule: true,
        },
        Case {
            label: "self parent",
            text: r#"[profile.a]
                     inherits = "a""#,
            warned: &["profile.a.inherits"],
            profile: "a",
            has_default_rule: true,
        },
        Case {
            label: "two-profile cycle",
            text: r#"[profile.a]
                     inherits = "b"
                     [profile.b]
                     inherits = "a""#,
            warned: &["profile.a.inherits", "profile.b.inherits"],
            profile: "a",
            has_default_rule: true,
        },
        Case {
            label: "a profile leading into a cycle keeps its place in the chain",
            text: r#"[profile.c]
                     inherits = "a"
                     rules = [{ when = "up", then = "c" }]
                     [profile.a]
                     inherits = "b"
                     [profile.b]
                     inherits = "a""#,
            warned: &["profile.a.inherits", "profile.b.inherits"],
            profile: "c",
            has_default_rule: true,
        },
        Case {
            label: "default inherits nothing",
            text: r#"[profile.default]
                     inherits = "a"
                     [profile.a]
                     inherits = "default""#,
            warned: &["profile.default.inherits"],
            profile: "a",
            has_default_rule: true,
        },
    ];
    let default_rule = r#"[profile.default]
        rules = [{ when = "left", then = "home" }]
    "#;
    for case in CASES {
        let (profiles, warnings) = resolved(&[default_rule, case.text]);
        assert_eq!(keys(&warnings), case.warned, "{}", case.label);
        let p = profiles.resolve(&name(case.profile));
        assert_eq!(
            rule(p, "left").is_some(),
            case.has_default_rule,
            "{}",
            case.label
        );
    }
}

#[test]
fn an_empty_then_list_swallows_an_inherited_rule() {
    let text = r#"
        [profile.default]
        rules = [{ when = "command+q", then = "alt+f4" }]
        [profile.kiosk]
        rules = [{ when = "command+q", then = [] }]
    "#;
    let (profiles, warnings) = resolved(&[text]);
    assert_eq!(warnings, vec![]);
    assert_eq!(
        rule(profiles.resolve(&name("kiosk")), "command+q"),
        Some(&Action::Swallow)
    );
    assert_eq!(
        rule(profiles.resolve(&name("default")), "command+q"),
        Some(&map("alt+f4"))
    );
}

#[test]
fn a_bad_key_in_one_rule_costs_only_that_rule() {
    let text = r#"
        [profile.t]
        modmap = { command = "banana", control = "ctrl" }
        rules = [
          { when = "command+left", then = "home" },
          { when = "command+nope", then = "home" },
          { when = "ctrl+c", then = "home" },
          { when = "command+c", then = "command+c" },
          { when = "command+x" },
          { then = "home" },
          "not a table",
          { when = "command+right", then = "end", hold = "forever" },
          { when = "command+up", then = ["ctrl+home", "bogus"] },
          { when = "command+down", then = 3 },
        ]
    "#;
    let (layer, warnings) = Layer::parse(text, Source::User);
    const WARNED: &[&str] = &[
        "profile.t.modmap.command",
        "profile.t.rules[1].when",
        "profile.t.rules[2].when",
        "profile.t.rules[3].then",
        "profile.t.rules[4].then",
        "profile.t.rules[5].when",
        "profile.t.rules[6]",
        "profile.t.rules[7].hold",
        "profile.t.rules[8].then[1]",
        "profile.t.rules[9].then",
    ];
    assert_eq!(keys(&warnings), WARNED);
    assert!(
        warnings
            .iter()
            .all(|w| w.problem.starts_with("user config: ")),
        "{warnings:?}"
    );

    let (profiles, _) = resolve(&[layer]);
    let p = profiles.resolve(&name("t"));
    assert_eq!(
        p.modmap.command,
        ModMap::IDENTITY.command,
        "bad modmap key keeps the base"
    );
    assert_eq!(p.modmap.control, OutMod::Ctrl, "its sibling key is read");
    let wanted: Vec<_> = p.rules.keys().copied().collect();
    let mut expect = vec![trigger("command+left"), trigger("command+right")];
    expect.sort();
    assert_eq!(wanted, expect);
    assert_eq!(
        rule(p, "command+right"),
        Some(&map("end")),
        "bad hold falls back to restore"
    );
}

#[test]
fn hold_on_a_list_is_reported_and_ignored() {
    let text = r#"
        [profile.t]
        rules = [{ when = "command+left", then = ["home", "end"], hold = "while_held" }]
    "#;
    let (layer, warnings) = Layer::parse(text, Source::System);
    assert_eq!(keys(&warnings), ["profile.t.rules[0].hold"]);
    let (profiles, _) = resolve(&[layer]);
    assert_eq!(
        rule(profiles.resolve(&name("t")), "command+left"),
        Some(&Action::Sequence(vec![stroke("home"), stroke("end")]))
    );
}

#[test]
fn bad_structure_costs_the_smallest_enclosing_piece() {
    struct Case {
        label: &'static str,
        text: &'static str,
        warned: &'static [&'static str],
    }
    const CASES: &[Case] = &[
        Case {
            label: "profile is not a table",
            text: "profile = 3",
            warned: &["profile"],
        },
        Case {
            label: "one profile is not a table",
            text: "[profile]\nx = 3",
            warned: &["profile.x"],
        },
        Case {
            label: "bad profile name",
            text: "[profile.Bad]\ninherits = \"default\"",
            warned: &["profile.Bad"],
        },
        Case {
            label: "bad inherits",
            text: "[profile.x]\ninherits = 3",
            warned: &["profile.x.inherits"],
        },
        Case {
            label: "modmap not a table",
            text: "[profile.x]\nmodmap = 3",
            warned: &["profile.x.modmap"],
        },
        Case {
            label: "rules not a list",
            text: "[profile.x]\nrules = 3",
            warned: &["profile.x.rules"],
        },
        Case {
            label: "unknown keys are ignored",
            text: "extra = 1\n[profile.x]\nwho = 2\nmodmap = { shift = \"ctrl\" }",
            warned: &[],
        },
    ];
    for case in CASES {
        let (_, warnings) = Layer::parse(case.text, Source::System);
        assert_eq!(keys(&warnings), case.warned, "{}", case.label);
    }
}

#[test]
fn non_toml_text_is_all_defaults_and_one_warning() {
    let (broken, warnings) = Layer::parse("[profile.default\nmodmap = ", Source::System);
    assert_eq!(broken, Layer::default());
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].key, "file");
    assert!(
        warnings[0]
            .problem
            .starts_with("system config: is not valid TOML")
    );

    let (builtin_layer, _) = builtin();
    let with_broken = resolve(&[builtin_layer.clone(), broken]);
    assert_eq!(
        with_broken,
        resolve(&[builtin_layer]),
        "a broken layer changes nothing"
    );
}

#[test]
fn a_bad_user_layer_leaves_the_system_layer_in_force() {
    let system_text = r#"[profile.default]
        rules = [{ when = "command+left", then = "home" }]"#;
    let (user, warnings) = Layer::parse(
        "[profile.default]\nrules = [{ when = \"command+left\", then = \"zzz\" }]",
        Source::User,
    );
    assert_eq!(keys(&warnings), ["profile.default.rules[0].then"]);
    let (profiles, _) = resolve(&[layer(system_text), user]);
    assert_eq!(
        rule(profiles.default_profile(), "command+left"),
        Some(&map("home"))
    );
}

#[test]
fn key_names_in_defaults_exist() {
    for k in ["backspace", "delete", "space", "tab", "f4"] {
        assert!(Key::named(k).is_some(), "{k}");
    }
}

/// A scratch directory under the system temp dir, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("keycap-config-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir is creatable");
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_missing_system_file_is_an_empty_layer_without_warning() {
    let dir = Scratch::new("missing");
    assert_eq!(system(&dir.0), (Layer::default(), vec![]));
}

#[test]
fn the_system_file_is_read_from_its_directory() {
    let dir = Scratch::new("present");
    std::fs::write(
        dir.0.join(FILE_NAME),
        "[profile.default]\nrules = [{ when = \"command+left\", then = \"home\" }]\n",
    )
    .expect("scratch file is writable");
    let (layer, warnings) = system(&dir.0);
    assert_eq!(warnings, vec![]);
    let (profiles, _) = resolve(&[layer]);
    assert_eq!(
        rule(profiles.default_profile(), "command+left"),
        Some(&map("home"))
    );
}

#[test]
fn an_unreadable_system_file_is_an_empty_layer_and_one_warning() {
    let dir = Scratch::new("unreadable");
    std::fs::create_dir(dir.0.join(FILE_NAME)).expect("a directory can stand in for the file");
    let (layer, warnings) = system(&dir.0);
    assert_eq!(layer, Layer::default());
    assert_eq!(warnings.len(), 1);
}
