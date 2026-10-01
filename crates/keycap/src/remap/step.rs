//! `Remapper::step`: one input in, the next state and what to write out.

use super::batch::Batch;
use super::model::{CoreStatus, Input, Out, Remapper};
use crate::escape::EscapeChord;
use crate::event::{Edge, OutEvent};
use crate::held::Held;
use crate::profile::Profiles;

impl Remapper {
    /// Feed one input. Keys are tracked by the escape chord first; while `Paused` they yield
    /// nothing. Repeats are dropped, since the compositor repeats a held virtual key.
    pub fn step(self, input: Input, profiles: &Profiles) -> (Self, Vec<Out>) {
        match input {
            Input::Key {
                device,
                layout,
                key,
                edge,
                ..
            } => {
                let (escape, chord) = self.escape.step(key, edge);
                let this = Remapper { escape, ..self };
                let at = (device, key);
                match (this.status, chord, edge) {
                    (CoreStatus::Paused, _, _) | (_, _, Edge::Repeat) => (this, Vec::new()),
                    (CoreStatus::Running, EscapeChord::Fired, _) => this.fire_escape(),
                    (CoreStatus::Running, EscapeChord::Quiet, Edge::Release) => {
                        this.settle(|held| held.release(at))
                    }
                    (CoreStatus::Running, EscapeChord::Quiet, Edge::Press) => {
                        let profile = profiles.resolve(&this.focus);
                        this.press(profile, layout, at)
                    }
                }
            }
            Input::Focus(name) => (
                Remapper {
                    focus: name,
                    ..self
                },
                Vec::new(),
            ),
            Input::DeviceGone(device) => self.settle(|held| held.device_gone(device)),
            Input::ReleaseAll => self.settle(Held::drain),
            Input::Resume => (
                Remapper {
                    status: CoreStatus::Running,
                    ..self
                },
                vec![Out::Status(CoreStatus::Running)],
            ),
        }
    }

    /// Run one `Held` transition and turn what it wrote into `Out`s.
    pub(super) fn settle(
        mut self,
        step: impl FnOnce(Held) -> (Held, Vec<OutEvent>),
    ) -> (Self, Vec<Out>) {
        let (held, outs) = Batch::new(std::mem::take(&mut self.held))
            .then(step)
            .finish();
        (self.with_held(held), outs)
    }

    /// Release everything, hand the keyboards back and stop mapping. The Esc press itself is
    /// never written.
    fn fire_escape(self) -> (Self, Vec<Out>) {
        let (this, mut outs) = self.settle(Held::drain);
        outs.extend([Out::ReleaseGrabs, Out::Status(CoreStatus::Paused)]);
        (
            Remapper {
                status: CoreStatus::Paused,
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
