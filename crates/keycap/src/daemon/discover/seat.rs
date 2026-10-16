//! Which logind seat a device node belongs to. keycap serves one seat, `seat0`, with one virtual
//! keyboard, so a keyboard assigned to another seat must be left to that seat's own session
//! instead of typing into this one. Pure but for the one file read.

use std::path::Path;

/// The seat keycap serves, and the one a device has when udev assigns none.
pub(crate) const OUR_SEAT: &str = "seat0";

/// The udev database prefix of a property line.
const SEAT_PROPERTY: &str = "E:ID_SEAT=";

/// The seat a udev database entry assigns: its `ID_SEAT`, else `seat0`.
fn seat_in(entry: &str) -> &str {
    entry
        .lines()
        .find_map(|line| line.strip_prefix(SEAT_PROPERTY))
        .map(str::trim)
        .filter(|seat| !seat.is_empty())
        .unwrap_or(OUR_SEAT)
}

/// Whether the udev database entry puts its device on our seat.
fn is_ours(entry: &str) -> bool {
    seat_in(entry) == OUR_SEAT
}

/// Whether the class entry `class_dir` (`/sys/class/input/eventN`) is on our seat, from the
/// udev database under `udev_data` (`/run/udev/data`, entries named `c<major>:<minor>`). A
/// node with no readable entry is taken to be ours: it is where devices land by default, and a
/// missing database must not leave the machine without a remapped keyboard.
pub(super) fn on_our_seat(class_dir: &Path, udev_data: &Path) -> bool {
    let Ok(numbers) = std::fs::read_to_string(class_dir.join("dev")) else {
        return true;
    };
    let entry = udev_data.join(format!("c{}", numbers.trim()));
    std::fs::read_to_string(entry).map_or(true, |text| is_ours(&text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::scratch::Scratch;

    #[test]
    fn a_device_is_ours_unless_udev_names_another_seat() {
        let cases: &[(&str, &str, bool)] = &[
            (
                "no seat property",
                "E:ID_INPUT=1\nE:ID_INPUT_KEYBOARD=1\n",
                true,
            ),
            ("seat0 named", "E:ID_SEAT=seat0\n", true),
            ("an empty value", "E:ID_SEAT=\n", true),
            (
                "another seat",
                "E:ID_INPUT=1\nE:ID_SEAT=seat1\nG:seat\n",
                false,
            ),
            ("a longer name is another seat", "E:ID_SEAT=seat01\n", false),
            (
                "the property is not a prefix of another",
                "E:XID_SEAT=seat1\n",
                true,
            ),
        ];
        for &(name, entry, want) in cases {
            assert_eq!(is_ours(entry), want, "{name}");
        }
    }

    #[test]
    fn the_entry_is_found_by_the_nodes_device_numbers_and_a_missing_one_is_ours() {
        let root = Scratch::new("seat");
        let udev = root.path().join("udev");
        std::fs::create_dir_all(&udev).unwrap();
        let class = |name: &str, numbers: Option<&str>| {
            let dir = root.path().join(name);
            std::fs::create_dir_all(&dir).unwrap();
            if let Some(numbers) = numbers {
                std::fs::write(dir.join("dev"), format!("{numbers}\n")).unwrap();
            }
            dir
        };
        std::fs::write(udev.join("c13:64"), "E:ID_SEAT=seat1\n").unwrap();
        std::fs::write(udev.join("c13:65"), "E:ID_INPUT=1\n").unwrap();
        assert!(
            !on_our_seat(&class("event0", Some("13:64")), &udev),
            "seat1"
        );
        assert!(
            on_our_seat(&class("event1", Some("13:65")), &udev),
            "no seat"
        );
        assert!(
            on_our_seat(&class("event2", Some("13:66")), &udev),
            "no entry"
        );
        assert!(
            on_our_seat(&class("event3", None), &udev),
            "no device numbers"
        );
    }
}
