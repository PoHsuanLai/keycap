//! The rule that says whether a node is a keyboard keycap should grab.

use super::allowance::VirtualAllowance;
use super::caps::Caps;
use crate::Key;
use crate::daemon::events::{ABS_MT_POSITION_X, ABS_X, EV_ABS, EV_KEY, EV_REL, REL_X};
use crate::daemon::{pointer, uinput};
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

/// Whether a node is a uinput device, read off its resolved (symlink-free) sysfs path: it sits
/// under `devices/virtual/input`, where uinput devices register. Another program writes such a
/// device, so grabbing it would feed that program's output back through keycap.
pub(crate) fn is_virtual(resolved: &Path) -> bool {
    let parts: Vec<_> = resolved.components().collect();
    let virtual_input = ["devices", "virtual", "input"];
    parts.windows(virtual_input.len()).any(|w| {
        w.iter()
            .map(|c| c.as_os_str())
            .eq(virtual_input.iter().map(std::ffi::OsStr::new))
    })
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
/// keyboard and pointer twins (grabbing them would feed our output back in) and palmrest's.
fn is_ours_or_palmrests(id: &Identity) -> bool {
    id.name == uinput::NAME
        || id.name.ends_with(pointer::NAME_SUFFIX)
        || (id.vendor, id.product) == (uinput::VENDOR, uinput::PRODUCT)
        || id.name.starts_with(PALMREST_PREFIX)
}

/// Whether the key mask has every letter and the space bar: what separates a keyboard from a
/// power button, a lid switch, a media remote or a mouse with a few keys.
fn has_typing_keys(caps: &Caps) -> bool {
    let has = |name: &str| Key::named(name).is_some_and(|k| caps.key.has(u32::from(k.code())));
    ('a'..='z').all(|c| has(c.encode_utf8(&mut [0; 4]))) && has("space")
}

/// Whether the node moves a pointer: a relative X axis, or a position on the absolute axes (a
/// touchpad's or a touchscreen's). A keyboard's consumer-control interface can also report
/// absolute axes (the Keychron K5 Max has `ABS_VOLUME`); those are not a pointer.
fn has_pointer_axes(caps: &Caps) -> bool {
    let relative = caps.ev.has(u32::from(EV_REL)) && caps.rel.has(u32::from(REL_X));
    let absolute = caps.ev.has(u32::from(EV_ABS))
        && [ABS_X, ABS_MT_POSITION_X]
            .iter()
            .any(|&axis| caps.abs.has(u32::from(axis)));
    relative || absolute
}

/// The rule, on capabilities, identity, whether the node is virtual and the allowance. A virtual device is never
/// grabbed, whoever made it, unless `allow` names it exactly: two remappers grabbing each
/// other's output is a feedback loop. A named allowance excludes every other device too.
/// keycap's own device and palmrest's stay excluded even
/// when named.
pub(crate) fn classify(
    caps: &Caps,
    id: &Identity,
    virtual_node: bool,
    allow: &VirtualAllowance,
) -> Eligibility {
    let keyboard = caps.ev.has(u32::from(EV_KEY)) && has_typing_keys(caps);
    let pointer = has_pointer_axes(caps);
    let refused_virtual = virtual_node && !allow.admits(&id.name);
    let excluded = refused_virtual || allow.names_another(&id.name) || is_ours_or_palmrests(id);
    match (excluded, keyboard, pointer) {
        (true, _, _) | (false, false, _) => Eligibility::NotKeyboard,
        (false, true, false) => Eligibility::Keyboard,
        (false, true, true) => Eligibility::KeyboardAndPointer,
    }
}
