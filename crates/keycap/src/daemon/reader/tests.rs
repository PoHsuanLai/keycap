//! The reader's decisions on a recording emitter and a private socket: no node is opened and
//! no device is created.

use super::run::poll_timeout;
use super::*;
use crate::daemon::events::{EV_KEY, EV_REL, InputEvent, REL_X};
use crate::daemon::inbox::{ClientId, Inbox, Message, Request, mailbox};
use crate::daemon::route::Routed;
use crate::daemon::scratch::Scratch;
use crate::daemon::socket::client::{connect, recv};
use crate::daemon::socket::{SeatFile, Steering, bind, socket_path, spawn};
use crate::daemon::uinput::RecordingEmitter;
use crate::{DeviceId, Edge, Key, Layout, Mono, Physical, RemapInput};
use keycap_protocol::{Mode, ProfileName, ToClient};
use rustix::fd::OwnedFd;
use std::time::Duration;

pub(super) struct Rig {
    _dir: Scratch,
    inbox: Inbox,
    pub(super) reader: Reader<RecordingEmitter>,
    pub(super) client: OwnedFd,
}

impl Rig {
    pub(super) fn new() -> Self {
        Self::watched(None, None)
    }

    /// A rig whose systemd notifications go to `notify` and whose watchdog interval is
    /// `watchdog`.
    fn watched(notify: Option<&str>, watchdog: Option<Duration>) -> Self {
        let dir = Scratch::new("reader");
        let path = socket_path(dir.path());
        let listener = bind(&path).unwrap();
        let (inbox, mailbox) = mailbox().unwrap();
        let initial =
            crate::daemon::devices::status_of(&Devices::new(), crate::CoreStatus::Running);
        let socket = spawn(
            listener,
            inbox.clone(),
            SeatFile(dir.path().join("seat0")),
            Steering::SeatUser.vouch(),
            initial,
            keycap_client::FileText::Missing,
        )
        .unwrap();
        let reader = Reader::new(Wiring {
            mailbox,
            socket,
            emitter: RecordingEmitter::default(),
            notifier: Notifier::new(notify).unwrap(),
            watchdog,
            roots: Roots {
                sys: dir.path().join("sys"),
                dev: dir.path().join("dev"),
                udev: dir.path().join("udev"),
                allow: VirtualAllowance::Nobody,
            },
            layers: Layers::builtin_only(),
        });
        let client = connect(&path);
        assert!(matches!(recv(&client), Some(ToClient::Hello { .. })));
        Rig {
            _dir: dir,
            inbox,
            reader,
            client,
        }
    }

    pub(super) fn key(&mut self, name: &str, edge: Edge) {
        self.reader.apply(Routed::Remap(RemapInput::Key {
            device: DeviceId(0),
            layout: Layout::Apple,
            key: Physical(Key::named(name).unwrap()),
            edge,
            at: Mono::default(),
        }));
    }

    pub(super) fn request(&mut self, request: Request) {
        self.reader.handle(Message::Request(request));
    }

    /// A combo device's pointer frame with `value` on `BTN_LEFT`, passed through.
    fn click(&mut self, value: i32) {
        self.reader.apply(Routed::Pass {
            device: DeviceId(0),
            events: vec![
                InputEvent {
                    kind: EV_KEY,
                    code: BTN_LEFT,
                    value,
                },
                InputEvent::SYNC,
            ],
        });
    }

    /// The `(code, value)` pairs of the last batch written, sync excluded.
    pub(super) fn last_write(&self) -> Vec<(u16, i32)> {
        self.reader
            .emitter
            .batches
            .last()
            .map(|batch| {
                batch
                    .iter()
                    .filter(|e| e.kind == EV_KEY)
                    .map(|e| (e.code, e.value))
                    .collect()
            })
            .unwrap_or_default()
    }
}

const BTN_LEFT: u16 = 0x110;

fn code(name: &str) -> u16 {
    Key::named(name).unwrap().code()
}

#[test]
fn a_focus_request_changes_what_the_same_keys_write() {
    let copy = |rig: &mut Rig| {
        rig.key("leftmeta", Edge::Press);
        rig.key("c", Edge::Press);
        let written = rig.last_write();
        rig.key("c", Edge::Release);
        rig.key("leftmeta", Edge::Release);
        written
    };
    let mut rig = Rig::new();
    assert_eq!(copy(&mut rig), [(code("c"), 1)], "default: Cmd+C is Ctrl+C");

    rig.request(Request::Focus(ProfileName::parse("terminal").unwrap()));
    assert_eq!(
        copy(&mut rig),
        [(code("leftshift"), 1), (code("c"), 1)],
        "terminal: Ctrl+Shift+C"
    );

    rig.request(Request::Focus(ProfileName::default_profile()));
    assert_eq!(copy(&mut rig), [(code("c"), 1)], "back to default");
}

