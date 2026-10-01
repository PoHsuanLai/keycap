//! The two sides of a key and the device it came from.

use crate::key::Key;

/// A key as the keyboard sent it. Never converted to `Logical`: a mapping decides that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Physical(pub Key);

/// A key as keycap writes it. Never converted from `Physical`: a mapping decides that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Logical(pub Key);

/// One grabbed keyboard, as numbered by the daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId(pub u32);
