//! The data of the device state machine.

use crate::daemon::discover::Identity;
use crate::{Backoff, DeviceId, Layout, Mono};
use keycap_protocol::Grab;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// When a keyboard may next be checked for grabbing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Retry {
    /// Any time; the backoff is what a refusal would wait.
    Ready(Backoff),
    /// After a refused grab: not before `until`.
    Waiting { until: Mono, backoff: Backoff },
}

impl Retry {
    pub(super) fn backoff(self) -> Backoff {
        match self {
            Retry::Ready(backoff) | Retry::Waiting { backoff, .. } => backoff,
        }
    }
}

/// One keyboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Tracked {
    pub(super) identity: Identity,
    pub(super) layout: Layout,
    /// Whether the gate has let the device through to be grabbed.
    pub(super) past_gate: bool,
    pub(super) grab: Grab,
    pub(super) retry: Retry,
}

/// All keyboards, by the id keycap gave them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Devices {
    pub(super) tracked: BTreeMap<DeviceId, Tracked>,
    pub(super) next: u32,
    /// The escape chord gave the keyboards back; they stay free until `Resume`. Otherwise
    /// keycap wants them grabbed.
    pub(super) released: bool,
    /// Layouts the config assigns by name or id, over what the vendor says.
    pub(super) overrides: crate::LayoutOverrides,
}

/// What the state machine asks the reader to do with a device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeviceEffect {
    /// Open the node and then check the keys down on it.
    Open {
        id: DeviceId,
        path: PathBuf,
    },
    Grab(DeviceId),
    Ungrab(DeviceId),
    /// Close the node; the kernel drops the grab with it.
    Close(DeviceId),
}

/// How a grab attempt ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GrabOutcome {
    Granted,
    /// `EBUSY`: another remapper holds the device.
    Busy,
    /// The grab succeeded but a key was down right after it: the desktop saw that press, so
    /// the grab was given back and must wait for the release.
    KeyDown,
    /// Any other failure.
    Failed,
}
