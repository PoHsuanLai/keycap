//! The harness's ioctls, through `input-linux`'s safe handles over evdev and uinput.
//!
//! No `unsafe` lives here: the crate owns the opcodes and the argument layouts, so this module
//! only turns the harness's plain values (raw `u16` codes) into its typed ones. A handle
//! borrows the descriptor for one call; the harness keeps ownership.

use input_linux::{
    AbsoluteAxis, AbsoluteInfoSetup, EvdevHandle, EventKind, InputProperty, Key, LedKind,
    RelativeAxis, UInputHandle,
};
pub(crate) use input_linux::{AbsoluteInfo as AbsInfo, InputId};
use rustix::fd::{AsFd, BorrowedFd};
use std::io;

/// Bytes of a uinput device name, the terminating NUL included.
const NAME_BYTES: usize = 80;

/// An evdev handle over a descriptor the caller keeps.
fn evdev(fd: &impl AsFd) -> EvdevHandle<BorrowedFd<'_>> {
    EvdevHandle::new(fd.as_fd())
}

/// A uinput handle over a descriptor the caller keeps.
fn uinput(fd: &impl AsFd) -> UInputHandle<BorrowedFd<'_>> {
    UInputHandle::new(fd.as_fd())
}

/// The error for a raw code that is not in the kernel's range for its kind.
fn out_of_range<E>(_: E) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "code out of range")
}

/// `struct uinput_setup`: the identity a fake device is created under. The name is NUL padded;
/// it ends at the first NUL.
#[derive(Debug, Clone, Copy)]
pub(crate) struct UinputSetup {
    pub id: InputId,
    pub name: [u8; NAME_BYTES],
    pub ff_effects_max: u32,
}

impl UinputSetup {
    /// The name without its padding.
    fn name_bytes(&self) -> &[u8] {
        let end = self.name.iter().position(|&b| b == 0).unwrap_or(NAME_BYTES);
        &self.name[..end]
    }
}

/// `struct uinput_abs_setup`: one absolute axis and its range.
#[derive(Debug, Clone, Copy)]
pub(crate) struct UinputAbsSetup {
    pub code: u16,
    pub absinfo: AbsInfo,
}

impl UinputAbsSetup {
    fn typed(self) -> io::Result<AbsoluteInfoSetup> {
        Ok(AbsoluteInfoSetup {
            axis: AbsoluteAxis::from_code(self.code).map_err(out_of_range)?,
            info: self.absinfo,
        })
    }
}

/// Which capability set a `UI_SET_*BIT` call writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BitSet {
    Ev,
    Key,
    Rel,
    Abs,
    Led,
    Prop,
}

/// Whether a device is held exclusively.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Grab {
    On,
    Off,
}

impl Grab {
    /// The handle's argument: true grabs, false releases.
    const fn is_on(self) -> bool {
        matches!(self, Grab::On)
    }
}

/// Enables one capability bit on a uinput fd before the device is created.
pub(crate) fn set_bit(fd: impl AsFd, set: BitSet, code: u16) -> io::Result<()> {
    let handle = uinput(&fd);
    match set {
        BitSet::Ev => handle.set_evbit(EventKind::try_from(code).map_err(out_of_range)?),
        BitSet::Key => handle.set_keybit(Key::from_code(code).map_err(out_of_range)?),
        BitSet::Rel => handle.set_relbit(RelativeAxis::from_code(code).map_err(out_of_range)?),
        BitSet::Abs => handle.set_absbit(AbsoluteAxis::from_code(code).map_err(out_of_range)?),
        BitSet::Led => handle.set_ledbit(LedKind::from_code(code).map_err(out_of_range)?),
        BitSet::Prop => handle.set_propbit(InputProperty::from_code(code).map_err(out_of_range)?),
    }
}

/// Declares the device's identity and absolute axes, then creates it: `UI_DEV_SETUP`, one
/// `UI_ABS_SETUP` per axis, `UI_DEV_CREATE`. The capability bits are already set.
pub(crate) fn dev_create(
    fd: impl AsFd,
    setup: UinputSetup,
    abs: &[UinputAbsSetup],
) -> io::Result<()> {
    let abs = abs
        .iter()
        .map(|axis| axis.typed())
        .collect::<io::Result<Vec<_>>>()?;
    uinput(&fd).create(&setup.id, setup.name_bytes(), setup.ff_effects_max, &abs)
}

/// Destroys the device.
pub(crate) fn dev_destroy(fd: impl AsFd) -> io::Result<()> {
    uinput(&fd).dev_destroy()
}

/// The sysfs name (`inputN`) of the created device, which names its directory under
/// `/sys/class/input` whatever else is registered under the same device name.
pub(crate) fn sysname(fd: impl AsFd) -> io::Result<String> {
    let bytes = uinput(&fd).sys_name()?;
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    Ok(String::from_utf8_lossy(&bytes[..end]).into_owned())
}

/// Takes or releases the exclusive grab on an evdev fd; `EBUSY` when another reader holds it.
pub(crate) fn grab(fd: impl AsFd, grab: Grab) -> io::Result<()> {
    evdev(&fd).grab(grab.is_on()).map_err(|e| match e.kind() {
        io::ErrorKind::ResourceBusy => {
            io::Error::new(io::ErrorKind::ResourceBusy, "device is grabbed")
        }
        _ => e,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grab_on_grabs_and_off_releases() {
        assert_eq!((Grab::On.is_on(), Grab::Off.is_on()), (true, false));
    }

    #[test]
    fn the_name_ends_at_its_first_nul() {
        let mut name = [0u8; NAME_BYTES];
        name[..2].copy_from_slice(b"kb");
        let setup = UinputSetup {
            id: InputId {
                bustype: 3,
                vendor: 1,
                product: 2,
                version: 1,
            },
            name,
            ff_effects_max: 0,
        };
        assert_eq!(setup.name_bytes(), b"kb");
    }

    #[test]
    fn an_axis_past_the_kernels_range_is_refused_before_any_ioctl() {
        let err = UinputAbsSetup {
            code: 0x40,
            absinfo: AbsInfo {
                value: 0,
                minimum: 0,
                maximum: 1,
                fuzz: 0,
                flat: 0,
                resolution: 0,
            },
        }
        .typed()
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }
}
