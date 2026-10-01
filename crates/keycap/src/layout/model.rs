//! `Layout`: which modifier arrangement a keyboard has.

/// The modifier arrangement of a keyboard, decided by its vendor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layout {
    /// Control, Option, Command beside the space bar.
    Apple,
    /// Control, Meta, Alt beside the space bar.
    Pc,
}

/// USB vendor ids of Apple keyboards: Apple Inc. and the Bluetooth-era Apple id.
const APPLE_VENDORS: [u16; 2] = [0x05ac, 0x004c];

impl Layout {
    /// The layout of a keyboard from its USB or Bluetooth vendor id.
    pub fn of_vendor(vendor: u16) -> Layout {
        if APPLE_VENDORS.contains(&vendor) {
            Layout::Apple
        } else {
            Layout::Pc
        }
    }
}