#[test]
fn a_user_config_reresolves_replies_with_its_warnings_and_clearing_it_restores_the_defaults() {
    let mut rig = Rig::new();
    let toml = "[profile.default]\nmodmap = { control = \"bogus\" }\nrules = [{ when = \"command+c\", then = \"x\" }]\n";
    rig.request(Request::UserConfig {
        client: ClientId(0),
        toml: toml.to_owned(),
    });
    let Some(ToClient::ConfigWarnings(warnings)) = recv(&rig.client) else {
        panic!("expected ConfigWarnings");
    };
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0].key, "profile.default.modmap.control");

    rig.key("leftmeta", Edge::Press);
    rig.key("c", Edge::Press);
    assert_eq!(
        rig.last_write(),
        [(code("leftctrl"), 0), (code("x"), 1)],
        "the user's rule is in force"
    );
    rig.key("c", Edge::Release);
    rig.key("leftmeta", Edge::Release);

    rig.request(Request::ClearUserConfig);
    rig.key("leftmeta", Edge::Press);
    rig.key("c", Edge::Press);
    assert_eq!(rig.last_write(), [(code("c"), 1)], "the defaults are back");
}

/// The warnings a `UserConfig` of `toml` was answered with.
fn config_warnings(rig: &mut Rig, toml: String) -> Vec<keycap_protocol::Warning> {
    rig.request(Request::UserConfig {
        client: ClientId(0),
        toml,
    });
    match recv(&rig.client) {
        Some(ToClient::ConfigWarnings(warnings)) => warnings,
        other => panic!("expected ConfigWarnings, got {other:?}"),
    }
}

#[test]
fn a_user_config_loses_the_rules_a_session_may_not_set_and_keeps_the_rest() {
    let mut rig = Rig::new();
    let toml = "[profile.default]\nrules = [\n\
        { when = \"enter\", then = \"a\" },\n\
        { when = \"command+j\", then = [\"a\", \"b\"] },\n\
        { when = \"command+k\", then = \"x\" },\n]\n";
    let warnings = config_warnings(&mut rig, toml.to_owned());
    assert_eq!(warnings.len(), 2, "{warnings:?}");

    rig.key("enter", Edge::Press);
    assert_eq!(
        rig.last_write(),
        [(code("enter"), 1)],
        "Enter alone is not remapped"
    );
    rig.key("enter", Edge::Release);
    rig.key("leftmeta", Edge::Press);
    rig.key("j", Edge::Press);
    assert_eq!(rig.last_write(), [(code("j"), 1)], "the list is not typed");
    rig.key("j", Edge::Release);
    rig.key("k", Edge::Press);
    assert_eq!(
        rig.last_write(),
        [(code("leftctrl"), 0), (code("x"), 1)],
        "the single stroke is in force"
    );
}

#[test]
fn a_user_config_of_too_many_rules_is_refused_whole_and_the_settings_in_force_stay() {
    let mut rig = Rig::new();
    let letters = ('a'..='z').map(String::from);
    let keys: Vec<String> = letters
        .chain((0..10).map(|d| d.to_string()))
        .chain((1..=12).map(|n| format!("f{n}")))
        .collect();
    let rules: Vec<String> = [
        "command+",
        "option+",
        "control+",
        "shift+",
        "command+shift+",
        "option+shift+",
    ]
    .iter()
    .flat_map(|r| {
        keys.iter()
            .map(move |k| format!("{{ when = \"{r}{k}\", then = \"a\" }}"))
    })
    .take(257)
    .collect();
    let warnings = config_warnings(
        &mut rig,
        format!("[profile.default]\nrules = [{}]\n", rules.join(",")),
    );
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].problem.contains("257"), "{warnings:?}");

    rig.key("leftmeta", Edge::Press);
    rig.key("c", Edge::Press);
    assert_eq!(
        rig.last_write(),
        [(code("c"), 1)],
        "the defaults are still in force"
    );
}

