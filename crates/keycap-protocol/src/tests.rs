//! Round trips, the wire form, name validation and the version check.

use crate::*;

fn keyboard() -> KeyboardInfo {
    KeyboardInfo {
        name: "Apple Keyboard".into(),
        path: "/dev/input/event4".into(),
        vendor: 0x5ac,
        product: 0x250,
    }
}

fn name(s: &str) -> ProfileName {
    ProfileName::parse(s).expect("test names are valid")
}

fn statuses() -> Vec<Status> {
    let kb = |grab| Keyboard {
        info: keyboard(),
        grab,
    };
    vec![
        Status {
            mode: Mode::Running,
            keyboards: vec![kb(Grab::Active), kb(Grab::Conflict)],
        },
        Status {
            mode: Mode::Running,
            keyboards: vec![kb(Grab::WaitingForRelease)],
        },
        Status {
            mode: Mode::Paused,
            keyboards: vec![kb(Grab::Active)],
        },
        Status {
            mode: Mode::Running,
            keyboards: vec![],
        },
    ]
}

fn samples() -> Vec<ToClient> {
    let mut all: Vec<ToClient> = statuses()
        .into_iter()
        .flat_map(|status| {
            [
                ToClient::Hello {
                    version: PROTOCOL_VERSION,
                    status: status.clone(),
                },
                ToClient::Status(status),
            ]
        })
        .collect();
    all.push(ToClient::ConfigWarnings(vec![]));
    all.push(ToClient::ConfigWarnings(vec![Warning {
        key: "profile.term.rules".into(),
        problem: "unknown key name".into(),
    }]));
    all.push(ToClient::Keymap(KeymapState::default()));
    all.push(ToClient::Keymap(KeymapState {
        overrides: "copy = Primary+C\nredo = none\n".into(),
        problems: vec![KeymapProblem {
            line: 3,
            what: "\"nope\" is not a standard action".into(),
        }],
    }));
    all.push(ToClient::Registered(vec![]));
    all.push(ToClient::Registered(vec![registration()]));
    all
}

fn registration() -> Registration {
    Registration {
        app: "mail".into(),
        actions: vec![RegisteredAction {
            id: "mail.compose".into(),
            chord: "Primary+Shift+N".into(),
        }],
    }
}

fn to_daemon_samples() -> Vec<ToDaemon> {
    vec![
        ToDaemon::Steer(Steer::Focus {
            profile: name("terminal"),
        }),
        ToDaemon::Steer(Steer::UserConfig {
            toml: "[profile.default]\n".into(),
        }),
        ToDaemon::Steer(Steer::Resume),
        ToDaemon::Keymap(KeymapCall::Watch),
        ToDaemon::Keymap(KeymapCall::Register(registration())),
        ToDaemon::Keymap(KeymapCall::List),
    ]
}

fn json<T: serde::Serialize>(msg: &T) -> String {
    String::from_utf8(encode(msg)).expect("JSON is UTF-8")
}

#[test]
fn every_message_round_trips() {
    for m in samples() {
        assert_eq!(decode::<ToClient>(&encode(&m)).expect("decodes"), m);
        assert!(encode(&m).len() < MAX_PACKET);
    }
    for m in to_daemon_samples() {
        assert_eq!(decode::<ToDaemon>(&encode(&m)).expect("decodes"), m);
    }
}

#[test]
fn wire_form_is_adjacently_tagged() {
    assert_eq!(
        json(&ToDaemon::Steer(Steer::Focus {
            profile: name("terminal")
        })),
        r#"{"kind":"steer","v":{"kind":"focus","v":{"profile":"terminal"}}}"#
    );
    assert_eq!(
        json(&ToDaemon::Steer(Steer::Resume)),
        r#"{"kind":"steer","v":{"kind":"resume"}}"#
    );
    assert_eq!(
        json(&ToClient::Hello {
            version: 1,
            status: Status {
                mode: Mode::Paused,
                keyboards: vec![]
            }
        }),
        r#"{"kind":"hello","v":{"version":1,"status":{"mode":"paused","keyboards":[]}}}"#
    );
    assert_eq!(
        json(&ToClient::Status(Status {
            mode: Mode::Running,
            keyboards: vec![Keyboard {
                info: KeyboardInfo {
                    name: "k".into(),
                    path: "/dev/input/event0".into(),
                    vendor: 1,
                    product: 2
                },
                grab: Grab::WaitingForRelease
            }]
        })),
        r#"{"kind":"status","v":{"mode":"running","keyboards":[{"info":{"name":"k","path":"/dev/input/event0","vendor":1,"product":2},"grab":"waiting_for_release"}]}}"#
    );
}

