//! `Batch`: `Held` threaded through the transitions of one input, with the events they write.

use super::model::RemapOut;
use crate::event::OutEvent;
use crate::held::Held;

/// The held keys after the transitions so far, and everything those wrote, in order.
pub(super) struct Batch {
    pub(super) held: Held,
    events: Vec<OutEvent>,
}

impl Batch {
    pub(super) fn new(held: Held) -> Self {
        Self {
            held,
            events: Vec::new(),
        }
    }

    /// Apply one `Held` transition and keep what it wrote.
    pub(super) fn then(self, step: impl FnOnce(Held) -> (Held, Vec<OutEvent>)) -> Self {
        let (held, more) = step(self.held);
        let mut events = self.events;
        events.extend(more);
        Self { held, events }
    }

    /// The held keys and the `RemapOut`s: one `Write` ended by `Sync`, or none when nothing was
    /// written.
    pub(super) fn finish(self) -> (Held, Vec<RemapOut>) {
        let mut events = self.events;
        if events.is_empty() {
            return (self.held, Vec::new());
        }
        events.push(OutEvent::Sync);
        (self.held, vec![RemapOut::Write(events)])
    }
}
