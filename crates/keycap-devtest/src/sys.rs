//! The harness's ioctls: the second module in the workspace allowed `unsafe`, and test-only.
//!
//! Every call goes through rustix's typed ioctl patterns, so the only unsafety is the promise
//! that the opcode and the argument type match the kernel's declaration; each block says which.
#![allow(unsafe_code)]

use rustix::fd::AsFd;
use rustix::io::Errno;
use rustix::ioctl::{self, Getter, IntegerSetter, NoArg, Opcode, Setter, opcode};
use std::io;

const EVIOCGRAB: Opcode = opcode::write::<i32>(b'E', 0x90);

const UI_DEV_CREATE: Opcode = opcode::none(b'U', 1);
const UI_DEV_DESTROY: Opcode = opcode::none(b'U', 2);
const UI_DEV_SETUP: Opcode = opcode::write::<UinputSetup>(b'U', 3);
const UI_ABS_SETUP: Opcode = opcode::write::<UinputAbsSetup>(b'U', 4);
const UI_SET_EVBIT: Opcode = opcode::write::<i32>(b'U', 100);
const UI_SET_KEYBIT: Opcode = opcode::write::<i32>(b'U', 101);
const UI_SET_RELBIT: Opcode = opcode::write::<i32>(b'U', 102);
const UI_SET_ABSBIT: Opcode = opcode::write::<i32>(b'U', 103);
const UI_SET_LEDBIT: Opcode = opcode::write::<i32>(b'U', 105);
const UI_SET_PROPBIT: Opcode = opcode::write::<i32>(b'U', 110);
const UI_GET_SYSNAME: Opcode = opcode::read::<[u8; SYSNAME_BYTES]>(b'U', 44);

/// Bytes `UI_GET_SYSNAME` is asked for; a sysname is `inputN`.
const SYSNAME_BYTES: usize = 64;

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

/// `struct input_absinfo`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct AbsInfo {
    pub value: i32,
    pub minimum: i32,
    pub maximum: i32,
    pub fuzz: i32,
    pub flat: i32,
    pub resolution: i32,
}

/// `struct uinput_abs_setup`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct UinputAbsSetup {
    pub code: u16,
    pub absinfo: AbsInfo,
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

