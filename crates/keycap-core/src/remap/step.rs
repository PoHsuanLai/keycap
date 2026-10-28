//! `Remapper::step`: one input in, the next state and what to write out.

use super::batch::Batch;
use super::model::{RemapInput, RemapOut, Remapper};
use crate::escape::Escape;
use crate::event::{Edge, OutEvent};
use crate::held::Held;
use crate::profile::Profiles;
use keycap_protocol::Mode;

impl Remapper {
    /// Feed one input. Keys are tracked by the escape chord first; while `Paused` they yield
    /// nothing. Repeats are dropped, since the compositor repeats a held virtual key.
    pub fn step(self, input: RemapInput, profiles: &Profiles) -> (Self, Vec<RemapOut>) {
        match input {
            RemapInput::Key {
                device,
                layout,
                key,
                edge,
                ..
            } => {
                let (escape, fired) = self.escape.step(device, key, edge);
                let this = Remapper { escape, ..self };
                let at = (device, key);
                match (this.status, fired, edge) {
                    (Mode::Paused, _, _) | (_, _, Edge::Repeat) => (this, Vec::new()),
                    (Mode::Running, true, _) => this.fire_escape(),
                    (Mode::Running, false, Edge::Release) => this.settle(|held| held.release(at)),
                    (Mode::Running, false, Edge::Press) => {
                        let profile = profiles.resolve(&this.focus);
                        this.press(profile, layout, at)
                    }
                }
            }
            RemapInput::Focus(name) => (
                Remapper {
                    focus: name,
                    ..self
                },
                Vec::new(),
            ),
            RemapInput::DeviceGone(device) => {
                let escape = self.escape.device_gone(device);
                Remapper { escape, ..self }.settle(|held| held.device_gone(device))
            }
            RemapInput::ReleaseAll => {
                let escape = Escape::new();
                Remapper { escape, ..self }.settle(Held::drain)
            }
            RemapInput::Resume => (
                Remapper {
                    status: Mode::Running,
                    escape: Escape::new(),
                    ..self
                },
                vec![RemapOut::Status(Mode::Running)],
            ),
        }
    }

    /// Run one `Held` transition and turn what it wrote into `RemapOut`s.
    pub(super) fn settle(
        mut self,
        step: impl FnOnce(Held) -> (Held, Vec<OutEvent>),
    ) -> (Self, Vec<RemapOut>) {
        let (held, outs) = Batch::new(std::mem::take(&mut self.held))
            .then(step)
            .finish();
        (self.with_held(held), outs)
    }

    /// Release everything, hand the keyboards back and stop mapping. The Esc press itself is
    /// never written.
    fn fire_escape(self) -> (Self, Vec<RemapOut>) {
        let (this, mut outs) = self.settle(Held::drain);
        outs.extend([RemapOut::ReleaseGrabs, RemapOut::Status(Mode::Paused)]);
        (
            Remapper {
                status: Mode::Paused,
                ..this
            },
            outs,
        )
    }

    /// Replace the held keys, dropping role entries for keys no longer held.
    pub(super) fn with_held(mut self, held: Held) -> Self {
        self.roles.retain(|at, _| held.is_held(*at));
        self.held = held;
        self
    }
}
