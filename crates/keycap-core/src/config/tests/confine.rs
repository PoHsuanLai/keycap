use super::*;

fn user(text: &str) -> ConfigLayer {
    let (layer, warnings) = ConfigLayer::parse(text, ConfigSource::User);
    assert_eq!(warnings, vec![], "fixture must parse cleanly: {text}");
    layer
}

fn default_rules(rules: &str) -> ConfigLayer {
    user(&format!("[profile.default]\nrules = [{rules}]\n"))
}

/// The rules of `layer`'s `default` profile.
fn rules_of(layer: &ConfigLayer) -> usize {
    layer
        .profiles
        .get(&name("default"))
        .map_or(0, |p| p.rules.len())
}

#[test]
fn a_user_rule_needs_a_modifier_and_writes_one_stroke_unless_the_system_file_says_otherwise() {
    const KEEP: usize = 1;
    const DROP: usize = 0;
    // (name, the rule, rules left when lists are refused, rules left when lists are allowed)
    let cases: &[(&str, &str, usize, usize)] = &[
        (
            "a chord",
            r#"{ when = "command+c", then = "ctrl+c" }"#,
            KEEP,
            KEEP,
        ),
        (
            "shift is a modifier",
            r#"{ when = "shift+capslock", then = "capslock" }"#,
            KEEP,
            KEEP,
        ),
        (
            "a key alone",
            r#"{ when = "enter", then = "ctrl+alt+t" }"#,
            DROP,
            DROP,
        ),
        (
            "capslock alone",
            r#"{ when = "capslock", then = "ctrl+space" }"#,
            DROP,
            DROP,
        ),
        (
            "a list",
            r#"{ when = "command+c", then = ["ctrl+alt+t", "h"] }"#,
            DROP,
            KEEP,
        ),
        (
            "a list of one",
            r#"{ when = "command+c", then = ["h"] }"#,
            DROP,
            KEEP,
        ),
        (
            "swallow",
            r#"{ when = "command+c", then = [] }"#,
            DROP,
            KEEP,
        ),
        (
            "a list on a key alone",
            r#"{ when = "enter", then = ["a", "b"] }"#,
            DROP,
            DROP,
        ),
    ];
    for &(case, rule, refused, allowed) in cases {
        for (sequences, want) in [(Sequences::Refuse, refused), (Sequences::Allow, allowed)] {
            let (layer, warnings) = default_rules(rule).confined(sequences).unwrap();
            assert_eq!(rules_of(&layer), want, "{case} with {sequences:?}");
            assert_eq!(warnings.len(), KEEP - want, "{case} with {sequences:?}");
        }
    }
}

#[test]
fn a_dropped_rule_is_reported_under_its_profile_with_its_chord_in_plain_words() {
    let layer = user(
        "[profile.default]\nrules = [{ when = \"enter\", then = \"a\" }]\n\
         [profile.terminal]\nrules = [{ when = \"command+shift+k\", then = [] }]\n",
    );
    let (_, warnings) = layer.confined(Sequences::Refuse).unwrap();
    assert_eq!(
        keys(&warnings),
        ["profile.default.rules", "profile.terminal.rules"]
    );
    assert!(warnings[0].problem.contains("\"enter\""), "{warnings:?}");
    assert!(
        warnings[1].problem.contains("\"command+shift+k\""),
        "{warnings:?}"
    );
    for w in &warnings {
        assert!(
            w.problem.contains("/etc/keycap/keycap.toml"),
            "says where: {w:?}"
        );
    }
}

/// `count` distinct chords, each a modifier combination over a letter, digit or function key.
fn distinct_rules(count: usize) -> String {
    const ROLES: &[&str] = &[
        "command+",
        "option+",
        "control+",
        "shift+",
        "command+shift+",
        "option+shift+",
    ];
    let keys: Vec<String> = ('a'..='z')
        .map(String::from)
        .chain((0..10).map(|d| d.to_string()))
        .chain((1..=24).map(|n| format!("f{n}")))
        .collect();
    let rules: Vec<String> = ROLES
        .iter()
        .flat_map(|r| {
            keys.iter()
                .map(move |k| format!("{{ when = \"{r}{k}\", then = \"a\" }}"))
        })
        .take(count)
        .collect();
    assert_eq!(rules.len(), count, "enough distinct chords");
    rules.join(", ")
}

#[test]
fn a_user_layer_of_at_most_256_rules_is_taken_and_one_of_257_is_refused_whole() {
    let at_limit = default_rules(&distinct_rules(MAX_USER_RULES));
    let (kept, warnings) = at_limit.confined(Sequences::Refuse).unwrap();
    assert_eq!((rules_of(&kept), warnings), (MAX_USER_RULES, vec![]));

    let over = default_rules(&distinct_rules(MAX_USER_RULES + 1));
    assert_eq!(
        over.confined(Sequences::Allow).unwrap_err(),
        Oversize::UserRules {
            found: MAX_USER_RULES + 1
        },
        "the system file's choice about lists does not lift the count"
    );
    assert!(
        Oversize::UserRules { found: 257 }
            .warning()
            .problem
            .contains("257")
    );
}

#[test]
fn only_the_system_file_chooses_whether_lists_are_allowed() {
    let cases: &[(&str, &str, Sequences, &[&str])] = &[
        ("absent", "", Sequences::Refuse, &[]),
        (
            "allow",
            "[user_layer]\nsequences = \"allow\"\n",
            Sequences::Allow,
            &[],
        ),
        (
            "refuse",
            "[user_layer]\nsequences = \"refuse\"\n",
            Sequences::Refuse,
            &[],
        ),
        (
            "not a choice",
            "[user_layer]\nsequences = \"yes\"\n",
            Sequences::Refuse,
            &["user_layer.sequences"],
        ),
        (
            "not a table",
            "user_layer = 3\n",
            Sequences::Refuse,
            &["user_layer"],
        ),
    ];
    for &(case, text, want, problems) in cases {
        let (layer, warnings) = ConfigLayer::parse(text, ConfigSource::System);
        assert_eq!(layer.sequences(), want, "{case}");
        assert_eq!(keys(&warnings), problems, "{case}");
    }
}
