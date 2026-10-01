//! The state of `Held`: data and accessors only.

use std::collections::BTreeMap;

use crate::event::{DeviceId, Logical, Physical};

/// The logical keys one physical press holds down, in press order.
///
/// A record may list a key another record also holds (left and right Command both write
/// LEFTCTRL): the key is written once and released when its last holder lets go.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Record {
    /// Keys this press holds, each once, in the order they were pressed.
    pub down: Vec<Logical>,
}

/// Every physical key currently held, with what its press wrote.
///
/// A release writes the inverse of the record and nothing else, and a logical key is down in the
/// written stream exactly while some record holds it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Held {
    pub(super) records: BTreeMap<(DeviceId, Physical), Record>,
}

impl Held {
    /// Nothing held.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether some record holds `key`, that is, whether the written stream has it down.
    pub fn is_down(&self, key: Logical) -> bool {
        self.records.values().any(|r| r.down.contains(&key))
    }

    /// Whether the physical key `at` is held.
    pub fn is_held(&self, at: (DeviceId, Physical)) -> bool {
        self.records.contains_key(&at)
    }

    /// Every logical key that is down, once each, in key order.
    pub fn down(&self) -> impl Iterator<Item = Logical> + '_ {
        let mut keys: Vec<Logical> = self
            .records
            .values()
            .flat_map(|r| r.down.iter().copied())
            .collect();
        keys.sort();
        keys.dedup();
        keys.into_iter()
    }

    /// Whether nothing is held.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}
