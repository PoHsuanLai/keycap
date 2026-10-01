//! Another program holding a device exclusively, as a second remapper would.

use crate::failure::{Failure, errno, io};
use crate::sys::{self, Grab};
use rustix::fd::OwnedFd;
use rustix::fs::{Mode, OFlags};
use std::path::Path;

/// An `EVIOCGRAB` held by the harness until it is dropped or released.
#[derive(Debug)]
pub struct ExternalGrab {
    fd: OwnedFd,
}

impl ExternalGrab {
    /// Opens `node` and grabs it.
    pub fn take(node: &Path) -> Result<Self, Failure> {
        let fd = rustix::fs::open(
            node,
            OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(errno(format!("opening {}", node.display())))?;
        sys::grab(&fd, Grab::On).map_err(io(format!("grabbing {}", node.display())))?;
        Ok(Self { fd })
    }

    /// Gives the device back.
    pub fn release(self) -> Result<(), Failure> {
        sys::grab(&self.fd, Grab::Off).map_err(io("releasing the harness's grab"))
    }
}
