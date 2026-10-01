//! The data of the escape-chord detector.

/// Whether a tracked key is down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(super) enum Level {
    #[default]
    Up,
    Down,
}

/// Which of left Shift, right Shift and Esc are down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Escape {
    pub(super) left_shift: Level,
    pub(super) right_shift: Level,
    pub(super) esc: Level,
}

/// What a key event did to the chord.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EscapeChord {
    /// Esc was pressed while both Shift keys were down.
    Fired,
    /// Anything else.
    Quiet,
}
