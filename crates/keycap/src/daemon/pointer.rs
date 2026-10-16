//! The pointer half of a combo device as a value: what a "keycap pointer" twin registers.
//! Pure; the ioctls that read the masks and the axis ranges belong to `grabber` and `sys`.

use super::discover::Bits;
use super::events::{MAX_CODE, is_button};
use super::sys::AbsInfo;
use super::uinput::REL_AXES;

/// What follows the original's name in a twin's name; discovery never grabs a node that ends
/// with it.
pub(crate) const NAME_SUFFIX: &str = "(keycap pointer)";
/// Bytes of a uinput device name, the terminating NUL included.
const NAME_BYTES: usize = 80;
/// `REL_MAX`.
const REL_MAX: u16 = 0x0f;
/// `ABS_MAX`.
pub(crate) const ABS_MAX: u16 = 0x3f;
/// `INPUT_PROP_MAX`.
const PROP_MAX: u16 = 0x1f;

/// The name of the twin of the device called `original`, cut to fit a uinput name.
pub(crate) fn twin_name(original: &str) -> String {
    let room = NAME_BYTES - 1 - NAME_SUFFIX.len() - 1;
    let mut cut = original.len().min(room);
    while !original.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{} {NAME_SUFFIX}", &original[..cut])
}

/// One absolute axis with the range and resolution the source reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AbsAxis {
    pub code: u16,
    pub info: AbsInfo,
}

/// The capability masks of a device node, as the kernel reports them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Masks {
    pub props: Bits,
    pub key: Bits,
    pub rel: Bits,
    pub abs: Bits,
}

impl Masks {
    /// The absolute axes the device has, whose ranges still have to be read.
    pub(crate) fn abs_axes(&self) -> Vec<u16> {
        set_bits(&self.abs, ABS_MAX)
    }
}

/// Every code in `0..=max` that `mask` has set.
fn set_bits(mask: &Bits, max: u16) -> Vec<u16> {
    (0..=max).filter(|&n| mask.has(u32::from(n))).collect()
}

/// Everything a twin registers: the identity, the properties and the pointer capabilities
/// cloned from the source device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PointerCaps {
    /// The original device's name, which the device service takes and adds the twin's suffix to.
    pub label: String,
    /// The twin's own name, for the log and for opening `/dev/uinput` directly.
    pub name: String,
    pub vendor: u16,
    pub product: u16,
    /// `INPUT_PROP_*` bits (a touchpad is `INPUT_PROP_POINTER`, a clickpad `BUTTONPAD`).
    pub props: Vec<u16>,
    /// `BTN_*` codes.
    pub buttons: Vec<u16>,
    pub rel: Vec<u16>,
    pub abs: Vec<AbsAxis>,
}

impl PointerCaps {
    /// The twin of the device `original`, from its masks and the ranges of its `abs` axes.
    pub(crate) fn new(
        original: &str,
        vendor: u16,
        product: u16,
        masks: &Masks,
        abs: Vec<AbsAxis>,
    ) -> Self {
        Self {
            label: original.to_owned(),
            name: twin_name(original),
            vendor,
            product,
            props: set_bits(&masks.props, PROP_MAX),
            buttons: (0..=MAX_CODE)
                .filter(|&code| is_button(code) && masks.key.has(u32::from(code)))
                .collect(),
            rel: set_bits(&masks.rel, REL_MAX),
            abs,
        }
    }

