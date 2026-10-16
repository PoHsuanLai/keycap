use super::*;
use crate::profile::ModMap;
use crate::role::OutMod;

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
    let (layer, warnings) = ConfigLayer::parse(text, ConfigSource::User);
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
    let (layer, warnings) = ConfigLayer::parse(text, ConfigSource::System);
    assert_eq!(keys(&warnings), ["profile.t.rules[0].hold"]);
    let (profiles, _) = resolve(&[layer]);
    assert_eq!(
        rule(profiles.resolve(&name("t")), "command+left"),
        Some(&RuleAction::Sequence(vec![stroke("home"), stroke("end")]))
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
        let (_, warnings) = ConfigLayer::parse(case.text, ConfigSource::System);
        assert_eq!(keys(&warnings), case.warned, "{}", case.label);
    }
}

#[test]
fn non_toml_text_is_all_defaults_and_one_warning() {
    let (broken, warnings) =
        ConfigLayer::parse("[profile.default\nmodmap = ", ConfigSource::System);
    assert_eq!(broken, ConfigLayer::default());
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].key, "file");
    assert!(
        warnings[0]
            .problem
            .starts_with("machine settings: is not valid TOML")
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
    let (user, warnings) = ConfigLayer::parse(
        "[profile.default]\nrules = [{ when = \"command+left\", then = \"zzz\" }]",
        ConfigSource::User,
    );
    assert_eq!(keys(&warnings), ["profile.default.rules[0].then"]);
    let (profiles, _) = resolve(&[layer(system_text), user]);
    assert_eq!(
        rule(profiles.default_profile(), "command+left"),
        Some(&map("home"))
    );
}
