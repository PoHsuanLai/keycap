//! `sd_notify`: readiness, watchdog pings and a status line to `$NOTIFY_SOCKET`.

use rustix::fd::OwnedFd;
use rustix::net::{self, AddressFamily, SendFlags, SocketAddrUnix, SocketFlags, SocketType};
use std::io;

/// One notification, in the `KEY=VALUE` form systemd reads.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Message<'a> {
    Ready,
    Watchdog,
    Status(&'a str),
}

impl Message<'_> {
    fn payload(&self) -> String {
        match self {
            Message::Ready => "READY=1".to_owned(),
            Message::Watchdog => "WATCHDOG=1".to_owned(),
            Message::Status(text) => format!("STATUS={}", text.replace('\n', " ")),
        }
    }
}

/// The socket address `$NOTIFY_SOCKET` names: a path, or an abstract name after a leading `@`.
fn address(socket: &str) -> io::Result<SocketAddrUnix> {
    match socket.strip_prefix('@') {
        Some(name) => SocketAddrUnix::new_abstract_name(name.as_bytes()),
        None => SocketAddrUnix::new(socket),
    }
    .map_err(io::Error::from)
}

/// Where notifications go; without a socket every call is a no-op.
#[derive(Debug)]
pub(crate) struct Notifier(Option<Target>);

#[derive(Debug)]
struct Target {
    fd: OwnedFd,
    addr: SocketAddrUnix,
}

impl Notifier {
    /// A notifier for `$NOTIFY_SOCKET`'s value, or one that does nothing for `None`.
    pub(crate) fn new(socket: Option<&str>) -> io::Result<Self> {
        let Some(socket) = socket else {
            return Ok(Self(None));
        };
        let addr = address(socket)?;
        let fd = net::socket_with(
            AddressFamily::UNIX,
            SocketType::DGRAM,
            SocketFlags::CLOEXEC,
            None,
        )?;
        Ok(Self(Some(Target { fd, addr })))
    }

    /// `READY=1`: startup is finished.
    pub(crate) fn ready(&self) -> io::Result<()> {
        self.send(&Message::Ready)
    }

    /// `WATCHDOG=1`: the daemon is alive.
    pub(crate) fn watchdog(&self) -> io::Result<()> {
        self.send(&Message::Watchdog)
    }

    /// `STATUS=text`, shown by `systemctl status`.
    pub(crate) fn status(&self, text: &str) -> io::Result<()> {
        self.send(&Message::Status(text))
    }

    /// Never blocks: a notify socket whose queue is full fails the send with `WouldBlock`, and
    /// the reader, which must keep polling, goes on.
    fn send(&self, message: &Message<'_>) -> io::Result<()> {
        let Some(target) = &self.0 else {
            return Ok(());
        };
        net::sendto(
            &target.fd,
            message.payload().as_bytes(),
            SendFlags::DONTWAIT,
            &target.addr,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::scratch::Scratch;
    use std::os::linux::net::SocketAddrExt;
    use std::os::unix::net::{SocketAddr, UnixDatagram};

    fn received(listener: &UnixDatagram) -> String {
        let mut buf = [0u8; 256];
        let n = listener.recv(&mut buf).expect("a datagram");
        String::from_utf8(buf[..n].to_vec()).expect("utf-8 payload")
    }

    #[test]
    fn payload_table() {
        let cases = [
            (Message::Ready, "READY=1"),
            (Message::Watchdog, "WATCHDOG=1"),
            (Message::Status("grabbed 2"), "STATUS=grabbed 2"),
            (Message::Status("a\nb"), "STATUS=a b"),
        ];
        for (message, want) in cases {
            assert_eq!(message.payload(), want, "{message:?}");
        }
    }

    #[test]
    fn a_path_or_abstract_socket_receives_each_message() {
        let dir = Scratch::new("notify");
        let path = dir.path().join("notify.sock");
        let path_listener = UnixDatagram::bind(&path).unwrap();
        let path_notifier = Notifier::new(path.to_str()).unwrap();

        // An abstract socket is addressed by its name after the at sign.
        let name = format!("keycap-notify-test-{}", std::process::id());
        let abstract_listener =
            UnixDatagram::bind_addr(&SocketAddr::from_abstract_name(name.as_bytes()).unwrap())
                .unwrap();
        let abstract_notifier = Notifier::new(Some(&format!("@{name}"))).unwrap();

        path_notifier.ready().unwrap();
        assert_eq!(received(&path_listener), "READY=1", "path ready");
        path_notifier.watchdog().unwrap();
        assert_eq!(received(&path_listener), "WATCHDOG=1", "path watchdog");
        path_notifier.status("2 keyboards").unwrap();
        assert_eq!(
            received(&path_listener),
            "STATUS=2 keyboards",
            "path status"
        );
        abstract_notifier.ready().unwrap();
        assert_eq!(received(&abstract_listener), "READY=1", "abstract ready");
    }

    #[test]
    fn without_a_socket_every_call_is_a_no_op() {
        let notifier = Notifier::new(None).unwrap();
        assert!(notifier.ready().is_ok());
        assert!(notifier.watchdog().is_ok());
        assert!(notifier.status("x").is_ok());
    }

    #[test]
    fn a_socket_nobody_listens_on_is_an_error_not_a_panic() {
        let dir = Scratch::new("notify-gone");
        let path = dir.path().join("nobody.sock");
        let notifier = Notifier::new(path.to_str()).unwrap();
        assert!(notifier.ready().is_err());
    }

    #[test]
    fn a_full_notify_socket_fails_the_send_instead_of_blocking_it() {
        let dir = Scratch::new("notify-full");
        let path = dir.path().join("full.sock");
        // Bound and never read: its queue fills.
        let _listener = UnixDatagram::bind(&path).unwrap();
        let notifier = Notifier::new(path.to_str()).unwrap();
        let (done, outcome) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let failure = (0..100_000).find_map(|_| notifier.watchdog().err());
            let _ = done.send(failure.map(|e| e.kind()));
        });
        let failure = outcome
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("the sender must not block on a full socket");
        assert_eq!(failure, Some(io::ErrorKind::WouldBlock));
    }
}
