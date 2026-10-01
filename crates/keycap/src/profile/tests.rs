use super::*;
use crate::key::Key;
use crate::role::{OutMod, OutMods, Role, Roles};

fn roles(rs: &[Role]) -> Roles {
    rs.iter().copied().collect()
}

fn mods(ms: &[OutMod]) -> OutMods {
    ms.iter().copied().collect()
}

fn key(name: &str) -> Key {
    Key::named(name).expect("test key names exist")
}

#[test]
fn trigger_parse_table() {
    type Want<'a, M> = Result<(&'a [M], &'a str), ChordError>;
    const CASES: &[(&str, Want<Role>)] = &[
        ("left", Ok((&[], "left"))),
        ("command+left", Ok((&[Role::Command], "left"))),
        ("command+shift+z", Ok((&[Role::Command, Role::Shift], "z"))),
        ("shift+command+z", Ok((&[Role::Command, Role::Shift], "z"))),
        (" option + backspace ", Ok((&[Role::Option], "backspace"))),
        ("", Err(ChordError::Empty)),
        ("command+", Err(ChordError::EmptyName)),
        ("command++z", Err(ChordError::EmptyName)),
        ("ctrl+c", Err(ChordError::OutputModifier(String::new()))),
        (
            "command+alt+c",
            Err(ChordError::OutputModifier(String::new())),
        ),
        ("foo+c", Err(ChordError::UnknownModifier(String::new()))),
        (
            "command+command+c",
            Err(ChordError::Duplicate(String::new())),
        ),
        ("command+shift", Err(ChordError::MissingKey(String::new()))),
        ("command", Err(ChordError::MissingKey(String::new()))),
        ("command+nope", Err(ChordError::UnknownKey(String::new()))),
    ];
    for (text, want) in CASES {
        let got = Trigger::parse(text);
        match want {
            Ok((rs, k)) => assert_eq!(
                got,
                Ok(Trigger {
                    roles: roles(rs),
                    key: key(k)
                }),
                "{text:?}"
            ),
            Err(e) => assert_eq!(
                got.map_err(|e| std::mem::discriminant(&e)),
                Err(std::mem::discriminant(e)),
                "{text:?}"
            ),
        }
    }
}

#[test]
fn stroke_parse_table() {
    type Want<'a, M> = Result<(&'a [M], &'a str), ChordError>;
    const CASES: &[(&str, Want<OutMod>)] = &[
        ("home", Ok((&[], "home"))),
        ("ctrl+shift+c", Ok((&[OutMod::Ctrl, OutMod::Shift], "c"))),
        ("super+space", Ok((&[OutMod::Super], "space"))),
        ("alt+f4", Ok((&[OutMod::Alt], "f4"))),
        ("command+c", Err(ChordError::RoleName(String::new()))),
        ("ctrl+control+c", Err(ChordError::RoleName(String::new()))),
        ("ctrl+ctrl+c", Err(ChordError::Duplicate(String::new()))),
        ("ctrl+", Err(ChordError::EmptyName)),
        ("ctrl", Err(ChordError::MissingKey(String::new()))),
        ("ctrl+nope", Err(ChordError::UnknownKey(String::new()))),
    ];
    for (text, want) in CASES {
        let got = Stroke::parse(text);
        match want {
            Ok((ms, k)) => assert_eq!(
                got,
                Ok(Stroke {
                    mods: mods(ms),
                    key: key(k)
                }),
                "{text:?}"
            ),
            Err(e) => assert_eq!(
                got.map_err(|e| std::mem::discriminant(&e)),
                Err(std::mem::discriminant(e)),
                "{text:?}"
            ),
        }
    }
}

#[test]
fn chord_errors_name_the_offender_and_the_fix() {
    let out = Trigger::parse("ctrl+c").unwrap_err();
    assert_eq!(out, ChordError::OutputModifier("ctrl".to_owned()));
    assert!(out.to_string().contains("role names"), "{out}");
    let role = Stroke::parse("command+c").unwrap_err();
    assert_eq!(role, ChordError::RoleName("command".to_owned()));
    assert!(role.to_string().contains("output names"), "{role}");
    assert_eq!(
        Trigger::parse("command+command+c"),
        Err(ChordError::Duplicate("command".to_owned()))
    );
}

#[test]
fn a_trigger_is_an_exact_match_key() {
    let plain = Trigger::parse("left").unwrap();
    let cmd = Trigger::parse("command+left").unwrap();
    let cmd_shift = Trigger::parse("command+shift+left").unwrap();
    assert!(plain != cmd && cmd != cmd_shift);
}

#[test]
fn hold_names_round_trip_and_restore_is_the_default() {
    for hold in Hold::ALL {
        assert_eq!(Hold::named(hold.name()), Some(hold));
    }
    assert_eq!(Hold::named("forever"), None);
    assert_eq!(Hold::default(), Hold::Restore);
}

#[test]
fn an_empty_sequence_is_a_swallow() {
    let stroke = Stroke::parse("home").unwrap();
    assert_eq!(Action::sequence(vec![]), Action::Swallow);
    assert_eq!(
        Action::sequence(vec![stroke]),
        Action::Sequence(vec![stroke])
    );
}

#[test]
fn modmap_writes_shift_for_shift() {
    let m = ModMap::IDENTITY;
    assert_eq!(m.out(Role::Shift), OutMod::Shift);
    assert_eq!(m.out(Role::Command), OutMod::Super);
    assert_eq!(m.out(Role::Option), OutMod::Alt);
    assert_eq!(m.out(Role::Control), OutMod::Ctrl);
}
