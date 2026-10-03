//! The transitions of `Held`. No output here carries `SYN`: the remapper appends `Sync`.

use std::collections::BTreeMap;

use super::model::{At, Held, Record};
use crate::event::{DeviceId, Edge, Logical, OutEvent};

impl Held {
    /// Record that `at` was pressed and wants `wants` down; returns the events to write.
    ///
    /// Only keys not down on the wire are written, so a key shared by two holders is pressed
    /// once, and pressing a key an owner had lifted brings it back and ends that lift. Duplicates
    /// in `wants` collapse. If `at` is already held (a release was missed) the old record is
    /// released first, and its events come before the new press. The lifts `at` owns stay.
    pub fn press(self, at: At, wants: Vec<Logical>) -> (Self, Vec<OutEvent>) {
        let (mut held, mut events) = self.release_record(at);
        let mut down = Vec::with_capacity(wants.len());
        for key in wants {
            if down.contains(&key) {
                continue;
            }
            events.extend(held.raise(key));
            down.push(key);
        }
        held.records.insert(at, Record { down });
        (held, events)
    }

    /// Forget `at`'s record and release its keys in reverse press order, except keys another
    /// record still holds or that are lifted off the wire. Lifts `at` owns stay.
    pub fn release_record(mut self, at: At) -> (Self, Vec<OutEvent>) {
        let Some(record) = self.records.remove(&at) else {
            return (self, Vec::new());
        };
        let mut events = Vec::new();
        for &key in record.down.iter().rev() {
            if !self.holds(key) && !self.is_lifted(key) {
                events.extend(self.write_release(key));
            }
        }
        self.lifts = prune(self.lifts, &self.records);
        (self, events)
    }

    /// A physical release: drop `at`'s record, then end the lifts `at` owns, pressing back
    /// every key that is still held and no other owner keeps lifted.
    pub fn release(self, at: At) -> (Self, Vec<OutEvent>) {
        let (held, mut events) = self.release_record(at);
        let (held, more) = held.unlift(at);
        events.extend(more);
        (held, events)
    }

    /// Release every record of `device`, in key order, then end the lifts its keys owned. A key
    /// the device's records held is not pressed back; one another device's record holds is.
    pub fn device_gone(self, device: DeviceId) -> (Self, Vec<OutEvent>) {
        let ats: Vec<At> = self.owners().filter(|(d, _)| *d == device).collect();
        let (held, mut events) = ats
            .iter()
            .fold((self, Vec::new()), |(held, mut events), &at| {
                let (held, more) = held.release_record(at);
                events.extend(more);
                (held, events)
            });
        let (held, more) = ats
            .into_iter()
            .fold((held, Vec::new()), |(held, mut events), at| {
                let (held, more) = held.unlift(at);
                events.extend(more);
                (held, events)
            });
        events.extend(more);
        (held, events)
    }

    /// Release everything held; the returned `Held` is empty. Lifted keys are already up and
    /// nothing is pressed back on the way out.
    pub fn drain(self) -> (Self, Vec<OutEvent>) {
        let ats: Vec<At> = self.records.keys().copied().collect();
        let (mut held, events) =
            ats.into_iter()
                .fold((self, Vec::new()), |(held, mut events), at| {
                    let (held, more) = held.release_record(at);
                    events.extend(more);
                    (held, events)
                });
        held.lifts.clear();
        held.bare.clear();
        (held, events)
    }

    /// Every physical key that owns a record or a lift, in key order.
    fn owners(&self) -> impl Iterator<Item = At> + '_ {
        let mut ats: Vec<At> = self
            .records
            .keys()
            .chain(self.lifts.keys())
            .copied()
            .collect();
        ats.sort();
        ats.dedup();
        ats.into_iter()
    }
}

/// Lifts without the keys no record holds, and without owners left with nothing.
fn prune(
    lifts: BTreeMap<At, Vec<Logical>>,
    records: &BTreeMap<At, Record>,
) -> BTreeMap<At, Vec<Logical>> {
    lifts
        .into_iter()
        .map(|(owner, keys)| {
            let kept: Vec<Logical> = keys
                .into_iter()
                .filter(|key| records.values().any(|r| r.down.contains(key)))
                .collect();
            (owner, kept)
        })
        .filter(|(_, keys)| !keys.is_empty())
        .collect()
}

pub(super) fn key_event(key: Logical, edge: Edge) -> OutEvent {
    OutEvent::Key { key, edge }
}
