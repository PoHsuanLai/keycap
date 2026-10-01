//! The socket thread: accepts clients, tells each its `Hello`, relays seat-owner requests to
//! the reader and the reader's status to everyone.

use super::handle::{SocketHandle, ToSocket};
use super::seat::{Peer, Seat, SeatFile, Tenancy, peer_of, request_of};
use crate::daemon::inbox::{ClientId, Inbox, Message, Request};
use crate::daemon::waker::Waker;
use keycap_protocol::{
    self as protocol, MAX_PACKET, PROTOCOL_VERSION, ProfileName, Status, ToClient, ToDaemon,
};
use rustix::event::{PollFd, PollFlags, poll};
use rustix::fd::{AsFd, OwnedFd};
use rustix::io::Errno;
use rustix::net::{AddressFamily, RecvFlags, SendFlags, SocketAddrUnix, SocketFlags, SocketType};
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

/// Starts the socket thread on a bound listener. `status` is what the first `Hello` carries.
pub(crate) fn spawn(
    listener: OwnedFd,
    inbox: Inbox,
    seat: SeatFile,
    status: Status,
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
        status,
        state: Seat::first_sight(seat.active_uid()),
        seat,
    };
    std::thread::Builder::new()
        .name("keycap-socket".to_owned())
        .spawn(move || server.run())?;
    Ok(SocketHandle { tx, waker })
}

struct Client {
    id: ClientId,
    fd: OwnedFd,
    /// The uid the kernel recorded when the client connected.
    uid: u32,
}

struct Server {
    listener: OwnedFd,
    waker: Waker,
    rx: Receiver<ToSocket>,
    inbox: Inbox,
    clients: Vec<Client>,
    next_client: u64,
    status: Status,
    seat: SeatFile,
    state: Seat,
}

/// What a poll found readable.
struct Ready {
    listener: bool,
    wake: bool,
    clients: Vec<ClientId>,
}

impl Server {
    fn run(mut self) {
        loop {
            let ready = match self.wait() {
                Ok(ready) => ready,
                Err(e) => {
                    eprintln!("keycap: socket poll failed: {e}");
                    return;
                }
            };
            let active = self.seat.active_uid();
            self.watch_seat(active);
            if ready.listener {
                self.accept();
            }
            if ready.wake && !self.drain() {
                return;
            }
            ready
                .clients
                .into_iter()
                .for_each(|id| self.receive(id, active));
            self.watch_seat(active);
        }
    }

    fn wait(&self) -> io::Result<Ready> {
        let timeout = Timespec {
            tv_sec: SEAT_RECHECK.as_secs().try_into().unwrap_or(1),
            tv_nsec: 0,
        };
        let wake = self.waker.fd();
        let mut fds = vec![
            PollFd::new(&self.listener, PollFlags::IN),
            PollFd::new(&wake, PollFlags::IN),
        ];
        fds.extend(
            self.clients
                .iter()
                .map(|c| PollFd::new(&c.fd, PollFlags::IN)),
        );
        match poll(&mut fds, Some(&timeout)) {
            Ok(_) | Err(Errno::INTR) => {}
            Err(e) => return Err(e.into()),
        }
        let hit = |f: &PollFd<'_>| !f.revents().is_empty();
        Ok(Ready {
            listener: hit(&fds[0]),
            wake: hit(&fds[1]),
            clients: self
                .clients
                .iter()
                .zip(&fds[2..])
                .filter(|(_, f)| hit(f))
                .map(|(c, _)| c.id)
                .collect(),
        })
    }

    fn accept(&mut self) {
        while let Ok(fd) =
            rustix::net::accept_with(&self.listener, SocketFlags::CLOEXEC | SocketFlags::NONBLOCK)
        {
            let Ok(cred) = rustix::net::sockopt::socket_peercred(&fd) else {
                continue;
            };
            let client = Client {
                id: ClientId(self.next_client),
                fd,
                uid: cred.uid.as_raw(),
            };
            self.next_client += 1;
            let hello = ToClient::Hello {
                version: PROTOCOL_VERSION,
                status: self.status.clone(),
            };
            if send(&client, &hello) {
                self.clients.push(client);
            }
        }
    }

    /// Delivers what the reader queued; false when the reader has gone (shutdown).
    fn drain(&mut self) -> bool {
        self.waker.clear();
        loop {
            match self.rx.try_recv() {
                Ok(ToSocket::Status(status)) => {
                    self.status = status.clone();
                    self.broadcast(&ToClient::Status(status));
                }
                Ok(ToSocket::Warnings { client, warnings }) => {
                    let reply = ToClient::ConfigWarnings(warnings);
                    self.clients.retain(|c| c.id != client || send(c, &reply));
                }
                Err(mpsc::TryRecvError::Empty) => return true,
                Err(mpsc::TryRecvError::Disconnected) => return false,
            }
        }
    }

    fn broadcast(&mut self, message: &ToClient) {
        self.clients.retain(|c| send(c, message));
    }

    /// Reads what client `id` sent. A client that closes, errors or sends anything that is not
    /// a message of the protocol is dropped; nobody else is affected.
    fn receive(&mut self, id: ClientId, active: Option<u32>) {
        let Some(client) = self.clients.iter().find(|c| c.id == id) else {
            return;
        };
        let peer = peer_of(client.uid, active);
        let mut buf = vec![0u8; MAX_PACKET + 1];
        let keep = loop {
            match rustix::net::recv(&client.fd, &mut buf, RecvFlags::DONTWAIT) {
                Ok((0, _)) => break false,
                Ok((n, _)) => match protocol::decode::<ToDaemon>(&buf[..n]) {
                    Ok(message) => self.relay(peer, id, message),
                    Err(e) => {
                        eprintln!("keycap: dropping a client that sent a malformed message: {e}");
                        break false;
                    }
                },
                Err(Errno::AGAIN) => break true,
                Err(_) => break false,
            }
        };
        if !keep {
            self.clients.retain(|c| c.id != id);
        }
    }

    fn relay(&self, peer: Peer, client: ClientId, message: ToDaemon) {
        match request_of(peer, client, message) {
            Some(request) => self.inbox.send(Message::Request(request)),
            None => {
                eprintln!("keycap: ignoring a message from a client that does not own the seat")
            }
        }
    }

    /// Looks at the seat: when its owner is gone or changed, the reader goes back to `default`
    /// and forgets the owner's settings.
    fn watch_seat(&mut self, active: Option<u32>) {
        let owners = self
            .clients
            .iter()
            .filter(|c| peer_of(c.uid, active) == Peer::SeatOwner)
            .count();
        let (state, tenancy) = self.state.look(active, owners);
        self.state = state;
        if tenancy == Tenancy::Vacated {
            self.inbox.send(Message::Request(Request::Focus(
                ProfileName::default_profile(),
            )));
            self.inbox.send(Message::Request(Request::ClearUserConfig));
        }
    }
}

/// Sends one message; false if the client is gone. A full queue drops the message only.
fn send(client: &Client, message: &ToClient) -> bool {
    match rustix::net::send(
        client.fd.as_fd(),
        &protocol::encode(message),
        SendFlags::DONTWAIT | SendFlags::NOSIGNAL,
    ) {
        Ok(_) | Err(Errno::AGAIN) => true,
        Err(_) => false,
    }
}
