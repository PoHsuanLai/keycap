//! The loop: wait, then mail, devices, gates, status.

use super::Reader;
use super::twins::Passed;
use crate::DeviceId;
use crate::daemon::clock;
use crate::daemon::devices::{poll_gates, status_of, summary};
use crate::daemon::discover::scan;
use crate::daemon::evdev::{End, drain};
use crate::daemon::readiness::Readiness;
use crate::daemon::route::{Routed, Skip, route};
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
    mail: Readiness,
    /// The desktop set a LED on the virtual keyboard.
    leds: Readiness,
    devices: Vec<DeviceId>,
}

impl<E: Emitter> Reader<E> {
    /// Scans for keyboards, tells systemd the daemon is ready, then loops until `poll` fails.
    pub(crate) fn run(mut self) -> Result<(), String> {
        for candidate in scan(
            &self.roots.sys,
            &self.roots.dev,
            &self.roots.udev,
            &self.roots.allow,
        ) {
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
        let now = clock::now();
        let wake = [self.devices.next_wake(now), Some(self.safety.until(now))]
            .into_iter()
            .flatten()
            .min();
        let timeout = poll_timeout(wake, self.watchdog);
        let ready = self.wait(timeout)?;
        if ready.mail == Readiness::Readable {
            for message in self.mailbox.drain() {
                self.handle(message);
            }
        }
        if self.safety.due(clock::now()) {
            self.looking = std::mem::take(&mut self.looking).everything();
            self.safety = self.safety.done(clock::now());
        }
        self.look();
        if ready.leds == Readiness::Readable {
            self.forward_leds();
        }
        for id in ready.devices {
            self.read(id);
        }
        self.devices = poll_gates(
            std::mem::take(&mut self.devices),
            &mut self.nodes,
            clock::now(),
        );
        self.sync_grabs();
        self.publish();
        Ok(())
    }

    fn wait(&self, timeout: Duration) -> Result<Ready, String> {
        let timespec = Timespec {
            tv_sec: timeout.as_secs().try_into().unwrap_or(i64::MAX),
            tv_nsec: i64::from(timeout.subsec_nanos()),
        };
        let mail = self.mailbox.fd();
        let led = self.emitter.led_fd();
        let nodes: Vec<_> = self.nodes.fds().collect();
        let mut fds = vec![PollFd::new(&mail, PollFlags::IN)];
        let led_slot = led.as_ref().map(|fd| {
            fds.push(PollFd::new(fd, PollFlags::IN));
            fds.len() - 1
        });
        let first_node = fds.len();
        fds.extend(nodes.iter().map(|(_, fd)| PollFd::new(*fd, PollFlags::IN)));
        match poll(&mut fds, Some(&timespec)) {
            Ok(_) | Err(Errno::INTR) => {}
            Err(e) => return Err(format!("poll failed: {e}")),
        }
        Ok(Ready {
            mail: Readiness::of(&fds[0]),
            leds: led_slot.map_or(Readiness::Quiet, |slot| Readiness::of(&fds[slot])),
            devices: nodes
                .iter()
                .zip(&fds[first_node..])
                .filter(|(_, f)| Readiness::of(f) == Readiness::Readable)
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
            let skip = match self.discarding.remove(&id) {
                true => Skip::UntilSync,
                false => Skip::Nothing,
            };
            let (routed, skip) = route(id, layout, &drained.events, clock::now(), skip);
            if skip == Skip::UntilSync {
                self.discarding.insert(id);
            }
            for routed in routed {
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
            Routed::Pass { device, events } => match self.twins.pass(device, &events) {
                Passed::Written => {}
                Passed::Lost(e) => eprintln!(
                    "keycap: writing the pointer of device {} failed: {e}; \
                     it passes through the keyboard now",
                    device.0
                ),
                Passed::NoTwin => {
                    self.buttons = std::mem::take(&mut self.buttons).observe(device, &events);
                    self.write(&events);
                }
            },
            Routed::Resync(device) => {
                eprintln!(
                    "keycap: device {} overflowed its queue; releasing what it held",
                    device.0
                );
                self.reset_device(device);
            }
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
        self.discarding.remove(&id);
        self.reset_device(id);
        let step = std::mem::take(&mut self.devices).gone(id);
        self.carry_out(step);
    }
}
