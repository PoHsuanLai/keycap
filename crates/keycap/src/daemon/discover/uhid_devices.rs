//! The Bluetooth devices on the user's desk, with the capability masks their nodes print under
//! `/sys/class/input`: which of them keycap grabs.

use super::allowance::VirtualAllowance;
use super::caps::{Bits, Caps};
use super::classify::{Eligibility, Identity, classify, is_virtual};
use std::path::PathBuf;

/// One node as sysfs shows it: name, vendor, product and the three masks.
struct Node {
    name: &'static str,
    vendor: u16,
    product: u16,
    ev: &'static str,
    key: &'static str,
    rel: &'static str,
    abs: &'static str,
}

const MAGIC_MOUSE: Node = Node {
    name: "Magic Mouse",
    vendor: 0x004c,
    product: 0x0269,
    ev: "1f",
    key: "70001 0 0 0 0",
    rel: "1943",
    abs: "0",
};

const KEYCHRON_KEYBOARD: Node = Node {
    name: "Keychron K5 Max Keyboard",
    vendor: 0x3434,
    product: 0x0a53,
    ev: "12001f",
    key: "3f00733fff 0 0 483ffff17aff32d bfd4444600000000 1 130ff38b17d007 ffff7bfad941dfff ffbeffdfffefffff fffffffffffffffe",
    rel: "1040",
    abs: "100000000",
};

const KEYCHRON_MOUSE: Node = Node {
    name: "Keychron K5 Max Mouse",
    vendor: 0x3434,
    product: 0x0a53,
    ev: "17",
    key: "1f0000 0 0 0 0",
    rel: "1943",
    abs: "0",
};

fn eligibility(node: &Node, allow: &VirtualAllowance) -> Eligibility {
    let caps = Caps {
        ev: Bits::parse(node.ev),
        key: Bits::parse(node.key),
        rel: Bits::parse(node.rel),
        abs: Bits::parse(node.abs),
    };
    let id = Identity {
        name: node.name.to_owned(),
        vendor: node.vendor,
        product: node.product,
        path: PathBuf::from("/dev/input/event0"),
    };
    // Bluetooth HID devices register under `devices/virtual/misc/uhid`, which is not
    // `devices/virtual/input`, so they are hardware to keycap.
    classify(&caps, &id, false, allow)
}

#[test]
fn the_magic_mouse_and_the_keyboards_mouse_interface_are_never_grabbed() {
    for node in [&MAGIC_MOUSE, &KEYCHRON_MOUSE] {
        assert_eq!(
            eligibility(node, &VirtualAllowance::Nobody),
            Eligibility::NotKeyboard,
            "{}",
            node.name
        );
    }
}

#[test]
fn the_keychron_keyboard_is_a_plain_keyboard_despite_its_wheel_axes() {
    assert_eq!(
        eligibility(&KEYCHRON_KEYBOARD, &VirtualAllowance::Nobody),
        Eligibility::Keyboard
    );
}

#[test]
fn the_uhid_sysfs_path_is_hardware() {
    let path = std::path::Path::new(
        "/sys/devices/virtual/misc/uhid/0005:3434:0A53.0024/input/input103/event16",
    );
    assert!(!is_virtual(path));
}
