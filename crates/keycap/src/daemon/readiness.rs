//! What one `poll` said about one fd.

use rustix::event::PollFd;

/// Whether `fd` reported anything in the poll that just returned.
pub(crate) fn ready(fd: &PollFd<'_>) -> bool {
    !fd.revents().is_empty()
}
