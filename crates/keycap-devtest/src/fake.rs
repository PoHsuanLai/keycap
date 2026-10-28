//! A keyboard that exists only as a uinput device: what a person's hands do, from a test.

use crate::event::{EV_ABS, EV_KEY, EV_LED, EV_REL, EV_SYN, Event, LED_CAPSL, drain, encode};
use crate::failure::{Failure, errno, io};
use crate::keys::{KEYS, code_of};
use crate::sys::{self, AbsInfo, BitSet, InputId, UinputAbsSetup, UinputSetup};
use crate::sysfs::{node_of_sysname, registered, wait_for};
use rustix::fd::OwnedFd;
use rustix::fs::{Mode, OFlags};
use std::path::{Path, PathBuf};
use std::time::Duration;

const UINPUT: &str = "/dev/uinput";
/// `BUS_USB`.
const BUS_USB: u16 = 0x03;
/// How long the node may take to appear after the device is created, and to vanish after it
/// is destroyed.
const NODE_WAIT: Duration = Duration::from_secs(5);

const LED_NUML: u16 = 0x00;
const LED_SCROLLL: u16 = 0x02;
const REL_X: u16 = 0x00;
const REL_Y: u16 = 0x01;
const BTN_LEFT: u16 = 0x110;
const BTN_TOOL_FINGER: u16 = 0x145;
const BTN_TOUCH: u16 = 0x14a;
const BTN_TOOL_DOUBLETAP: u16 = 0x14d;
const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;
const ABS_MT_SLOT: u16 = 0x2f;
const ABS_MT_POSITION_X: u16 = 0x35;
const ABS_MT_POSITION_Y: u16 = 0x36;
const ABS_MT_TRACKING_ID: u16 = 0x39;
const INPUT_PROP_POINTER: u16 = 0;
const INPUT_PROP_BUTTONPAD: u16 = 2;

/// Whether the fake is also a pointer, as a receiver with a trackpad is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pointer {
    None,
    /// Relative motion and a left button.
    Mouse,
    /// A mouse's half plus a buttonpad's multitouch axes and `INPUT_PROP_POINTER` and
    /// `INPUT_PROP_BUTTONPAD`.
    Touchpad,
}

/// What the fake registers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeSpec {
    pub name: String,
    pub vendor: u16,
    pub product: u16,
    /// Whether the fake has lock LEDs (Num, Caps and Scroll Lock) a compositor can light.
    pub locks: bool,
    pub pointer: Pointer,
}

impl FakeSpec {
    /// An Apple keyboard: vendor `0x05ac`, so keycap gives it the Apple layout.
    pub fn apple(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            vendor: 0x05ac,
            product: 0x0267,
            locks: true,
            pointer: Pointer::None,
        }
    }

    /// A PC keyboard: vendor `0x046d`.
    pub fn pc(name: &str) -> Self {
        Self {
            vendor: 0x046d,
            product: 0xc31c,
            ..Self::apple(name)
        }
    }

    /// The same keyboard with a pointer.
    pub fn with_pointer(self, pointer: Pointer) -> Self {
        Self { pointer, ..self }
    }
}

/// A created fake keyboard; destroyed on drop.
#[derive(Debug)]
pub struct FakeKeyboard {
    fd: OwnedFd,
    name: String,
    sysname: String,
    node: PathBuf,
}

