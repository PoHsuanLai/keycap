//! A client's end of the socket, for tests.

use keycap_protocol::{self as protocol, ToClient, ToDaemon};
use rustix::fd::OwnedFd;
use rustix::net::{AddressFamily, RecvFlags, SendFlags, SocketAddrUnix, SocketType};
use std::path::Path;
use std::time::Duration;

/// How long a test waits for the daemon before giving up.
pub(crate) const WAIT: Duration = Duration::from_secs(5);

pub(crate) fn connect(path: &Path) -> OwnedFd {
    let fd = rustix::net::socket(AddressFamily::UNIX, SocketType::SEQPACKET, None).unwrap();
    rustix::net::connect(&fd, &SocketAddrUnix::new(path).unwrap()).unwrap();
    rustix::net::sockopt::set_socket_timeout(&fd, rustix::net::sockopt::Timeout::Recv, Some(WAIT))
        .unwrap();
    fd
}

/// What the daemon sent next; `None` when it closed the connection.
pub(crate) fn recv(fd: &OwnedFd) -> Option<ToClient> {
    let mut buf = vec![0u8; protocol::MAX_PACKET];
    match rustix::net::recv(fd, &mut buf, RecvFlags::empty()) {
        Ok((0, _)) | Err(_) => None,
        Ok((n, _)) => Some(protocol::decode(&buf[..n]).unwrap()),
    }
}

pub(crate) fn send(fd: &OwnedFd, message: &ToDaemon) {
    rustix::net::send(fd, &protocol::encode(message), SendFlags::NOSIGNAL).unwrap();
}
