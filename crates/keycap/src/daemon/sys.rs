//! The ioctls: the one module allowed `unsafe`.
//!
//! Every call goes through rustix's typed ioctl patterns, so the only unsafety left is the
//! promise that the opcode and the argument type match the kernel's declaration. Each block
//! states which declaration it matches.
#![allow(unsafe_code)]

use crate::KeysDown;
use rustix::fd::AsFd;
use rustix::ffi::c_void;
use rustix::io::Errno;
use rustix::ioctl::{
    self, Getter, IntegerSetter, Ioctl, IoctlOutput, NoArg, Opcode, Setter, opcode,
};
use std::io;
use std::mem::MaybeUninit;

/// Bytes in the `EVIOCGKEY` bitmap: `KEY_MAX / 8 + 1`.
const KEY_STATE_BYTES: usize = 96;

const EVIOCGKEY: Opcode = opcode::read::<[u8; KEY_STATE_BYTES]>(b'E', 0x18);
const EVIOCGRAB: Opcode = opcode::write::<i32>(b'E', 0x90);

/// Bytes in the `EVIOCGLED` bitmap: `LED_MAX` is 0x0f, so 16 bits.
const LED_STATE_BYTES: usize = 2;
const EVIOCGLED: Opcode = opcode::read::<[u8; LED_STATE_BYTES]>(b'E', 0x19);

/// Bytes of the `EVIOCGPROP` bitmap: `INPUT_PROP_MAX / 8 + 1` is 4, read as one 64-bit word.
const PROP_BYTES: usize = 8;
/// Bytes of the `EVIOCGBIT(EV_REL)` and `EVIOCGBIT(EV_ABS)` bitmaps: `REL_MAX` and `ABS_MAX`
/// fit one 64-bit word.
const AXIS_BYTES: usize = 8;
const EV_KEY: u8 = 0x01;
const EV_REL: u8 = 0x02;
const EV_ABS: u8 = 0x03;

const EVIOCGPROP: Opcode = opcode::read::<[u8; PROP_BYTES]>(b'E', 0x09);
const EVIOCGBIT_KEY: Opcode = opcode::read::<[u8; KEY_STATE_BYTES]>(b'E', 0x20 + EV_KEY);
const EVIOCGBIT_REL: Opcode = opcode::read::<[u8; AXIS_BYTES]>(b'E', 0x20 + EV_REL);
const EVIOCGBIT_ABS: Opcode = opcode::read::<[u8; AXIS_BYTES]>(b'E', 0x20 + EV_ABS);
/// The opcode of `EVIOCGABS(axis)`: it differs per axis, so it is computed at the call.
const fn eviocgabs(axis: u8) -> Opcode {
    opcode::read::<AbsInfo>(b'E', 0x40 + axis)
}

const UI_DEV_CREATE: Opcode = opcode::none(b'U', 1);
const UI_DEV_DESTROY: Opcode = opcode::none(b'U', 2);
const UI_DEV_SETUP: Opcode = opcode::write::<UinputSetup>(b'U', 3);
const UI_SET_EVBIT: Opcode = opcode::write::<i32>(b'U', 100);
const UI_SET_KEYBIT: Opcode = opcode::write::<i32>(b'U', 101);
const UI_SET_RELBIT: Opcode = opcode::write::<i32>(b'U', 102);
const UI_SET_ABSBIT: Opcode = opcode::write::<i32>(b'U', 103);
const UI_SET_LEDBIT: Opcode = opcode::write::<i32>(b'U', 105);
const UI_SET_PROPBIT: Opcode = opcode::write::<i32>(b'U', 110);
const UI_ABS_SETUP: Opcode = opcode::write::<UinputAbsSetup>(b'U', 4);

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

