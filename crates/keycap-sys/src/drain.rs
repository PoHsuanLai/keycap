//! Draining one nonblocking event node: every event it has queued, until it has none or is gone.

use rustix::fd::AsFd;
use rustix::io::Errno;

/// Bytes of a `struct input_event` on a 64-bit kernel: a 16-byte timestamp, then type, code
/// and value.
pub const EVENT_BYTES: usize = 24;
/// How many events one read asks for.
const READ_EVENTS: usize = 64;

/// How a drain ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    /// Nothing more queued; the node is still there.
    Idle,
    /// Reading failed or hit end of file: the keyboard was unplugged (`ENODEV`) or the node broke.
    Gone,
}

/// What a node had queued, and how the reading ended; events read before a failure are kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drained<E> {
    pub events: Vec<E>,
    pub end: End,
}

/// Reads the nonblocking `fd` until it would block, turning each whole 24-byte record into an
/// event with `decode`. A trailing partial record is ignored (the kernel never returns one).
pub fn drain<E>(fd: impl AsFd, decode: impl Fn(&[u8; EVENT_BYTES]) -> E) -> Drained<E> {
    let mut buf = [0u8; EVENT_BYTES * READ_EVENTS];
    let mut events = Vec::new();
    let end = loop {
        match rustix::io::read(&fd, &mut buf) {
            Ok(0) => break End::Gone,
            Ok(n) => events.extend(buf[..n].as_chunks::<EVENT_BYTES>().0.iter().map(&decode)),
            Err(Errno::AGAIN) => break End::Idle,
            Err(Errno::INTR) => {}
            Err(_) => break End::Gone,
        }
    };
    Drained { events, end }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustix::fd::OwnedFd;
    use rustix::net::{AddressFamily, SocketFlags, SocketType, socketpair};

    /// A record whose first byte is `tag`.
    fn record(tag: u8) -> [u8; EVENT_BYTES] {
        let mut b = [0u8; EVENT_BYTES];
        b[0] = tag;
        b
    }

    /// A nonblocking pair: the second end plays the kernel's evdev node.
    fn pair() -> (OwnedFd, OwnedFd) {
        socketpair(
            AddressFamily::UNIX,
            SocketType::STREAM,
            SocketFlags::NONBLOCK | SocketFlags::CLOEXEC,
            None,
        )
        .unwrap()
    }

    #[test]
    fn a_drain_returns_everything_queued_then_goes_idle() {
        let (node, kernel) = pair();
        let queued: Vec<u8> = (0..100).collect();
        let bytes: Vec<u8> = queued.iter().flat_map(|&t| record(t)).collect();
        rustix::io::write(&kernel, &bytes).unwrap();
        let drained = drain(&node, |b| b[0]);
        assert_eq!(drained.events, queued, "more than one read's worth");
        assert_eq!(drained.end, End::Idle);
        assert!(drain(&node, |b| b[0]).events.is_empty(), "nothing left");
    }

    #[test]
    fn a_closed_node_ends_the_drain_as_gone_after_its_last_events() {
        let (node, kernel) = pair();
        rustix::io::write(&kernel, &record(7)).unwrap();
        drop(kernel);
        let drained = drain(&node, |b| b[0]);
        assert_eq!(
            drained,
            Drained {
                events: vec![7],
                end: End::Gone
            }
        );
    }
}
