//! The "keycap pointer" twins of the grabbed combo devices: which exist, what each holds down,
//! and the pure rule that says when one is made or let go.

use crate::DeviceId;
use crate::daemon::buttons::Buttons;
use crate::daemon::events::InputEvent;
use crate::daemon::uinput::PointerSink;
use std::collections::{BTreeMap, BTreeSet};

/// Which devices were just grabbed and which just let go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GrabChange {
    pub taken: Vec<DeviceId>,
    pub given: Vec<DeviceId>,
}

/// What changed between the devices grabbed `before` and the ones grabbed `now`, in id order.
pub(super) fn grab_change(before: &BTreeSet<DeviceId>, now: &BTreeSet<DeviceId>) -> GrabChange {
    GrabChange {
        taken: now.difference(before).copied().collect(),
        given: before.difference(now).copied().collect(),
    }
}

/// One twin and the buttons held down on it.
#[derive(Debug)]
struct Twin<T> {
    sink: T,
    buttons: Buttons,
}

/// What passing a device's pointer events on came to.
#[derive(Debug)]
pub(super) enum Passed {
    /// The device has no twin: the keyboard carries its pointer.
    NoTwin,
    Written,
    /// The twin refused the write and has been dropped; the keyboard carries the pointer
    /// from now on.
    Lost(String),
}

/// The twins, by the device whose pointer each carries.
#[derive(Debug)]
pub(super) struct Twins<T>(BTreeMap<DeviceId, Twin<T>>);

impl<T> Default for Twins<T> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl<T: PointerSink> Twins<T> {
    /// `sink` now carries the pointer of `device`.
    pub(super) fn insert(&mut self, device: DeviceId, sink: T) {
        self.0.insert(
            device,
            Twin {
                sink,
                buttons: Buttons::default(),
            },
        );
    }

    /// Writes the pointer `events` of `device` to its twin, if it has one.
    pub(super) fn pass(&mut self, device: DeviceId, events: &[InputEvent]) -> Passed {
        let Some(twin) = self.0.get_mut(&device) else {
            return Passed::NoTwin;
        };
        twin.buttons = std::mem::take(&mut twin.buttons).observe(device, events);
        match twin.sink.write(events) {
            Ok(()) => Passed::Written,
            Err(e) => {
                self.0.remove(&device);
                Passed::Lost(e.to_string())
            }
        }
    }

    /// Releases what `device`'s twin holds down, if it has one. A twin that cannot be written
    /// is dropped, which ends everything it held.
    pub(super) fn release(&mut self, device: DeviceId) -> Result<(), String> {
        let Some(twin) = self.0.get_mut(&device) else {
            return Ok(());
        };
        let (buttons, events) = std::mem::take(&mut twin.buttons).release_device(device);
        twin.buttons = buttons;
        twin.sink.write(&events).map_err(|e| {
            self.0.remove(&device);
            e.to_string()
        })
    }

    /// Releases what every twin holds down; the devices whose twin could not be written.
    pub(super) fn release_all(&mut self) -> Vec<(DeviceId, String)> {
        let devices: Vec<DeviceId> = self.0.keys().copied().collect();
        devices
            .into_iter()
            .filter_map(|device| self.release(device).err().map(|e| (device, e)))
            .collect()
    }

    /// The device was let go: its twin releases what it held and is destroyed.
    pub(super) fn retire(&mut self, device: DeviceId) -> Result<(), String> {
        let released = self.release(device);
        self.0.remove(&device);
        released
    }

