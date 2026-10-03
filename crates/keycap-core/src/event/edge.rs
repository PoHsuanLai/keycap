//! `Edge`: a key's transition.

/// What a key did: went down, came up, or auto-repeated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Edge {
    Press,
    Release,
    Repeat,
}

impl Edge {
    /// The edge for an evdev key event value (0 release, 1 press, 2 repeat); other values are
    /// not key transitions.
    pub const fn from_value(value: i32) -> Option<Edge> {
        match value {
            0 => Some(Edge::Release),
            1 => Some(Edge::Press),
            2 => Some(Edge::Repeat),
            _ => None,
        }
    }

    /// The evdev value this edge is written as.
    pub const fn value(self) -> i32 {
        match self {
            Edge::Release => 0,
            Edge::Press => 1,
            Edge::Repeat => 2,
        }
    }
}
