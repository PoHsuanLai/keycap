//! The socket thread end to end on a private path with a fake seat file. A client of these
//! tests is a process of this test binary: the shell when the harness is told to treat every
//! program of the seat's user as one, an ordinary program when the steering table is empty.

use super::client::{WAIT, connect, recv, send};
use super::shell::{ShellCheck, Vouch};
use super::*;
use crate::daemon::inbox::{Mailbox, Message, Request, mailbox};
use crate::daemon::scratch::Scratch;
use crate::keymap::Reading;
use keycap_protocol::{
    self as protocol, KeymapState, Mode, PROTOCOL_VERSION, ProfileName, RegisteredAction,
    Registration, Status, ToClient, ToDaemon, Warning,
};
use peephole::{FixtureProc, Prober};
use peephole_core::Table;
use rustix::net::SendFlags;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn own_uid() -> u32 {
    rustix::process::getuid().as_raw()
}

fn running() -> Status {
    Status {
        mode: Mode::Running,
        keyboards: Vec::new(),
    }
}

fn paused() -> Status {
    Status {
        mode: Mode::Paused,
        keyboards: Vec::new(),
    }
}

fn write_seat(path: &Path, uid: Option<u32>) {
    let text = match uid {
        Some(uid) => format!("IS_SEAT0=1\nACTIVE_UID={uid}\nUIDS={uid}\n"),
        None => "IS_SEAT0=1\n".to_owned(),
    };
    std::fs::write(path, text).unwrap();
}

/// A socket thread on `<dir>/keycap.sock` reading the seat from `<dir>/seat0`.
struct Harness {
    _dir: Scratch,
    socket: PathBuf,
    seat: PathBuf,
    mailbox: Mailbox,
    handle: SocketHandle,
}

impl Harness {
    fn new(active_uid: Option<u32>) -> Self {
        Self::with_keymap(active_uid, Reading::Missing)
    }

    fn with_keymap(active_uid: Option<u32>, keymap: Reading) -> Self {
        Self::start(active_uid, keymap, Steering::SeatUser.vouch())
    }

    /// A harness in which no client is the shell: the steering table has no rows and the
    /// `/proc` it reads is empty, so every program of the seat's user is an ordinary one.
    fn without_a_shell(active_uid: Option<u32>) -> Self {
        let nobody = ShellCheck::new(Prober::new(FixtureProc::new()), Table::empty());
        Self::start(active_uid, Reading::Missing, Box::new(nobody))
    }

    fn start(active_uid: Option<u32>, keymap: Reading, vouch: Box<dyn Vouch>) -> Self {
        let dir = Scratch::new("socket");
        let socket = socket_path(dir.path());
        let seat = dir.path().join("seat0");
        // The thread reads the seat file as it starts, so the file exists first.
        write_seat(&seat, active_uid);
        let listener = bind(&socket).unwrap();
        let (inbox, mailbox) = mailbox().unwrap();
        let handle = spawn(
            listener,
            inbox,
            SeatFile(seat.clone()),
            vouch,
            running(),
            keymap,
        )
        .unwrap();
        Self {
            _dir: dir,
            socket,
            seat,
            mailbox,
            handle,
        }
    }

    fn set_active(&self, uid: Option<u32>) {
        write_seat(&self.seat, uid);
    }

    /// Everything the reader would be woken for, waiting for `n` messages.
    fn requests(&self, n: usize) -> Vec<Request> {
        let deadline = Instant::now() + WAIT;
        let mut got = Vec::new();
        while got.len() < n && Instant::now() < deadline {
            got.extend(self.mailbox.drain().into_iter().map(|m| match m {
                Message::Request(r) => r,
                other => panic!("unexpected {other:?}"),
            }));
            std::thread::sleep(Duration::from_millis(5));
        }
        got
    }

    fn nothing_queued(&self) -> bool {
        self.mailbox.drain().is_empty()
    }
}

fn focus(name: &str) -> ToDaemon {
    ToDaemon::Focus {
        profile: ProfileName::parse(name).unwrap(),
    }
}

fn hello(status: Status) -> ToClient {
    ToClient::Hello {
        version: PROTOCOL_VERSION,
        status,
    }
}