#[test]
fn a_focus_on_a_profile_nobody_defined_writes_as_default() {
    let copy = |rig: &mut Rig| {
        rig.key("leftmeta", Edge::Press);
        rig.key("c", Edge::Press);
        let written = rig.last_write();
        rig.key("c", Edge::Release);
        rig.key("leftmeta", Edge::Release);
        written
    };
    let mut rig = Rig::new();
    rig.request(Request::Focus(ProfileName::parse("terminal").unwrap()));
    assert_eq!(copy(&mut rig).len(), 2, "terminal: Ctrl+Shift+C");
    rig.request(Request::Focus(ProfileName::parse("mystery").unwrap()));
    assert_eq!(
        copy(&mut rig),
        [(code("c"), 1)],
        "an undefined name is the default profile"
    );
}

#[test]
fn the_pause_chord_is_pushed_once_and_resume_is_pushed_back() {
    let mut rig = Rig::new();
    rig.key("leftshift", Edge::Press);
    rig.key("rightshift", Edge::Press);
    rig.key("esc", Edge::Press);
    rig.reader.publish();
    rig.reader.publish();
    let Some(ToClient::Status(status)) = recv(&rig.client) else {
        panic!("expected the pause status");
    };
    assert_eq!(status.mode, Mode::Paused);

    rig.request(Request::Resume);
    rig.reader.publish();
    let Some(ToClient::Status(status)) = recv(&rig.client) else {
        panic!("expected the running status, not a repeat of the pause");
    };
    assert_eq!(status.mode, Mode::Running);
}

#[test]
fn pointer_frames_are_written_as_they_came() {
    let mut rig = Rig::new();
    let frame = vec![
        InputEvent {
            kind: EV_REL,
            code: REL_X,
            value: 3,
        },
        InputEvent::SYNC,
    ];
    rig.reader.apply(Routed::Pass {
        device: DeviceId(0),
        events: frame.clone(),
    });
    assert_eq!(rig.reader.emitter.batches, [frame]);
}

#[test]
fn poll_timeout_table() {
    let ms = Duration::from_millis;
    let cases: &[(&str, Option<Duration>, Option<Duration>, Duration)] = &[
        ("nothing scheduled, no watchdog", None, None, ms(500)),
        ("a gate open, no watchdog", Some(ms(50)), None, ms(50)),
        (
            "a retry far off, no watchdog",
            Some(ms(30_000)),
            None,
            ms(500),
        ),
        (
            "nothing scheduled, 1 s watchdog",
            None,
            Some(ms(1000)),
            ms(500),
        ),
        (
            "nothing scheduled, 400 ms watchdog",
            None,
            Some(ms(400)),
            ms(200),
        ),
        (
            "a gate open under a watchdog",
            Some(ms(50)),
            Some(ms(1000)),
            ms(50),
        ),
        (
            "a retry past the watchdog half",
            Some(ms(900)),
            Some(ms(1000)),
            ms(500),
        ),
        (
            "a due retry",
            Some(Duration::ZERO),
            Some(ms(1000)),
            Duration::ZERO,
        ),
    ];
    for &(name, wake, watchdog, want) in cases {
        assert_eq!(poll_timeout(wake, watchdog), want, "{name}");
    }
}

#[test]
fn the_loop_wakes_for_a_message_and_applies_it() {
    let mut rig = Rig::new();
    rig.inbox.send(Message::Request(Request::Focus(
        ProfileName::parse("terminal").unwrap(),
    )));
    let started = std::time::Instant::now();
    rig.reader.once().unwrap();
    assert!(
        started.elapsed() < Duration::from_millis(400),
        "woken by the eventfd, not by the idle timeout: {:?}",
        started.elapsed()
    );
    rig.key("leftmeta", Edge::Press);
    rig.key("c", Edge::Press);
    assert_eq!(
        rig.last_write(),
        [(code("leftshift"), 1), (code("c"), 1)],
        "the focus sent through the inbox is in force"
    );
}

#[test]
fn every_pass_pets_the_watchdog_and_sleeps_at_most_half_its_interval() {
    let dir = Scratch::new("reader-notify");
    let path = dir.path().join("notify.sock");
    let listener = std::os::unix::net::UnixDatagram::bind(&path).unwrap();
    listener
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut rig = Rig::watched(path.to_str(), Some(Duration::from_millis(200)));
    for pass in 0..2 {
        let started = std::time::Instant::now();
        rig.reader.once().unwrap();
        assert!(
            started.elapsed() < Duration::from_millis(400),
            "pass {pass} slept {:?}, over the idle timeout",
            started.elapsed()
        );
        let mut buf = [0u8; 64];
        let n = listener.recv(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"WATCHDOG=1", "pass {pass}");
    }
}

