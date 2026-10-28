//! The /sys scan: every eligible keyboard node under injectable roots.

use super::allowance::VirtualAllowance;
use super::caps::{Bits, Caps};
use super::classify::{Eligibility, Identity, classify, is_virtual};
use std::path::{Path, PathBuf};

/// Where input class devices are listed.
pub(crate) const SYS_INPUT: &str = "/sys/class/input";
/// Where their device nodes are.
pub(crate) const DEV_INPUT: &str = "/dev/input";
/// Where udev keeps what it knows about each device, by device numbers.
pub(crate) const UDEV_DATA: &str = "/run/udev/data";

/// A node worth grabbing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub identity: Identity,
    pub eligibility: Eligibility,
}

/// Every keyboard node listed under `sys_root` (normally `/sys/class/input`), as a path under
/// `dev_root` (normally `/dev/input`), ordered by node name. Unreadable entries are skipped:
/// a node that vanishes mid-scan is simply not a candidate, and neither is one that udev
/// (`udev_data`, normally `/run/udev/data`) puts on a seat other than ours.
pub(crate) fn scan(
    sys_root: &Path,
    dev_root: &Path,
    udev_data: &Path,
    allow: &VirtualAllowance,
) -> Vec<Candidate> {
    let mut nodes: Vec<PathBuf> = std::fs::read_dir(sys_root)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| super::hotplug::concerns_us(p))
        .filter(|p| super::seat::on_our_seat(p, udev_data))
        .collect();
    nodes.sort();
    nodes
        .iter()
        .filter_map(|dir| candidate(dir, dev_root, allow))
        .collect()
}

/// One sysfs node as a candidate, if it is a keyboard.
fn candidate(dir: &Path, dev_root: &Path, allow: &VirtualAllowance) -> Option<Candidate> {
    let read = |rel: &str| std::fs::read_to_string(dir.join(rel)).unwrap_or_default();
    let hex = |rel: &str| u16::from_str_radix(read(rel).trim(), 16).unwrap_or(0);
    let caps = Caps {
        ev: Bits::parse(&read("device/capabilities/ev")),
        key: Bits::parse(&read("device/capabilities/key")),
        rel: Bits::parse(&read("device/capabilities/rel")),
        abs: Bits::parse(&read("device/capabilities/abs")),
    };
    let identity = Identity {
        name: read("device/name").trim().to_owned(),
        vendor: hex("device/id/vendor"),
        product: hex("device/id/product"),
        path: dev_root.join(dir.file_name()?),
    };
    // The class entry is a symlink into the device tree; where it leads says what made the node.
    let virtual_node = is_virtual(&std::fs::canonicalize(dir).ok()?);
    match classify(&caps, &identity, virtual_node, allow) {
        Eligibility::NotKeyboard => None,
        eligibility => Some(Candidate {
            identity,
            eligibility,
        }),
    }
}
