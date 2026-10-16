//! Reading the vendor file: the defaults a desktop ships in `/usr/lib/keycap/keycap.toml`, laid
//! under the machine file in `/etc/keycap`, which stays the administrator's. It is read like the
//! machine file is, through the one reader in `trusted.rs`.

use std::path::Path;

use keycap_core::{ConfigLayer, ConfigSource};
use keycap_protocol::Warning;
use peephole::{ProcFs, RealProc};

use super::trusted::read_layer;

/// Where the vendor file is unless `$KEYCAP_VENDOR_DIR` says otherwise.
pub const VENDOR_DIR: &str = "/usr/lib/keycap";

/// The vendor layer from `dir`, judged on the real file system.
pub fn vendor(dir: &Path) -> (ConfigLayer, Vec<Warning>) {
    vendor_through(&RealProc::system(), dir)
}

/// [`vendor`] over any file-system view, so a test can stand a file with an owner in a tree.
pub(super) fn vendor_through<P: ProcFs>(fs: &P, dir: &Path) -> (ConfigLayer, Vec<Warning>) {
    read_layer(fs, dir, ConfigSource::Vendor)
}
