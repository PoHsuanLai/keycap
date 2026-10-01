//! The state of `Held`: data and accessors only.

use std::collections::BTreeMap;

use crate::event::{DeviceId, Logical, Physical};

/// A physical key on one device: the owner of a record and of a lift.
pub type At = (DeviceId, Physical);

/// The logical keys one physical press holds down, in press order.
///
/// A record may list a key another record also holds (left and right Command both write
/// LEFTCTRL): the key is written once and released when its last holder lets go.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Record {
    /// Keys this press holds, each once, in the order they were pressed.
    pub down: Vec<Logical>,
}

/// Every physical key currently held, with what its press wrote, and the keys lifted off the
/// wire.
///
/// A release writes the inverse of the record and nothing else. A *lift* is a key some record
/// holds that its owner has taken up for a while (Command holds LEFTCTRL, and Command+Left
/// writes Home without Ctrl): the key stays in its records but is not on the wire, and the
/// owner's release puts it back if it is still held. A logical key is down in the written
/// stream exactly while some record holds it and no owner has lifted it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Held {
    pub(super) records: BTreeMap<At, Record>,
    pub(super) lifts: BTreeMap<At, Vec<Logical>>,
}

impl Held {
    /// Nothing held.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the written stream has `key` down: some record holds it and nobody lifted it.
    pub fn is_down(&self, key: Logical) -> bool {
        self.holds(key) && !self.is_lifted(key)
    }

    /// Whether some record holds `key`, down on the wire or lifted off it.
    pub fn holds(&self, key: Logical) -> bool {
        self.records.values().any(|r| r.down.contains(&key))
    }

    /// Whether some owner has lifted `key` off the wire.
    pub fn is_lifted(&self, key: Logical) -> bool {
        self.lifts.values().any(|keys| keys.contains(&key))
    }

    /// Whether the physical key `at` is held.
    pub fn is_held(&self, at: At) -> bool {
        self.records.contains_key(&at)
    }

    /// Every logical key that is down on the wire, once each, in key order.
    pub fn down(&self) -> impl Iterator<Item = Logical> + '_ {
        let mut keys: Vec<Logical> = self
            .records
            .values()
            .flat_map(|r| r.down.iter().copied())
            .filter(|&key| !self.is_lifted(key))
            .collect();
        keys.sort();
        keys.dedup();
        keys.into_iter()
    }

    /// Whether nothing is held and nothing lifted.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty() && self.lifts.is_empty()
    }
}
