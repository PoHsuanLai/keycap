//! The reader thread: one `poll` over every keyboard node, the inbox and a timeout, feeding
//! the pure remapper and carrying its outputs to the virtual keyboard. It waits on nothing but
//! `poll`.

mod caps;
#[cfg(test)]
mod caps_tests;
mod grabs;
mod handle;
#[cfg(test)]
mod limit_tests;
#[cfg(test)]
mod pointer_tests;
mod run;
#[cfg(test)]
mod tests;
mod twins;
mod unknown;

use super::buttons::Buttons;
use super::caps::CapsFix;
use super::devices::Devices;
use super::discover::VirtualAllowance;
use super::env::Env;
use super::grabber::EvdevGrabber;
use super::inbox::Mailbox;
use super::leds::LedState;
use super::sdnotify::Notifier;
use super::settings::Layers;
use super::socket::SocketHandle;
use super::uinput::Emitter;
use crate::{DeviceId, Profiles, Remapper};
use keycap_protocol::Status;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;
use twins::Twins;
use unknown::Unknowns;

/// What discovery looks at: where the keyboards are listed, where their nodes are, and which
/// virtual device it may take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Roots {
    pub sys: PathBuf,
    pub dev: PathBuf,
    pub allow: VirtualAllowance,
}

impl Roots {
    /// The roots and the allowance the environment names.
    pub(crate) fn of(env: &Env) -> Self {
        Self {
            sys: env.sys_input.clone(),
            dev: env.dev_input.clone(),
            allow: env.grab_virtual.clone(),
        }
    }
}

/// Everything the reader is given to start with.
pub(crate) struct Wiring<E> {
    pub mailbox: Mailbox,
    pub socket: SocketHandle,
    pub emitter: E,
    pub notifier: Notifier,
    /// How often systemd wants a ping, when it does.
    pub watchdog: Option<Duration>,
    pub roots: Roots,
    pub layers: Layers,
}

/// The reader's state: the pure core, the devices, and the ends of everything it talks to.
pub(crate) struct Reader<E: Emitter> {
    mailbox: Mailbox,
    socket: SocketHandle,
    emitter: E,
    notifier: Notifier,
    watchdog: Option<Duration>,
    roots: Roots,
    nodes: EvdevGrabber,
    layers: Layers,
    profiles: Profiles,
    remapper: Remapper,
    devices: Devices,
    /// The mouse buttons written for combo devices, released with the device.
    buttons: Buttons,
    /// The pointer twins of the grabbed combo devices.
    twins: Twins<E::Twin>,
    /// The devices seen grabbed at the last look, to tell which were grabbed or let go since.
    held: BTreeSet<DeviceId>,
    /// The lamps the desktop last set on the virtual keyboard.
    lamps: LedState,
    /// The Caps Lock lock as keycap follows it, and whether the user asked for it.
    caps: CapsFix,
    /// Focused names the config does not define, already logged.
    unknowns: Unknowns,
    /// Devices whose queue overflowed and whose events are discarded up to the next
    /// `SYN_REPORT`.
    discarding: BTreeSet<DeviceId>,
    /// The last status the socket thread was told.
    published: Status,
}

impl<E: Emitter> Reader<E> {
    pub(crate) fn new(wiring: Wiring<E>) -> Self {
        let (profiles, warnings) = wiring.layers.resolve();
        warnings.iter().for_each(handle::log_warning);
        let devices = Devices::new().with_overrides(wiring.layers.overrides());
        let remapper = Remapper::new();
        Self {
            published: super::devices::status_of(&devices, remapper.status()),
            mailbox: wiring.mailbox,
            socket: wiring.socket,
            emitter: wiring.emitter,
            notifier: wiring.notifier,
            watchdog: wiring.watchdog,
            roots: wiring.roots,
            nodes: EvdevGrabber::default(),
            layers: wiring.layers,
            profiles,
            remapper,
            devices,
            buttons: Buttons::default(),
            twins: Twins::default(),
            held: BTreeSet::new(),
            lamps: LedState::default(),
            caps: CapsFix::default(),
            unknowns: Unknowns::default(),
            discarding: BTreeSet::new(),
        }
    }
}
