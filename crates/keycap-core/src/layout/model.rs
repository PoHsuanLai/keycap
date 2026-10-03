//! `Layout`: which modifier arrangement a keyboard has.

/// The modifier arrangement of a keyboard, decided by its vendor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layout {
    /// Control, Option, Command beside the space bar.
    Apple,
    /// Control, Meta, Alt beside the space bar.
    Pc,
}

/// USB vendor ids of Apple keyboards: Apple Inc. and the Bluetooth-era Apple id. A keyboard
/// with a Mac/Windows switch (Keychron, ...) reports the same ids in both positions, so it is
/// `Pc` here until the config names it (`[[device]]`).
const APPLE_VENDORS: [u16; 2] = [0x05ac, 0x004c];

impl Layout {
    /// The layout a config name stands for.
    pub fn named(name: &str) -> Option<Layout> {
        match name {
            "apple" => Some(Layout::Apple),
            "pc" => Some(Layout::Pc),
            _ => None,
        }
    }

    /// The layout of a keyboard from its USB or Bluetooth vendor id.
    pub fn of_vendor(vendor: u16) -> Layout {
        if APPLE_VENDORS.contains(&vendor) {
            Layout::Apple
        } else {
            Layout::Pc
        }
    }
}
