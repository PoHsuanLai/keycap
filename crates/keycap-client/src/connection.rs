//! The socket: connect, check `Hello`, send and receive one message per packet.

use keycap_protocol::{
    self as protocol, Compat, KEYMAP_PROTOCOL_VERSION, MAX_PACKET, MIN_PROTOCOL_VERSION,
    PROTOCOL_VERSION, Status, ToClient, ToDaemon,
};
use rustix::fd::OwnedFd;
use rustix::io::Errno;
use rustix::net::sockopt::{Timeout, set_socket_timeout};
use rustix::net::{AddressFamily, RecvFlags, SendFlags, SocketAddrUnix, SocketFlags, SocketType};
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Why talking to keycap failed, in plain words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientError {
    /// Nothing is listening at the path.
    Connect { path: PathBuf, reason: String },
    /// Sending or receiving failed.
    Io(String),
    /// A packet was not a message of the protocol, or the first one was not `Hello`.
    Protocol(String),
    /// The daemon's `Hello` names a version this client does not speak.
    Incompatible { version: u16 },
    /// The daemon is older than the messages asked for.
    TooOld { version: u16 },
    /// The daemon did not answer in time.
    Silent,
    /// The daemon closed the connection.
    Closed,
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClientError::Connect { path, reason } => {
                write!(f, "cannot connect to {}: {reason}", path.display())
            }
            ClientError::Io(what) | ClientError::Protocol(what) => write!(f, "{what}"),
            ClientError::Incompatible { version } => write!(
                f,
                "the daemon speaks protocol version {version}; this program speaks \
                 {MIN_PROTOCOL_VERSION} to {PROTOCOL_VERSION}"
            ),
            ClientError::TooOld { version } => write!(
                f,
                "the daemon speaks protocol version {version}, older than the \
                 {KEYMAP_PROTOCOL_VERSION} the keymap needs"
            ),
            ClientError::Silent => write!(f, "the daemon sent nothing in time"),
            ClientError::Closed => write!(f, "the daemon closed the connection"),
        }
    }
}

impl std::error::Error for ClientError {}

/// How long to wait before asking a daemon with a full queue again.
const CONNECT_RETRY: Duration = Duration::from_millis(5);

/// Connects, asking again while the daemon's queue of waiting connections is full, for at most
/// `wait`.
fn connect_by(fd: &OwnedFd, addr: &SocketAddrUnix, wait: Duration) -> Result<(), Errno> {
    let started = Instant::now();
    loop {
        match rustix::net::connect(fd, addr) {
            Err(Errno::AGAIN | Errno::INTR) if started.elapsed() < wait => {
                std::thread::sleep(CONNECT_RETRY);
            }
            result => return result,
        }
    }
}

/// Makes `fd` block again, with a send timeout of `wait` so a daemon that stops reading cannot
/// hold a program for ever either.
fn blocking(fd: &OwnedFd, wait: Duration) -> Result<(), ClientError> {
    let io = |what: &str, e: Errno| ClientError::Io(format!("{what}: {e}"));
    let flags = rustix::fs::fcntl_getfl(fd).map_err(|e| io("reading the socket flags", e))?;
    rustix::fs::fcntl_setfl(fd, flags.difference(rustix::fs::OFlags::NONBLOCK))
        .map_err(|e| io("making the socket blocking", e))?;
    set_socket_timeout(fd, Timeout::Send, Some(wait.max(Duration::from_millis(1))))
        .map_err(|e| io("setting the send timeout", e))
}

/// What waiting for the next packet came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Heard {
    Message(ToClient),
    /// Nothing arrived in time.
    Silence,
    /// The daemon closed the connection.
    Closed,
}

/// A connection whose `Hello` has been read and found compatible. Dropping it closes it.
#[derive(Debug)]
pub struct Connection {
    fd: OwnedFd,
    version: u16,
    status: Status,
}

impl Connection {
    /// Connects to the socket at `path` and reads `Hello`, waiting at most `wait` for it.
    pub fn open(path: &Path, wait: Duration) -> Result<Connection, ClientError> {
        // CLOEXEC: a program that starts shells or other programs must not hand them this
        // connection, or the daemon would go on counting it after the program has exited.
        // Non-blocking only while connecting: a daemon whose queue of waiting connections is
        // full must not hold a program's start-up for ever.
        let fd = rustix::net::socket_with(
            AddressFamily::UNIX,
            SocketType::SEQPACKET,
            SocketFlags::CLOEXEC | SocketFlags::NONBLOCK,
            None,
        )
        .map_err(|e| ClientError::Io(format!("creating a socket: {e}")))?;
        let addr = SocketAddrUnix::new(path).map_err(|e| ClientError::Io(e.to_string()))?;
        connect_by(&fd, &addr, wait).map_err(|e| ClientError::Connect {
            path: path.to_path_buf(),
            reason: match e {
                Errno::AGAIN => "it did not answer in time".to_owned(),
                other => other.to_string(),
            },
        })?;
        blocking(&fd, wait)?;
        let connection = Connection {
            fd,
            version: 0,
            status: Status {
                mode: protocol::Mode::Running,
                keyboards: Vec::new(),
            },
        };
        match connection.recv(wait)? {
            Heard::Message(first) => connection.greeted(first),
            Heard::Silence => Err(ClientError::Silent),
            Heard::Closed => Err(ClientError::Closed),
        }
    }

    fn greeted(mut self, first: ToClient) -> Result<Connection, ClientError> {
        if let Compat::Incompatible { version } = protocol::compat(&first) {
            return Err(ClientError::Incompatible { version });
        }
        match first {
            ToClient::Hello { version, status } => {
                self.version = version;
                self.status = status;
                Ok(self)
            }
            other => Err(ClientError::Protocol(format!(
                "unexpected first packet: {other:?}"
            ))),
        }
    }

