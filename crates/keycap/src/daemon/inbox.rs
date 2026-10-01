//! What the other threads tell the reader: one channel and one eventfd, so the reader's `poll`
//! wakes for a client's message, a config reload and a hotplug alike.

use super::discover::Hotplug;
use super::waker::Waker;
use crate::Layer;
use keycap_protocol::{ProfileName, Warning};
use rustix::fd::BorrowedFd;
use std::io;
use std::sync::mpsc::{self, Receiver, Sender};

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
    /// The system config file was re-read.
    System(Layer, Vec<Warning>),
    Hotplug(Hotplug),
}

/// The sending end, cloned into every thread that talks to the reader.
#[derive(Debug, Clone)]
pub(crate) struct Inbox {
    tx: Sender<Message>,
    waker: Waker,
}

impl Inbox {
    /// Queues `message` and wakes the reader. A reader that has gone is the daemon ending.
    pub(crate) fn send(&self, message: Message) {
        if self.tx.send(message).is_ok() {
            self.waker.wake();
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
    let (tx, rx) = mpsc::channel();
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
    fn a_forwarder_wraps_what_a_watcher_sends() {
        let (inbox, mailbox) = mailbox().unwrap();
        let (tx, rx) = mpsc::channel();
        forward("test", rx, inbox, |(layer, warnings)| {
            Message::System(layer, warnings)
        })
        .unwrap();
        tx.send((Layer::default(), Vec::new())).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let got = loop {
            let got = mailbox.drain();
            if !got.is_empty() || std::time::Instant::now() > deadline {
                break got;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert_eq!(got, [Message::System(Layer::default(), Vec::new())]);
    }
}
