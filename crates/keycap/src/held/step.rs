//! The transitions of `Held`. No output here carries `SYN`: the remapper appends `Sync`.

use super::model::{Held, Record};
use crate::event::{DeviceId, Edge, Logical, OutEvent, Physical};

type At = (DeviceId, Physical);

impl Held {
    /// Record that `at` was pressed and wants `wants` down; returns the events to write.
    ///
    /// Only keys no record already holds are written, so a key shared by two holders is pressed
    /// once. Duplicates in `wants` collapse. If `at` is already held (a release was missed) the
    /// old record is released first, and its events come before the new press.
    pub fn press(self, at: At, wants: Vec<Logical>) -> (Self, Vec<OutEvent>) {
        let (mut held, mut events) = self.release(at);
        let mut down = Vec::with_capacity(wants.len());
        for key in wants {
            if down.contains(&key) {
                continue;
            }
            if !held.is_down(key) {
                events.push(key_event(key, Edge::Press));
            }
            down.push(key);
        }
        held.records.insert(at, Record { down });
        (held, events)
    }

    /// Forget `at` and release its keys in reverse press order, except keys another record still
    /// holds. A key that is not held yields no events.
    pub fn release(mut self, at: At) -> (Self, Vec<OutEvent>) {
        let Some(record) = self.records.remove(&at) else {
            return (self, Vec::new());
        };
        let events = record
            .down
            .into_iter()
            .rev()
            .filter(|&key| !self.is_down(key))
            .map(|key| key_event(key, Edge::Release))
            .collect();
        (self, events)
    }

    /// Release every record of `device`, in key order.
    pub fn device_gone(self, device: DeviceId) -> (Self, Vec<OutEvent>) {
        let ats: Vec<At> = self
            .records
            .keys()
            .copied()
            .filter(|(d, _)| *d == device)
            .collect();
        release_all(self, ats)
    }

    /// Release everything held; the returned `Held` is empty.
    pub fn drain(self) -> (Self, Vec<OutEvent>) {
        let ats: Vec<At> = self.records.keys().copied().collect();
        release_all(self, ats)
    }
}

fn release_all(held: Held, ats: Vec<At>) -> (Held, Vec<OutEvent>) {
    ats.into_iter()
        .fold((held, Vec::new()), |(held, mut events), at| {
            let (held, more) = held.release(at);
            events.extend(more);
            (held, events)
        })
}

fn key_event(key: Logical, edge: Edge) -> OutEvent {
    OutEvent::Key { key, edge }
}
