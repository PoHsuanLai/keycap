//! The transitions: each takes the state and what happened, and returns the next state and the
//! effects wanted.

use super::model::{DeviceEffect, Devices, GrabOutcome, Retry, Standing, Tracked};
use crate::daemon::discover::Identity;
use crate::{Backoff, Decision, DeviceId, GrabGate, KeysDown, Layout, Mono};
use keycap_protocol::Grab;
use std::path::Path;
use std::time::Duration;

/// How often a keyboard with a key down is looked at again.
pub(crate) const GATE_POLL: Duration = Duration::from_millis(50);

type Step = (Devices, Vec<DeviceEffect>);

impl Tracked {
    /// Whether the gate may be checked now.
    fn due(&self, now: Mono) -> bool {
        match self.retry {
            Retry::Ready(_) => true,
            Retry::Waiting { until, .. } => until <= now,
        }
    }

    /// Back to waiting at the gate, with the grab shown as `grab`.
    fn at_gate(self, grab: Grab) -> Self {
        Self {
            gate: GrabGate::new(),
            grab,
            ..self
        }
    }
}

impl Devices {
    /// No keyboards, grabbing wanted.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// A keyboard node was seen. One already known by path is a no-op, so a repeated
    /// notification costs nothing.
    pub(crate) fn appeared(mut self, identity: Identity) -> Step {
        if self.id_of(&identity.path).is_some() {
            return (self, Vec::new());
        }
        let id = DeviceId(self.next);
        let effect = DeviceEffect::Open {
            id,
            path: identity.path.clone(),
        };
        let tracked = Tracked {
            layout: Layout::of_vendor(identity.vendor),
            identity,
            gate: GrabGate::new(),
            grab: Grab::WaitingForRelease,
            retry: Retry::Ready(Backoff::DEVICE),
        };
        self.tracked.insert(id, tracked);
        self.next += 1;
        (self, vec![effect])
    }

    /// A keyboard is gone, or could not be opened: forget it.
    pub(crate) fn gone(mut self, id: DeviceId) -> Step {
        let effects = self
            .tracked
            .remove(&id)
            .map(|_| DeviceEffect::Close(id))
            .into_iter()
            .collect();
        (self, effects)
    }

    /// The keys down on `id`, read now. A keyboard that is waiting at the gate and due is
    /// grabbed when none is down.
    pub(crate) fn checked(mut self, id: DeviceId, keys: KeysDown, now: Mono) -> Step {
        let wanted = self.standing == Standing::Grabbing;
        let Some(tracked) = self.tracked.remove(&id) else {
            return (self, Vec::new());
        };
        let (tracked, effects) = match (wanted && tracked.due(now), tracked.gate.step(keys)) {
            (true, (gate, Decision::Grab)) => {
                (Tracked { gate, ..tracked }, vec![DeviceEffect::Grab(id)])
            }
            (true, (_, Decision::Wait)) => {
                let grab = match tracked.grab {
                    Grab::Conflict => Grab::Conflict,
                    _ => Grab::WaitingForRelease,
                };
                (Tracked { grab, ..tracked }, Vec::new())
            }
            _ => (tracked, Vec::new()),
        };
        self.tracked.insert(id, tracked);
        (self, effects)
    }

    /// How the grab asked for by `Grab(id)` ended. A refusal goes back to the gate and waits
    /// out the next backoff delay (1 s doubling to 30 s), so a held device is never hammered.
    pub(crate) fn grab_result(mut self, id: DeviceId, outcome: GrabOutcome, now: Mono) -> Step {
        let Some(tracked) = self.tracked.remove(&id) else {
            return (self, Vec::new());
        };
        let tracked = match outcome {
            GrabOutcome::Granted => Tracked {
                grab: Grab::Active,
                retry: Retry::Ready(tracked.retry.backoff().reset()),
                ..tracked
            },
            GrabOutcome::Busy => refused(tracked, Grab::Conflict, now),
            GrabOutcome::Failed => refused(tracked, Grab::WaitingForRelease, now),
        };
        self.tracked.insert(id, tracked);
        (self, Vec::new())
    }

    /// The escape chord: every grab is given back and none is taken until `resumed`.
    pub(crate) fn released(self) -> Step {
        let effects = self
            .tracked
            .iter()
            .filter(|(_, t)| t.gate == GrabGate::Grabbed)
            .map(|(&id, _)| DeviceEffect::Ungrab(id))
            .collect();
        let tracked = self
            .tracked
            .into_iter()
            .map(|(id, t)| {
                let retry = Retry::Ready(t.retry.backoff().reset());
                (
                    id,
                    Tracked {
                        retry,
                        ..t.at_gate(Grab::WaitingForRelease)
                    },
                )
            })
            .collect();
        (
            Devices {
                tracked,
                standing: Standing::Released,
                ..self
            },
            effects,
        )
    }

    /// Grabbing is wanted again; each keyboard goes through its gate at the next look.
    pub(crate) fn resumed(self) -> Step {
        (
            Devices {
                standing: Standing::Grabbing,
                ..self
            },
            Vec::new(),
        )
    }

    /// The keyboard at `path`, if known.
    pub(crate) fn id_of(&self, path: &Path) -> Option<DeviceId> {
        self.tracked
            .iter()
            .find(|(_, t)| t.identity.path == path)
            .map(|(&id, _)| id)
    }

    /// The layout of `id` while it is grabbed and being remapped; events of any other
    /// keyboard are not ours to map.
    pub(crate) fn active(&self, id: DeviceId) -> Option<Layout> {
        self.tracked
            .get(&id)
            .filter(|t| t.grab == Grab::Active)
            .map(|t| t.layout)
    }

    /// The keyboards whose gate should be looked at now.
    pub(crate) fn pending(&self, now: Mono) -> Vec<DeviceId> {
        match self.standing {
            Standing::Released => Vec::new(),
            Standing::Grabbing => self
                .tracked
                .iter()
                .filter(|(_, t)| t.gate == GrabGate::Pending && t.due(now))
                .map(|(&id, _)| id)
                .collect(),
        }
    }

    /// How long until something here needs a look: soon while a gate is open, else when the
    /// earliest refused grab may be retried.
    pub(crate) fn next_wake(&self, now: Mono) -> Option<Duration> {
        match self.standing {
            Standing::Released => None,
            Standing::Grabbing => self
                .tracked
                .values()
                .filter(|t| t.gate == GrabGate::Pending)
                .map(|t| match t.retry {
                    Retry::Waiting { until, .. } if until > now => until.since(now),
                    _ => GATE_POLL,
                })
                .min(),
        }
    }
}

/// `tracked` after a refused grab: at the gate, showing `grab`, not due before the next delay.
fn refused(tracked: Tracked, grab: Grab, now: Mono) -> Tracked {
    let (delay, backoff) = tracked.retry.backoff().step();
    Tracked {
        retry: Retry::Waiting {
            until: now.after(delay),
            backoff,
        },
        ..tracked.at_gate(grab)
    }
}
