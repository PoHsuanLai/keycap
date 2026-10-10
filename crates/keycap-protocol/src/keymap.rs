//! The words the keymap messages are made of: the person's overrides, what was wrong with them,
//! and the actions an app registers.
//!
//! Plain strings on purpose: the overrides are chordkit's line format and a chord is chordkit's
//! text, converted by the client that owns chordkit. This crate stays light.

use serde::{Deserialize, Serialize};

/// One thing wrong with the keymap file, in plain words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeymapProblem {
    /// The line it concerns, counted from 1; 0 when it concerns the whole file.
    pub line: u32,
    pub what: String,
}

/// The person's keymap as keycap serves it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct KeymapState {
    /// The overrides in chordkit's line format (`copy = Primary+C`), only lines that read.
    /// Empty when the person has changed nothing.
    pub overrides: String,
    /// What was skipped or refused, so a settings screen can say so.
    pub problems: Vec<KeymapProblem>,
}

/// One action an app handles, as chordkit writes it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RegisteredAction {
    /// `app.action`, such as `mail.compose`.
    pub id: String,
    /// The portable default chord, such as `Primary+Shift+N`.
    pub chord: String,
}

/// An app's actions, told to keycap so Settings can list them. Informational: each app resolves
/// its own keys in-process; this changes no binding.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Registration {
    /// The app's name, the first part of its action ids.
    pub app: String,
    pub actions: Vec<RegisteredAction>,
}
