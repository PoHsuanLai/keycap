//! `Layout`: which modifier arrangement a keyboard has.

/// The modifier arrangement of a keyboard, decided by its vendor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layout {
    /// Control, Option, Command beside the space bar.
    Apple,
    /// Control, Meta, Alt beside the space bar.
    Pc,
}

/// Vendor ids whose keyboards send the Apple arrangement: Apple Inc., the Bluetooth-era Apple
/// id, and Keychron (0x3434), whose keyboards swap Alt and Meta in firmware when the side switch
/// is on Mac. A Keychron with the switch on Windows sends the PC arrangement and would be read
/// as Apple: keep the switch on Mac.
const APPLE_VENDORS: [u16; 3] = [0x05ac, 0x004c, 0x3434];

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