    /// The devices that have a twin.
    #[cfg(test)]
    pub(super) fn devices(&self) -> Vec<DeviceId> {
        self.0.keys().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::events::{EV_ABS, EV_KEY};
    use crate::daemon::pointer::{Masks, PointerCaps};
    use crate::daemon::uinput::{Emitter, RecordingEmitter, TwinLife};

    const A: DeviceId = DeviceId(0);
    const B: DeviceId = DeviceId(1);
    const LEFT: u16 = 0x110;

    const fn ev(kind: u16, code: u16, value: i32) -> InputEvent {
        InputEvent { kind, code, value }
    }

    fn set(ids: &[DeviceId]) -> BTreeSet<DeviceId> {
        ids.iter().copied().collect()
    }

    fn caps() -> PointerCaps {
        PointerCaps::new("pad", 1, 2, &Masks::default(), vec![])
    }

    #[test]
    fn grab_change_table() {
        // (name, grabbed before, grabbed now, taken, given)
        type Case = (
            &'static str,
            &'static [DeviceId],
            &'static [DeviceId],
            &'static [DeviceId],
            &'static [DeviceId],
        );
        const CASES: &[Case] = &[
            ("nothing before or after", &[], &[], &[], &[]),
            ("a device is grabbed", &[], &[A], &[A], &[]),
            ("a device is let go", &[A], &[], &[], &[A]),
            ("no change", &[A, B], &[A, B], &[], &[]),
            ("one in, one out", &[A], &[B], &[B], &[A]),
            ("a pause lets everything go", &[A, B], &[], &[], &[A, B]),
            ("a resume grabs everything", &[], &[A, B], &[A, B], &[]),
        ];
        for &(name, before, now, taken, given) in CASES {
            let got = grab_change(&set(before), &set(now));
            assert_eq!(got.taken, taken, "{name}: taken");
            assert_eq!(got.given, given, "{name}: given");
        }
    }

    #[test]
    fn pointer_events_go_to_the_twin_and_a_device_without_one_is_left_to_the_keyboard() {
        let mut emitter = RecordingEmitter::default();
        let mut twins = Twins::default();
        twins.insert(A, emitter.twin(&caps()).unwrap());
        let frame = [ev(EV_ABS, 0, 5), InputEvent::SYNC];
        assert!(matches!(twins.pass(A, &frame), Passed::Written));
        assert!(matches!(twins.pass(B, &frame), Passed::NoTwin));
        assert_eq!(emitter.twins[0].borrow().batches, [frame.to_vec()]);
    }

    #[test]
    fn a_held_button_is_released_on_the_twin_when_the_device_is_let_go() {
        let mut emitter = RecordingEmitter::default();
        let mut twins = Twins::default();
        twins.insert(A, emitter.twin(&caps()).unwrap());
        twins.pass(A, &[ev(EV_KEY, LEFT, 1), InputEvent::SYNC]);
        assert_eq!(twins.retire(A), Ok(()));
        let log = emitter.twins[0].borrow();
        assert_eq!(
            log.batches.last().unwrap(),
            &[ev(EV_KEY, LEFT, 0), InputEvent::SYNC],
            "the release is the last thing the twin is told"
        );
        assert_eq!(log.life, TwinLife::Destroyed);
        assert_eq!(twins.devices(), []);
    }

    #[test]
    fn a_released_button_is_not_released_again() {
        let mut emitter = RecordingEmitter::default();
        let mut twins = Twins::default();
        twins.insert(A, emitter.twin(&caps()).unwrap());
        twins.pass(A, &[ev(EV_KEY, LEFT, 1), InputEvent::SYNC]);
        twins.pass(A, &[ev(EV_KEY, LEFT, 0), InputEvent::SYNC]);
        let before = emitter.twins[0].borrow().batches.len();
        assert_eq!(twins.retire(A), Ok(()));
        assert_eq!(emitter.twins[0].borrow().batches.len(), before);
    }

    #[test]
    fn release_all_frees_every_twin_and_keeps_them_alive() {
        let mut emitter = RecordingEmitter::default();
        let mut twins = Twins::default();
        twins.insert(A, emitter.twin(&caps()).unwrap());
        twins.insert(B, emitter.twin(&caps()).unwrap());
        twins.pass(A, &[ev(EV_KEY, LEFT, 1), InputEvent::SYNC]);
        twins.pass(B, &[ev(EV_KEY, LEFT, 1), InputEvent::SYNC]);
        assert_eq!(twins.release_all(), []);
        for log in &emitter.twins {
            let log = log.borrow();
            assert_eq!(
                log.batches.last().unwrap(),
                &[ev(EV_KEY, LEFT, 0), InputEvent::SYNC]
            );
            assert_eq!(log.life, TwinLife::Alive);
        }
    }

    #[test]
    fn a_twin_that_cannot_be_written_is_dropped() {
        let mut emitter = RecordingEmitter::default();
        let mut twins = Twins::default();
        twins.insert(A, emitter.twin(&caps()).unwrap());
        emitter.twins[0].borrow_mut().failing = 1;
        let lost = twins.pass(A, &[ev(EV_KEY, LEFT, 1), InputEvent::SYNC]);
        assert!(matches!(lost, Passed::Lost(_)), "{lost:?}");
        assert_eq!(emitter.twins[0].borrow().life, TwinLife::Destroyed);
        assert!(matches!(twins.pass(A, &[InputEvent::SYNC]), Passed::NoTwin));
    }
}
