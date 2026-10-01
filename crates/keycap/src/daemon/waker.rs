//! `Waker`: an eventfd one thread writes so another, asleep in `poll`, wakes.

use rustix::event::{EventfdFlags, eventfd};
use rustix::fd::{AsFd, BorrowedFd, OwnedFd};
use std::io;
use std::sync::Arc;

/// A shared eventfd: any clone can `wake`, the owner polls `fd` and `clear`s after waking.
#[derive(Debug, Clone)]
pub(crate) struct Waker(Arc<OwnedFd>);

impl Waker {
    pub(crate) fn new() -> io::Result<Self> {
        let fd = eventfd(0, EventfdFlags::CLOEXEC | EventfdFlags::NONBLOCK)?;
        Ok(Self(Arc::new(fd)))
    }

    /// Makes the fd readable. A full counter already means "awake", so a failure is ignored.
    pub(crate) fn wake(&self) {
        let _ = rustix::io::write(&*self.0, &1u64.to_ne_bytes());
    }

    /// Resets the counter so the fd stops being readable.
    pub(crate) fn clear(&self) {
        let mut buf = [0u8; 8];
        let _ = rustix::io::read(&*self.0, &mut buf);
    }

    /// The fd to hand to `poll`.
    pub(crate) fn fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustix::event::{PollFd, PollFlags, poll};
    use rustix::time::Timespec;

    fn readable(waker: &Waker) -> bool {
        let none = Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        let fd = waker.fd();
        let mut fds = [PollFd::new(&fd, PollFlags::IN)];
        poll(&mut fds, Some(&none)).is_ok_and(|n| n == 1)
    }

    #[test]
    fn a_wake_from_a_clone_is_seen_until_cleared() {
        let waker = Waker::new().unwrap();
        assert!(!readable(&waker), "quiet at first");
        waker.clone().wake();
        assert!(readable(&waker), "woken by the clone");
        waker.clear();
        assert!(!readable(&waker), "quiet after clear");
    }
}
