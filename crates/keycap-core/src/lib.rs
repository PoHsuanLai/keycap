//! keycap-core: the pure remap core of keycap.
//!
//! The core is vocabulary (`Key`, `Physical`, `Logical`, `Role`, `OutMod`, `Edge`, `Mono`) and
//! the layout tables that say which physical key plays which role. Input and output speak
//! different vocabularies and the types keep them apart; see ARCHITECTURE.md section 8.
//!
//! Nothing here opens a file, a device or a socket and nothing reads a clock: time arrives as a
//! `Mono` argument, config arrives as strings. The daemon crate `keycap` adds those, and a
//! compositor can link this crate alone. `scripts/check-boundary.sh` holds that.

mod backoff;
mod config;
mod escape;
mod event;
mod gate;
mod held;
mod key;
mod layout;
mod profile;
mod remap;
mod role;
mod time;

pub use backoff::Backoff;
pub use config::{
    ConfigLayer, ConfigSource, DEFAULTS_TOML, LayoutOverrides, MAX_INHERIT_STEPS, MAX_PROFILES,
    MAX_RULES_PER_PROFILE, MAX_USER_RULES, Oversize, Sequences, builtin, layout_overrides, resolve,
    within_limits,
};
pub use escape::{Escape, EscapeChord};
pub use event::{DeviceId, Edge, Logical, OutEvent, Physical};
pub use gate::{Decision, GrabGate, KEYS_DOWN_BYTES, KeysDown};
pub use held::{Held, Record};
pub use key::Key;
pub use layout::{Layout, role};
pub use profile::{ChordError, Hold, ModMap, Profile, Profiles, RuleAction, Stroke, Trigger};
pub use remap::{RemapInput, RemapOut, Remapper};
pub use role::{OutMod, OutMods, Role, Roles};
pub use time::Mono;
