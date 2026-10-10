//! The socket thread: accepts clients, tells each its `Hello`, relays seat-owner requests to
//! the reader and the reader's status to everyone.

use super::handle::{SocketHandle, ToSocket};
use super::inbound::{Stream, pull};
use super::limits::{
    ACCEPTS_PER_PASS, AcceptFailure, Admission, Caps, Complaints, Intake,
    MAX_KEYMAP_CLIENTS_PER_UID, MESSAGES_PER_PASS, Voice, accept_failure, admit,
};
use super::seat::{Peer, Seat, SeatFile, Tenancy, peer_of, request_of};
use super::shelf::{Call, Reply, Shelf};
use crate::daemon::clock;
use crate::daemon::inbox::{ClientId, Inbox, Message, Request};
use crate::daemon::readiness::Readiness;
use crate::daemon::waker::Waker;
use crate::keymap::Reading;
use keycap_protocol::{
    self as protocol, PROTOCOL_VERSION, ProfileName, Status, ToClient, ToDaemon,
};
use rustix::event::{PollFd, PollFlags, poll};
use rustix::fd::{AsFd, OwnedFd};
use rustix::io::Errno;
use rustix::net::{AddressFamily, SendFlags, SocketAddrUnix, SocketFlags, SocketType};
use rustix::time::Timespec;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

/// The socket's file name inside the runtime directory.
const SOCKET_NAME: &str = "keycap.sock";

/// How often the seat file is read when no client says anything.
const SEAT_RECHECK: Duration = Duration::from_secs(1);

/// `<runtime_dir>/keycap.sock`.
pub(crate) fn socket_path(runtime_dir: &Path) -> PathBuf {
    runtime_dir.join(SOCKET_NAME)
}

/// Binds the listening socket, replacing a stale file, mode 0666 so any local desktop can
/// connect (the seat check, not the file mode, decides whom keycap obeys).
pub(crate) fn bind(path: &Path) -> io::Result<OwnedFd> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    let fd = rustix::net::socket_with(
        AddressFamily::UNIX,
        SocketType::SEQPACKET,
        SocketFlags::CLOEXEC | SocketFlags::NONBLOCK,
        None,
    )?;
    rustix::net::bind(&fd, &SocketAddrUnix::new(path)?)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o666))?;
    rustix::net::listen(&fd, 16)?;
    Ok(fd)
}

/// Starts the socket thread on a bound listener. `status` is what the first `Hello` carries and
/// `keymap` the first reading of the keymap file, served to those who ask.
pub(crate) fn spawn(
    listener: OwnedFd,
    inbox: Inbox,
    seat: SeatFile,
    status: Status,
    keymap: Reading,
) -> io::Result<SocketHandle> {
    let waker = Waker::new()?;
    let (tx, rx) = mpsc::channel();
    let server = Server {
        listener,
        waker: waker.clone(),
        rx,
        inbox,
        clients: Vec::new(),
        next_client: 0,
        intake: Intake::Open,
        complaints: Complaints::default(),
        status,
        shelf: Shelf::starting_with(keymap),
        state: Seat::first_sight(seat.active_uid()),
        seat,
    };
    std::thread::Builder::new()
        .name("keycap-socket".to_owned())
        .spawn(move || server.run())?;
    Ok(SocketHandle { tx, waker })
}

/// What a connection has shown itself to be. The seat owner's connections steer keycap; a
/// connection that only reads the keymap or registers actions is an observer for the seat even
/// when it runs as the seat owner's uid, so its coming and going never resets the reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    /// Nothing sent yet; counted with the owners, as every connection was before.
    Undecided,
    /// Has sent a request to the reader.
    Steering,
    /// Has used only the keymap messages.
    KeymapOnly,
}

struct Client {
    id: ClientId,
    fd: OwnedFd,
    /// The uid the kernel recorded when the client connected.
    uid: u32,
    role: Role,
}

struct Server {
    listener: OwnedFd,
    waker: Waker,
    rx: Receiver<ToSocket>,
    inbox: Inbox,
    clients: Vec<Client>,
    next_client: u64,
    /// Whether the listener is polled: not while `accept` is failing for want of descriptors.
    intake: Intake,
    /// When each uid was last complained about in the log.
    complaints: Complaints,
    status: Status,
    /// The keymap, who watches it and what each connection registered.
    shelf: Shelf,
    seat: SeatFile,
    state: Seat,
}

/// What a poll found readable.
struct Ready {
    listener: Readiness,
    wake: Readiness,
    clients: Vec<ClientId>,
}

/// Whether a client is still connected after something was done to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Link {
    Alive,
    Gone,
}

/// The reader dropped its end of the channel: the daemon is shutting down.
struct ReaderGone;

