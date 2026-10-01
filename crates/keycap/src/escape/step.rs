//! The chord's transition.

use super::model::{Escape, EscapeChord, Level};
use crate::event::{Edge, Physical};
use crate::key::Key;

impl Escape {
    /// Nothing held.
    pub fn new() -> Self {
        Self::default()
    }

    /// Track `key`'s `edge`. The chord fires only when Esc is the last of the three pressed;
    /// repeats change nothing.
    pub fn step(mut self, key: Physical, edge: Edge) -> (Self, EscapeChord) {
        let level = match edge {
            Edge::Press => Level::Down,
            Edge::Release => Level::Up,
            Edge::Repeat => return (self, EscapeChord::Quiet),
        };
        let slot = match key.0 {
            Key::LEFTSHIFT => &mut self.left_shift,
            Key::RIGHTSHIFT => &mut self.right_shift,
            Key::ESC => &mut self.esc,
            _ => return (self, EscapeChord::Quiet),
        };
        *slot = level;
        let shifts = (self.left_shift, self.right_shift) == (Level::Down, Level::Down);
        let fired = shifts && key.0 == Key::ESC && level == Level::Down;
        (
            self,
            if fired {
                EscapeChord::Fired
            } else {
                EscapeChord::Quiet
            },
        )
    }
}
