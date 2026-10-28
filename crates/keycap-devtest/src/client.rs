//! A desktop's end of the socket: `Hello`, `Status`, `Focus`, `Resume`.

use crate::failure::{Failure, errno};
use keycap_client::Heard;
use keycap_protocol::{
    self as protocol, Grab, PROTOCOL_VERSION, Status, Steer, ToClient, ToDaemon,
};
use rustix::fd::OwnedFd;
use rustix::io::Errno;
use rustix::net::sockopt::{Timeout, set_socket_timeout};
use rustix::net::{AddressFamily, RecvFlags, SendFlags, SocketAddrUnix, SocketType};
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

/// How long `connect` waits for the socket to exist.
const CONNECT_WAIT: Duration = Duration::from_secs(5);

/// A connected client that remembers the last status and everything it was told.
#[derive(Debug)]
pub struct Client {
    fd: OwnedFd,
    status: Status,
    log: Option<std::fs::File>,
    opened: Instant,
}

impl Client {
    /// Connects, waits for `Hello` and checks its version. What the client hears is appended to
    /// `log`, a file the failure report includes.
    pub fn connect(socket: &Path, log: Option<&Path>) -> Result<Self, Failure> {
        let deadline = Instant::now() + CONNECT_WAIT;
        let fd = loop {
            let fd = rustix::net::socket(AddressFamily::UNIX, SocketType::SEQPACKET, None)
                .map_err(errno("creating a socket"))?;
            let addr = SocketAddrUnix::new(socket).map_err(errno("the socket path"))?;
            match rustix::net::connect(&fd, &addr) {
                Ok(()) => break fd,
                Err(_) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(e) => {
                    return Err(Failure::new(format!(
                        "connecting to {}: {e} (after {CONNECT_WAIT:?})",
                        socket.display()
                    )));
                }
            }
        };
        let log = log.and_then(|p| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
                .ok()
        });
        let mut client = Self {
            fd,
            status: Status {
                mode: protocol::Mode::Running,
                keyboards: Vec::new(),
            },
            log,
            opened: Instant::now(),
        };
        match client.recv(Duration::from_secs(5))? {
            Heard::Message(ToClient::Hello { version, status }) if version == PROTOCOL_VERSION => {
                client.note(&status);
                client.status = status;
                Ok(client)
            }
            other => Err(Failure::new(format!(
                "the first packet should be Hello with version {PROTOCOL_VERSION}: {other:?}"
            ))),
        }
    }

    fn note(&mut self, status: &Status) {
        self.write_log(&format!("status {}", summary(status)));
    }

    fn write_log(&mut self, line: &str) {
        let at = self.opened.elapsed().as_secs_f64();
        if let Some(file) = &mut self.log {
            let _ = writeln!(file, "{at:9.3}s {line}");
        }
    }

    /// The next message within `wait`.
    pub fn recv(&mut self, wait: Duration) -> Result<Heard, Failure> {
        set_socket_timeout(
            &self.fd,
            Timeout::Recv,
            Some(wait.max(Duration::from_millis(1))),
        )
        .map_err(errno("setting the receive timeout"))?;
        let mut buf = vec![0u8; protocol::MAX_PACKET];
        match rustix::net::recv(&self.fd, &mut buf, RecvFlags::empty()) {
            Ok((0, _)) | Err(Errno::CONNRESET) => Ok(Heard::Closed),
            Ok((n, _)) => protocol::decode(&buf[..n])
                .map(Heard::Message)
                .map_err(|e| Failure::new(format!("undecodable packet: {e}"))),
            Err(Errno::AGAIN) => Ok(Heard::Silence),
            Err(e) => Err(Failure::new(format!("receiving: {e}"))),
        }
    }

    /// Sends one message.
    pub fn send(&self, message: &ToDaemon) -> Result<(), Failure> {
        rustix::net::send(&self.fd, &protocol::encode(message), SendFlags::NOSIGNAL)
            .map(drop)
            .map_err(errno("sending"))
    }

    /// Tells keycap which profile the focused window wants. The daemon does not answer, so a
    /// test that types next waits `FOCUS_SETTLE` first.
    pub fn focus(&self, profile: &str) -> Result<(), Failure> {
        let profile = protocol::ProfileName::parse(profile)
            .map_err(|e| Failure::new(format!("profile name {profile:?}: {e}")))?;
        self.send(&ToDaemon::Steer(Steer::Focus { profile }))
    }

    /// Ends a pause.
    pub fn resume(&self) -> Result<(), Failure> {
        self.send(&ToDaemon::Steer(Steer::Resume))
    }

    /// The last status heard.
    pub fn status(&self) -> &Status {
        &self.status
    }

    /// Reads until `matches` holds for a status, or `wait` passes; the failure lists every
    /// status heard on the way, so a wrong state is visible.
    pub fn wait_status(
        &mut self,
        what: &str,
        wait: Duration,
        matches: impl Fn(&Status) -> bool,
    ) -> Result<Status, Failure> {
        let deadline = Instant::now() + wait;
        let mut heard = vec![summary(&self.status)];
        loop {
            if matches(&self.status) {
                return Ok(self.status.clone());
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Failure::new(format!(
                    "status never became {what} within {wait:?}; heard: {}",
                    heard.join(" -> ")
                )));
            }
            match self.recv(left.min(Duration::from_millis(200)))? {
                Heard::Message(ToClient::Status(status)) => {
                    self.note(&status);
                    heard.push(summary(&status));
                    self.status = status;
                }
                Heard::Message(other) => self.write_log(&format!("{other:?}")),
                Heard::Silence => {}
                Heard::Closed => {
                    return Err(Failure::new(format!(
                        "the daemon closed the connection while waiting for {what}; heard: {}",
                        heard.join(" -> ")
                    )));
                }
            }
        }
    }

    /// Waits until the keyboard called `name` is held in the way `grab` says.
    pub fn wait_grab(&mut self, name: &str, grab: Grab, wait: Duration) -> Result<Status, Failure> {
        self.wait_status(&format!("{name} {grab:?}"), wait, |s| {
            s.keyboards
                .iter()
                .any(|k| k.info.name == name && k.grab == grab)
        })
    }
}

/// How long a test waits after `Focus` before typing: the message crosses a thread and a
/// channel to the reader, and the daemon sends no acknowledgement.
pub const FOCUS_SETTLE: Duration = Duration::from_millis(300);

/// A status on one line: `Running [name Active, other Conflict]`.
pub fn summary(status: &Status) -> String {
    let keyboards: Vec<String> = status
        .keyboards
        .iter()
        .map(|k| format!("{} {:?}", k.info.name, k.grab))
        .collect();
    format!("{:?} [{}]", status.mode, keyboards.join(", "))
}
