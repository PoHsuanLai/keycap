use super::*;

/// `count` distinct `when` chords, each a role combination over a letter, digit or function key.
fn chords(count: usize) -> Vec<String> {
    const ROLES: &[&str] = &[
        "",
        "command+",
        "option+",
        "control+",
        "shift+",
        "command+shift+",
        "option+shift+",
        "control+shift+",
        "command+option+",
        "command+control+",
        "option+control+",
        "command+option+shift+",
    ];
    let keys: Vec<String> = ('a'..='z')
        .map(String::from)
        .chain((0..10).map(|d| d.to_string()))
        .chain((1..=12).map(|n| format!("f{n}")))
        .collect();
    ROLES
        .iter()
        .flat_map(|r| keys.iter().map(move |k| format!("{r}{k}")))
        .take(count)
        .collect()
}

fn profile_with_rules(name: &str, count: usize) -> String {
    let rules: Vec<String> = chords(count)
        .iter()
        .map(|c| format!("{{ when = \"{c}\", then = \"a\" }}"))
        .collect();
    format!("[profile.{name}]\nrules = [{}]\n", rules.join(", "))
}

fn chain(links: usize) -> String {
    (0..links)
        .map(|i| {
            let parent = if i == 0 {
                "default".to_owned()
            } else {
                format!("p{}", i - 1)
            };
            format!("[profile.p{i}]\ninherits = \"{parent}\"\n")
        })
        .collect()
}

fn verdict(text: &str) -> Result<(), Oversize> {
    within_limits(&[layer(text)])
}

#[test]
fn what_is_within_the_limits_passes_and_the_built_in_defaults_are_far_inside() {
    let (defaults, _) = builtin();
    assert_eq!(within_limits(&[defaults]), Ok(()));
    let many: String = (0..MAX_PROFILES)
        .map(|i| format!("[profile.p{i}]\n"))
        .collect();
    assert_eq!(verdict(&many), Ok(()));
    assert_eq!(
        verdict(&profile_with_rules("big", MAX_RULES_PER_PROFILE)),
        Ok(())
    );
    assert_eq!(verdict(&chain(MAX_INHERIT_STEPS)), Ok(()));
}

#[test]
fn one_more_than_a_limit_is_refused_with_what_to_shorten() {
    let too_many: String = (0..=MAX_PROFILES)
        .map(|i| format!("[profile.p{i}]\n"))
        .collect();
    assert_eq!(
        verdict(&too_many),
        Err(Oversize::Profiles {
            found: MAX_PROFILES + 1
        })
    );
    assert_eq!(
        verdict(&profile_with_rules("big", MAX_RULES_PER_PROFILE + 1)),
        Err(Oversize::Rules {
            profile: name("big"),
            found: MAX_RULES_PER_PROFILE + 1
        })
    );
    let deep = within_limits(&[layer(&chain(MAX_INHERIT_STEPS + 1))]);
    assert!(
        matches!(deep, Err(Oversize::InheritSteps { found, .. }) if found == MAX_INHERIT_STEPS + 1),
        "{deep:?}"
    );
}

#[test]
fn the_rules_of_every_layer_count_together() {
    let half = MAX_RULES_PER_PROFILE / 2 + 1;
    let first = layer(&profile_with_rules("big", half));
    let second_text: String = chords(2 * half)
        .iter()
        .skip(half)
        .map(|c| format!("{{ when = \"{c}\", then = \"b\" }}"))
        .collect::<Vec<_>>()
        .join(", ");
    let second = layer(&format!("[profile.big]\nrules = [{second_text}]\n"));
    assert!(matches!(
        within_limits(&[first, second]),
        Err(Oversize::Rules { .. })
    ));
}

#[test]
fn a_cycle_is_not_a_deep_chain() {
    let text = "[profile.a]\ninherits = \"b\"\n[profile.b]\ninherits = \"a\"\n";
    assert_eq!(verdict(text), Ok(()));
}

#[test]
fn the_refusal_says_what_is_over_and_that_the_old_settings_stay() {
    let w = Oversize::Profiles { found: 65 }.warning();
    assert_eq!(w.key, "profile");
    assert!(w.problem.contains("65") && w.problem.contains("earlier settings"));
    let w = Oversize::InheritSteps {
        profile: name("leaf"),
        found: 9,
    }
    .warning();
    assert_eq!(w.key, "profile.leaf.inherits");
}

#[test]
fn the_deepest_allowed_chain_flattens_each_profile_over_its_parent() {
    let text = format!(
        "[profile.default]\nrules = [{{ when = \"left\", then = \"a\" }}]\n{}",
        chain(MAX_INHERIT_STEPS)
    );
    let (profiles, warnings) = resolved(&[text.as_str()]);
    assert_eq!(warnings, vec![]);
    let last = profiles.resolve(&name(&format!("p{}", MAX_INHERIT_STEPS - 1)));
    assert_eq!(
        rule(last, "left"),
        Some(&map("a")),
        "default's rule is inherited"
    );
}
