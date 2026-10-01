//! The reader's decisions on a recording emitter and a private socket: no node is opened and
//! no device is created.

use super::run::poll_timeout;
use super::*;
use crate::daemon::events::{EV_KEY, EV_REL, InputEvent, REL_X};
use crate::daemon::inbox::{ClientId, Inbox, Message, Request, mailbox};
use crate::daemon::route::Routed;
use crate::daemon::scratch::Scratch;
use crate::daemon::socket::client::{connect, recv};
use crate::daemon::socket::{SeatFile, bind, socket_path, spawn};
use crate::daemon::uinput::RecordingEmitter;
use crate::{DeviceId, Edge, Input, Key, Layout, Mono, Physical};
use keycap_protocol::{Mode, ProfileName, ToClient};
use rustix::fd::OwnedFd;
use std::time::Duration;

struct Rig {
    _dir: Scratch,
    inbox: Inbox,
    reader: Reader<RecordingEmitter>,
    client: OwnedFd,
}

impl Rig {
    fn new() -> Self {
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
            initial,
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

    fn key(&mut self, name: &str, edge: Edge) {
        self.reader.apply(Routed::Remap(Input::Key {
            device: DeviceId(0),
            layout: Layout::Apple,
            key: Physical(Key::named(name).unwrap()),
            edge,
            at: Mono::default(),
        }));
    }

    fn request(&mut self, request: Request) {
        self.reader.handle(Message::Request(request));
    }

    /// The `(code, value)` pairs of the last batch written, sync excluded.
    fn last_write(&self) -> Vec<(u16, i32)> {
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
    rig.reader.apply(Routed::Pass(frame.clone()));
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
