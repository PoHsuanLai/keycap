//! `role`: what a physical key means as a modifier on a layout.

use super::Layout;
use crate::event::Physical;
use crate::key::Key;
use crate::role::Role;

/// The role of `key` on `layout`, or `None` if it is not a modifier.
///
/// By position: the key beside the space bar is Command on both layouts (Apple: Meta, PC: Alt)
/// and the next one out is Option (Apple: Alt, PC: Meta).
pub fn role(layout: Layout, key: Physical) -> Option<Role> {
    let (beside_space, next_out) = match layout {
        Layout::Apple => (
            [Key::LEFTMETA, Key::RIGHTMETA],
            [Key::LEFTALT, Key::RIGHTALT],
        ),
        Layout::Pc => (
            [Key::LEFTALT, Key::RIGHTALT],
            [Key::LEFTMETA, Key::RIGHTMETA],
        ),
    };
    let key = key.0;
    if beside_space.contains(&key) {
        Some(Role::Command)
    } else if next_out.contains(&key) {
        Some(Role::Option)
    } else if [Key::LEFTCTRL, Key::RIGHTCTRL].contains(&key) {
        Some(Role::Control)
    } else if [Key::LEFTSHIFT, Key::RIGHTSHIFT].contains(&key) {
        Some(Role::Shift)
    } else {
        None
    }
}
