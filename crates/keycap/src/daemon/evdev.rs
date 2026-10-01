//! Draining one grabbed node: every event it has queued, until it has none or is gone.

use super::events::{EVENT_BYTES, InputEvent, parse};
use rustix::fd::AsFd;
use rustix::io::Errno;

/// How many events one read asks for.
const READ_EVENTS: usize = 64;

/// How a drain ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum End {
    /// Nothing more queued; the node is still there.
    Idle,
    /// Reading failed or hit end of file: the keyboard was unplugged (`ENODEV`) or the node broke.
    Gone,
}

/// What a node had queued, and how the reading ended; events read before a failure are kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Drained {
    pub events: Vec<InputEvent>,
    pub end: End,
}

/// Reads the nonblocking `fd` until it would block.
pub(crate) fn drain(fd: impl AsFd) -> Drained {
    let mut buf = [0u8; EVENT_BYTES * READ_EVENTS];
    let mut events = Vec::new();
    loop {
        match rustix::io::read(&fd, &mut buf) {
            Ok(0) => {
                break Drained {
                    events,
                    end: End::Gone,
                };
            }
            Ok(n) => events.extend(parse(&buf[..n])),
            Err(Errno::AGAIN) => {
                break Drained {
                    events,
                    end: End::Idle,
                };
            }
            Err(Errno::INTR) => {}
            Err(_) => {
                break Drained {
                    events,
                    end: End::Gone,
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::events::{EV_KEY, EV_SYN, SYN_REPORT};
    use rustix::fd::OwnedFd;
    use rustix::net::{AddressFamily, SocketFlags, SocketType, socketpair};

    fn ev(kind: u16, code: u16, value: i32) -> InputEvent {
        InputEvent { kind, code, value }
    }

    /// A nonblocking pipe-like pair: the second end plays the kernel's evdev node.
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
        let queued: Vec<InputEvent> = (0..100).map(|i| ev(EV_KEY, 30, i % 2)).collect();
        let bytes: Vec<u8> = queued.iter().flat_map(|e| e.to_bytes()).collect();
        rustix::io::write(&kernel, &bytes).unwrap();
        let drained = drain(&node);
        assert_eq!(drained.events, queued, "more than one read's worth");
        assert_eq!(drained.end, End::Idle);
        assert_eq!(drain(&node).events, [], "nothing left");
    }

    #[test]
    fn a_closed_node_ends_the_drain_as_gone_after_its_last_events() {
        let (node, kernel) = pair();
        let last = ev(EV_SYN, SYN_REPORT, 0);
        rustix::io::write(&kernel, &last.to_bytes()).unwrap();
        drop(kernel);
        let drained = drain(&node);
        assert_eq!(
            drained,
            Drained {
                events: vec![last],
                end: End::Gone
            }
        );
    }
}
