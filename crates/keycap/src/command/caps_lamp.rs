//! The Caps Lock lamp of a keyboard for `keycap status`, read from the kernel's LED class in
//! sysfs (`<sys>/<eventN>/device/inputN::capslock/brightness`). While keycap holds the keyboard
//! the lamp follows the desktop's Caps Lock state, so it is what the letters will do.

use crate::daemon::leds::Lamp;
use std::fs;
use std::path::Path;

/// Whether the Caps Lock lamp of the keyboard at `device_path` (`/dev/input/eventN`) is lit;
/// `None` when the keyboard has no lamp or its state cannot be read.
pub(super) fn lamp_of(sys_input: &Path, device_path: &str) -> Option<Lamp> {
    let node = Path::new(device_path).file_name()?;
    let device = sys_input.join(node).join("device");
    let lamp_dir = fs::read_dir(device)
        .ok()?
        .filter_map(Result::ok)
        .find(|entry| entry.file_name().to_string_lossy().ends_with("::capslock"))?;
    let brightness = fs::read_to_string(lamp_dir.path().join("brightness")).ok()?;
    match brightness.trim().parse::<u32>().ok()? {
        0 => Some(Lamp::Off),
        _ => Some(Lamp::On),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::scratch::Scratch;

    fn lamp_file(root: &Path, node: &str, led: &str, brightness: &str) {
        let dir = root.join(node).join("device").join(led);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("brightness"), brightness).unwrap();
    }

    #[test]
    fn the_lamp_is_read_from_the_leds_of_the_keyboards_node() {
        let scratch = Scratch::new("caps-lamp");
        lamp_file(scratch.path(), "event16", "input20::numlock", "1\n");
        lamp_file(scratch.path(), "event16", "input20::capslock", "1\n");
        lamp_file(scratch.path(), "event17", "input21::capslock", "0\n");
        let lamp = |path| lamp_of(scratch.path(), path);
        assert_eq!(lamp("/dev/input/event16"), Some(Lamp::On));
        assert_eq!(lamp("/dev/input/event17"), Some(Lamp::Off));
        assert_eq!(lamp("/dev/input/event99"), None, "no such keyboard");
    }

    #[test]
    fn a_keyboard_without_a_caps_lock_lamp_or_with_junk_has_none() {
        let scratch = Scratch::new("caps-lamp-none");
        lamp_file(scratch.path(), "event3", "input3::numlock", "1");
        lamp_file(scratch.path(), "event4", "input4::capslock", "lit");
        assert_eq!(lamp_of(scratch.path(), "/dev/input/event3"), None);
        assert_eq!(lamp_of(scratch.path(), "/dev/input/event4"), None);
    }
}
