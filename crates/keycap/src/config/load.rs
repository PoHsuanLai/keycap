//! Reading the system file: `keycap.toml` in the config directory.

use std::io::ErrorKind;
use std::path::Path;

use keycap_protocol::Warning;

use super::layer::{Layer, Source};
use super::lenient::warning;

/// The file name the system layer is read from, in the system config directory.
pub const FILE_NAME: &str = "keycap.toml";

/// The system layer from `dir`. A missing file is an empty layer and no warning; an unreadable
/// or non-TOML one is an empty layer and one warning, because the keyboard must keep working
/// whatever the file says.
pub fn system(dir: &Path) -> (Layer, Vec<Warning>) {
    let path = dir.join(FILE_NAME);
    match std::fs::read_to_string(&path) {
        Ok(text) => Layer::parse(&text, Source::System),
        Err(e) if e.kind() == ErrorKind::NotFound => (Layer::default(), Vec::new()),
        Err(e) => (
            Layer::default(),
            vec![warning(
                &path.display().to_string(),
                format!("{}: cannot be read: {e}", Source::System),
            )],
        ),
    }
}
