//! The "keycap pointer" virtual device: the pointer half of one combo device, created when the
//! device is grabbed and destroyed when it is let go.
//!
//! Compiled and reviewed but deliberately unexercised by the test suite, like the keyboard
//! device: every test uses `RecordingEmitter`'s twins. What it registers is the pure
//! `PointerCaps`, which is tested.

use super::events::{EV_ABS, EV_KEY, EV_REL, EV_SYN, InputEvent};
use super::pointer::PointerCaps;
use super::sys::{self, BitSet, InputId, UinputSetup};
use super::uinput::{BUS_VIRTUAL, OutputError, PointerSink, UINPUT, open, write_batch};
use rustix::fd::OwnedFd;
use std::io;
use std::path::Path;

/// A created uinput pointer; destroyed on drop.
#[derive(Debug)]
pub(crate) struct PointerTwin {
    fd: OwnedFd,
}

impl PointerTwin {
    /// Creates a device with `caps`.
    pub(crate) fn create(caps: &PointerCaps) -> Result<Self, OutputError> {
        let unavailable = |e: io::Error| OutputError::Unavailable(e.to_string());
        let fd = open(Path::new(UINPUT)).map_err(unavailable)?;
        // From here the device may exist, so `Drop` destroys it should anything below fail.
        let twin = Self { fd };
        register(&twin.fd, caps).map_err(unavailable)?;
        sys::dev_setup(&twin.fd, setup(caps)).map_err(unavailable)?;
        sys::dev_create(&twin.fd).map_err(unavailable)?;
        Ok(twin)
    }
}

impl Drop for PointerTwin {
    fn drop(&mut self) {
        let _ = sys::dev_destroy(&self.fd);
    }
}

impl PointerSink for PointerTwin {
    fn write(&mut self, events: &[InputEvent]) -> Result<(), OutputError> {
        write_batch(&self.fd, events)
    }
}

/// Declares the event types, buttons, axes with their ranges and properties of the twin.
fn register(fd: &OwnedFd, caps: &PointerCaps) -> io::Result<()> {
    let kinds = [
        Some(EV_SYN),
        (!caps.buttons.is_empty()).then_some(EV_KEY),
        (!caps.rel.is_empty()).then_some(EV_REL),
        (!caps.abs.is_empty()).then_some(EV_ABS),
    ];
    kinds
        .into_iter()
        .flatten()
        .try_for_each(|kind| sys::set_bit(fd, BitSet::Ev, kind))?;
    caps.buttons
        .iter()
        .try_for_each(|&code| sys::set_bit(fd, BitSet::Key, code))?;
    caps.rel
        .iter()
        .try_for_each(|&axis| sys::set_bit(fd, BitSet::Rel, axis))?;
    caps.abs.iter().try_for_each(|axis| {
        sys::set_bit(fd, BitSet::Abs, axis.code)?;
        sys::abs_setup(fd, axis.code, axis.info)
    })?;
    caps.props
        .iter()
        .try_for_each(|&prop| sys::set_bit(fd, BitSet::Prop, prop))
}

/// The identity the twin is created under: the original's vendor and product, so the desktop's
/// per-device quirks still match, on the virtual bus.
fn setup(caps: &PointerCaps) -> UinputSetup {
    let mut name = [0u8; 80];
    let bytes = caps.name.as_bytes();
    let len = bytes.len().min(name.len() - 1);
    name[..len].copy_from_slice(&bytes[..len]);
    UinputSetup {
        id: InputId {
            bustype: BUS_VIRTUAL,
            vendor: caps.vendor,
            product: caps.product,
            version: 1,
        },
        name,
        ff_effects_max: 0,
    }
}