impl FakeKeyboard {
    /// Creates the device and waits for its node.
    pub fn create(spec: &FakeSpec) -> Result<Self, Failure> {
        let fd = rustix::fs::open(
            UINPUT,
            OFlags::RDWR | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(errno(format!(
            "opening {UINPUT} (is the uinput module loaded and is it writable by this user?)"
        )))?;
        let abs =
            register(&fd, spec).map_err(io("registering the fake keyboard's capabilities"))?;
        sys::dev_create(&fd, setup(spec), &abs).map_err(io("UI_DEV_SETUP, UI_DEV_CREATE"))?;
        let sysname = sys::sysname(&fd).map_err(io("UI_GET_SYSNAME"))?;
        let node = wait_for(NODE_WAIT, || node_of_sysname(&sysname)).ok_or_else(|| {
            Failure::new(format!(
                "the fake keyboard {:?} ({sysname}) has no /dev/input node after {NODE_WAIT:?}",
                spec.name
            ))
        })?;
        Ok(Self {
            fd,
            name: spec.name.clone(),
            sysname,
            node,
        })
    }

    /// The device node the kernel made for it.
    pub fn node(&self) -> &Path {
        &self.node
    }

    /// The name it registered under.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Writes `events` as one batch, then a `SYN_REPORT`.
    pub fn emit(&self, events: &[Event]) -> Result<(), Failure> {
        let mut frame = events.to_vec();
        frame.push(Event::SYNC);
        let bytes = encode(&frame);
        let n =
            rustix::io::write(&self.fd, &bytes).map_err(errno("writing to the fake keyboard"))?;
        if n == bytes.len() {
            Ok(())
        } else {
            Err(Failure::new(format!(
                "short write to the fake keyboard: {n} of {} bytes",
                bytes.len()
            )))
        }
    }

    fn key(&self, name: &str, value: i32) -> Result<(), Failure> {
        let code = code_of(name).ok_or_else(|| Failure::new(format!("no key named {name:?}")))?;
        self.emit(&[Event::key(code, value)])
    }

    /// Presses `name`, in its own frame.
    pub fn press(&self, name: &str) -> Result<(), Failure> {
        self.key(name, 1)
    }

    /// Releases `name`, in its own frame.
    pub fn release(&self, name: &str) -> Result<(), Failure> {
        self.key(name, 0)
    }

    /// Presses and releases `name`.
    pub fn tap(&self, name: &str) -> Result<(), Failure> {
        self.press(name)?;
        self.release(name)
    }

    /// Presses each of `names` in order, then releases them in reverse.
    pub fn chord(&self, names: &[&str]) -> Result<(), Failure> {
        names.iter().try_for_each(|n| self.press(n))?;
        names.iter().rev().try_for_each(|n| self.release(n))
    }

    /// The LED events that reached the fake since the last call, as `(led, value)`: what a
    /// compositor (through keycap) wrote to the keyboard it believes it is typing on.
    pub fn leds(&self) -> Vec<(u16, i32)> {
        drain(&self.fd)
            .0
            .into_iter()
            .filter(|e| e.kind == EV_LED)
            .map(|e| (e.code, e.value))
            .collect()
    }

    /// Waits until the LED `code` is written as `value`; the events seen on the way are in the
    /// failure.
    pub fn expect_led(&self, code: u16, value: i32, wait: Duration) -> Result<(), Failure> {
        let mut seen = Vec::new();
        let hit = wait_for(wait, || {
            seen.extend(self.leds());
            seen.contains(&(code, value)).then_some(())
        });
        hit.ok_or_else(|| {
            Failure::new(format!(
                "LED {code} was not set to {value} within {wait:?}; LED writes seen: {seen:?}"
            ))
        })
    }

    /// Unplugs it: destroys the device and waits until the kernel has removed it.
    pub fn destroy(self) {
        drop(self);
    }
}

impl Drop for FakeKeyboard {
    fn drop(&mut self) {
        let _ = sys::dev_destroy(&self.fd);
        // The next test creates a device under the same name; wait until this one is gone so
        // discovery never sees two.
        let _ = wait_for(NODE_WAIT, || (!registered(&self.sysname)).then_some(()));
    }
}

fn setup(spec: &FakeSpec) -> UinputSetup {
    let mut name = [0u8; 80];
    let bytes = spec.name.as_bytes();
    let n = bytes.len().min(name.len() - 1);
    name[..n].copy_from_slice(&bytes[..n]);
    UinputSetup {
        id: InputId {
            bustype: BUS_USB,
            vendor: spec.vendor,
            product: spec.product,
            version: 1,
        },
        name,
        ff_effects_max: 0,
    }
}

/// Declares what the fake can send: every key in the harness's table, the LEDs and the pointer
/// the spec asks for. Returns the absolute axes with their ranges, which go in at creation.
fn register(fd: &OwnedFd, spec: &FakeSpec) -> std::io::Result<Vec<UinputAbsSetup>> {
    let bit = |set, code| sys::set_bit(fd, set, code);
    bit(BitSet::Ev, EV_SYN)?;
    bit(BitSet::Ev, EV_KEY)?;
    KEYS.iter()
        .try_for_each(|&(_, code)| bit(BitSet::Key, code))?;
    if spec.locks {
        bit(BitSet::Ev, EV_LED)?;
        [LED_NUML, LED_CAPSL, LED_SCROLLL]
            .into_iter()
            .try_for_each(|led| bit(BitSet::Led, led))?;
    }
    match spec.pointer {
        Pointer::None => Ok(Vec::new()),
        Pointer::Mouse => relative_half(fd).map(|()| Vec::new()),
        Pointer::Touchpad => relative_half(fd).and_then(|()| touchpad_half(fd)),
    }
}

fn relative_half(fd: &OwnedFd) -> std::io::Result<()> {
    sys::set_bit(fd, BitSet::Ev, EV_REL)?;
    sys::set_bit(fd, BitSet::Rel, REL_X)?;
    sys::set_bit(fd, BitSet::Rel, REL_Y)?;
    sys::set_bit(fd, BitSet::Key, BTN_LEFT)
}

fn touchpad_half(fd: &OwnedFd) -> std::io::Result<Vec<UinputAbsSetup>> {
    sys::set_bit(fd, BitSet::Ev, EV_ABS)?;
    [BTN_TOUCH, BTN_TOOL_FINGER, BTN_TOOL_DOUBLETAP]
        .into_iter()
        .try_for_each(|code| sys::set_bit(fd, BitSet::Key, code))?;
    let axes: [(u16, i32, i32); 6] = [
        (ABS_X, 0, 1200),
        (ABS_Y, 0, 800),
        (ABS_MT_SLOT, 0, 4),
        (ABS_MT_POSITION_X, 0, 1200),
        (ABS_MT_POSITION_Y, 0, 800),
        (ABS_MT_TRACKING_ID, 0, 65535),
    ];
    let setups = axes.map(|(code, minimum, maximum)| UinputAbsSetup {
        code,
        absinfo: AbsInfo {
            value: 0,
            minimum,
            maximum,
            fuzz: 0,
            flat: 0,
            resolution: 12,
        },
    });
    axes.iter()
        .try_for_each(|&(code, _, _)| sys::set_bit(fd, BitSet::Abs, code))?;
    sys::set_bit(fd, BitSet::Prop, INPUT_PROP_POINTER)?;
    sys::set_bit(fd, BitSet::Prop, INPUT_PROP_BUTTONPAD)?;
    Ok(setups.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_setup_names_the_vendor_and_truncates_a_long_name() {
        let s = setup(&FakeSpec::apple("kb"));
        assert_eq!((s.id.vendor, s.id.product), (0x05ac, 0x0267));
        assert_eq!(&s.name[..3], b"kb\0");
        let long = setup(&FakeSpec::pc(&"x".repeat(200)));
        assert_eq!(long.name[79], 0, "always NUL-terminated");
        assert_eq!(long.id.vendor, 0x046d);
    }

    #[test]
    fn the_spec_builders_keep_the_identity_when_adding_a_pointer() {
        let spec = FakeSpec::apple("kb").with_pointer(Pointer::Touchpad);
        assert_eq!(spec.pointer, Pointer::Touchpad);
        assert_eq!(spec.vendor, 0x05ac);
        assert!(spec.locks);
    }
}
