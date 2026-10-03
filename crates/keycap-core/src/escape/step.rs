//! The chord's transitions.

use super::model::{Escape, EscapeChord};
use crate::event::{DeviceId, Edge, Physical};
use crate::key::Key;

impl Escape {
    /// Nothing held.
    pub fn new() -> Self {
        Self::default()
    }

    /// Track `key`'s `edge` on `device`. The chord fires only when Esc is the last of the three
    /// pressed, with each Shift down on some keyboard; repeats change nothing.
    pub fn step(mut self, device: DeviceId, key: Physical, edge: Edge) -> (Self, EscapeChord) {
        if !matches!(key.0, Key::LEFTSHIFT | Key::RIGHTSHIFT | Key::ESC) {
            return (self, EscapeChord::Quiet);
        }
        match edge {
            Edge::Press => self.down.insert((device, key)),
            Edge::Release => self.down.remove(&(device, key)),
            Edge::Repeat => return (self, EscapeChord::Quiet),
        };
        let fired = edge == Edge::Press
            && key.0 == Key::ESC
            && self.is_down(Key::LEFTSHIFT)
            && self.is_down(Key::RIGHTSHIFT);
        let chord = if fired {
            EscapeChord::Fired
        } else {
            EscapeChord::Quiet
        };
        (self, chord)
    }

    /// Forget every key `device` had down.
    pub fn device_gone(mut self, device: DeviceId) -> Self {
        self.down.retain(|(d, _)| *d != device);
        self
    }

    /// Whether `key` is down on some keyboard.
    fn is_down(&self, key: Key) -> bool {
        self.down.iter().any(|(_, physical)| physical.0 == key)
    }
}
