//! `Nodes`: open a device node, take and release the exclusive grab on it, close it and ask
//! which keys it holds down, all by id.

use super::discover::{Bits, Identity};
use super::events::InputEvent;
use super::leds::LedsLit;
use super::pointer::{AbsAxis, Masks, PointerCaps};
use crate::{DeviceId, KeysDown};
use keycap_sys::{self as sys, Capability, GrabError};
use rustix::fd::OwnedFd;
use rustix::fs::{Mode, OFlags};
use std::collections::BTreeMap;
use std::io;
use std::path::Path;

/// The device nodes keycap holds open, and the grab on them; the real one issues `EVIOCGRAB`,
/// tests use `FakeGrabber`.
pub(crate) trait Nodes {
    /// Opens the node for reading and for injecting LED events, and starts managing it under
    /// `id`.
    fn open(&mut self, id: DeviceId, path: &Path) -> io::Result<()>;
    /// Stops managing the device and closes it; the kernel drops its grab with the fd.
    fn close(&mut self, id: DeviceId);
    /// The keys the kernel says are down on the device.
    fn keys_down(&self, id: DeviceId) -> io::Result<KeysDown>;
    /// Takes the grab; `GrabError::Busy` means another program holds the device.
    fn grab(&mut self, id: DeviceId) -> Result<(), GrabError>;
    /// Releases the grab. A device that is gone or was never grabbed is ignored.
    fn ungrab(&mut self, id: DeviceId);
}

/// The open evdev fds, by id.
#[derive(Debug, Default)]
pub(crate) struct EvdevGrabber {
    fds: BTreeMap<DeviceId, OwnedFd>,
}

impl EvdevGrabber {
    /// The open node of `id`.
    pub(crate) fn fd(&self, id: DeviceId) -> Option<&OwnedFd> {
        self.fds.get(&id)
    }

    /// Writes `events` to the node of `id`: how a LED reaches the keyboard it is lit on. The
    /// node is open for writing so the grab holder can inject events.
    pub(crate) fn write(&self, id: DeviceId, events: &[InputEvent]) -> io::Result<()> {
        let fd = self.fds.get(&id).ok_or_else(not_open)?;
        let bytes: Vec<u8> = events.iter().flat_map(|e| e.to_bytes()).collect();
        let n = rustix::io::write(fd, &bytes)?;
        if n == bytes.len() {
            Ok(())
        } else {
            Err(io::Error::from(io::ErrorKind::WriteZero))
        }
    }

    /// The LEDs lit on the node of `id`, as the kernel keeps them.
    pub(crate) fn leds_lit(&self, id: DeviceId) -> io::Result<LedsLit> {
        let fd = self.fds.get(&id).ok_or_else(not_open)?;
        sys::leds_lit(fd).map(LedsLit::from_bytes)
    }

    /// The pointer half of the node of `id`, read from the kernel: the key, relative and
    /// absolute capabilities, the properties and the range of every absolute axis.
    pub(crate) fn pointer_caps(
        &self,
        id: DeviceId,
        identity: &Identity,
    ) -> io::Result<PointerCaps> {
        let fd = self.fds.get(&id).ok_or_else(not_open)?;
        let mask = |capability| sys::capability_bits(fd, capability).map(|b| Bits::from_bytes(&b));
        let masks = Masks {
            props: mask(Capability::Prop)?,
            key: mask(Capability::Key)?,
            rel: mask(Capability::Rel)?,
            abs: mask(Capability::Abs)?,
        };
        let abs = masks
            .abs_axes()
            .into_iter()
            .map(|code| sys::abs_info(fd, code).map(|info| AbsAxis { code, info }))
            .collect::<io::Result<Vec<_>>>()?;
        Ok(PointerCaps::new(
            &identity.name,
            identity.vendor,
            identity.product,
            &masks,
            abs,
        ))
    }

    /// Every open node, by id.
    pub(crate) fn fds(&self) -> impl Iterator<Item = (DeviceId, &OwnedFd)> {
        self.fds.iter().map(|(&id, fd)| (id, fd))
    }
}

fn not_open() -> io::Error {
    io::Error::from(io::ErrorKind::NotFound)
}

