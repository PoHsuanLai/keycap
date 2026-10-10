//! `KeycapSource`: this desktop's `chordkit::KeymapSource`.

use crate::{fallback, fetch, system, watch};
use chordkit::{
    ChangeCallback, Desktop, Keymap, KeymapSource, Overrides, Platform, SourceError, Watch,
};
use keycap_protocol::{CONFIG_DIR, KEYMAP_FILE, KeymapProblem, SOCKET_PATH};
use std::path::{Path, PathBuf};

/// Where our desktop's keymap is the person's own: the platform keycap serves.
const OURS: Platform = Platform::Linux {
    desktop: Desktop::Ours,
};

/// Where the person's keymap came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The keycap daemon.
    Daemon,
    /// The keymap file, read directly because no daemon answered.
    File,
}

/// The person's keymap and what was wrong with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Personal {
    pub overrides: Overrides,
    /// Skipped or refused lines, in plain words.
    pub problems: Vec<String>,
    pub origin: Origin,
}

/// The keymap of our desktop: chordkit's conventions for it, the desktop's own bindings
/// ([`crate::system_bindings`]) and the person's keymap laid over them.
#[derive(Debug, Clone)]
pub struct KeycapSource {
    socket: PathBuf,
    keymap_file: PathBuf,
}

impl KeycapSource {
    /// A source talking to the daemon at `socket` and falling back to `keymap_file`.
    pub fn new(socket: impl Into<PathBuf>, keymap_file: impl Into<PathBuf>) -> KeycapSource {
        KeycapSource {
            socket: socket.into(),
            keymap_file: keymap_file.into(),
        }
    }

    /// The source for this machine: `$KEYCAP_SOCKET` (else `/run/keycap/keycap.sock`) and the
    /// keymap file in `$KEYCAP_CONFIG_DIR` (else `/etc/keycap`), read through `lookup`.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> KeycapSource {
        let non_empty = |name: &str| lookup(name).filter(|value| !value.is_empty());
        let config = non_empty("KEYCAP_CONFIG_DIR").unwrap_or_else(|| CONFIG_DIR.to_owned());
        KeycapSource::new(
            non_empty("KEYCAP_SOCKET").unwrap_or_else(|| SOCKET_PATH.to_owned()),
            Path::new(&config).join(KEYMAP_FILE),
        )
    }

    /// The source for this machine, from the process environment.
    pub fn standard() -> KeycapSource {
        KeycapSource::from_lookup(|name| std::env::var(name).ok())
    }

    pub(crate) fn socket(&self) -> &Path {
        &self.socket
    }

    /// The person's keymap: from the daemon, or from the file when no daemon answers.
    pub fn personal(&self) -> Personal {
        match fetch::keymap(&self.socket) {
            Ok(state) => {
                let (overrides, bad) = Overrides::parse(&state.overrides);
                let mut problems: Vec<String> = state.problems.iter().map(describe).collect();
                problems.extend(bad.iter().map(ToString::to_string));
                Personal {
                    overrides,
                    problems,
                    origin: Origin::Daemon,
                }
            }
            Err(_) => {
                let (overrides, problems) = fallback::read(&self.keymap_file);
                Personal {
                    overrides,
                    problems,
                    origin: Origin::File,
                }
            }
        }
    }

    /// The keymap of `platform` with, on ours, the desktop's own bindings, and the person's
    /// keymap over them; and every problem in plain words, the chords the desktop keeps for
    /// itself included.
    pub fn load_reporting(&self, platform: Platform) -> (Keymap, Vec<String>) {
        let base = Keymap::conventional(platform);
        let base = if platform == OURS {
            base.with_system(system::system_bindings())
        } else {
            base
        };
        let personal = self.personal();
        let (keymap, refused) = base.with_overrides(&personal.overrides);
        let mut problems = personal.problems;
        for refusal in refused.iter().map(ToString::to_string) {
            // The daemon already checked the same chords against the same table.
            if !problems.contains(&refusal) {
                problems.push(refusal);
            }
        }
        (keymap, problems)
    }
}

fn describe(problem: &KeymapProblem) -> String {
    match problem.line {
        0 => problem.what.clone(),
        line => format!("line {line}: {}", problem.what),
    }
}

impl KeymapSource for KeycapSource {
    fn load(&self, platform: Platform) -> Result<Keymap, SourceError> {
        Ok(self.load_reporting(platform).0)
    }

    /// Calls `on_change` when the daemon says the keymap changed, or, while no daemon answers,
    /// when the keymap file does. Keep the returned `Watch` alive.
    fn watch(&self, on_change: ChangeCallback) -> Result<Watch, SourceError> {
        watch::spawn(self.socket.clone(), self.keymap_file.clone(), on_change)
            .map(Watch::holding)
            .map_err(|e| SourceError::Unsupported(format!("couldn't start watching: {e}")))
    }
}

#[cfg(test)]
mod tests;