#[test]
fn profile_names_are_validated() {
    let long_ok = "a".repeat(64);
    let long_bad = "a".repeat(65);
    let cases: &[(&str, &str, Result<(), NameError>)] = &[
        ("plain", "terminal", Ok(())),
        ("digits and separators", "a_b-c9", Ok(())),
        ("default", "default", Ok(())),
        ("longest", &long_ok, Ok(())),
        ("empty", "", Err(NameError::Empty)),
        ("one byte too long", &long_bad, Err(NameError::TooLong(65))),
        ("uppercase", "Term", Err(NameError::BadChar('T'))),
        ("space", "a b", Err(NameError::BadChar(' '))),
        ("dot", "a.b", Err(NameError::BadChar('.'))),
        ("non-ascii", "caf\u{e9}", Err(NameError::BadChar('\u{e9}'))),
    ];
    for (case, input, want) in cases {
        let got = ProfileName::parse(input).map(|n| n.as_str().to_owned());
        assert_eq!(got, want.clone().map(|()| (*input).to_owned()), "{case}");
    }
    // The default profile is named "default"; a name displays bare.
    assert_eq!(ProfileName::default_profile(), name("default"));
    assert_eq!(name("term").to_string(), "term");
}

#[test]
fn invalid_name_fails_to_deserialize() {
    for bad in [r#""Term""#, r#""""#, r#""a b""#] {
        assert!(serde_json::from_str::<ProfileName>(bad).is_err(), "{bad}");
    }
    let focus = r#"{"kind":"steer","v":{"kind":"focus","v":{"profile":"No Good"}}}"#;
    assert!(decode::<ToDaemon>(focus.as_bytes()).is_err());
    assert_eq!(
        serde_json::from_str::<ProfileName>(r#""ok-1""#).expect("valid"),
        name("ok-1")
    );
}

#[test]
fn oversized_packet_is_too_long_before_parsing() {
    let err = decode::<ToDaemon>(&vec![b' '; MAX_PACKET + 1]).expect_err("too long");
    assert!(matches!(err, ProtocolError::TooLong(n) if n == MAX_PACKET + 1));
}

#[test]
fn junk_is_malformed_not_a_panic() {
    let err = decode::<ToDaemon>(b"{").expect_err("malformed");
    assert!(matches!(err, ProtocolError::Malformed(_)));
    assert!(decode::<ToDaemon>(br#"{"kind":"nope"}"#).is_err());
}

#[test]
fn only_a_hello_of_another_version_is_incompatible() {
    let hello = |version| ToClient::Hello {
        version,
        status: Status {
            mode: Mode::Running,
            keyboards: vec![],
        },
    };
    let cases: &[(&str, ToClient, Compat)] = &[
        ("hello of ours", hello(PROTOCOL_VERSION), Compat::Compatible),
        (
            "hello of the next version",
            hello(PROTOCOL_VERSION + 1),
            Compat::Incompatible {
                version: PROTOCOL_VERSION + 1,
            },
        ),
        (
            "status",
            ToClient::Status(Status {
                mode: Mode::Paused,
                keyboards: vec![],
            }),
            Compat::Compatible,
        ),
        (
            "warnings",
            ToClient::ConfigWarnings(vec![]),
            Compat::Compatible,
        ),
    ];
    for (case, msg, want) in cases {
        assert_eq!(compat(msg), *want, "{case}");
    }
}

#[test]
fn keymap_messages_have_a_stable_wire_form() {
    assert_eq!(
        json(&ToDaemon::Keymap(KeymapCall::Watch)),
        r#"{"kind":"keymap","v":{"kind":"watch"}}"#
    );
    assert_eq!(
        json(&ToDaemon::Keymap(KeymapCall::List)),
        r#"{"kind":"keymap","v":{"kind":"list"}}"#
    );
    assert_eq!(
        json(&ToDaemon::Keymap(KeymapCall::Register(registration()))),
        r#"{"kind":"keymap","v":{"kind":"register","v":{"app":"mail","actions":[{"id":"mail.compose","chord":"Primary+Shift+N"}]}}}"#
    );
    assert_eq!(
        json(&ToClient::Keymap(KeymapState {
            overrides: "undo = none\n".into(),
            problems: vec![]
        })),
        r#"{"kind":"keymap","v":{"overrides":"undo = none\n","problems":[]}}"#
    );
}

#[test]
fn a_keymap_state_with_no_problems_field_is_refused_not_guessed() {
    let text = r#"{"kind":"keymap","v":{"overrides":""}}"#;
    assert!(decode::<ToClient>(text.as_bytes()).is_err());
}

#[test]
fn the_version_window_is_min_through_current() {
    let hello = |version| ToClient::Hello {
        version,
        status: Status {
            mode: Mode::Running,
            keyboards: vec![],
        },
    };
    assert_eq!(compat(&hello(PROTOCOL_VERSION)), Compat::Compatible);
    assert_eq!(compat(&hello(MIN_PROTOCOL_VERSION)), Compat::Compatible);
    assert_eq!(
        compat(&hello(MIN_PROTOCOL_VERSION - 1)),
        Compat::Incompatible {
            version: MIN_PROTOCOL_VERSION - 1
        }
    );
}
