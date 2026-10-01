//! keycap: the Mac keyboard mapping daemon, and its pure core.
//!
//! The core is vocabulary (`Key`, `Physical`, `Logical`, `Role`, `OutMod`, `Edge`, `Mono`) and
//! the layout tables that say which physical key plays which role. Input and output speak
//! different vocabularies and the types keep them apart; see ARCHITECTURE.md section 8.

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
pub use config::{DEBOUNCE, FILE_NAME, Layer, Source, builtin, resolve, spawn, system};
pub use escape::{Escape, EscapeChord};
pub use event::{DeviceId, Edge, Logical, OutEvent, Physical};
pub use gate::{Decision, GrabGate, KeysDown};
pub use held::{Held, Record};
pub use key::Key;
pub use layout::{Layout, role};
pub use profile::{Action, ChordError, Hold, ModMap, Profile, Profiles, Stroke, Trigger};
pub use remap::{CoreStatus, Input, Out, Remapper};
pub use role::{OutMod, OutMods, Role, Roles};
pub use time::Mono;
