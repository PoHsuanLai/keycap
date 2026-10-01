//! `Grabber`: take and release the exclusive grab on one device, by id; `Nodes`: and open it,
//! close it and ask which keys it holds down.

use super::sys::{self, Grab, GrabError};
use crate::{DeviceId, KeysDown};
use rustix::fd::OwnedFd;
use rustix::fs::{Mode, OFlags};
use std::collections::BTreeMap;
use std::io;
use std::path::Path;

/// Grabs and releases devices; the real one issues `EVIOCGRAB`, tests use `FakeGrabber`.
pub(crate) trait Grabber {
    /// Takes the grab; `GrabError::Busy` means another program holds the device.
    fn grab(&mut self, id: DeviceId) -> Result<(), GrabError>;
    /// Releases the grab. A device that is gone or was never grabbed is ignored.
    fn ungrab(&mut self, id: DeviceId);
}

/// The device nodes keycap holds open, on top of the grab.
pub(crate) trait Nodes: Grabber {
    /// Opens the node for reading and starts managing it under `id`.
    fn open(&mut self, id: DeviceId, path: &Path) -> io::Result<()>;
    /// Stops managing the device and closes it; the kernel drops its grab with the fd.
    fn close(&mut self, id: DeviceId);
    /// The keys the kernel says are down on the device.
    fn keys_down(&self, id: DeviceId) -> io::Result<KeysDown>;
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
            OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
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
            .and_then(sys::keys_down)
    }
}

impl Grabber for EvdevGrabber {
    fn grab(&mut self, id: DeviceId) -> Result<(), GrabError> {
        match self.fds.get(&id) {
            Some(fd) => sys::grab(fd, Grab::On),
            None => Err(not_open().into()),
        }
    }

    fn ungrab(&mut self, id: DeviceId) {
        if let Some(fd) = self.fds.get(&id) {
            let _ = sys::grab(fd, Grab::Off);
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

    impl Grabber for FakeGrabber {
        fn grab(&mut self, id: DeviceId) -> Result<(), GrabError> {
            if self.busy.contains(&id) {
                return Err(GrabError::Busy);
            }
            self.held.insert(id);
            Ok(())
        }

        fn ungrab(&mut self, id: DeviceId) {
            self.held.remove(&id);
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
            let mut bytes = [0u8; crate::gate::KEYS_DOWN_BYTES];
            bytes[0] = u8::from(self.keys.contains(&id));
            Ok(KeysDown::from_bytes(bytes))
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
