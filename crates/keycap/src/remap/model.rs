//! The remapper's vocabulary and state: data and accessors only.

use std::collections::BTreeMap;

use keycap_protocol::ProfileName;

use crate::escape::Escape;
use crate::event::{DeviceId, Edge, OutEvent, Physical};
use crate::held::{At, Held};
use crate::layout::Layout;
use crate::role::{Role, Roles};
use crate::time::Mono;

/// What the daemon feeds the remapper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// A key event from a grabbed keyboard; `layout` is that keyboard's.
    Key {
        device: DeviceId,
        layout: Layout,
        key: Physical,
        edge: Edge,
        at: Mono,
    },
    /// The desktop's focused profile; it applies from the next press.
    Focus(ProfileName),
    /// A keyboard vanished: everything its keys wrote is released.
    DeviceGone(DeviceId),
    /// Release everything held (shutdown, the daemon's reset).
    ReleaseAll,
    /// Leave `Paused` after the escape chord.
    Resume,
}

/// What the remapper asks the daemon to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Out {
    /// Events for the virtual keyboard, always ending with one `OutEvent::Sync`, never empty.
    Write(Vec<OutEvent>),
    /// Give every keyboard back to the kernel.
    ReleaseGrabs,
    /// The core changed state.
    Status(CoreStatus),
}

/// Whether the remapper maps keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CoreStatus {
    #[default]
    Running,
    /// The escape chord fired: keys yield nothing until `Input::Resume`.
    Paused,
}

/// The mapping state machine.
///
/// `held` is the truth about what the wire has down. `roles` names which of the held records
/// belong to role keys; it is read through `held` and pruned with it, so it cannot list a key
/// `held` has let go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remapper {
    pub(super) held: Held,
    pub(super) roles: BTreeMap<At, Role>,
    pub(super) escape: Escape,
    pub(super) focus: ProfileName,
    pub(super) status: CoreStatus,
}

impl Remapper {
    /// Nothing held, running, focused on `default`.
    pub fn new() -> Self {
        Self {
            held: Held::new(),
            roles: BTreeMap::new(),
            escape: Escape::new(),
            focus: ProfileName::default_profile(),
            status: CoreStatus::Running,
        }
    }

    /// Whether keys are being mapped.
    pub fn status(&self) -> CoreStatus {
        self.status
    }

    /// The roles of the role keys held now.
    pub(super) fn held_roles(&self) -> Roles {
        self.roles
            .iter()
            .filter(|(at, _)| self.held.is_held(**at))
            .map(|(_, role)| *role)
            .collect()
    }
}

impl Default for Remapper {
    fn default() -> Self {
        Self::new()
    }
}
