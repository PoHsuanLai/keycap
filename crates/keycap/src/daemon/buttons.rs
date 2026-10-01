//! The mouse buttons a combo device holds down through the virtual keyboard. Pointer events
//! pass through unmapped, so the remapper's `Held` never sees them; without this record an
//! unplug or the escape chord would leave the button down on the wire. Pure.

use super::events::{EV_KEY, InputEvent, is_button};
use crate::DeviceId;
use std::collections::{BTreeMap, BTreeSet};

/// The `BTN_*` codes down on the wire, by the device that pressed them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Buttons {
    down: BTreeMap<DeviceId, BTreeSet<u16>>,
}

impl Buttons {
    /// Records the button transitions among `events`, which `device` just had written for it.
    pub(crate) fn observe(mut self, device: DeviceId, events: &[InputEvent]) -> Self {
        for event in events
            .iter()
            .filter(|e| e.kind == EV_KEY && is_button(e.code))
        {
            let held = self.down.entry(device).or_default();
            match event.value {
                0 => {
                    held.remove(&event.code);
                }
                1 => {
                    held.insert(event.code);
                }
                _ => {}
            }
        }
        self.down.retain(|_, held| !held.is_empty());
        self
    }

    /// `device` is gone: the releases of its buttons, ending in a `SYN_REPORT`, minus any
    /// button another device still holds. Empty when it held nothing.
    pub(crate) fn release_device(mut self, device: DeviceId) -> (Self, Vec<InputEvent>) {
        let Some(mine) = self.down.remove(&device) else {
            return (self, Vec::new());
        };
        let others: BTreeSet<u16> = self.down.values().flatten().copied().collect();
        let freed = mine.difference(&others).copied().collect();
        (self, release_frame(freed))
    }

    /// Every device's buttons released at once, as one frame.
    pub(crate) fn release_all(self) -> (Self, Vec<InputEvent>) {
        let all = self.down.into_values().flatten().collect();
        (Self::default(), release_frame(all))
    }

    /// Nothing is down: the virtual device was recreated and starts clean.
    pub(crate) fn forget(self) -> Self {
        Self::default()
    }
}

/// The release of each code, in order, closed by a `SYN_REPORT`; empty for no codes.
fn release_frame(codes: BTreeSet<u16>) -> Vec<InputEvent> {
    if codes.is_empty() {
        return Vec::new();
    }
    codes
        .into_iter()
        .map(|code| InputEvent {
            kind: EV_KEY,
            code,
            value: 0,
        })
        .chain([InputEvent::SYNC])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::events::{EV_REL, REL_X};

    const A: DeviceId = DeviceId(0);
    const B: DeviceId = DeviceId(1);
    const LEFT: u16 = 0x110;
    const RIGHT: u16 = 0x111;

    const fn ev(kind: u16, code: u16, value: i32) -> InputEvent {
        InputEvent { kind, code, value }
    }

    const fn btn(code: u16, value: i32) -> InputEvent {
        ev(EV_KEY, code, value)
    }

    fn releases(codes: &[u16]) -> Vec<InputEvent> {
        codes
            .iter()
            .map(|&c| btn(c, 0))
            .chain([InputEvent::SYNC])
            .collect()
    }

    #[test]
    fn a_released_button_is_not_released_again_and_a_held_one_is() {
        let table: &[(&str, &[InputEvent], Vec<InputEvent>)] = &[
            ("nothing seen", &[], vec![]),
            ("press then release", &[btn(LEFT, 1), btn(LEFT, 0)], vec![]),
            ("press only", &[btn(LEFT, 1)], releases(&[LEFT])),
            (
                "two held, in code order",
                &[btn(RIGHT, 1), btn(LEFT, 1)],
                releases(&[LEFT, RIGHT]),
            ),
            (
                "one of two released",
                &[btn(LEFT, 1), btn(RIGHT, 1), btn(LEFT, 0)],
                releases(&[RIGHT]),
            ),
            (
                "a repeat changes nothing",
                &[btn(LEFT, 1), btn(LEFT, 2)],
                releases(&[LEFT]),
            ),
            (
                "motion and keys are not buttons",
                &[ev(EV_REL, REL_X, 4), btn(30, 1)],
                vec![],
            ),
        ];
        for (name, events, want) in table {
            let (after, got) = Buttons::default().observe(A, events).release_device(A);
            assert_eq!(&got, want, "{name}");
            assert_eq!(after, Buttons::default(), "{name}: nothing left");
        }
    }

    #[test]
    fn a_device_releases_only_its_own_buttons() {
        let held = Buttons::default()
            .observe(A, &[btn(LEFT, 1)])
            .observe(B, &[btn(RIGHT, 1)]);
        let (after, got) = held.clone().release_device(A);
        assert_eq!(got, releases(&[LEFT]));
        let (_, rest) = after.release_all();
        assert_eq!(rest, releases(&[RIGHT]), "B's button is still tracked");
    }

    #[test]
    fn a_button_another_device_still_holds_stays_down() {
        let held = Buttons::default()
            .observe(A, &[btn(LEFT, 1)])
            .observe(B, &[btn(LEFT, 1)]);
        let (_, got) = held.release_device(A);
        assert_eq!(got, [], "B still holds BTN_LEFT");
    }

    #[test]
    fn release_all_is_one_frame_for_every_device_and_leaves_nothing() {
        let held = Buttons::default()
            .observe(A, &[btn(LEFT, 1)])
            .observe(B, &[btn(RIGHT, 1)]);
        let (after, got) = held.release_all();
        assert_eq!(got, releases(&[LEFT, RIGHT]));
        assert_eq!(after.release_all().1, [], "a second release writes nothing");
    }

    #[test]
    fn forgetting_writes_nothing_and_clears_everything() {
        let held = Buttons::default().observe(A, &[btn(LEFT, 1)]).forget();
        assert_eq!(held, Buttons::default());
    }

    #[test]
    fn releasing_a_device_that_held_nothing_writes_nothing() {
        let (after, got) = Buttons::default().release_device(B);
        assert_eq!((after, got), (Buttons::default(), vec![]));
    }
}
