//! The one device discovery may grab, in place of all others, when the environment says so.

/// Which virtual device, if any, discovery may grab. A virtual device is normally never
/// grabbed; a test whose keyboard is itself a uinput device names it here. Naming a device
/// also makes it the only one grabbed, so a test run on a machine with a real keyboard cannot
/// take that keyboard.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum VirtualAllowance {
    /// No virtual device is grabbed.
    #[default]
    Nobody,
    /// The device whose name is exactly this one is eligible like a hardware device, and no
    /// other device is.
    Named(String),
}

impl VirtualAllowance {
    /// The allowance a `$KEYCAP_GRAB_VIRTUAL` value gives: a non-empty value names the device.
    pub(crate) fn from_env_value(value: Option<&str>) -> Self {
        match value {
            Some(name) if !name.is_empty() => Self::Named(name.to_owned()),
            _ => Self::Nobody,
        }
    }

    /// Whether a virtual device called `name` may be grabbed.
    pub(super) fn admits(&self, name: &str) -> bool {
        matches!(self, Self::Named(allowed) if allowed == name)
    }

    /// Whether the allowance names a device other than `name`, which then stays alone.
    pub(super) fn names_another(&self, name: &str) -> bool {
        matches!(self, Self::Named(allowed) if allowed != name)
    }
}

#[cfg(test)]
mod tests {
    use super::super::caps::{Bits, Caps};
    use super::super::classify::{Eligibility, Identity, classify};
    use super::*;
    use crate::Key;
    use crate::daemon::events::{EV_KEY, EV_SYN};
    use crate::daemon::uinput;
    use std::path::PathBuf;

    const HARDWARE: bool = false;
    const VIRTUAL: bool = true;

    /// A keyboard's capability masks: the letters and the space bar.
    fn keyboard_caps() -> Caps {
        let mut words = [0u64; 12];
        let letters = ('a'..='z').map(|c| c.to_string());
        for name in letters.chain(["space".to_owned()]) {
            let code = usize::from(Key::named(&name).expect("a key name").code());
            words[code / 64] |= 1 << (code % 64);
        }
        let key = words
            .iter()
            .rev()
            .map(|w| format!("{w:x}"))
            .collect::<Vec<_>>()
            .join(" ");
        Caps {
            ev: Bits::parse(&format!("{:x}", (1u32 << EV_SYN) | (1 << EV_KEY))),
            key: Bits::parse(&key),
            rel: Bits::default(),
            abs: Bits::default(),
        }
    }

    fn identity(name: &str, vendor: u16, product: u16) -> Identity {
        Identity {
            name: name.to_owned(),
            vendor,
            product,
            path: PathBuf::from("/dev/input/event9"),
        }
    }

    #[test]
    fn env_value_table() {
        const CASES: &[(&str, Option<&str>, VirtualAllowance)] = &[
            ("unset", None, VirtualAllowance::Nobody),
            ("empty", Some(""), VirtualAllowance::Nobody),
        ];
        for (name, value, want) in CASES {
            assert_eq!(&VirtualAllowance::from_env_value(*value), want, "{name}");
        }
        assert_eq!(
            VirtualAllowance::from_env_value(Some("ci keyboard")),
            VirtualAllowance::Named("ci keyboard".to_owned()),
            "a name"
        );
    }

    #[test]
    fn classify_with_an_allowance_table() {
        let named = VirtualAllowance::Named("ci keyboard".to_owned());
        let cases: Vec<(&str, Identity, bool, VirtualAllowance, Eligibility)> = vec![
            (
                "the named virtual device is eligible",
                identity("ci keyboard", 0x05ac, 0x0267),
                VIRTUAL,
                named.clone(),
                Eligibility::Keyboard,
            ),
            (
                "no allowance leaves a virtual device alone",
                identity("ci keyboard", 0x05ac, 0x0267),
                VIRTUAL,
                VirtualAllowance::Nobody,
                Eligibility::NotKeyboard,
            ),
            (
                "another virtual device stays excluded",
                identity("other remapper", 0x1234, 0x5678),
                VIRTUAL,
                named.clone(),
                Eligibility::NotKeyboard,
            ),
            (
                "a name that only starts like the allowed one",
                identity("ci keyboard 2", 0x1234, 0x5678),
                VIRTUAL,
                named.clone(),
                Eligibility::NotKeyboard,
            ),
            (
                "the name in another case",
                identity("CI Keyboard", 0x1234, 0x5678),
                VIRTUAL,
                named.clone(),
                Eligibility::NotKeyboard,
            ),
            (
                "a named allowance leaves hardware alone",
                identity("Some Keyboard", 0x1234, 0x5678),
                HARDWARE,
                named.clone(),
                Eligibility::NotKeyboard,
            ),
            (
                "no allowance leaves hardware eligible",
                identity("Some Keyboard", 0x1234, 0x5678),
                HARDWARE,
                VirtualAllowance::Nobody,
                Eligibility::Keyboard,
            ),
            (
                "keycap's own keyboard stays excluded even when named",
                identity(uinput::NAME, uinput::VENDOR, uinput::PRODUCT),
                VIRTUAL,
                VirtualAllowance::Named(uinput::NAME.to_owned()),
                Eligibility::NotKeyboard,
            ),
            (
                "palmrest's devices stay excluded even when named",
                identity("palmrest keys", 0x0001, 0x0003),
                VIRTUAL,
                VirtualAllowance::Named("palmrest keys".to_owned()),
                Eligibility::NotKeyboard,
            ),
        ];
        for (name, id, origin, allow, want) in cases {
            assert_eq!(
                classify(&keyboard_caps(), &id, origin, &allow),
                want,
                "{name}"
            );
        }
    }
}