    /// Whether the main virtual keyboard cannot carry this pointer: it has absolute axes, or
    /// relative ones the keyboard does not register.
    pub(crate) fn needs_twin(&self) -> bool {
        !self.abs.is_empty() || self.rel.iter().any(|axis| !REL_AXES.contains(axis))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bits(set: &[u16]) -> Bits {
        let mut bytes = [0u8; 96];
        for &n in set {
            bytes[usize::from(n / 8)] |= 1 << (n % 8);
        }
        Bits::from_bytes(&bytes)
    }

    fn masks(props: &[u16], key: &[u16], rel: &[u16], abs: &[u16]) -> Masks {
        Masks {
            props: bits(props),
            key: bits(key),
            rel: bits(rel),
            abs: bits(abs),
        }
    }

    fn axis(code: u16, maximum: i32) -> AbsAxis {
        AbsAxis {
            code,
            info: AbsInfo {
                maximum,
                resolution: 12,
                ..AbsInfo::default()
            },
        }
    }

    const BTN_LEFT: u16 = 0x110;
    const BTN_TOUCH: u16 = 0x14a;
    const KEY_A: u16 = 30;
    const REL_DIAL: u16 = 0x07;

    #[test]
    fn a_twin_is_named_after_its_original_and_always_fits() {
        let long = "x".repeat(200);
        let accented = format!("{}é", "x".repeat(50));
        let cases: &[(&str, &str)] = &[
            (
                "Apple Magic Keyboard",
                "Apple Magic Keyboard (keycap pointer)",
            ),
            ("", " (keycap pointer)"),
            (&long, &format!("{} (keycap pointer)", "x".repeat(62))),
            (&accented, &format!("{accented} (keycap pointer)")),
        ];
        for &(original, want) in cases {
            let got = twin_name(original);
            assert_eq!(got, want, "{original:?}");
            assert!(got.len() < NAME_BYTES, "{original:?}: {} bytes", got.len());
        }
        let on_a_boundary = format!("{}é", "x".repeat(61));
        let cut = twin_name(&on_a_boundary);
        assert_eq!(
            cut,
            format!("{} (keycap pointer)", "x".repeat(61)),
            "no split char"
        );
    }

    #[test]
    fn caps_keep_the_pointer_half_and_leave_the_keys_behind() {
        let masks = masks(
            &[0, 2],
            &[KEY_A, BTN_LEFT, BTN_TOUCH, 0x2c0, 0x160],
            &[0, 1, 8],
            &[0, 1, 0x2f],
        );
        let abs = vec![axis(0, 1200), axis(1, 800), axis(0x2f, 4)];
        let caps = PointerCaps::new("Combo Receiver", 0x05ac, 0x0250, &masks, abs.clone());
        assert_eq!(caps.name, "Combo Receiver (keycap pointer)");
        assert_eq!((caps.vendor, caps.product), (0x05ac, 0x0250));
        assert_eq!(caps.props, [0, 2]);
        assert_eq!(
            caps.buttons,
            [BTN_LEFT, BTN_TOUCH, 0x2c0],
            "KEY_A and KEY_OK stay on the keyboard"
        );
        assert_eq!(caps.rel, [0, 1, 8]);
        assert_eq!(caps.abs, abs);
        assert_eq!(masks.abs_axes(), [0, 1, 0x2f]);
    }

    #[test]
    fn a_twin_is_needed_for_what_the_virtual_keyboard_cannot_carry() {
        let touchpad = PointerCaps::new(
            "t",
            1,
            2,
            &masks(&[], &[BTN_TOUCH], &[], &[0]),
            vec![axis(0, 9)],
        );
        let cases: &[(&str, PointerCaps, bool)] = &[
            (
                "a keyboard alone",
                PointerCaps::new("k", 1, 2, &masks(&[], &[KEY_A], &[], &[]), vec![]),
                false,
            ),
            (
                "a mouse's motion, wheel and buttons",
                PointerCaps::new(
                    "m",
                    1,
                    2,
                    &masks(&[], &[BTN_LEFT], &[0, 1, 6, 8, 0x0b, 0x0c], &[]),
                    vec![],
                ),
                false,
            ),
            ("a touchpad", touchpad, true),
            (
                "a relative axis the keyboard does not register",
                PointerCaps::new("d", 1, 2, &masks(&[], &[], &[0, 1, REL_DIAL], &[]), vec![]),
                true,
            ),
        ];
        for (name, caps, want) in cases {
            assert_eq!(caps.needs_twin(), *want, "{name}");
        }
    }
}
