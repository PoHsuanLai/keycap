//! The reader's end of the socket thread.

use crate::daemon::inbox::ClientId;
use crate::daemon::waker::Waker;
use crate::keymap::Reading;
use keycap_protocol::{Status, Warning};
use std::sync::mpsc::Sender;

/// What the reader tells the socket thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ToSocket {
    /// The status changed: every client hears it, and later `Hello`s carry it.
    Status(Status),
    /// What a client's `UserConfig` got wrong, for that client only.
    Warnings {
        client: ClientId,
        warnings: Vec<Warning>,
    },
    /// The keymap file was read again: watchers hear the new keymap when it changed.
    Keymap(Reading),
}

/// A channel plus the eventfd that wakes the socket thread, so the reader never waits on a client.
#[derive(Debug, Clone)]
pub(crate) struct SocketHandle {
    pub(super) tx: Sender<ToSocket>,
    pub(super) waker: Waker,
}

impl SocketHandle {
    /// Queues `message` and wakes the socket thread. A thread that has gone is logged by its
    /// own exit.
    pub(crate) fn send(&self, message: ToSocket) {
        if self.tx.send(message).is_ok() {
            self.waker.wake();
        }
    }
}