/// Enables one capability bit on a uinput fd before `UI_DEV_CREATE`.
pub(crate) fn set_bit(fd: impl AsFd, set: BitSet, code: u16) -> io::Result<()> {
    let value = usize::from(code);
    // SAFETY: linux/uinput.h declares UI_SET_{EV,KEY,REL,ABS,LED,PROP}BIT as _IOW('U', n, int)
    // but the kernel reads the argument as an integer value, not a pointer (uinput_ioctl_handler
    // passes `arg` straight to set_bit). IntegerSetter passes exactly that integer.
    let r = unsafe {
        match set {
            BitSet::Ev => ioctl::ioctl(&fd, IntegerSetter::<UI_SET_EVBIT>::new_usize(value)),
            BitSet::Key => ioctl::ioctl(&fd, IntegerSetter::<UI_SET_KEYBIT>::new_usize(value)),
            BitSet::Rel => ioctl::ioctl(&fd, IntegerSetter::<UI_SET_RELBIT>::new_usize(value)),
            BitSet::Abs => ioctl::ioctl(&fd, IntegerSetter::<UI_SET_ABSBIT>::new_usize(value)),
            BitSet::Led => ioctl::ioctl(&fd, IntegerSetter::<UI_SET_LEDBIT>::new_usize(value)),
            BitSet::Prop => ioctl::ioctl(&fd, IntegerSetter::<UI_SET_PROPBIT>::new_usize(value)),
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

/// Declares one absolute axis's range.
pub(crate) fn abs_setup(fd: impl AsFd, setup: UinputAbsSetup) -> io::Result<()> {
    // SAFETY: UI_ABS_SETUP is _IOW('U', 4, struct uinput_abs_setup); `UinputAbsSetup` is its
    // #[repr(C)] mirror (a u16 code, padding, then struct input_absinfo) and the opcode is
    // sized from it.
    unsafe { ioctl::ioctl(fd, Setter::<UI_ABS_SETUP, UinputAbsSetup>::new(setup)) }
        .map_err(io::Error::from)
}

/// Creates the device.
pub(crate) fn dev_create(fd: impl AsFd) -> io::Result<()> {
    // SAFETY: UI_DEV_CREATE is _IO('U', 1): no argument.
    unsafe { ioctl::ioctl(fd, NoArg::<UI_DEV_CREATE>::new()) }.map_err(io::Error::from)
}

/// Destroys the device.
pub(crate) fn dev_destroy(fd: impl AsFd) -> io::Result<()> {
    // SAFETY: UI_DEV_DESTROY is _IO('U', 2): no argument.
    unsafe { ioctl::ioctl(fd, NoArg::<UI_DEV_DESTROY>::new()) }.map_err(io::Error::from)
}

/// The sysfs name (`inputN`) of the created device, which names its directory under
/// `/sys/class/input` whatever else is registered under the same device name.
pub(crate) fn sysname(fd: impl AsFd) -> io::Result<String> {
    // SAFETY: linux/uinput.h declares UI_GET_SYSNAME(len) as _IOC(_IOC_READ, 'U', 44, len); the
    // opcode is built for `[u8; 64]` and the kernel writes at most that many bytes, a
    // NUL-terminated string, into it.
    let bytes = unsafe { ioctl::ioctl(fd, Getter::<UI_GET_SYSNAME, [u8; SYSNAME_BYTES]>::new()) }
        .map_err(io::Error::from)?;
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    Ok(String::from_utf8_lossy(&bytes[..end]).into_owned())
}

/// Takes or releases the exclusive grab on an evdev fd; `EBUSY` when another reader holds it.
pub(crate) fn grab(fd: impl AsFd, grab: Grab) -> io::Result<()> {
    let arg = match grab {
        Grab::On => 1,
        Grab::Off => 0,
    };
    // SAFETY: linux/input.h declares EVIOCGRAB as _IOW('E', 0x90, int) and evdev_ioctl reads
    // the argument as an integer value (non-zero grabs), not a pointer; IntegerSetter passes
    // exactly that integer.
    unsafe { ioctl::ioctl(fd, IntegerSetter::<EVIOCGRAB>::new_usize(arg)) }.map_err(|e| match e {
        Errno::BUSY => io::Error::new(io::ErrorKind::ResourceBusy, "device is grabbed"),
        other => io::Error::from(other),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opcodes_match_the_kernel_headers() {
        // _IOC(dir, type, nr, size): dir in bits 30..32 (write 1, read 2), size in 16..30,
        // type in 8..16, nr in 0..8.
        const CASES: &[(&str, Opcode, Opcode)] = &[
            ("EVIOCGRAB", EVIOCGRAB, 0x4004_4590),
            ("UI_DEV_CREATE", UI_DEV_CREATE, 0x5501),
            ("UI_DEV_DESTROY", UI_DEV_DESTROY, 0x5502),
            ("UI_DEV_SETUP", UI_DEV_SETUP, 0x405c_5503),
            ("UI_ABS_SETUP", UI_ABS_SETUP, 0x401c_5504),
            ("UI_SET_EVBIT", UI_SET_EVBIT, 0x4004_5564),
            ("UI_SET_KEYBIT", UI_SET_KEYBIT, 0x4004_5565),
            ("UI_SET_RELBIT", UI_SET_RELBIT, 0x4004_5566),
            ("UI_SET_ABSBIT", UI_SET_ABSBIT, 0x4004_5567),
            ("UI_SET_LEDBIT", UI_SET_LEDBIT, 0x4004_5569),
            ("UI_SET_PROPBIT", UI_SET_PROPBIT, 0x4004_556e),
            ("UI_GET_SYSNAME(64)", UI_GET_SYSNAME, 0x8040_552c),
        ];
        for &(name, got, want) in CASES {
            assert_eq!(got, want, "{name}: {got:#x}");
        }
    }

    #[test]
    fn structs_have_the_kernel_sizes() {
        assert_eq!(std::mem::size_of::<UinputSetup>(), 92);
        assert_eq!(std::mem::size_of::<AbsInfo>(), 24);
        assert_eq!(std::mem::size_of::<UinputAbsSetup>(), 28);
    }
}