#[test]
fn the_socket_is_world_connectable_and_a_stale_file_is_replaced() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Scratch::new("socket-bind");
    let path = socket_path(dir.path());
    std::fs::write(&path, "stale").unwrap();
    let _listener = bind(&path).unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o666);
    assert_eq!(
        path.file_name().and_then(|n| n.to_str()),
        Some("keycap.sock")
    );
}

#[test]
fn a_client_gets_hello_with_the_current_status_first() {
    let h = Harness::new(Some(own_uid()));
    let a = connect(&h.socket);
    assert_eq!(recv(&a), Some(hello(running())));

    h.handle.send(ToSocket::Status(paused()));
    assert_eq!(recv(&a), Some(ToClient::Status(paused())));

    let late = connect(&h.socket);
    assert_eq!(
        recv(&late),
        Some(hello(paused())),
        "late clients learn the latest status"
    );
}

#[test]
fn the_seat_owner_is_obeyed_and_everyone_hears_status() {
    let h = Harness::new(Some(own_uid()));
    let owner = connect(&h.socket);
    assert_eq!(recv(&owner), Some(hello(running())));

    send(&owner, &focus("terminal"));
    send(&owner, &ToDaemon::Resume);
    assert_eq!(
        h.requests(2),
        [
            Request::Focus(ProfileName::parse("terminal").unwrap()),
            Request::Resume
        ]
    );

    h.handle.send(ToSocket::Status(paused()));
    assert_eq!(recv(&owner), Some(ToClient::Status(paused())));
}

#[test]
fn a_user_config_reaches_the_reader_and_its_warnings_come_back_to_that_client_only() {
    let h = Harness::new(Some(own_uid()));
    let a = connect(&h.socket);
    let b = connect(&h.socket);
    assert_eq!(recv(&a), Some(hello(running())));
    assert_eq!(recv(&b), Some(hello(running())));

    send(
        &a,
        &ToDaemon::UserConfig {
            toml: "[profile.x]\nmodmap = 3\n".to_owned(),
        },
    );
    let requests = h.requests(1);
    let [Request::UserConfig { client, toml }] = requests.as_slice() else {
        panic!("{requests:?}");
    };
    assert_eq!(toml, "[profile.x]\nmodmap = 3\n");

    let warnings = vec![Warning {
        key: "profile.x.modmap".to_owned(),
        problem: "user config: not a table".to_owned(),
    }];
    h.handle.send(ToSocket::Warnings {
        client: *client,
        warnings: warnings.clone(),
    });
    assert_eq!(recv(&a), Some(ToClient::ConfigWarnings(warnings)));

    // b heard nothing of it: its next packet is the status pushed afterwards.
    h.handle.send(ToSocket::Status(paused()));
    assert_eq!(recv(&b), Some(ToClient::Status(paused())));
}

#[test]
fn an_observer_that_sends_a_request_is_dropped_and_nothing_is_relayed() {
    let h = Harness::new(Some(own_uid() + 1));
    let observer = connect(&h.socket);
    let bystander = connect(&h.socket);
    assert_eq!(recv(&observer), Some(hello(running())));
    assert_eq!(recv(&bystander), Some(hello(running())));

    send(&observer, &focus("terminal"));
    assert_eq!(recv(&observer), None, "an observer has no business asking");
    assert!(h.nothing_queued(), "the observer's request was not relayed");

    h.handle.send(ToSocket::Status(paused()));
    assert_eq!(
        recv(&bystander),
        Some(ToClient::Status(paused())),
        "others are unaffected, and still listening"
    );
}

#[test]
fn every_kind_of_request_from_anyone_but_the_shell_ends_its_connection() {
    let requests = [
        focus("terminal"),
        ToDaemon::UserConfig {
            toml: String::new(),
        },
        ToDaemon::Resume,
    ];
    let harnesses = [
        ("another user", Harness::new(Some(own_uid() + 1))),
        (
            "a program that is not the shell",
            Harness::without_a_shell(Some(own_uid())),
        ),
    ];
    for (who, h) in harnesses {
        for request in &requests {
            let client = connect(&h.socket);
            assert_eq!(recv(&client), Some(hello(running())));
            send(&client, request);
            assert_eq!(recv(&client), None, "{who}: {request:?}");
        }
        assert!(h.nothing_queued(), "{who}");
    }
}

