//! The one virtual device keycap writes: "keycap keyboard".
//!
//! Compiled and reviewed but deliberately unexercised by the test suite: every test uses
//! `RecordingEmitter`. The events it writes are the pure `events` functions, which are
//! tested; what is untested here is only device creation and recreation and the read of LED
//! changes.

use super::evdev::drain;
use super::events::{
    EV_KEY, EV_LED, EV_REL, EV_SYN, InputEvent, MAX_CODE, REL_HWHEEL, REL_HWHEEL_HI_RES, REL_WHEEL,
    REL_WHEEL_HI_RES, REL_X, REL_Y,
};
use super::leds::{LED_CODES, LedEvent, decode as decode_led};
use super::sys::{self, BitSet, InputId, UinputSetup};
use rustix::fd::{AsFd, BorrowedFd, OwnedFd};
use rustix::fs::{Mode, OFlags};
use std::io;
use std::path::Path;

/// The uinput control node.
pub(crate) const UINPUT: &str = "/dev/uinput";
/// `BUS_VIRTUAL`.
const BUS_VIRTUAL: u16 = 0x06;
/// The environment variable that must be `1` before `--check-uinput` opens anything.
pub(crate) const PROBE_PERMIT: &str = "KEYCAP_ALLOW_UINPUT_PROBE";

/// The name the virtual keyboard registers under; discovery never grabs a device with it.
pub(crate) const NAME: &str = "keycap keyboard";
/// Vendor id of the virtual keyboard: "KC" in ASCII. Not a registered USB vendor; it only has
/// to differ from palmrest's 0x0001 and from every keyboard keycap would grab.
pub(crate) const VENDOR: u16 = 0x4b43;
/// Product id of the virtual keyboard.
pub(crate) const PRODUCT: u16 = 0x0001;

/// Every key and button code the virtual keyboard can write: a combo device's mouse buttons
/// pass through it too.
pub(crate) const CODES: std::ops::RangeInclusive<u16> = 1..=MAX_CODE;
/// The relative axes it can write, for the pointer half of a combo device.
pub(crate) const REL_AXES: [u16; 6] = [
    REL_X,
    REL_Y,
    REL_WHEEL,
    REL_HWHEEL,
    REL_WHEEL_HI_RES,
    REL_HWHEEL_HI_RES,
];

/// A virtual device write that failed.
#[derive(Debug, thiserror::Error)]
pub(crate) enum OutputError {
    #[error("uinput is not available: {0}")]
    Unavailable(String),
    #[error("writing to the virtual device failed: {0}")]
    Write(#[from] io::Error),
}

/// Where written events go.
pub(crate) trait Emitter {
    /// Writes `events` in order; one call is one atomic batch.
    fn write(&mut self, events: &[InputEvent]) -> Result<(), OutputError>;
    /// Destroys the device and creates a fresh one, which has every key up. For when a write
    /// failed and what the device holds is no longer known.
    fn recreate(&mut self) -> Result<(), OutputError>;
    /// The fd that becomes readable when the desktop sets a LED, for the reader's `poll`.
    fn led_fd(&self) -> Option<BorrowedFd<'_>>;
    /// The LED changes the desktop wrote to the device since the last call.
    fn leds(&mut self) -> Vec<LedEvent>;
}

/// A created uinput device; destroyed on drop.
#[derive(Debug)]
pub(crate) struct Device {
    fd: OwnedFd,
}

impl Device {
    /// Creates "keycap keyboard".
    pub(crate) fn create() -> Result<Self, OutputError> {
        let unavailable = |e: io::Error| OutputError::Unavailable(e.to_string());
        let fd = open(Path::new(UINPUT)).map_err(unavailable)?;
        register(&fd).map_err(unavailable)?;
        // From here the device exists, so `Drop` destroys it should anything below fail.
        let device = Self { fd };
        sys::dev_setup(&device.fd, setup()).map_err(unavailable)?;
        sys::dev_create(&device.fd).map_err(unavailable)?;
        Ok(device)
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        let _ = sys::dev_destroy(&self.fd);
    }
}

impl Emitter for Device {
    fn write(&mut self, events: &[InputEvent]) -> Result<(), OutputError> {
        if events.is_empty() {
            return Ok(());
        }
        let bytes: Vec<u8> = events.iter().flat_map(|e| e.to_bytes()).collect();
        let n = rustix::io::write(&self.fd, &bytes).map_err(io::Error::from)?;
        if n == bytes.len() {
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::WriteZero, "short uinput write").into())
        }
    }

    fn recreate(&mut self) -> Result<(), OutputError> {
        // The old device goes first so the desktop never sees two keycap keyboards. Should the
        // new one fail, the old fd stays, its writes fail, and the caller decides what next.
        let _ = sys::dev_destroy(&self.fd);
        *self = Device::create()?;
        Ok(())
    }

    fn led_fd(&self) -> Option<BorrowedFd<'_>> {
        Some(self.fd.as_fd())
    }

    fn leds(&mut self) -> Vec<LedEvent> {
        drain(&self.fd)
            .events
            .into_iter()
            .filter_map(decode_led)
            .collect()
    }
}

fn open(node: &Path) -> io::Result<OwnedFd> {
    Ok(rustix::fs::open(
        node,
        OFlags::RDWR | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )?)
}