    /// The protocol version the daemon said in its `Hello`.
    pub fn version(&self) -> u16 {
        self.version
    }

    /// The status in the `Hello`.
    pub fn status(&self) -> &Status {
        &self.status
    }

    /// Fails unless the daemon speaks the keymap messages.
    pub fn require_keymap(self) -> Result<Connection, ClientError> {
        if self.version >= KEYMAP_PROTOCOL_VERSION {
            Ok(self)
        } else {
            Err(ClientError::TooOld {
                version: self.version,
            })
        }
    }

    /// Sends one message.
    pub fn send(&self, message: &ToDaemon) -> Result<(), ClientError> {
        rustix::net::send(&self.fd, &protocol::encode(message), SendFlags::NOSIGNAL)
            .map(drop)
            .map_err(|e| ClientError::Io(format!("sending: {e}")))
    }

    /// The next message, waiting at most `wait`.
    pub fn recv(&self, wait: Duration) -> Result<Heard, ClientError> {
        set_socket_timeout(
            &self.fd,
            Timeout::Recv,
            Some(wait.max(Duration::from_millis(1))),
        )
        .map_err(|e| ClientError::Io(format!("setting the receive timeout: {e}")))?;
        let mut buf = vec![0u8; MAX_PACKET];
        match rustix::net::recv(&self.fd, &mut buf, RecvFlags::empty()) {
            Ok((0, _)) | Err(Errno::CONNRESET) => Ok(Heard::Closed),
            Ok((n, _)) => protocol::decode(&buf[..n])
                .map(Heard::Message)
                .map_err(|e| ClientError::Protocol(e.to_string())),
            Err(Errno::AGAIN) => Ok(Heard::Silence),
            Err(e) => Err(ClientError::Io(format!("receiving: {e}"))),
        }
    }

    /// Receives until `pick` takes a message, within `within` in all; other messages are
    /// skipped. Silence is `ClientError::Silent` and a closed connection `ClientError::Closed`.
    pub(crate) fn await_message<T>(
        &self,
        within: Duration,
        mut pick: impl FnMut(ToClient) -> Option<T>,
    ) -> Result<T, ClientError> {
        let deadline = Instant::now() + within;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.recv(left)? {
                Heard::Message(message) => {
                    if let Some(found) = pick(message) {
                        return Ok(found);
                    }
                }
                Heard::Silence => return Err(ClientError::Silent),
                Heard::Closed => return Err(ClientError::Closed),
            }
            if left.is_zero() {
                return Err(ClientError::Silent);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeDaemon;
    use keycap_protocol::KeymapState;

    #[test]
    fn open_reads_the_hello_and_checks_its_version() {
        let current = FakeDaemon::start(PROTOCOL_VERSION, KeymapState::default());
        let connection = Connection::open(&current.socket, Duration::from_secs(2)).expect("opens");
        assert_eq!(connection.version(), PROTOCOL_VERSION);
        assert!(connection.require_keymap().is_ok());

        let old = FakeDaemon::start(MIN_PROTOCOL_VERSION, KeymapState::default());
        let connection = Connection::open(&old.socket, Duration::from_secs(2)).expect("opens");
        assert_eq!(
            connection.require_keymap().map(|_| ()),
            Err(ClientError::TooOld {
                version: MIN_PROTOCOL_VERSION
            })
        );

        let future = FakeDaemon::start(PROTOCOL_VERSION + 1, KeymapState::default());
        assert_eq!(
            Connection::open(&future.socket, Duration::from_secs(2)).map(|_| ()),
            Err(ClientError::Incompatible {
                version: PROTOCOL_VERSION + 1
            })
        );
    }

    #[test]
    fn nothing_listening_is_a_connect_error_that_names_the_path() {
        let err = Connection::open(
            Path::new("/nonexistent/keycap.sock"),
            Duration::from_secs(1),
        )
        .map(|_| ())
        .expect_err("no daemon");
        assert!(matches!(err, ClientError::Connect { .. }));
        assert!(
            err.to_string().contains("/nonexistent/keycap.sock"),
            "{err}"
        );
    }

    #[test]
    fn the_connection_is_not_inherited_by_programs_the_caller_starts() {
        let daemon = FakeDaemon::start(PROTOCOL_VERSION, KeymapState::default());
        let connection = Connection::open(&daemon.socket, Duration::from_secs(2)).expect("opens");
        let flags = rustix::io::fcntl_getfd(&connection.fd).expect("flags");
        assert!(flags.contains(rustix::io::FdFlags::CLOEXEC));
    }

    #[test]
    fn a_daemon_with_a_full_queue_is_given_up_on_at_the_deadline() {
        let dir = crate::testing::Scratch::new("full-queue");
        let path = dir.path().join("keycap.sock");
        let listener =
            rustix::net::socket(AddressFamily::UNIX, SocketType::SEQPACKET, None).expect("socket");
        rustix::net::bind(&listener, &SocketAddrUnix::new(&path).expect("addr")).expect("bind");
        rustix::net::listen(&listener, 0).expect("listen");
        // Nobody accepts: the first connection fills the queue, the second has to wait.
        let first =
            rustix::net::socket(AddressFamily::UNIX, SocketType::SEQPACKET, None).expect("socket");
        rustix::net::connect(&first, &SocketAddrUnix::new(&path).expect("addr")).expect("queued");

        let started = std::time::Instant::now();
        let err = Connection::open(&path, Duration::from_millis(200))
            .map(|_| ())
            .expect_err("the queue is full");
        assert!(matches!(err, ClientError::Connect { .. }), "{err}");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "it gave up in time"
        );
    }
}
