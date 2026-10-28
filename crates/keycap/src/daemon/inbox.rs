//! What the other threads tell the reader: one channel and one eventfd, so the reader's `poll`
//! wakes for a client's message, a config reload and a hotplug alike.

use super::discover::Hotplug;
use super::waker::Waker;
use crate::ConfigLayer;
use keycap_protocol::{ProfileName, Warning};
use rustix::fd::BorrowedFd;
use std::io;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};

/// Which socket client a request came from, so the reply finds it again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ClientId(pub u64);

/// A seat owner's message, already vetted by the socket thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Request {
    Focus(ProfileName),
    /// The owner's settings, not yet parsed; the reader answers `client` with the warnings.
    UserConfig {
        client: ClientId,
        toml: String,
    },
    /// The seat has no owner any more: drop the user layer.
    ClearUserConfig,
    Resume,
}

/// Everything the reader can be woken for besides its devices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Message {
    Request(Request),
    /// The vendor config file was re-read.
    Vendor(ConfigLayer, Vec<Warning>),
    /// The system config file was re-read.
    System(ConfigLayer, Vec<Warning>),
    Hotplug(Hotplug),
}

/// Messages the reader may have waiting. It takes them all each pass, so only a reader that is
/// stuck, or a client asking faster than the socket thread's rate limit allows, fills this.
const INBOX_MAX: usize = 256;

/// Why a message was not queued for the reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NotQueued {
    /// The inbox is full: the message was not queued.
    Full,
    /// The reader has gone: the daemon is ending.
    ReaderGone,
}

/// The sending end, cloned into every thread that talks to the reader.
#[derive(Debug, Clone)]
pub(crate) struct Inbox {
    tx: SyncSender<Message>,
    waker: Waker,
}

impl Inbox {
    /// Queues `message` and wakes the reader, waiting for room if the inbox is full: for the
    /// watcher threads, whose messages must not be lost and which have nothing else to do.
    pub(crate) fn send(&self, message: Message) {
        if self.tx.send(message).is_ok() {
            self.waker.wake();
        }
    }

    /// Queues `message` and wakes the reader, or says why not without waiting: for the socket
    /// thread, which must keep serving every other client.
    pub(crate) fn try_send(&self, message: Message) -> Result<(), NotQueued> {
        match self.tx.try_send(message) {
            Ok(()) => {
                self.waker.wake();
                Ok(())
            }
            Err(TrySendError::Full(_)) => Err(NotQueued::Full),
            Err(TrySendError::Disconnected(_)) => Err(NotQueued::ReaderGone),
        }
    }
}

/// The reader's end.
#[derive(Debug)]
pub(crate) struct Mailbox {
    rx: Receiver<Message>,
    waker: Waker,
}

impl Mailbox {
    /// The fd that is readable while messages wait.
    pub(crate) fn fd(&self) -> BorrowedFd<'_> {
        self.waker.fd()
    }

    /// Everything queued, oldest first; the fd goes quiet.
    pub(crate) fn drain(&self) -> Vec<Message> {
        self.waker.clear();
        self.rx.try_iter().collect()
    }
}

/// A connected inbox and mailbox.
pub(crate) fn mailbox() -> io::Result<(Inbox, Mailbox)> {
    let waker = Waker::new()?;
    let (tx, rx) = mpsc::sync_channel(INBOX_MAX);
    Ok((
        Inbox {
            tx,
            waker: waker.clone(),
        },
        Mailbox { rx, waker },
    ))
}

/// Moves everything arriving on `rx` (a watcher's own channel) into the inbox as `wrap` says,
/// on a thread that ends when the sender is dropped.
pub(crate) fn forward<T: Send + 'static>(
    name: &str,
    rx: Receiver<T>,
    inbox: Inbox,
    wrap: fn(T) -> Message,
) -> io::Result<()> {
    std::thread::Builder::new()
        .name(format!("keycap-{name}"))
        .spawn(move || rx.iter().for_each(|item| inbox.send(wrap(item))))
        .map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_arrive_in_order_and_drain_empties_the_mailbox() {
        let (inbox, mailbox) = mailbox().unwrap();
        inbox.send(Message::Request(Request::Resume));
        inbox
            .clone()
            .send(Message::Request(Request::ClearUserConfig));
        assert_eq!(
            mailbox.drain(),
            [
                Message::Request(Request::Resume),
                Message::Request(Request::ClearUserConfig)
            ]
        );
        assert_eq!(mailbox.drain(), []);
    }

    #[test]
    fn a_full_inbox_refuses_without_waiting_and_makes_room_when_drained() {
        let (inbox, mailbox) = mailbox().unwrap();
        for _ in 0..INBOX_MAX {
            assert_eq!(inbox.try_send(Message::Request(Request::Resume)), Ok(()));
        }
        assert_eq!(
            inbox.try_send(Message::Request(Request::Resume)),
            Err(NotQueued::Full)
        );
        assert_eq!(mailbox.drain().len(), INBOX_MAX);
        assert_eq!(inbox.try_send(Message::Request(Request::Resume)), Ok(()));
        drop(mailbox);
        assert_eq!(
            inbox.try_send(Message::Request(Request::Resume)),
            Err(NotQueued::ReaderGone)
        );
    }

    #[test]
    fn a_forwarder_wraps_what_a_watcher_sends() {
        let (inbox, mailbox) = mailbox().unwrap();
        let (tx, rx) = mpsc::channel();
        forward("test", rx, inbox, |(layer, warnings)| {
            Message::System(layer, warnings)
        })
        .unwrap();
        tx.send((ConfigLayer::default(), Vec::new())).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let got = loop {
            let got = mailbox.drain();
            if !got.is_empty() || std::time::Instant::now() > deadline {
                break got;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert_eq!(got, [Message::System(ConfigLayer::default(), Vec::new())]);
    }
}