/// `struct input_absinfo`: the range and resolution of one absolute axis.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
struct UinputAbsSetup {
    code: u16,
    absinfo: AbsInfo,
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

/// The LEDs the kernel believes are lit on the device, as its little-endian bitmap.
pub(crate) fn leds_lit(fd: impl AsFd) -> io::Result<[u8; LED_STATE_BYTES]> {
    // SAFETY: linux/input.h declares EVIOCGLED(len) as _IOC(_IOC_READ, 'E', 0x19, len); the
    // opcode is built for `[u8; 2]` and the kernel writes at most that many bytes into it.
    unsafe { ioctl::ioctl(fd, Getter::<EVIOCGLED, [u8; LED_STATE_BYTES]>::new()) }
        .map_err(io::Error::from)
}

/// A read ioctl whose opcode is chosen at the call: `EVIOCGABS` carries the axis in its number.
struct Probe<T> {
    opcode: Opcode,
    out: MaybeUninit<T>,
}

// SAFETY: every `Probe` is built by this module with an opcode whose declared size is
// `size_of::<T>()` (`opcode::read::<T>`), `as_ptr` points at storage of exactly that type, and
// the kernel only writes into it on a successful call, which is when `output_from_ptr` reads it;
// `T` is plain integers, for which every bit pattern is valid.
unsafe impl<T> Ioctl for Probe<T> {
    type Output = T;
    const IS_MUTATING: bool = true;

    fn opcode(&self) -> Opcode {
        self.opcode
    }

    fn as_ptr(&mut self) -> *mut c_void {
        self.out.as_mut_ptr().cast()
    }

