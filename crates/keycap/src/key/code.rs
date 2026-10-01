//! `Key`: an evdev key code, named through the table.

use super::names::NAMES;

/// An evdev key code (`KEY_*` in `linux/input-event-codes.h`).
///
/// Every `u16` is a key the kernel may send, so `from_code` accepts any code; the name table
/// covers the standard keyboard, keypad and media keys, and `name` reports `"unknown"` for a
/// code outside it. Code that spells a key in text goes through `named`, so a typo is a parse
/// failure and never a wrong key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key(u16);

/// The `name` of a code the table does not list.
const UNNAMED: &str = "unknown";

impl Key {
    pub const ESC: Key = Key(1);
    pub const LEFTCTRL: Key = Key(29);
    pub const LEFTSHIFT: Key = Key(42);
    pub const RIGHTSHIFT: Key = Key(54);
    pub const LEFTALT: Key = Key(56);
    pub const RIGHTCTRL: Key = Key(97);
    pub const RIGHTALT: Key = Key(100);
    pub const LEFTMETA: Key = Key(125);
    pub const RIGHTMETA: Key = Key(126);

    /// The key a device reported as `code`; any code is a key.
    pub const fn from_code(code: u16) -> Key {
        Key(code)
    }

    /// The key called `name` (lowercase, as in `KEY_<NAME>`), if the table has it.
    pub fn named(name: &str) -> Option<Key> {
        NAMES.iter().find(|(n, _)| *n == name).map(|&(_, c)| Key(c))
    }

    /// The table name of this key, or `"unknown"` for a code the table does not list.
    pub fn name(self) -> &'static str {
        NAMES
            .iter()
            .find(|&&(_, c)| c == self.0)
            .map_or(UNNAMED, |&(n, _)| n)
    }

    /// The evdev code.
    pub const fn code(self) -> u16 {
        self.0
    }
}