#[test]
fn a_program_that_is_not_the_shell_may_watch_and_register_but_never_steers_or_counts_as_the_owner()
{
    let h = Harness::without_a_shell(Some(own_uid()));
    let app = connect(&h.socket);
    assert_eq!(recv(&app), Some(hello(running())));
    send(&app, &ToDaemon::Register(mail()));
    assert_eq!(registered(&app), [mail()], "its registration stands");
    send(&app, &ToDaemon::WatchKeymap);
    assert!(matches!(recv(&app), Some(ToClient::Keymap(_))));
    drop(app);

    let another = connect(&h.socket);
    assert_eq!(recv(&another), Some(hello(running())));
    drop(another);
    std::thread::sleep(Duration::from_millis(400));
    assert!(
        h.nothing_queued(),
        "its coming and going never resets the reader"
    );
}

#[test]
fn a_malformed_packet_closes_only_its_sender() {
    let h = Harness::new(Some(own_uid()));
    let sender = connect(&h.socket);
    let bystander = connect(&h.socket);
    assert_eq!(recv(&sender), Some(hello(running())));
    assert_eq!(recv(&bystander), Some(hello(running())));

    send(&sender, &focus("terminal"));
    rustix::net::send(&sender, b"not json", SendFlags::NOSIGNAL).unwrap();
    assert_eq!(recv(&sender), None, "the sender of garbage is dropped");
    assert_eq!(
        h.requests(1),
        [Request::Focus(ProfileName::parse("terminal").unwrap())],
        "the message before the garbage was read"
    );
    assert!(h.nothing_queued());

    h.handle.send(ToSocket::Status(paused()));
    assert_eq!(recv(&bystander), Some(ToClient::Status(paused())));
}

