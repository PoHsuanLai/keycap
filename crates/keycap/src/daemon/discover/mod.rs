//! Finding keyboards: the /sys scan, the pure rule that says which nodes are keyboards, and
//! the watch on `/dev/input` that says when nodes come and go.

mod allowance;
mod caps;
mod classify;
mod hotplug;
mod scan;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod uhid_devices;

pub(crate) use allowance::VirtualAllowance;
pub(crate) use caps::Bits;
pub(crate) use classify::Identity;
pub(crate) use hotplug::{Hotplug, spawn};
pub(crate) use scan::{DEV_INPUT, SYS_INPUT, scan};
