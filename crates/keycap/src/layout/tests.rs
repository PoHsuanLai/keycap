use super::*;
use crate::event::Physical;
use crate::key::Key;
use crate::role::Role;

#[test]
fn role_by_position() {
    const CASES: &[(Layout, Key, Option<Role>)] = &[
        (Layout::Apple, Key::LEFTMETA, Some(Role::Command)),
        (Layout::Apple, Key::RIGHTMETA, Some(Role::Command)),
        (Layout::Apple, Key::LEFTALT, Some(Role::Option)),
        (Layout::Apple, Key::RIGHTALT, Some(Role::Option)),
        (Layout::Pc, Key::LEFTALT, Some(Role::Command)),
        (Layout::Pc, Key::RIGHTALT, Some(Role::Command)),
        (Layout::Pc, Key::LEFTMETA, Some(Role::Option)),
        (Layout::Pc, Key::RIGHTMETA, Some(Role::Option)),
        (Layout::Apple, Key::LEFTCTRL, Some(Role::Control)),
        (Layout::Apple, Key::RIGHTCTRL, Some(Role::Control)),
        (Layout::Pc, Key::LEFTCTRL, Some(Role::Control)),
        (Layout::Pc, Key::RIGHTCTRL, Some(Role::Control)),
        (Layout::Apple, Key::LEFTSHIFT, Some(Role::Shift)),
        (Layout::Apple, Key::RIGHTSHIFT, Some(Role::Shift)),
        (Layout::Pc, Key::LEFTSHIFT, Some(Role::Shift)),
        (Layout::Pc, Key::RIGHTSHIFT, Some(Role::Shift)),
        (Layout::Apple, Key::ESC, None),
        (Layout::Pc, Key::ESC, None),
    ];
    for &(layout, key, want) in CASES {
        assert_eq!(
            role(layout, Physical(key)),
            want,
            "{layout:?} {}",
            key.name()
        );
    }
}

#[test]
fn unnamed_codes_are_not_modifiers() {
    assert_eq!(role(Layout::Apple, Physical(Key::from_code(0xffff))), None);
}

#[test]
fn layout_of_vendor_table() {
    const CASES: &[(u16, Layout)] = &[
        (0x05ac, Layout::Apple),
        (0x004c, Layout::Apple),
        (0x3434, Layout::Pc),
        (0x046d, Layout::Pc),
        (0x0000, Layout::Pc),
    ];
    for &(vendor, want) in CASES {
        assert_eq!(Layout::of_vendor(vendor), want, "{vendor:#06x}");
    }
}
