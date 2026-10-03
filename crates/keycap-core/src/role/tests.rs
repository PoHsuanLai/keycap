use super::*;
use crate::key::Key;

#[test]
fn role_names_round_trip() {
    const CASES: &[(&str, Option<Role>)] = &[
        ("command", Some(Role::Command)),
        ("option", Some(Role::Option)),
        ("control", Some(Role::Control)),
        ("shift", Some(Role::Shift)),
        ("ctrl", None),
        ("Command", None),
        ("", None),
    ];
    for &(name, want) in CASES {
        assert_eq!(Role::named(name), want, "{name:?}");
        if let Some(role) = want {
            assert_eq!(role.name(), name);
        }
    }
}

#[test]
fn out_mod_names_and_keys() {
    const CASES: &[(&str, Option<(OutMod, Key)>)] = &[
        ("ctrl", Some((OutMod::Ctrl, Key::LEFTCTRL))),
        ("alt", Some((OutMod::Alt, Key::LEFTALT))),
        ("super", Some((OutMod::Super, Key::LEFTMETA))),
        ("shift", Some((OutMod::Shift, Key::LEFTSHIFT))),
        ("command", None),
        ("control", None),
    ];
    for &(name, want) in CASES {
        assert_eq!(OutMod::named(name), want.map(|(m, _)| m), "{name:?}");
        if let Some((m, key)) = want {
            assert_eq!(m.name(), name);
            assert_eq!(m.key(), key);
        }
    }
}

#[test]
fn roles_with_without_contains() {
    let held = Roles::EMPTY.with(Role::Command).with(Role::Shift);
    assert!(Roles::EMPTY.is_empty());
    assert!(!held.is_empty());
    assert!(held.contains(Role::Command));
    assert!(held.contains(Role::Shift));
    assert!(!held.contains(Role::Option));
    let released = held.without(Role::Command);
    assert!(!released.contains(Role::Command));
    assert!(released.contains(Role::Shift));
    assert_eq!(released.without(Role::Shift), Roles::EMPTY);
    assert_eq!(held.with(Role::Command), held, "with is idempotent");
    assert_eq!(Roles::EMPTY.without(Role::Control), Roles::EMPTY);
}

#[test]
fn roles_iterate_in_declaration_order() {
    let held: Roles = [Role::Shift, Role::Command, Role::Control]
        .into_iter()
        .collect();
    assert_eq!(
        held.iter().collect::<Vec<_>>(),
        [Role::Command, Role::Control, Role::Shift]
    );
    assert_eq!(Roles::EMPTY.iter().count(), 0);
}

#[test]
fn out_mods_set_operations() {
    let a = OutMods::EMPTY.with(OutMod::Ctrl).with(OutMod::Alt);
    let b = OutMods::EMPTY.with(OutMod::Alt).with(OutMod::Super);
    assert_eq!(
        a.union(b).iter().collect::<Vec<_>>(),
        [OutMod::Ctrl, OutMod::Alt, OutMod::Super]
    );
    assert_eq!(a.difference(b).iter().collect::<Vec<_>>(), [OutMod::Ctrl]);
    assert_eq!(b.difference(a).iter().collect::<Vec<_>>(), [OutMod::Super]);
    assert_eq!(a.difference(a), OutMods::EMPTY);
    assert!(a.contains(OutMod::Ctrl) && !a.contains(OutMod::Shift));
    assert!(!a.without(OutMod::Ctrl).contains(OutMod::Ctrl));
}

#[test]
fn every_member_has_its_own_bit() {
    for (i, a) in Role::ALL.into_iter().enumerate() {
        for b in Role::ALL[i + 1..].iter().copied() {
            assert!(!Roles::EMPTY.with(a).contains(b), "{a:?} {b:?}");
        }
    }
    let all: Roles = Role::ALL.into_iter().collect();
    assert_eq!(all.iter().count(), Role::ALL.len());
}
