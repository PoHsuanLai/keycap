//! What one `poll` said about one fd.

use rustix::event::PollFd;

/// Whether an fd had anything to report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Readiness {
    Quiet,
    Readable,
}

impl Readiness {
    /// What `fd` reported in the poll that just returned.
    pub(crate) fn of(fd: &PollFd<'_>) -> Self {
        if fd.revents().is_empty() {
            Readiness::Quiet
        } else {
            Readiness::Readable
        }
    }
}
