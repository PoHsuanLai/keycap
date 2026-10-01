//! The key names the harness speaks: the same lowercase `KEY_*` names keycap prints, for the
//! keys a fake keyboard has and the keys keycap's mappings write. A test in keycap checks
//! every row against keycap's own table.

/// Name and evdev code of each key the harness knows.
#[rustfmt::skip]
pub const KEYS: &[(&str, u16)] = &[
    ("esc", 1), ("1", 2), ("2", 3), ("3", 4), ("4", 5), ("5", 6), ("6", 7), ("7", 8), ("8", 9),
    ("9", 10), ("0", 11), ("minus", 12), ("equal", 13), ("backspace", 14), ("tab", 15),
    ("q", 16), ("w", 17), ("e", 18), ("r", 19), ("t", 20), ("y", 21), ("u", 22), ("i", 23),
    ("o", 24), ("p", 25), ("enter", 28), ("leftctrl", 29), ("a", 30), ("s", 31), ("d", 32),
    ("f", 33), ("g", 34), ("h", 35), ("j", 36), ("k", 37), ("l", 38), ("leftshift", 42),
    ("z", 44), ("x", 45), ("c", 46), ("v", 47), ("b", 48), ("n", 49), ("m", 50),
    ("rightshift", 54), ("leftalt", 56), ("space", 57), ("capslock", 58), ("f1", 59),
    ("f2", 60), ("f3", 61), ("f4", 62), ("f5", 63), ("f6", 64), ("f7", 65), ("f8", 66),
    ("f9", 67), ("f10", 68), ("f11", 87), ("f12", 88), ("rightctrl", 97), ("rightalt", 100),
    ("home", 102), ("up", 103), ("pageup", 104), ("left", 105), ("right", 106), ("end", 107),
    ("down", 108), ("pagedown", 109), ("insert", 110), ("delete", 111), ("leftmeta", 125),
    ("rightmeta", 126), ("f13", 183), ("f14", 184), ("f15", 185), ("f16", 186), ("f17", 187),
    ("f18", 188), ("f19", 189), ("f20", 190), ("f21", 191), ("f22", 192), ("f23", 193),
    ("f24", 194),
];

/// The code of the key called `name`.
pub fn code_of(name: &str) -> Option<u16> {
    KEYS.iter().find(|&&(n, _)| n == name).map(|&(_, c)| c)
}

/// The name of the key with `code`, or `key<code>` when the harness has none.
pub fn name_of(code: u16) -> String {
    KEYS.iter()
        .find(|&&(_, c)| c == code)
        .map_or_else(|| format!("key{code}"), |&(n, _)| n.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_codes_are_unique_and_round_trip() {
        for &(name, code) in KEYS {
            assert_eq!(code_of(name), Some(code), "{name}");
            assert_eq!(name_of(code), name, "{code}");
            assert_eq!(
                KEYS.iter()
                    .filter(|&&(n, c)| n == name || c == code)
                    .count(),
                1,
                "{name} and {code} each appear once"
            );
        }
    }

    #[test]
    fn unknown_codes_have_a_readable_name() {
        assert_eq!(name_of(0x2ff), "key767");
        assert_eq!(code_of("nope"), None);
    }
}
