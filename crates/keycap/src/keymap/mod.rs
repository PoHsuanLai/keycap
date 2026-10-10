//! The person's keymap file: `keymap` in the config directory, in chordkit's overrides line
//! format. This module reads it, judges it against the last good copy and watches it; serving
//! it is the socket thread's. The nouns (actions, chords, overrides) are chordkit's.

mod file;
mod settle;

pub use file::{FILE_NAME, Reading, read};
pub use settle::settle;

use std::path::PathBuf;
use std::sync::mpsc::Sender;

/// Watches the config directory for edits of the keymap file and sends every settled re-read to
/// `out`. Returns the watcher, which must be kept alive.
pub fn spawn(dir: PathBuf, out: Sender<Reading>) -> notify::Result<notify::RecommendedWatcher> {
    crate::config::spawn_reading(dir, FILE_NAME, out, read)
}
