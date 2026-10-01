//! The loop: wait, then mail, devices, gates, status.

use super::Reader;
use crate::DeviceId;
use crate::daemon::clock;
use crate::daemon::devices::{drive, poll_gates, status_of, summary};
use crate::daemon::discover::scan;
use crate::daemon::evdev::{End, drain};
use crate::daemon::route::{Routed, route};
use crate::daemon::socket::ToSocket;
use crate::daemon::uinput::Emitter;
use rustix::event::{PollFd, PollFlags, poll};
use rustix::io::Errno;
use rustix::time::Timespec;
use std::time::Duration;

/// How long `poll` sleeps when nothing is scheduled and there is no watchdog.
const IDLE: Duration = Duration::from_millis(500);

/// How long `poll` may sleep: until the next gate or backoff deadline, but never past half the
/// watchdog interval, so the ping is always on time.
pub(super) fn poll_timeout(wake: Option<Duration>, watchdog: Option<Duration>) -> Duration {
    let cap = watchdog.map_or(IDLE, |w| w / 2);
    wake.map_or(cap, |w| w.min(cap))
}

/// What a poll found readable.
struct Ready {
    mail: bool,
    devices: Vec<DeviceId>,
}

impl<E: Emitter> Reader<E> {
    /// Scans for keyboards, tells systemd the daemon is ready, then loops until `poll` fails.
    pub(crate) fn run(mut self) -> Result<(), String> {
        for candidate in scan(&self.roots.sys, &self.roots.dev) {
            self.appear(candidate.identity);
        }
        self.publish();
        if let Err(e) = self.notifier.ready() {
            eprintln!("keycap: READY=1 failed: {e}");
        }
        eprintln!("keycap: ready: {}", summary(&self.published));
        loop {
            self.once()?;
        }
    }

    /// One pass: ping the watchdog, wait, and handle what woke it.
    pub(super) fn once(&mut self) -> Result<(), String> {
        if let Err(e) = self.notifier.watchdog() {
            eprintln!("keycap: watchdog ping failed: {e}");
        }
        let timeout = poll_timeout(self.devices.next_wake(clock::now()), self.watchdog);
        let ready = self.wait(timeout)?;
        if ready.mail {
            for message in self.mailbox.drain() {
                self.handle(message);
            }
        }
        for id in ready.devices {
            self.read(id);
        }
        self.devices = poll_gates(
            std::mem::take(&mut self.devices),
            &mut self.nodes,
            clock::now(),
        );
        self.publish();
        Ok(())
    }

    fn wait(&self, timeout: Duration) -> Result<Ready, String> {
        let timespec = Timespec {
            tv_sec: timeout.as_secs().try_into().unwrap_or(i64::MAX),
            tv_nsec: i64::from(timeout.subsec_nanos()),
        };
        let mail = self.mailbox.fd();
        let nodes: Vec<_> = self.nodes.fds().collect();
        let mut fds = vec![PollFd::new(&mail, PollFlags::IN)];
        fds.extend(nodes.iter().map(|(_, fd)| PollFd::new(*fd, PollFlags::IN)));
        match poll(&mut fds, Some(&timespec)) {
            Ok(_) | Err(Errno::INTR) => {}
            Err(e) => return Err(format!("poll failed: {e}")),
        }
        let hit = |f: &PollFd<'_>| !f.revents().is_empty();
        Ok(Ready {
            mail: hit(&fds[0]),
            devices: nodes
                .iter()
                .zip(&fds[1..])
                .filter(|(_, f)| hit(f))
                .map(|((id, _), _)| *id)
                .collect(),
        })
    }

    /// Everything `id` has queued. A keyboard that is not grabbed is drained and ignored: the
    /// kernel already delivers its keys to the desktop.
    fn read(&mut self, id: DeviceId) {
        let Some(fd) = self.nodes.fd(id) else {
            return;
        };
        let drained = drain(fd);
        if let Some(layout) = self.devices.active(id) {
            for routed in route(id, layout, &drained.events, clock::now()) {
                self.apply(routed);
            }
        }
        if drained.end == End::Gone {
            self.lose(id);
        }
    }

    pub(super) fn apply(&mut self, routed: Routed) {
        match routed {
            Routed::Remap(input) => self.feed(input),
            Routed::Pass(events) => self.write(&events),
        }
    }

    /// The pushed status, if it differs from the last one pushed.
    pub(super) fn publish(&mut self) {
        let status = status_of(&self.devices, self.remapper.status());
        if status != self.published {
            if let Err(e) = self.notifier.status(&summary(&status)) {
                eprintln!("keycap: status line failed: {e}");
            }
            self.socket.send(ToSocket::Status(status.clone()));
            self.published = status;
        }
    }

    /// A keyboard is gone: whatever it held down is released, and its node closed.
    pub(super) fn lose(&mut self, id: DeviceId) {
        self.feed(crate::Input::DeviceGone(id));
        let (devices, effects) = std::mem::take(&mut self.devices).gone(id);
        self.devices = drive(devices, effects, &mut self.nodes, clock::now());
    }
}