#[test]
fn a_failed_write_recreates_the_device_and_the_core_forgets_what_it_thought_was_down() {
    let mut rig = Rig::new();
    rig.reader.emitter.failing = 1;
    rig.key("leftmeta", Edge::Press);
    assert_eq!(rig.reader.emitter.recreated, 1, "the device was replaced");
    assert_eq!(
        rig.reader.emitter.batches.len(),
        0,
        "the failed batch is not retried"
    );

    // Held was reset: the release of a key whose press never reached the wire writes nothing,
    // where a stale record would write a Ctrl release the new device never saw pressed.
    rig.key("leftmeta", Edge::Release);
    assert_eq!(
        rig.reader.emitter.batches.len(),
        0,
        "nothing to release on the new device"
    );

    rig.key("c", Edge::Press);
    assert_eq!(rig.last_write(), [(code("c"), 1)], "the new device works");
    assert_eq!(
        rig.reader.emitter.recreated, 1,
        "and was not replaced again"
    );
}

#[test]
fn a_failed_write_forgets_the_buttons_of_the_old_device() {
    let mut rig = Rig::new();
    rig.click(1);
    rig.reader.emitter.failing = 1;
    rig.click(0);
    assert_eq!(rig.reader.emitter.recreated, 1);
    let before = rig.reader.emitter.batches.len();
    rig.reader.lose(DeviceId(0));
    assert_eq!(
        rig.reader.emitter.batches.len(),
        before,
        "the new device has no button down, so none is released"
    );
}

#[test]
fn an_unplugged_combo_device_releases_the_buttons_it_held_once_and_only_those_still_down() {
    let mut rig = Rig::new();
    rig.click(1);
    assert_eq!(rig.reader.emitter.batches.len(), 1);
    rig.reader.lose(DeviceId(0));
    assert_eq!(
        rig.reader.emitter.batches.last(),
        Some(&vec![
            InputEvent {
                kind: EV_KEY,
                code: BTN_LEFT,
                value: 0
            },
            InputEvent::SYNC
        ]),
        "BTN_LEFT up, framed"
    );
    rig.reader.lose(DeviceId(0));
    assert_eq!(rig.reader.emitter.batches.len(), 2, "released once");

    // Step two: a button that was already released is not released again when the device goes.
    rig.click(1);
    rig.click(0);
    rig.reader.lose(DeviceId(0));
    assert_eq!(
        rig.reader.emitter.batches.len(),
        4,
        "press and release only, nothing at the loss"
    );
}

#[test]
fn the_escape_chord_releases_held_buttons_after_the_keys() {
    let mut rig = Rig::new();
    rig.click(1);
    rig.key("leftshift", Edge::Press);
    rig.key("rightshift", Edge::Press);
    rig.key("esc", Edge::Press);
    let batches = &rig.reader.emitter.batches;
    assert_eq!(
        batches.last(),
        Some(&vec![
            InputEvent {
                kind: EV_KEY,
                code: BTN_LEFT,
                value: 0
            },
            InputEvent::SYNC
        ]),
        "the last write releases the button: {batches:?}"
    );
    let released = batches
        .iter()
        .flatten()
        .filter(|e| e.code == BTN_LEFT && e.value == 0)
        .count();
    assert_eq!(released, 1);
}

#[test]
fn an_overflowed_device_has_what_it_held_released_and_keeps_being_read() {
    let mut rig = Rig::new();
    rig.key("leftmeta", Edge::Press);
    assert_eq!(rig.last_write(), [(code("leftctrl"), 1)]);
    rig.click(1);
    rig.reader.apply(Routed::Resync(DeviceId(0)));
    let written: Vec<(u16, i32)> = rig
        .reader
        .emitter
        .batches
        .iter()
        .flatten()
        .filter(|e| e.kind == EV_KEY)
        .map(|e| (e.code, e.value))
        .collect();
    assert_eq!(
        &written[written.len() - 2..],
        [(code("leftctrl"), 0), (BTN_LEFT, 0)],
        "Ctrl and the button are up: {written:?}"
    );
    rig.key("c", Edge::Press);
    assert_eq!(
        rig.last_write(),
        [(code("c"), 1)],
        "the next press is mapped from a clean state"
    );
}