impl Nodes for EvdevGrabber {
    fn open(&mut self, id: DeviceId, path: &Path) -> io::Result<()> {
        let fd = rustix::fs::open(
            path,
            OFlags::RDWR | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        self.fds.insert(id, fd);
        Ok(())
    }

    fn close(&mut self, id: DeviceId) {
        self.fds.remove(&id);
    }

    fn keys_down(&self, id: DeviceId) -> io::Result<KeysDown> {
        self.fds
            .get(&id)
            .ok_or_else(not_open)
            .and_then(|fd| sys::key_state(fd).map(KeysDown::from_bytes))
    }

    fn grab(&mut self, id: DeviceId) -> Result<(), GrabError> {
        match self.fds.get(&id) {
            Some(fd) => sys::grab(fd),
            None => Err(not_open().into()),
        }
    }

    fn ungrab(&mut self, id: DeviceId) {
        if let Some(fd) = self.fds.get(&id) {
            let _ = sys::release(fd);
        }
    }
}

#[cfg(test)]
pub(crate) use fake::FakeGrabber;

#[cfg(test)]
mod fake {
    use super::*;
    use std::collections::BTreeSet;

    /// Grabs nothing; remembers which devices are held, refuses the ones marked busy, and
    /// plays nodes whose held keys and open failures the test sets.
    #[derive(Debug, Default)]
    pub(crate) struct FakeGrabber {
        busy: BTreeSet<DeviceId>,
        held: BTreeSet<DeviceId>,
        open: BTreeSet<DeviceId>,
        keys: BTreeSet<DeviceId>,
        unopenable: BTreeSet<DeviceId>,
        pressed_by_grab: BTreeSet<DeviceId>,
    }

    impl FakeGrabber {
        /// A device another program already holds.
        pub(crate) fn with_busy(ids: impl IntoIterator<Item = DeviceId>) -> Self {
            Self {
                busy: ids.into_iter().collect(),
                ..Self::default()
            }
        }

        /// Another program takes hold of `id`.
        pub(crate) fn make_busy(&mut self, id: DeviceId) {
            self.busy.insert(id);
        }

        /// Another program lets go of `id`.
        pub(crate) fn free(&mut self, id: DeviceId) {
            self.busy.remove(&id);
        }

        /// A key goes down on `id`.
        pub(crate) fn press_key(&mut self, id: DeviceId) {
            self.keys.insert(id);
        }

        /// Every key comes up on `id`.
        pub(crate) fn release_keys(&mut self, id: DeviceId) {
            self.keys.remove(&id);
        }

        /// A key goes down on `id` in the instant its grab succeeds: the press the compositor
        /// saw and the grab then hid.
        pub(crate) fn press_key_on_grab(&mut self, id: DeviceId) {
            self.pressed_by_grab.insert(id);
        }

        /// Opening `id` fails, as for a node the user may not read yet.
        pub(crate) fn refuse_open(&mut self, id: DeviceId) {
            self.unopenable.insert(id);
        }

        pub(crate) fn open_ids(&self) -> Vec<DeviceId> {
            self.open.iter().copied().collect()
        }

        pub(crate) fn held(&self) -> Vec<DeviceId> {
            self.held.iter().copied().collect()
        }
    }

    impl Nodes for FakeGrabber {
        fn open(&mut self, id: DeviceId, _path: &Path) -> io::Result<()> {
            if self.unopenable.contains(&id) {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            self.open.insert(id);
            Ok(())
        }

        fn close(&mut self, id: DeviceId) {
            self.open.remove(&id);
            self.held.remove(&id);
        }

        fn keys_down(&self, id: DeviceId) -> io::Result<KeysDown> {
            let mut bytes = [0u8; crate::KEYS_DOWN_BYTES];
            bytes[0] = u8::from(self.keys.contains(&id));
            Ok(KeysDown::from_bytes(bytes))
        }

        fn grab(&mut self, id: DeviceId) -> Result<(), GrabError> {
            if self.busy.contains(&id) {
                return Err(GrabError::Busy);
            }
            self.held.insert(id);
            if self.pressed_by_grab.remove(&id) {
                self.keys.insert(id);
            }
            Ok(())
        }

        fn ungrab(&mut self, id: DeviceId) {
            self.held.remove(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_busy_device_is_refused_and_the_others_are_held() {
        let mut g = FakeGrabber::with_busy([DeviceId(2)]);
        assert!(g.grab(DeviceId(1)).is_ok());
        assert!(matches!(g.grab(DeviceId(2)), Err(GrabError::Busy)));
        assert_eq!(g.held(), [DeviceId(1)]);
        g.ungrab(DeviceId(1));
        g.ungrab(DeviceId(9));
        assert_eq!(g.held(), []);
    }

    #[test]
    fn the_real_grabber_refuses_a_device_it_does_not_hold() {
        let mut g = EvdevGrabber::default();
        match g.grab(DeviceId(1)) {
            Err(GrabError::Other(e)) => assert_eq!(e.kind(), io::ErrorKind::NotFound),
            other => panic!("{other:?}"),
        }
        g.ungrab(DeviceId(1));
    }
}
