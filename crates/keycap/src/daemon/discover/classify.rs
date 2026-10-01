//! The rule that says whether a node is a keyboard keycap should grab.

use super::caps::Caps;
use crate::daemon::events::{EV_KEY, EV_REL, REL_X};
use crate::daemon::uinput;
use crate::key::Key;
use std::path::PathBuf;

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

/// What keycap does with a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Eligibility {
    /// A keyboard: grab it.
    Keyboard,
    /// A keyboard that is also a pointer: grab it and pass the pointer events through.
    KeyboardAndPointer,
    /// Anything else, including our own virtual device: leave it alone.
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

/// The rule, on capabilities and identity alone.
pub(crate) fn classify(caps: &Caps, id: &Identity) -> Eligibility {
    let keyboard = caps.ev.has(u32::from(EV_KEY)) && has_typing_keys(caps);
    let pointer = caps.ev.has(u32::from(EV_REL)) && caps.rel.has(u32::from(REL_X));
    match (is_ours_or_palmrests(id), keyboard, pointer) {
        (true, _, _) | (false, false, _) => Eligibility::NotKeyboard,
        (false, true, false) => Eligibility::Keyboard,
        (false, true, true) => Eligibility::KeyboardAndPointer,
    }
}
