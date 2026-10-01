//! The ioctls: the one module allowed `unsafe`.
//!
//! Every call goes through rustix's typed ioctl patterns, so the only unsafety left is the
//! promise that the opcode and the argument type match the kernel's declaration. Each block
//! states which declaration it matches.
#![allow(unsafe_code)]

use crate::KeysDown;
use rustix::fd::AsFd;
use rustix::io::Errno;
use rustix::ioctl::{self, Getter, IntegerSetter, NoArg, Opcode, Setter, opcode};
use std::io;

/// Bytes in the `EVIOCGKEY` bitmap: `KEY_MAX / 8 + 1`.
const KEY_STATE_BYTES: usize = 96;

const EVIOCGKEY: Opcode = opcode::read::<[u8; KEY_STATE_BYTES]>(b'E', 0x18);
const EVIOCGRAB: Opcode = opcode::write::<i32>(b'E', 0x90);

const UI_DEV_CREATE: Opcode = opcode::none(b'U', 1);
const UI_DEV_DESTROY: Opcode = opcode::none(b'U', 2);
const UI_DEV_SETUP: Opcode = opcode::write::<UinputSetup>(b'U', 3);
const UI_SET_EVBIT: Opcode = opcode::write::<i32>(b'U', 100);
const UI_SET_KEYBIT: Opcode = opcode::write::<i32>(b'U', 101);
const UI_SET_RELBIT: Opcode = opcode::write::<i32>(b'U', 102);
const UI_SET_LEDBIT: Opcode = opcode::write::<i32>(b'U', 105);

/// `struct input_id`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InputId {
    pub bustype: u16,
    pub vendor: u16,
    pub product: u16,
    pub version: u16,
}

/// `struct uinput_setup`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct UinputSetup {
    pub id: InputId,
    pub name: [u8; 80],
    pub ff_effects_max: u32,
}

/// Whether a device is held exclusively.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Grab {
    On,
    Off,
}

impl Grab {
    /// The ioctl's argument: non-zero grabs, zero releases.
    const fn arg(self) -> usize {
        match self {
            Grab::On => 1,
            Grab::Off => 0,
        }
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
    // SAFETY: linux/input.h declares EVIOCGRAB as _IOW('E', 0x90, int) and evdev_ioctl reads
    // the argument as an integer value (non-zero grabs), not a pointer; IntegerSetter passes
    // exactly that integer.
    unsafe { ioctl::ioctl(fd, IntegerSetter::<EVIOCGRAB>::new_usize(grab.arg())) }.map_err(|e| {
        match e {
            Errno::BUSY => GrabError::Busy,
            other => GrabError::Other(io::Error::from(other)),
        }
    })
}

/// The keys the kernel believes are down on the device.
pub(crate) fn keys_down(fd: impl AsFd) -> io::Result<KeysDown> {
    // SAFETY: linux/input.h declares EVIOCGKEY(len) as _IOC(_IOC_READ, 'E', 0x18, len); the
    // opcode is built for `[u8; 96]` and the kernel writes at most that many bytes into it.
    unsafe { ioctl::ioctl(fd, Getter::<EVIOCGKEY, [u8; KEY_STATE_BYTES]>::new()) }
        .map(KeysDown::from_bytes)
        .map_err(io::Error::from)
}

/// Which capability bit set a `UI_SET_*BIT` call writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BitSet {
    Ev,
    Key,
    Rel,
    Led,
}

/// Enables one capability bit on a uinput fd before `UI_DEV_CREATE`.
pub(crate) fn set_bit(fd: impl AsFd, set: BitSet, code: u16) -> io::Result<()> {
    let value = usize::from(code);
    // SAFETY: linux/uinput.h declares UI_SET_{EV,KEY,REL,LED}BIT as _IOW('U', n, int) but the
    // kernel reads the argument as an integer value, not a pointer (uinput_ioctl_handler
    // passes `arg` straight to set_bit). IntegerSetter passes exactly that integer.
    let r = unsafe {
        match set {
            BitSet::Ev => ioctl::ioctl(&fd, IntegerSetter::<UI_SET_EVBIT>::new_usize(value)),
            BitSet::Key => ioctl::ioctl(&fd, IntegerSetter::<UI_SET_KEYBIT>::new_usize(value)),
            BitSet::Rel => ioctl::ioctl(&fd, IntegerSetter::<UI_SET_RELBIT>::new_usize(value)),
            BitSet::Led => ioctl::ioctl(&fd, IntegerSetter::<UI_SET_LEDBIT>::new_usize(value)),
        }
    };
    r.map_err(io::Error::from)
}

/// Declares the device's identity.
pub(crate) fn dev_setup(fd: impl AsFd, setup: UinputSetup) -> io::Result<()> {
    // SAFETY: UI_DEV_SETUP is _IOW('U', 3, struct uinput_setup); `UinputSetup` is its
    // #[repr(C)] mirror and the opcode is sized from it.
    unsafe { ioctl::ioctl(fd, Setter::<UI_DEV_SETUP, UinputSetup>::new(setup)) }
        .map_err(io::Error::from)
}

/// Creates the device. Never called by any test.
pub(crate) fn dev_create(fd: impl AsFd) -> io::Result<()> {
    // SAFETY: UI_DEV_CREATE is _IO('U', 1): no argument.
    unsafe { ioctl::ioctl(fd, NoArg::<UI_DEV_CREATE>::new()) }.map_err(io::Error::from)
}

/// Destroys the device.
pub(crate) fn dev_destroy(fd: impl AsFd) -> io::Result<()> {
    // SAFETY: UI_DEV_DESTROY is _IO('U', 2): no argument.
    unsafe { ioctl::ioctl(fd, NoArg::<UI_DEV_DESTROY>::new()) }.map_err(io::Error::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opcodes_match_the_kernel_headers() {
        // Values from linux/input.h and linux/uinput.h: _IOC(dir, type, nr, size) with dir in
        // bits 30..32 (write 1, read 2), size in 16..30, type in 8..16, nr in 0..8.
        const CASES: &[(&str, Opcode, Opcode)] = &[
            ("EVIOCGKEY(96)", EVIOCGKEY, 0x8060_4518),
            ("EVIOCGRAB", EVIOCGRAB, 0x4004_4590),
            ("UI_DEV_CREATE", UI_DEV_CREATE, 0x5501),
            ("UI_DEV_DESTROY", UI_DEV_DESTROY, 0x5502),
            ("UI_DEV_SETUP", UI_DEV_SETUP, 0x405c_5503),
            ("UI_SET_EVBIT", UI_SET_EVBIT, 0x4004_5564),
            ("UI_SET_KEYBIT", UI_SET_KEYBIT, 0x4004_5565),
            ("UI_SET_RELBIT", UI_SET_RELBIT, 0x4004_5566),
            ("UI_SET_LEDBIT", UI_SET_LEDBIT, 0x4004_5569),
        ];
        for &(name, got, want) in CASES {
            assert_eq!(got, want, "{name}: {got:#x}");
        }
    }

    #[test]
    fn structs_have_the_kernel_sizes() {
        assert_eq!(std::mem::size_of::<InputId>(), 8);
        assert_eq!(std::mem::size_of::<UinputSetup>(), 92);
    }

    #[test]
    fn grab_arguments_are_one_and_zero() {
        assert_eq!((Grab::On.arg(), Grab::Off.arg()), (1, 0));
    }
}
