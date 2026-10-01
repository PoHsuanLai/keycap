//! The socket thread end to end on a private path with a fake seat file.

use super::client::{WAIT, connect, recv, send};
use super::*;
use crate::daemon::inbox::{Mailbox, Message, Request, mailbox};
use crate::daemon::scratch::Scratch;
use keycap_protocol::{
    self as protocol, Mode, PROTOCOL_VERSION, ProfileName, Status, ToClient, ToDaemon, Warning,
};
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
        let dir = Scratch::new("socket");
        let socket = socket_path(dir.path());
        let seat = dir.path().join("seat0");
        // The thread reads the seat file as it starts, so the file exists first.
        write_seat(&seat, active_uid);
        let listener = bind(&socket).unwrap();
        let (inbox, mailbox) = mailbox().unwrap();
        let handle = spawn(listener, inbox, SeatFile(seat.clone()), running()).unwrap();
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
fn an_observer_is_ignored_and_a_malformed_packet_closes_only_its_sender() {
    let h = Harness::new(Some(own_uid() + 1));
    let observer = connect(&h.socket);
    let bystander = connect(&h.socket);
    assert_eq!(recv(&observer), Some(hello(running())));
    assert_eq!(recv(&bystander), Some(hello(running())));

    send(&observer, &focus("terminal"));
    send(
        &observer,
        &ToDaemon::UserConfig {
            toml: String::new(),
        },
    );
    send(&observer, &ToDaemon::Resume);
    // Garbage ends the conversation; by the time the daemon hangs up, it has read the
    // messages before it.
    rustix::net::send(&observer, b"not json", SendFlags::NOSIGNAL).unwrap();
    assert_eq!(recv(&observer), None, "the sender of garbage is dropped");
    assert!(
        h.nothing_queued(),
        "the observer's requests were not relayed"
    );

    h.handle.send(ToSocket::Status(paused()));
    assert_eq!(
        recv(&bystander),
        Some(ToClient::Status(paused())),
        "others are unaffected"
    );
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
    rustix::net::send(&client, b"x", SendFlags::NOSIGNAL).unwrap();
    assert_eq!(recv(&client), None);
    assert!(h.nothing_queued(), "the former owner is now an observer");
}

#[test]
fn no_seat_file_means_nobody_is_obeyed() {
    let h = Harness::new(None);
    std::fs::remove_file(&h.seat).unwrap();
    let client = connect(&h.socket);
    assert_eq!(recv(&client), Some(hello(running())));
    send(&client, &focus("terminal"));
    rustix::net::send(&client, b"x", SendFlags::NOSIGNAL).unwrap();
    assert_eq!(recv(&client), None);
    assert!(h.nothing_queued());
}
