//! keycap: the Mac keyboard mapping daemon, and its pure core.
//!
//! The core is vocabulary (`Key`, `Physical`, `Logical`, `Role`, `OutMod`, `Edge`, `Mono`) and
//! the layout tables that say which physical key plays which role. Input and output speak
//! different vocabularies and the types keep them apart; see ARCHITECTURE.md section 8.

mod event;
mod key;
mod layout;
mod role;
mod time;

pub use event::{DeviceId, Edge, Logical, OutEvent, Physical};
pub use key::Key;
pub use layout::{Layout, role};
pub use role::{OutMod, OutMods, Role, Roles};
pub use time::Mono;
