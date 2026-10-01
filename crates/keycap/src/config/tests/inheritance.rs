use super::*;
use crate::profile::ModMap;
use crate::role::OutMod;

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
        /// The profile checked: it must still have default's `left` rule.
        profile: &'static str,
    }
    const CASES: &[Case] = &[
        Case {
            label: "unknown parent",
            text: r#"[profile.a]
                     inherits = "nowhere"
                     rules = [{ when = "up", then = "c" }]"#,
            warned: &["profile.a.inherits"],
            profile: "a",
        },
        Case {
            label: "self parent",
            text: r#"[profile.a]
                     inherits = "a""#,
            warned: &["profile.a.inherits"],
            profile: "a",
        },
        Case {
            label: "two-profile cycle",
            text: r#"[profile.a]
                     inherits = "b"
                     [profile.b]
                     inherits = "a""#,
            warned: &["profile.a.inherits", "profile.b.inherits"],
            profile: "a",
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
        },
        Case {
            label: "default inherits nothing",
            text: r#"[profile.default]
                     inherits = "a"
                     [profile.a]
                     inherits = "default""#,
            warned: &["profile.default.inherits"],
            profile: "a",
        },
    ];
    let default_rule = r#"[profile.default]
        rules = [{ when = "left", then = "home" }]
    "#;
    for case in CASES {
        let (profiles, warnings) = resolved(&[default_rule, case.text]);
        assert_eq!(keys(&warnings), case.warned, "{}", case.label);
        let p = profiles.resolve(&name(case.profile));
        assert!(rule(p, "left").is_some(), "{}", case.label);
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
        Some(&RuleAction::Swallow)
    );
    assert_eq!(
        rule(profiles.resolve(&name("default")), "command+q"),
        Some(&map("alt+f4"))
    );
}