impl Server {
    fn run(mut self) {
        loop {
            self.intake = self.intake.at(clock::now());
            let ready = match self.wait() {
                Ok(ready) => ready,
                Err(e) => {
                    eprintln!("keycap: socket poll failed: {e}");
                    return;
                }
            };
            let active = self.seat.active_uid();
            self.watch_seat(active);
            if ready.listener == Readiness::Readable {
                self.accept();
            }
            if ready.wake == Readiness::Readable && self.drain().is_err() {
                return;
            }
            ready
                .clients
                .into_iter()
                .for_each(|id| self.receive(id, active));
            self.watch_seat(active);
            let live: Vec<ClientId> = self.clients.iter().map(|c| c.id).collect();
            self.shelf.retain(&live);
        }
    }

    fn wait(&self) -> io::Result<Ready> {
        let sleep = self.intake.sleep(clock::now(), SEAT_RECHECK);
        let timeout = Timespec {
            tv_sec: sleep.as_secs().try_into().unwrap_or(1),
            tv_nsec: i64::from(sleep.subsec_nanos()),
        };
        let wake = self.waker.fd();
        let mut fds = vec![PollFd::new(&wake, PollFlags::IN)];
        let listener_slot = (self.intake == Intake::Open).then(|| {
            fds.push(PollFd::new(&self.listener, PollFlags::IN));
            fds.len() - 1
        });
        let first_client = fds.len();
        fds.extend(
            self.clients
                .iter()
                .map(|c| PollFd::new(&c.fd, PollFlags::IN)),
        );
        match poll(&mut fds, Some(&timeout)) {
            Ok(_) | Err(Errno::INTR) => {}
            Err(e) => return Err(e.into()),
        }
        Ok(Ready {
            listener: listener_slot.map_or(Readiness::Quiet, |slot| Readiness::of(&fds[slot])),
            wake: Readiness::of(&fds[0]),
            clients: self
                .clients
                .iter()
                .zip(&fds[first_client..])
                .filter(|(_, f)| Readiness::of(f) == Readiness::Readable)
                .map(|(c, _)| c.id)
                .collect(),
        })
    }

    /// Accepts what is waiting, a bounded number per pass. A failure that retrying cannot fix
    /// (no descriptors left) leaves the connection pending, so the listener is not polled for a
    /// while instead of waking the loop in a spin.
    fn accept(&mut self) {
        for _ in 0..ACCEPTS_PER_PASS {
            let accepted = rustix::net::accept_with(
                &self.listener,
                SocketFlags::CLOEXEC | SocketFlags::NONBLOCK,
            );
            match accepted {
                Ok(fd) => self.admit(fd),
                Err(e) => match accept_failure(e) {
                    AcceptFailure::NothingPending => return,
                    AcceptFailure::Retry => {}
                    AcceptFailure::Pause => {
                        eprintln!("keycap: accept failed: {e}; not accepting for a moment");
                        self.intake = Intake::paused_at(clock::now());
                        return;
                    }
                },
            }
        }
    }

    /// Lets a connection in, or closes it at once when the daemon or its uid has enough.
    fn admit(&mut self, fd: OwnedFd) {
        let Ok(cred) = rustix::net::sockopt::socket_peercred(&fd) else {
            return;
        };
        let uid = cred.uid.as_raw();
        match admit(
            self.clients
                .iter()
                .filter(|c| c.role != Role::KeymapOnly)
                .map(|c| c.uid),
            uid,
            self.seat.active_uid(),
            Caps::SERVICE,
        ) {
            Admission::Admit => {}
            Admission::TooManyClients => return self.complain(uid, "refused: the daemon is full"),
            Admission::TooManyForUser => {
                return self.complain(uid, "refused: too many connections for this user");
            }
        }
        let client = Client {
            id: ClientId(self.next_client),
            fd,
            uid,
            role: Role::Undecided,
        };
        self.next_client += 1;
        let hello = ToClient::Hello {
            version: PROTOCOL_VERSION,
            status: self.status.clone(),
        };
        if send(&client, &hello) == Link::Alive {
            self.clients.push(client);
        }
    }

    /// Writes a complaint about `uid` to the log, at most once a minute per uid.
    fn complain(&mut self, uid: u32, what: &str) {
        let (complaints, voice) = std::mem::take(&mut self.complaints).note(uid, clock::now());
        self.complaints = complaints;
        if voice == Voice::Speak {
            eprintln!("keycap: client uid {uid}: {what}");
        }
    }

    /// Delivers what the reader queued; an error when the reader has gone (shutdown).
    fn drain(&mut self) -> Result<(), ReaderGone> {
        self.waker.clear();
        loop {
            match self.rx.try_recv() {
                Ok(ToSocket::Status(status)) => {
                    self.status = status.clone();
                    self.broadcast(&ToClient::Status(status));
                }
                Ok(ToSocket::Warnings { client, warnings }) => {
                    let reply = ToClient::ConfigWarnings(warnings);
                    self.clients
                        .retain(|c| c.id != client || send(c, &reply) == Link::Alive);
                }
                Ok(ToSocket::Keymap(reading)) => {
                    if let Some(replies) = self.shelf.read(reading) {
                        self.shelf
                            .state()
                            .problems
                            .iter()
                            .for_each(|p| eprintln!("keycap: keymap: line {}: {}", p.line, p.what));
                        self.deliver(replies);
                    }
                }
                Err(mpsc::TryRecvError::Empty) => return Ok(()),
                Err(mpsc::TryRecvError::Disconnected) => return Err(ReaderGone),
            }
        }
    }

