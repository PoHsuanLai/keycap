//! Reading a machine-wide layer file: the one reader the machine file in `/etc/keycap` and the
//! vendor file in `/usr/lib/keycap` share.
//!
//! Both are trusted like an administrator's word: they may hold the rules that only an
//! administrator can set, such as one on a key pressed alone. So each is read only when it is
//! root-only (see [`peephole::dirs_trust`]): the file and every folder above it are root's and
//! not writable by anyone else, and the file is not a link. Anything less is not a layer.

use std::io::{self, ErrorKind};
use std::path::Path;

use keycap_core::{ConfigLayer, ConfigSource};
use keycap_protocol::Warning;
use peephole::{ProcFs, dirs_trust, weaker};
use peephole_core::FileTrust;

use super::load::FILE_NAME;

/// The largest layer file read: a real one is a few dozen lines, and a bigger one is not ours.
pub const LAYER_FILE_CAP: u64 = 64 * 1024;

/// Why a layer file that exists is not a layer.
#[derive(Debug)]
enum Refusal {
    /// Someone other than root can change the file or a folder above it, or it is a link.
    NotRootOnly,
    /// The file is not text.
    NotText,
    /// The file could not be read.
    Unreadable(io::Error),
}

impl Refusal {
    /// The problem in plain words, for the person who reads the log.
    fn text(&self, source: ConfigSource) -> String {
        match self {
            Refusal::NotRootOnly => format!(
                "{source}: not used, because someone other than an administrator could change \
                 this file or a folder it is in"
            ),
            Refusal::NotText => format!("{source}: not used, because the file is not text"),
            Refusal::Unreadable(e) => format!("{source}: cannot be read: {e}"),
        }
    }
}

/// The layer `source` from `keycap.toml` in `dir`, judged on `fs`. A missing file or folder is no
/// layer and no warning; a file that is not trusted or cannot be read is no layer and one
/// warning, because the keyboard must keep working whatever the file says.
pub(super) fn read_layer<P: ProcFs>(
    fs: &P,
    dir: &Path,
    source: ConfigSource,
) -> (ConfigLayer, Vec<Warning>) {
    let path = dir.join(FILE_NAME);
    match read_trusted(fs, &path) {
        Layer::Text(text) => ConfigLayer::parse(&text, source),
        Layer::Absent => (ConfigLayer::default(), Vec::new()),
        Layer::Refused(refusal) => (
            ConfigLayer::default(),
            vec![Warning {
                key: path.display().to_string(),
                problem: refusal.text(source),
            }],
        ),
    }
}

/// What the file of one layer came to: not there, its text, or a refusal to use it.
enum Layer {
    /// There is no such file or folder.
    Absent,
    /// The text of a file that is root-only.
    Text(String),
    /// The file exists and is not used.
    Refused(Refusal),
}

/// The text of the file at `path` when it exists and is root-only.
fn read_trusted<P: ProcFs>(fs: &P, path: &Path) -> Layer {
    let file = match fs.trust_of(path) {
        Ok(trust) => trust,
        Err(e) if e.kind() == ErrorKind::NotFound => return Layer::Absent,
        Err(e) => return Layer::Refused(Refusal::Unreadable(e)),
    };
    let dirs = match dirs_trust(fs, path) {
        Ok(dirs) => dirs,
        Err(e) => return Layer::Refused(Refusal::Unreadable(e)),
    };
    if weaker(dirs, file) != FileTrust::RootOnly {
        return Layer::Refused(Refusal::NotRootOnly);
    }
    let bytes = match fs.read_file_capped(path, LAYER_FILE_CAP) {
        Ok(bytes) => bytes,
        Err(e) => return Layer::Refused(Refusal::Unreadable(e)),
    };
    match String::from_utf8(bytes) {
        Ok(text) => Layer::Text(text),
        Err(_) => Layer::Refused(Refusal::NotText),
    }
}
