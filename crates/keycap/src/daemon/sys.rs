//! The ioctls, through `input-linux`'s safe handles over evdev and uinput.
//!
//! No `unsafe` lives here: the crate owns the opcodes and the argument layouts, so this module
//! only turns our plain values (raw `u16` codes, byte arrays) into its typed ones and its
//! errors into `io::Error`. A handle borrows the descriptor for one call; the caller keeps
//! ownership and closing it still releases a grab and destroys a device.

use crate::KeysDown;
use input_linux::{
    AbsoluteAxis, AbsoluteInfoSetup, EvdevHandle, EventKind, InputProperty, Key, LedKind,
    RelativeAxis, UInputHandle,
};
pub(crate) use input_linux::{AbsoluteInfo as AbsInfo, InputId};
use rustix::fd::{AsFd, BorrowedFd};
use std::io;

/// Bytes in the `EVIOCGKEY` bitmap: `KEY_MAX / 8 + 1`.
const KEY_STATE_BYTES: usize = 96;
/// Bytes in the `EVIOCGLED` bitmap: `LED_MAX` is 0x0f, so 16 bits.
const LED_STATE_BYTES: usize = 2;
/// Bytes of the `EVIOCGPROP` bitmap: `INPUT_PROP_MAX / 8 + 1` is 4, read as one 64-bit word.
const PROP_BYTES: usize = 8;
/// Bytes of the `EVIOCGBIT(EV_REL)` and `EVIOCGBIT(EV_ABS)` bitmaps: `REL_MAX` and `ABS_MAX`
/// fit one 64-bit word.
const AXIS_BYTES: usize = 8;
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

/// `struct uinput_setup`: the identity a virtual device is created under. The name is NUL
/// padded; it ends at the first NUL.
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

/// One absolute axis a virtual device declares, with its range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AbsSetup {
    pub code: u16,
    pub info: AbsInfo,
}

impl AbsSetup {
    fn typed(self) -> io::Result<AbsoluteInfoSetup> {
        Ok(AbsoluteInfoSetup {
            axis: AbsoluteAxis::from_code(self.code).map_err(out_of_range)?,
            info: self.info,
        })
    }
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

/// Why a grab failed; `Busy` is another program already holding the device.
#[derive(Debug, thiserror::Error)]
pub(crate) enum GrabError {
    #[error("another program has grabbed the device")]
    Busy,
    #[error("grab failed: {0}")]
    Other(#[from] io::Error),
}

/// Takes or releases the exclusive grab on an evdev fd.
pub(crate) fn grab(fd: impl AsFd, grab: Grab) -> Result<(), GrabError> {
    evdev(&fd).grab(grab.is_on()).map_err(|e| match e.kind() {
        io::ErrorKind::ResourceBusy => GrabError::Busy,
        _ => GrabError::Other(e),
    })
}

/// The keys the kernel believes are down on the device.
pub(crate) fn keys_down(fd: impl AsFd) -> io::Result<KeysDown> {
    let mut bytes = [0u8; KEY_STATE_BYTES];
    evdev(&fd).key_state(&mut bytes)?;
    Ok(KeysDown::from_bytes(bytes))
}

/// The LEDs the kernel believes are lit on the device, as its little-endian bitmap.
pub(crate) fn leds_lit(fd: impl AsFd) -> io::Result<[u8; LED_STATE_BYTES]> {
    let mut bytes = [0u8; LED_STATE_BYTES];
    evdev(&fd).led_state(&mut bytes)?;
    Ok(bytes)
}

/// The range and resolution of absolute axis `axis` (`EVIOCGABS`).
pub(crate) fn abs_info(fd: impl AsFd, axis: u16) -> io::Result<AbsInfo> {
    let axis = AbsoluteAxis::from_code(axis).map_err(out_of_range)?;
    evdev(&fd).absolute_info(axis)
}

/// Which capability bitmap `capability_bits` reads from an evdev fd.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Capability {
    Key,
    Rel,
    Abs,
    Prop,
}

/// A device's capability bitmap as the kernel's bytes: `EVIOCGBIT` for keys and axes,
/// `EVIOCGPROP` for the input properties. Little-endian 64-bit words, as `Bits::from_bytes`
/// reads them.
pub(crate) fn capability_bits(fd: impl AsFd, capability: Capability) -> io::Result<Vec<u8>> {
    let handle = evdev(&fd);
    let bits = |kind, len| {
        let mut bytes = vec![0u8; len];
        handle.event_bits_raw(kind, &mut bytes).map(|_| bytes)
    };
    match capability {
        Capability::Key => bits(EventKind::Key, KEY_STATE_BYTES),
        Capability::Rel => bits(EventKind::Relative, AXIS_BYTES),
        Capability::Abs => bits(EventKind::Absolute, AXIS_BYTES),
        Capability::Prop => {
            let mut bytes = vec![0u8; PROP_BYTES];
            handle.device_properties_raw(&mut bytes).map(|_| bytes)
        }
    }
}

/// Which capability bit set a `UI_SET_*BIT` call writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BitSet {
    Ev,
    Key,
    Rel,
    Abs,
    Led,
    Prop,
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
pub(crate) fn dev_create(fd: impl AsFd, setup: UinputSetup, abs: &[AbsSetup]) -> io::Result<()> {
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
        name[..3].copy_from_slice(b"kb!");
        let setup = UinputSetup {
            id: InputId {
                bustype: 6,
                vendor: 1,
                product: 2,
                version: 1,
            },
            name,
            ff_effects_max: 0,
        };
        assert_eq!(setup.name_bytes(), b"kb!");
        let full = UinputSetup {
            name: [b'x'; NAME_BYTES],
            ..setup
        };
        assert_eq!(full.name_bytes().len(), NAME_BYTES);
    }

    #[test]
    fn a_code_past_its_kind_is_refused_before_any_ioctl() {
        let err = AbsSetup {
            code: 0x40,
            info: AbsInfo::default(),
        }
        .typed()
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(Key::from_code(0x2ff).is_ok());
        assert!(Key::from_code(0x300).is_err());
        assert!(LedKind::from_code(0x0f).is_ok());
        assert!(LedKind::from_code(0x10).is_err());
    }
}
