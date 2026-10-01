//! The rule that says whether a node is a keyboard keycap should grab.

use super::allowance::VirtualAllowance;
use super::caps::Caps;
use crate::daemon::events::{EV_KEY, EV_REL, REL_X};
use crate::daemon::uinput;
use crate::key::Key;
use std::path::{Path, PathBuf};

/// Name prefix of palmrest's virtual devices, which keycap must never grab.
const PALMREST_PREFIX: &str = "palmrest";

/// Who a node says it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Identity {
    pub name: String,
    pub vendor: u16,
    pub product: u16,
    pub path: PathBuf,
}

/// Where a node comes from, read off its resolved sysfs path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Origin {
    /// A device behind a bus: USB, Bluetooth, PS/2, I2C.
    Hardware,
    /// A uinput device, which another program writes: grabbing it would feed that program's
    /// output back through keycap.
    Virtual,
}

impl Origin {
    /// The origin of the node whose resolved (symlink-free) sysfs path is `resolved`: virtual
    /// when it sits under `devices/virtual/input`, which is where uinput devices register.
    pub(crate) fn of_sysfs(resolved: &Path) -> Self {
        let parts: Vec<_> = resolved.components().collect();
        let virtual_input = ["devices", "virtual", "input"];
        let under = parts.windows(virtual_input.len()).any(|w| {
            w.iter()
                .map(|c| c.as_os_str())
                .eq(virtual_input.iter().map(std::ffi::OsStr::new))
        });
        if under {
            Origin::Virtual
        } else {
            Origin::Hardware
        }
    }
}

/// What keycap does with a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Eligibility {
    /// A keyboard: grab it.
    Keyboard,
    /// A keyboard that is also a pointer: grab it and pass the pointer events through.
    KeyboardAndPointer,
    /// Anything else, including every virtual device: leave it alone.
    NotKeyboard,
}

/// Whether the node is one of the virtual devices that must never be grabbed: keycap's own
/// (grabbing it would feed our output back in) and palmrest's.
fn is_ours_or_palmrests(id: &Identity) -> bool {
    id.name == uinput::NAME
        || (id.vendor, id.product) == (uinput::VENDOR, uinput::PRODUCT)
        || id.name.starts_with(PALMREST_PREFIX)
}

/// Whether the key mask has every letter and the space bar: what separates a keyboard from a
/// power button, a lid switch, a media remote or a mouse with a few keys.
fn has_typing_keys(caps: &Caps) -> bool {
    let has = |name: &str| Key::named(name).is_some_and(|k| caps.key.has(u32::from(k.code())));
    ('a'..='z').all(|c| has(c.encode_utf8(&mut [0; 4]))) && has("space")
}

/// The rule, on capabilities, identity, origin and the allowance. A virtual device is never
/// grabbed, whoever made it, unless `allow` names it exactly: two remappers grabbing each
/// other's output is a feedback loop. keycap's own device and palmrest's stay excluded even
/// when named.
pub(crate) fn classify(
    caps: &Caps,
    id: &Identity,
    origin: Origin,
    allow: &VirtualAllowance,
) -> Eligibility {
    let keyboard = caps.ev.has(u32::from(EV_KEY)) && has_typing_keys(caps);
    let pointer = caps.ev.has(u32::from(EV_REL)) && caps.rel.has(u32::from(REL_X));
    let refused_virtual = origin == Origin::Virtual && !allow.admits(&id.name);
    let excluded = refused_virtual || is_ours_or_palmrests(id);
    match (excluded, keyboard, pointer) {
        (true, _, _) | (false, false, _) => Eligibility::NotKeyboard,
        (false, true, false) => Eligibility::Keyboard,
        (false, true, true) => Eligibility::KeyboardAndPointer,
    }
}
