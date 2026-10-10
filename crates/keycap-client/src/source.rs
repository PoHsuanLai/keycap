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

/// Where a layer of overrides came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The keycap daemon, serving the machine's keymap file.
    Daemon,
    /// The machine's keymap file, read directly because no daemon answered.
    MachineFile,
    /// The person's own keymap file.
    PersonFile,
}

impl Origin {
    /// How a problem from this layer is introduced.
    fn label(self) -> &'static str {
        match self {
            Origin::Daemon | Origin::MachineFile => "System keymap",
            Origin::PersonFile => "Your keymap",
        }
    }
}

/// One layer of overrides and what was wrong with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layer {
    pub overrides: Overrides,
    /// Skipped or refused lines, in plain words, not yet labelled with their origin.
    pub problems: Vec<String>,
    pub origin: Origin,
}

/// The keymap of our desktop: chordkit's conventions for it, the desktop's own bindings
/// ([`crate::system_bindings`]), the machine's keymap and the person's own keymap laid over
/// them, the person's last.
#[derive(Debug, Clone)]
pub struct KeycapSource {
    socket: PathBuf,
    machine_file: PathBuf,
    person_file: PathBuf,
}

impl KeycapSource {
    /// A source talking to the daemon at `socket`, falling back to `machine_file`, and reading
    /// the person's overrides from `person_file`. An empty `person_file` means none.
    pub fn new(
        socket: impl Into<PathBuf>,
        machine_file: impl Into<PathBuf>,
        person_file: impl Into<PathBuf>,
    ) -> KeycapSource {
        KeycapSource {
            socket: socket.into(),
            machine_file: machine_file.into(),
            person_file: person_file.into(),
        }
    }

    /// The source for this machine and person, read through `lookup`: `$KEYCAP_SOCKET` (else
    /// `/run/keycap/keycap.sock`), the machine file in `$KEYCAP_CONFIG_DIR` (else `/etc/keycap`)
    /// and the person's file `keycap/keymap` in `$XDG_CONFIG_HOME` (else `$HOME/.config`).
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> KeycapSource {
        let non_empty = |name: &str| lookup(name).filter(|value| !value.is_empty());
        let config = non_empty("KEYCAP_CONFIG_DIR").unwrap_or_else(|| CONFIG_DIR.to_owned());
        let person = non_empty("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| non_empty("HOME").map(|home| Path::new(&home).join(".config")))
            .map(|dir| dir.join("keycap").join(KEYMAP_FILE))
            .unwrap_or_default();
        KeycapSource::new(
            non_empty("KEYCAP_SOCKET").unwrap_or_else(|| SOCKET_PATH.to_owned()),
            Path::new(&config).join(KEYMAP_FILE),
            person,
        )
    }

    /// The source for this machine and person, from the process environment.
    pub fn standard() -> KeycapSource {
        KeycapSource::from_lookup(|name| std::env::var(name).ok())
    }

    pub(crate) fn socket(&self) -> &Path {
        &self.socket
    }

    /// The file Settings writes the person's shortcuts to. It needs no privilege.
    pub fn person_file(&self) -> &Path {
        &self.person_file
    }

    /// The machine's keymap: from the daemon, or from its file when no daemon answers.
    pub fn machine(&self) -> Layer {
        match fetch::keymap(&self.socket) {
            Ok(state) => {
                let (overrides, bad) = Overrides::parse(&state.overrides);
                let mut problems: Vec<String> = state.problems.iter().map(describe).collect();
                problems.extend(bad.iter().map(ToString::to_string));
                Layer {
                    overrides,
                    problems,
                    origin: Origin::Daemon,
                }
            }
            Err(_) => {
                let (overrides, problems) = fallback::read(&self.machine_file);
                Layer {
                    overrides,
                    problems,
                    origin: Origin::MachineFile,
                }
            }
        }
    }

    /// The person's own keymap.
    pub fn person(&self) -> Layer {
        let (overrides, problems) = fallback::read(&self.person_file);
        Layer {
            overrides,
            problems,
            origin: Origin::PersonFile,
        }
    }

    /// The keymap of `platform` with, on ours, the desktop's own bindings, then the machine's
    /// overrides, then the person's (which win); and every problem in plain words, labelled by
    /// the file it came from, the chords the desktop keeps for itself included.
    pub fn load_reporting(&self, platform: Platform) -> (Keymap, Vec<String>) {
        let base = Keymap::conventional(platform);
        let base = if platform == OURS {
            base.with_system(system::system_bindings())
        } else {
            base
        };
        let (keymap, mut problems) = lay(base, self.machine());
        let (keymap, more) = lay(keymap, self.person());
        problems.extend(more);
        (keymap, problems)
    }
}

/// `keymap` with `layer` over it; the layer's problems and refusals, labelled.
fn lay(keymap: Keymap, layer: Layer) -> (Keymap, Vec<String>) {
    let (keymap, refused) = keymap.with_overrides(&layer.overrides);
    let mut problems = layer.problems;
    for refusal in refused.iter().map(ToString::to_string) {
        // The daemon already checked the same chords against the same table.
        if !problems.contains(&refusal) {
            problems.push(refusal);
        }
    }
    let label = layer.origin.label();
    (
        keymap,
        problems
            .into_iter()
            .map(|problem| format!("{label}: {problem}"))
            .collect(),
    )
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
        watch::spawn(
            self.socket.clone(),
            self.machine_file.clone(),
            self.person_file.clone(),
            on_change,
        )
        .map(Watch::holding)
        .map_err(|e| SourceError::Unsupported(format!("couldn't start watching: {e}")))
    }
}

#[cfg(test)]
mod tests;