#[test]
fn a_client_beyond_the_per_user_cap_is_closed_at_once_and_the_others_are_served() {
    let h = Harness::new(Some(own_uid()));
    let admitted: Vec<_> = (0..limits::MAX_CLIENTS_PER_UID)
        .map(|_| {
            let c = connect(&h.socket);
            assert_eq!(recv(&c), Some(hello(running())));
            c
        })
        .collect();
    let refused = connect(&h.socket);
    assert_eq!(recv(&refused), None, "closed without a hello");

    h.handle.send(ToSocket::Status(paused()));
    for client in &admitted {
        assert_eq!(recv(client), Some(ToClient::Status(paused())));
    }

    drop(admitted);
    drop(refused);
    // Their places are free again once the daemon has seen them go: a new client is let in.
    let deadline = Instant::now() + WAIT;
    let welcomed = loop {
        let c = connect(&h.socket);
        if let Some(ToClient::Hello { .. }) = recv(&c) {
            break true;
        }
        if Instant::now() > deadline {
            break false;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(
        welcomed,
        "the cap is on live clients, not on connections ever made"
    );
}

#[test]
fn a_flood_from_one_client_is_all_delivered_in_order_without_starving_the_others() {
    let h = Harness::new(Some(own_uid()));
    let flooder = connect(&h.socket);
    let other = connect(&h.socket);
    assert_eq!(recv(&flooder), Some(hello(running())));
    assert_eq!(recv(&other), Some(hello(running())));

    let names: Vec<String> = (0..40).map(|i| format!("p{i}")).collect();
    names.iter().for_each(|n| send(&flooder, &focus(n)));
    let got = h.requests(names.len());
    let want: Vec<Request> = names
        .iter()
        .map(|n| Request::Focus(ProfileName::parse(n).unwrap()))
        .collect();
    assert_eq!(got, want, "every message arrives, in order, across passes");

    h.handle.send(ToSocket::Status(paused()));
    assert_eq!(recv(&other), Some(ToClient::Status(paused())));
}

#[test]
fn an_oversized_packet_is_malformed() {
    let h = Harness::new(Some(own_uid()));
    let client = connect(&h.socket);
    assert_eq!(recv(&client), Some(hello(running())));
    rustix::net::send(
        &client,
        &vec![b' '; protocol::MAX_PACKET + 1],
        SendFlags::NOSIGNAL,
    )
    .unwrap();
    assert_eq!(recv(&client), None);
    // The owner's only connection ended, so the seat is vacated; the oversized packet itself
    // was relayed as nothing.
    assert_eq!(
        h.requests(2),
        [
            Request::Focus(ProfileName::default_profile()),
            Request::ClearUserConfig
        ]
    );
    assert!(h.nothing_queued());
}

#[test]
fn the_owner_leaving_sends_the_default_focus_and_clears_the_user_config() {
    let h = Harness::new(Some(own_uid()));
    let first = connect(&h.socket);
    let second = connect(&h.socket);
    assert_eq!(recv(&first), Some(hello(running())));
    assert_eq!(recv(&second), Some(hello(running())));
    send(&first, &focus("terminal"));
    assert_eq!(h.requests(1).len(), 1);

    drop(first);
    // The seat still has an owner connection, so nothing is vacated.
    send(&second, &ToDaemon::Resume);
    assert_eq!(
        h.requests(1),
        [Request::Resume],
        "no reset while one connection remains"
    );

    drop(second);
    assert_eq!(
        h.requests(2),
        [
            Request::Focus(ProfileName::default_profile()),
            Request::ClearUserConfig
        ]
    );
}

#[test]
fn a_new_active_user_resets_the_reader_and_the_old_owner_is_no_longer_obeyed() {
    let h = Harness::new(Some(own_uid()));
    let client = connect(&h.socket);
    assert_eq!(recv(&client), Some(hello(running())));
    send(&client, &focus("terminal"));
    assert_eq!(h.requests(1).len(), 1);

    h.set_active(Some(own_uid() + 1));
    assert_eq!(
        h.requests(2),
        [
            Request::Focus(ProfileName::default_profile()),
            Request::ClearUserConfig
        ],
        "noticed within the periodic re-read"
    );
    send(&client, &focus("terminal"));
    assert_eq!(recv(&client), None, "the former owner is now an observer");
    assert!(h.nothing_queued(), "and its request was not relayed");
}

#[test]
fn no_seat_file_means_nobody_is_obeyed() {
    let h = Harness::new(None);
    std::fs::remove_file(&h.seat).unwrap();
    let client = connect(&h.socket);
    assert_eq!(recv(&client), Some(hello(running())));
    send(&client, &focus("terminal"));
    assert_eq!(recv(&client), None, "an observer asking is dropped");
    assert!(h.nothing_queued());
}

fn text(t: &str) -> Reading {
    Reading::Text(t.to_owned())
}

fn keymap_of(overrides: &str) -> ToClient {
    ToClient::Keymap(KeymapState {
        overrides: overrides.to_owned(),
        problems: Vec::new(),
    })
}

fn mail() -> Registration {
    Registration {
        app: "mail".to_owned(),
        actions: vec![RegisteredAction {
            id: "mail.compose".to_owned(),
            chord: "Primary+Shift+N".to_owned(),
        }],
    }
}

/// What `client` is told about the registered apps right now.
fn registered(client: &rustix::fd::OwnedFd) -> Vec<Registration> {
    send(client, &ToDaemon::ListRegistered);
    loop {
        match recv(client) {
            Some(ToClient::Registered(all)) => return all,
            Some(_) => {}
            None => panic!("the daemon hung up"),
        }
    }
}

#[test]
fn a_watcher_hears_the_keymap_at_once_and_again_on_each_change_and_others_never() {
    let h = Harness::with_keymap(Some(own_uid()), text("copy = Primary+Alt+C\n"));
    let watcher = connect(&h.socket);
    let bystander = connect(&h.socket);
    assert_eq!(recv(&watcher), Some(hello(running())));
    assert_eq!(recv(&bystander), Some(hello(running())));

    send(&watcher, &ToDaemon::WatchKeymap);
    let Some(ToClient::Keymap(first)) = recv(&watcher) else {
        panic!("no keymap");
    };
    assert!(first.overrides.starts_with("copy = "), "{first:?}");
    assert_eq!(first.problems, []);

    h.handle
        .send(ToSocket::Keymap(text("paste = Primary+Alt+V\n")));
    let Some(ToClient::Keymap(second)) = recv(&watcher) else {
        panic!("no change");
    };
    assert!(second.overrides.starts_with("paste = "), "{second:?}");

    // A re-read that changes nothing is not news; the next thing heard is the status.
    h.handle
        .send(ToSocket::Keymap(text("paste = Primary+Alt+V\n")));
    h.handle.send(ToSocket::Status(paused()));
    assert_eq!(recv(&watcher), Some(ToClient::Status(paused())));
    assert_eq!(
        recv(&bystander),
        Some(ToClient::Status(paused())),
        "a client that never asked hears no keymap"
    );
}

#[test]
fn a_bad_edit_keeps_what_watchers_hold_and_tells_them_why() {
    let h = Harness::with_keymap(Some(own_uid()), text("copy = Primary+Alt+C\n"));
    let watcher = connect(&h.socket);
    assert_eq!(recv(&watcher), Some(hello(running())));
    send(&watcher, &ToDaemon::WatchKeymap);
    let Some(ToClient::Keymap(good)) = recv(&watcher) else {
        panic!("no keymap");
    };
    h.handle
        .send(ToSocket::Keymap(text("this is not a keymap\n")));
    let Some(ToClient::Keymap(kept)) = recv(&watcher) else {
        panic!("no problem report");
    };
    assert_eq!(kept.overrides, good.overrides);
    assert!(!kept.problems.is_empty());
}

#[test]
fn the_keymap_is_open_to_observers_who_stay_connected_but_registering_is_not() {
    let h = Harness::with_keymap(Some(own_uid() + 1), text("undo = none\n"));
    let observer = connect(&h.socket);
    assert_eq!(recv(&observer), Some(hello(running())));
    send(&observer, &ToDaemon::WatchKeymap);
    assert_eq!(recv(&observer), Some(keymap_of("undo = none\n")));
    send(&observer, &ToDaemon::Register(mail()));
    assert_eq!(
        registered(&observer),
        [],
        "an observer's registration is ignored"
    );
    send(&observer, &focus("terminal"));
    assert_eq!(recv(&observer), None, "a real request is still refused");
}

#[test]
fn registrations_are_listed_to_everyone_and_end_with_their_connection() {
    let h = Harness::new(Some(own_uid()));
    let app = connect(&h.socket);
    let settings = connect(&h.socket);
    assert_eq!(recv(&app), Some(hello(running())));
    assert_eq!(recv(&settings), Some(hello(running())));

    send(&app, &ToDaemon::Register(mail()));
    // The registration and the list travel on different connections, so ask until it shows.
    let deadline = Instant::now() + WAIT;
    while registered(&settings).is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(registered(&settings), [mail()]);

    drop(app);
    let deadline = Instant::now() + WAIT;
    while !registered(&settings).is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(registered(&settings), []);
    assert!(h.nothing_queued(), "registering reached the reader");
}

#[test]
fn a_keymap_watcher_coming_and_going_never_resets_the_reader() {
    let h = Harness::new(Some(own_uid()));
    for _ in 0..3 {
        let watcher = connect(&h.socket);
        assert_eq!(recv(&watcher), Some(hello(running())));
        send(&watcher, &ToDaemon::WatchKeymap);
        assert!(matches!(recv(&watcher), Some(ToClient::Keymap(_))));
        drop(watcher);
    }
    let registrar = connect(&h.socket);
    assert_eq!(recv(&registrar), Some(hello(running())));
    send(&registrar, &ToDaemon::Register(mail()));
    drop(registrar);
    std::thread::sleep(Duration::from_millis(400));
    assert!(h.nothing_queued(), "no focus reset, no cleared user config");
}

#[test]
fn keymap_connections_do_not_use_up_the_owners_share_and_a_real_owner_still_resets_on_leaving() {
    let h = Harness::new(Some(own_uid()));
    let watchers: Vec<_> = (0..limits::MAX_CLIENTS_PER_UID + 4)
        .map(|_| {
            let c = connect(&h.socket);
            assert_eq!(recv(&c), Some(hello(running())));
            send(&c, &ToDaemon::WatchKeymap);
            assert!(matches!(recv(&c), Some(ToClient::Keymap(_))));
            c
        })
        .collect();
    let owner = connect(&h.socket);
    assert_eq!(recv(&owner), Some(hello(running())), "room for the owner");
    send(&owner, &focus("terminal"));
    assert_eq!(h.requests(1).len(), 1);
    drop(watchers);
    drop(owner);
    assert_eq!(
        h.requests(2),
        [
            Request::Focus(ProfileName::default_profile()),
            Request::ClearUserConfig
        ]
    );
}
