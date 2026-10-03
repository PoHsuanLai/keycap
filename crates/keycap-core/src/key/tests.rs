use super::names::NAMES;
use super::*;

#[test]
fn names_resolve_to_known_codes() {
    const CASES: &[(&str, u16)] = &[
        ("esc", 1),
        ("tab", 15),
        ("a", 30),
        ("grave", 41),
        ("space", 57),
        ("f4", 62),
        ("home", 102),
        ("left", 105),
        ("end", 107),
        ("delete", 111),
        ("backspace", 14),
        ("leftmeta", 125),
    ];
    for &(name, code) in CASES {
        assert_eq!(Key::named(name), Some(Key::from_code(code)), "{name}");
        assert_eq!(Key::from_code(code).name(), name, "{code}");
    }
}

#[test]
fn constants_match_their_names() {
    const CASES: &[(Key, &str)] = &[
        (Key::ESC, "esc"),
        (Key::LEFTCTRL, "leftctrl"),
        (Key::RIGHTCTRL, "rightctrl"),
        (Key::LEFTALT, "leftalt"),
        (Key::RIGHTALT, "rightalt"),
        (Key::LEFTMETA, "leftmeta"),
        (Key::RIGHTMETA, "rightmeta"),
        (Key::LEFTSHIFT, "leftshift"),
        (Key::RIGHTSHIFT, "rightshift"),
    ];
    for &(key, name) in CASES {
        assert_eq!(key.name(), name);
        assert_eq!(Key::named(name), Some(key), "{name}");
    }
}

#[test]
fn names_and_codes_are_unique() {
    for (i, (n, c)) in NAMES.iter().enumerate() {
        assert!(NAMES[i + 1..].iter().all(|(m, d)| m != n && d != c), "{n}");
    }
}

#[test]
fn names_are_lowercase_tokens() {
    for (n, _) in NAMES {
        assert!(
            n.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
            "{n}"
        );
    }
}

#[test]
fn table_covers_the_standard_keyboard() {
    const CASES: &[&str] = &[
        "q",
        "0",
        "f1",
        "f12",
        "f13",
        "f24",
        "minus",
        "semicolon",
        "kp0",
        "kpenter",
        "volumeup",
        "mute",
        "playpause",
        "brightnessup",
        "capslock",
        "rightmeta",
        "compose",
        "sysrq",
    ];
    for name in CASES {
        assert!(Key::named(name).is_some(), "{name}");
    }
}

#[test]
fn unlisted_code_is_a_key_without_a_name() {
    let key = Key::from_code(0xffff);
    assert_eq!(key.code(), 0xffff);
    assert_eq!(key.name(), "unknown");
    assert_eq!(Key::named("unknown"), None);
    assert_eq!(Key::named("A"), None);
}
