//! Lifts: keys a record holds that an owner has taken off the wire for a while.

use super::model::{At, Bare, Held};
use crate::event::{Logical, OutEvent};

impl Held {
    /// `owner` takes `keys` off the wire. Each key some record holds is remembered as lifted by
    /// `owner`, and released if it was down; keys nobody holds are ignored. The records keep the
    /// keys, so the written stream differs from them only while the lift lasts.
    pub fn lift(mut self, owner: At, keys: Vec<Logical>) -> (Self, Vec<OutEvent>) {
        let mut events = Vec::new();
        for key in keys {
            if !self.holds(key) || self.lifted_by(owner, key) {
                continue;
            }
            if self.is_down(key) {
                self.mark_stray(key);
                events.extend(self.write_release(key));
            }
            self.lifts.entry(owner).or_default().push(key);
        }
        (self, events)
    }

    /// End every lift `owner` holds: a key still held by some record and lifted by nobody else
    /// is pressed back, in the order it was lifted.
    pub fn unlift(mut self, owner: At) -> (Self, Vec<OutEvent>) {
        let keys = self.lifts.remove(&owner).unwrap_or_default();
        let mut events = Vec::new();
        for key in keys {
            if self.is_down(key) {
                events.push(self.write_press(key, Bare::Restored));
            }
        }
        (self, events)
    }

    /// Add to `at`'s record the keys of `wants` that are not already down, pressing them. A key
    /// another record keeps down is left to that record, so it comes up when that one is
    /// released and not with `at`. An absent record starts empty.
    pub fn extend(mut self, at: At, wants: Vec<Logical>) -> (Self, Vec<OutEvent>) {
        let mut record = self.records.remove(&at).unwrap_or_default();
        let mut events = Vec::new();
        for key in wants {
            if record.down.contains(&key) || self.is_down(key) {
                continue;
            }
            events.extend(self.raise(key));
            record.down.push(key);
        }
        self.records.insert(at, record);
        (self, events)
    }

    /// Press `key` unless it is down, and end every lift of it: a deliberate press wins over a
    /// lift, so the lifted and the written views cannot disagree.
    pub(super) fn raise(&mut self, key: Logical) -> Option<OutEvent> {
        let event = (!self.is_down(key)).then(|| self.write_press(key, Bare::Pressed));
        self.lifts
            .values_mut()
            .for_each(|keys| keys.retain(|&k| k != key));
        self.lifts.retain(|_, keys| !keys.is_empty());
        event
    }

    fn lifted_by(&self, owner: At, key: Logical) -> bool {
        self.lifts
            .get(&owner)
            .is_some_and(|keys| keys.contains(&key))
    }
}
