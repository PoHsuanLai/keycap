//! Reading the machine file: `keycap.toml` in the config directory.

use std::path::Path;

use keycap_core::{ConfigLayer, ConfigSource};
use keycap_protocol::Warning;
use peephole::{ProcFs, RealProc};

use super::trusted::read_layer;

/// The file name the machine and vendor layers are read from, in their directories.
pub const FILE_NAME: &str = "keycap.toml";

/// The machine layer from `dir`, judged on the real file system. It is read only when it is
/// root-only, by the check in `trusted.rs` that the vendor file shares. A missing file is an
/// empty layer and no warning; a file anyone but root could change, or one that is unreadable
/// or not TOML, is an empty layer and one warning, because the keyboard must keep working
/// whatever the file says.
pub fn system(dir: &Path) -> (ConfigLayer, Vec<Warning>) {
    system_through(&RealProc::system(), dir)
}

/// [`system`] over any file-system view, so a test can stand a file with an owner in a tree.
pub(super) fn system_through<P: ProcFs>(fs: &P, dir: &Path) -> (ConfigLayer, Vec<Warning>) {
    read_layer(fs, dir, ConfigSource::System)
}