    /// Sends each reply to its client; one that cannot be reached is dropped.
    fn deliver(&mut self, replies: Vec<Reply>) {
        let gone: Vec<ClientId> = replies
            .iter()
            .filter(|r| {
                self.clients
                    .iter()
                    .find(|c| c.id == r.to)
                    .is_some_and(|c| send(c, &r.message) == Link::Gone)
            })
            .map(|r| r.to)
            .collect();
        self.clients.retain(|c| !gone.contains(&c.id));
    }

    fn broadcast(&mut self, message: &ToClient) {
        self.clients.retain(|c| send(c, message) == Link::Alive);
    }

    /// Reads what client `id` sent, a bounded number of messages per pass. A client that
    /// closes, errors, sends anything that is not a message of the protocol, or sends a
    /// request without owning the seat is dropped; nobody else is affected.
    fn receive(&mut self, id: ClientId, active: Option<u32>) {
        let Some(client) = self.clients.iter().find(|c| c.id == id) else {
            return;
        };
        let (uid, peer) = (client.uid, peer_of(client.uid, active));
        let pulled = pull(&client.fd, MESSAGES_PER_PASS);
        let mut link = match pulled.stream {
            Stream::Closed => Link::Gone,
            Stream::More | Stream::Idle => Link::Alive,
        };
        for packet in pulled.packets {
            match protocol::decode::<ToDaemon>(&packet) {
                Ok(message) => match Call::of(message) {
                    Ok(call) => {
                        if self.keymap_only(id, uid, active) == Link::Gone {
                            self.complain(uid, "too many keymap connections; dropped");
                            link = Link::Gone;
                            break;
                        }
                        let replies = self.shelf.call(id, call);
                        self.deliver(replies);
                    }
                    Err(message) => match request_of(peer, id, message) {
                        Ok(request) => {
                            self.set_role(id, Role::Steering);
                            self.inbox.send(Message::Request(request));
                        }
                        Err(refusal) => {
                            self.complain(uid, refusal.what());
                            link = Link::Gone;
                            break;
                        }
                    },
                },
                Err(e) => {
                    self.complain(uid, &format!("sent a malformed message ({e}); dropped"));
                    link = Link::Gone;
                    break;
                }
            }
        }
        if link == Link::Gone {
            self.clients.retain(|c| c.id != id);
        }
    }

    fn set_role(&mut self, id: ClientId, role: Role) {
        if let Some(client) = self.clients.iter_mut().find(|c| c.id == id) {
            client.role = role;
        }
    }

    /// A keymap message from `id`: a connection that has not steered anything is an observer
    /// from now on, which is no change of the seat's owner (it was never one), so the seat is
    /// brought up to date without a reset. `Gone` when `uid` already has its share.
    fn keymap_only(&mut self, id: ClientId, uid: u32, active: Option<u32>) -> Link {
        let current = self.clients.iter().find(|c| c.id == id).map(|c| c.role);
        if current != Some(Role::Undecided) {
            return Link::Alive;
        }
        let sharing = self
            .clients
            .iter()
            .filter(|c| c.uid == uid && c.role == Role::KeymapOnly)
            .count();
        if sharing >= MAX_KEYMAP_CLIENTS_PER_UID {
            return Link::Gone;
        }
        self.set_role(id, Role::KeymapOnly);
        let (state, _) = self.state.look(active, self.owners(active));
        self.state = state;
        Link::Alive
    }

    /// The connections of the seat's owner that count as the owner being present.
    fn owners(&self, active: Option<u32>) -> usize {
        self.clients
            .iter()
            .filter(|c| c.role != Role::KeymapOnly && peer_of(c.uid, active) == Peer::SeatOwner)
            .count()
    }

    /// Looks at the seat: when its owner is gone or changed, the reader goes back to `default`
    /// and forgets the owner's settings.
    fn watch_seat(&mut self, active: Option<u32>) {
        let (state, tenancy) = self.state.look(active, self.owners(active));
        self.state = state;
        if tenancy == Tenancy::Vacated {
            self.inbox.send(Message::Request(Request::Focus(
                ProfileName::default_profile(),
            )));
            self.inbox.send(Message::Request(Request::ClearUserConfig));
        }
    }
}

/// Sends one message; `Gone` if the client is. A full queue drops the message only.
fn send(client: &Client, message: &ToClient) -> Link {
    match rustix::net::send(
        client.fd.as_fd(),
        &protocol::encode(message),
        SendFlags::DONTWAIT | SendFlags::NOSIGNAL,
    ) {
        Ok(_) | Err(Errno::AGAIN) => Link::Alive,
        Err(_) => Link::Gone,
    }
}