    unsafe fn output_from_ptr(
        _: IoctlOutput,
        ptr: *mut c_void,
    ) -> rustix::io::Result<Self::Output> {
        // SAFETY: the caller passes the pointer `as_ptr` gave after a successful ioctl, which
        // initialised the value.
        Ok(unsafe { ptr.cast::<T>().read() })
    }
}

/// The range and resolution of absolute axis `axis` (`EVIOCGABS`).
pub(crate) fn abs_info(fd: impl AsFd, axis: u16) -> io::Result<AbsInfo> {
    let axis = u8::try_from(axis).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    let probe = Probe::<AbsInfo> {
        opcode: eviocgabs(axis),
        out: MaybeUninit::uninit(),
    };
    // SAFETY: linux/input.h declares EVIOCGABS(abs) as _IOR('E', 0x40 + abs, struct
    // input_absinfo); `AbsInfo` is its #[repr(C)] mirror and the opcode is sized from it.
    unsafe { ioctl::ioctl(fd, probe) }.map_err(io::Error::from)
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
    // SAFETY (each arm): linux/input.h declares EVIOCGBIT(ev, len) as _IOC(_IOC_READ, 'E',
    // 0x20 + ev, len) and EVIOCGPROP(len) as _IOC(_IOC_READ, 'E', 0x09, len); each opcode is
    // built for the array type it is read into, and the kernel writes at most that many bytes.
    let bytes = unsafe {
        match capability {
            Capability::Key => {
                ioctl::ioctl(fd, Getter::<EVIOCGBIT_KEY, [u8; KEY_STATE_BYTES]>::new())
                    .map(Vec::from)
            }
            Capability::Rel => {
                ioctl::ioctl(fd, Getter::<EVIOCGBIT_REL, [u8; AXIS_BYTES]>::new()).map(Vec::from)
            }
            Capability::Abs => {
                ioctl::ioctl(fd, Getter::<EVIOCGBIT_ABS, [u8; AXIS_BYTES]>::new()).map(Vec::from)
            }
            Capability::Prop => {
                ioctl::ioctl(fd, Getter::<EVIOCGPROP, [u8; PROP_BYTES]>::new()).map(Vec::from)
            }
        }
    };
    bytes.map_err(io::Error::from)
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

/// Enables one capability bit on a uinput fd before `UI_DEV_CREATE`.
pub(crate) fn set_bit(fd: impl AsFd, set: BitSet, code: u16) -> io::Result<()> {
    let value = usize::from(code);
    // SAFETY: linux/uinput.h declares UI_SET_{EV,KEY,REL,ABS,LED,PROP}BIT as _IOW('U', n, int) but the
    // kernel reads the argument as an integer value, not a pointer (uinput_ioctl_handler
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

/// Declares the range and resolution of one absolute axis on a uinput fd, before
/// `UI_DEV_CREATE`.
pub(crate) fn abs_setup(fd: impl AsFd, code: u16, absinfo: AbsInfo) -> io::Result<()> {
    // SAFETY: UI_ABS_SETUP is _IOW('U', 4, struct uinput_abs_setup); `UinputAbsSetup` is its
    // #[repr(C)] mirror (u16 code, then `struct input_absinfo`) and the opcode is sized from it.
    unsafe {
        ioctl::ioctl(
            fd,
            Setter::<UI_ABS_SETUP, UinputAbsSetup>::new(UinputAbsSetup { code, absinfo }),
        )
    }
    .map_err(io::Error::from)
}

/// Declares the device's identity.
pub(crate) fn dev_setup(fd: impl AsFd, setup: UinputSetup) -> io::Result<()> {
    // SAFETY: UI_DEV_SETUP is _IOW('U', 3, struct uinput_setup); `UinputSetup` is its
    // #[repr(C)] mirror and the opcode is sized from it.
    unsafe { ioctl::ioctl(fd, Setter::<UI_DEV_SETUP, UinputSetup>::new(setup)) }
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
            ("EVIOCGLED(2)", EVIOCGLED, 0x8002_4519),
            ("EVIOCGPROP(8)", EVIOCGPROP, 0x8008_4509),
            ("EVIOCGBIT(EV_KEY, 96)", EVIOCGBIT_KEY, 0x8060_4521),
            ("EVIOCGBIT(EV_REL, 8)", EVIOCGBIT_REL, 0x8008_4522),
            ("EVIOCGBIT(EV_ABS, 8)", EVIOCGBIT_ABS, 0x8008_4523),
            ("EVIOCGABS(ABS_X)", eviocgabs(0x00), 0x8018_4540),
            ("EVIOCGABS(ABS_MT_SLOT)", eviocgabs(0x2f), 0x8018_456f),
            ("EVIOCGABS(ABS_MAX)", eviocgabs(0x3f), 0x8018_457f),
            ("UI_DEV_CREATE", UI_DEV_CREATE, 0x5501),
            ("UI_DEV_DESTROY", UI_DEV_DESTROY, 0x5502),
            ("UI_DEV_SETUP", UI_DEV_SETUP, 0x405c_5503),
            ("UI_SET_EVBIT", UI_SET_EVBIT, 0x4004_5564),
            ("UI_SET_KEYBIT", UI_SET_KEYBIT, 0x4004_5565),
            ("UI_SET_RELBIT", UI_SET_RELBIT, 0x4004_5566),
            ("UI_SET_ABSBIT", UI_SET_ABSBIT, 0x4004_5567),
            ("UI_SET_LEDBIT", UI_SET_LEDBIT, 0x4004_5569),
            ("UI_SET_PROPBIT", UI_SET_PROPBIT, 0x4004_556e),
            ("UI_ABS_SETUP", UI_ABS_SETUP, 0x401c_5504),
        ];
        for &(name, got, want) in CASES {
            assert_eq!(got, want, "{name}: {got:#x}");
        }
    }

    #[test]
    fn structs_have_the_kernel_sizes() {
        assert_eq!(std::mem::size_of::<InputId>(), 8);
        assert_eq!(std::mem::size_of::<UinputSetup>(), 92);
        assert_eq!(std::mem::size_of::<AbsInfo>(), 24);
        assert_eq!(std::mem::size_of::<UinputAbsSetup>(), 28);
    }

    #[test]
    fn grab_arguments_are_one_and_zero() {
        assert_eq!((Grab::On.arg(), Grab::Off.arg()), (1, 0));
    }
}
