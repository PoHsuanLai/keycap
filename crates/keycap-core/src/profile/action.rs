//! `RuleAction` and `Hold`: what a matched trigger does.

use super::chord::Stroke;

/// How long a mapped stroke's modifiers stay down after the key is released.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Hold {
    /// The held roles' own outputs are put back on release.
    #[default]
    Restore,
    /// The stroke's modifiers stay down until the last role of the trigger is released.
    WhileHeld,
}

// The names are the ones a rule's `hold` key uses.
names!(Hold {
    Restore => "restore",
    WhileHeld => "while_held",
});

/// What a matched trigger writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleAction {
    /// The stroke, held while the physical key is held (so it repeats).
    Map(Stroke, Hold),
    /// Each stroke tapped in order on press; the release sends nothing.
    Sequence(Vec<Stroke>),
    /// The press and release are consumed and nothing is written.
    Swallow,
}

impl RuleAction {
    /// Tapping `strokes` in order; no strokes at all is `Swallow`, so a `Sequence` is never
    /// empty.
    pub fn sequence(strokes: Vec<Stroke>) -> RuleAction {
        if strokes.is_empty() {
            RuleAction::Swallow
        } else {
            RuleAction::Sequence(strokes)
        }
    }
}
