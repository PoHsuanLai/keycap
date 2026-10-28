//! The data of the grab gate.

/// Bytes in the `EVIOCGKEY` bitmap: `KEY_MAX / 8 + 1`.
pub const KEYS_DOWN_BYTES: usize = 96;

/// The kernel's bitmap of keys currently down on a device (`EVIOCGKEY`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeysDown([u8; KEYS_DOWN_BYTES]);

impl KeysDown {
    /// The bitmap as the ioctl filled it.
    pub const fn from_bytes(bytes: [u8; KEYS_DOWN_BYTES]) -> Self {
        Self(bytes)
    }

    /// Whether any key is down.
    pub fn any(&self) -> bool {
        self.0.iter().any(|&b| b != 0)
    }
}

/// What the daemon does with the device after a check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Decision {
    /// A key is down: look again later.
    Wait,
    /// Grab the device now.
    Grab,
}
