//! `Grabber`: take and release the exclusive grab on one device, by id.

use super::sys::{self, Grab, GrabError};
use crate::DeviceId;
use rustix::fd::OwnedFd;
use std::collections::BTreeMap;
use std::io;

/// Grabs and releases devices; the real one issues `EVIOCGRAB`, tests use `FakeGrabber`.
pub(crate) trait Grabber {
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
    /// Starts managing an opened device node.
    pub(crate) fn insert(&mut self, id: DeviceId, fd: OwnedFd) {
        self.fds.insert(id, fd);
    }

    /// Stops managing a device and closes it; the kernel drops its grab with the fd.
    pub(crate) fn remove(&mut self, id: DeviceId) {
        self.fds.remove(&id);
    }
}

impl Grabber for EvdevGrabber {
    fn grab(&mut self, id: DeviceId) -> Result<(), GrabError> {
        match self.fds.get(&id) {
            Some(fd) => sys::grab(fd, Grab::On),
            None => Err(io::Error::from(io::ErrorKind::NotFound).into()),
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

    /// Grabs nothing; remembers which devices are held and refuses the ones marked busy.
    #[derive(Debug, Default)]
    pub(crate) struct FakeGrabber {
        busy: BTreeSet<DeviceId>,
        held: BTreeSet<DeviceId>,
    }

    impl FakeGrabber {
        /// A device another program already holds.
        pub(crate) fn with_busy(ids: impl IntoIterator<Item = DeviceId>) -> Self {
            Self {
                busy: ids.into_iter().collect(),
                held: BTreeSet::new(),
            }
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
