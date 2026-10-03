use super::*;
use crate::profile::ModMap;
use crate::role::OutMod;

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