/// Declares every event type, key, axis and LED the device can write; the LEDs are what the
/// desktop writes back to it.
fn register(fd: &OwnedFd) -> io::Result<()> {
    [EV_SYN, EV_KEY, EV_REL, EV_LED]
        .into_iter()
        .try_for_each(|kind| sys::set_bit(fd, BitSet::Ev, kind))?;
    CODES
        .into_iter()
        .try_for_each(|code| sys::set_bit(fd, BitSet::Key, code))?;
    REL_AXES
        .into_iter()
        .try_for_each(|axis| sys::set_bit(fd, BitSet::Rel, axis))?;
    LED_CODES
        .into_iter()
        .try_for_each(|led| sys::set_bit(fd, BitSet::Led, led))
}

/// The identity the device is created under.
fn setup() -> UinputSetup {
    let mut name = [0u8; 80];
    name[..NAME.len()].copy_from_slice(NAME.as_bytes());
    UinputSetup {
        id: InputId {
            bustype: BUS_VIRTUAL,
            vendor: VENDOR,
            product: PRODUCT,
            version: 1,
        },
        name,
        ff_effects_max: 0,
    }
}

/// Whether `--check-uinput` may open the node: only when the permit variable is exactly `1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProbePermit {
    Granted,
    Refused,
}

impl ProbePermit {
    /// The permit the variable's value gives.
    pub(crate) fn from_env_value(v: Option<&str>) -> Self {
        match v {
            Some("1") => Self::Granted,
            _ => Self::Refused,
        }
    }
}

/// Opens and closes the uinput node without creating anything: can this user make devices?
pub(crate) fn probe(permit: ProbePermit, node: &Path) -> Result<(), OutputError> {
    match permit {
        ProbePermit::Refused => Err(OutputError::Unavailable(format!(
            "set {PROBE_PERMIT}=1 to let --check-uinput open {UINPUT}"
        ))),
        ProbePermit::Granted => open(node)
            .map(drop)
            .map_err(|e| OutputError::Unavailable(format!("{}: {e}", node.display()))),
    }
}

/// Remembers every batch instead of writing it; fails the writes a test asks it to, counts
/// recreations, and hands over the LED changes a test queues.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct RecordingEmitter {
    pub batches: Vec<Vec<InputEvent>>,
    /// How many of the next writes fail.
    pub failing: u32,
    /// How many times the device was recreated.
    pub recreated: u32,
    /// What `leds` returns next.
    pub queued_leds: Vec<LedEvent>,
}

#[cfg(test)]
impl Emitter for RecordingEmitter {
    fn write(&mut self, events: &[InputEvent]) -> Result<(), OutputError> {
        if self.failing > 0 {
            self.failing -= 1;
            return Err(io::Error::other("injected failure").into());
        }
        if events.is_empty() {
            return Ok(());
        }
        self.batches.push(events.to_vec());
        Ok(())
    }

    fn recreate(&mut self) -> Result<(), OutputError> {
        self.recreated += 1;
        Ok(())
    }

    fn led_fd(&self) -> Option<BorrowedFd<'_>> {
        None
    }

    fn leds(&mut self) -> Vec<LedEvent> {
        std::mem::take(&mut self.queued_leds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::events::{SYN_REPORT, decode, is_button};

    #[test]
    fn probe_permit_table() {
        const CASES: &[(Option<&str>, ProbePermit)] = &[
            (None, ProbePermit::Refused),
            (Some(""), ProbePermit::Refused),
            (Some("yes"), ProbePermit::Refused),
            (Some("0"), ProbePermit::Refused),
            (Some("1"), ProbePermit::Granted),
        ];
        for &(value, want) in CASES {
            assert_eq!(ProbePermit::from_env_value(value), want, "{value:?}");
        }
    }

    #[test]
    fn refused_probe_never_touches_the_node() {
        // A path that cannot exist: the error must be the refusal, not an open failure.
        let err = probe(ProbePermit::Refused, Path::new("/nonexistent/uinput")).unwrap_err();
        assert!(err.to_string().contains(PROBE_PERMIT), "{err}");
    }

    #[test]
    fn granted_probe_reports_a_missing_node() {
        let err = probe(ProbePermit::Granted, Path::new("/nonexistent/uinput")).unwrap_err();
        assert!(err.to_string().contains("/nonexistent/uinput"), "{err}");
    }

    #[test]
    fn identity_is_ours_and_not_palmrests() {
        let s = setup();
        assert_eq!(&s.name[..NAME.len()], NAME.as_bytes());
        assert!(s.name[NAME.len()..].iter().all(|&b| b == 0));
        assert_ne!(s.id.vendor, 0x0001, "palmrest's vendor");
    }

    #[test]
    fn registers_every_key_and_button_a_device_can_pass_through() {
        assert_eq!((*CODES.start(), *CODES.end()), (1, 0x2ff));
        assert!(is_button(0x110) && CODES.contains(&0x110), "BTN_LEFT");
        assert!(CODES.contains(&0x160), "KEY_OK");
        assert!(REL_AXES.contains(&REL_X) && REL_AXES.contains(&REL_HWHEEL_HI_RES));
        assert_eq!(LED_CODES, [0, 1, 2], "num, caps and scroll lock");
    }

    #[test]
    fn recording_emitter_keeps_batches_apart() {
        let sync = InputEvent {
            kind: EV_SYN,
            code: SYN_REPORT,
            value: 0,
        };
        let mut sink = RecordingEmitter::default();
        sink.write(&[sync]).unwrap();
        sink.write(&[sync, sync]).unwrap();
        assert_eq!(
            sink.batches.iter().map(Vec::len).collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(
            decode(sink.batches[0][0]),
            crate::daemon::events::Decoded::Sync
        );
    }
}
