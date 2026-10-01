//! Carrying the state machine's effects out through `Nodes`.

use super::model::{DeviceEffect, Devices, GrabOutcome};
use crate::daemon::grabber::Nodes;
use crate::daemon::sys::GrabError;
use crate::{DeviceId, Mono};
use std::collections::VecDeque;

/// Performs `effects` and every effect their outcomes cause; returns the devices after.
pub(crate) fn drive(
    devices: Devices,
    effects: Vec<DeviceEffect>,
    nodes: &mut impl Nodes,
    now: Mono,
) -> Devices {
    let mut queue: VecDeque<DeviceEffect> = effects.into();
    let mut devices = devices;
    while let Some(effect) = queue.pop_front() {
        let (next, more) = carry_out(devices, effect, nodes, now);
        devices = next;
        queue.extend(more);
    }
    devices
}

/// Looks at the gate of every keyboard that is due, grabbing the ones with no key down. A
/// keyboard whose keys cannot be read is gone.
pub(crate) fn poll_gates(devices: Devices, nodes: &mut impl Nodes, now: Mono) -> Devices {
    devices
        .pending(now)
        .into_iter()
        .fold(devices, |devices, id| {
            let (devices, effects) = match nodes.keys_down(id) {
                Ok(keys) => devices.checked(id, keys, now),
                Err(e) => {
                    eprintln!(
                        "keycap: reading the keys down on device {} failed: {e}",
                        id.0
                    );
                    devices.gone(id)
                }
            };
            drive(devices, effects, nodes, now)
        })
}

/// Looks at the keys down right after a successful grab. A press between the gate's check and
/// the grab reached the desktop but its release would not, so such a grab is given back.
fn confirm_grab(id: DeviceId, nodes: &mut impl Nodes) -> GrabOutcome {
    match nodes.keys_down(id) {
        Ok(keys) if !keys.any() => GrabOutcome::Granted,
        Ok(_) => {
            nodes.ungrab(id);
            eprintln!(
                "keycap: a key went down on device {} as it was grabbed; waiting for its release",
                id.0
            );
            GrabOutcome::KeyDown
        }
        Err(e) => {
            nodes.ungrab(id);
            eprintln!(
                "keycap: reading the keys down on device {} after the grab failed: {e}",
                id.0
            );
            GrabOutcome::Failed
        }
    }
}

fn carry_out(
    devices: Devices,
    effect: DeviceEffect,
    nodes: &mut impl Nodes,
    now: Mono,
) -> (Devices, Vec<DeviceEffect>) {
    match effect {
        DeviceEffect::Open { id, path } => {
            match nodes.open(id, &path).and_then(|()| nodes.keys_down(id)) {
                Ok(keys) => devices.checked(id, keys, now),
                Err(e) => {
                    eprintln!("keycap: {}: {e}", path.display());
                    devices.gone(id)
                }
            }
        }
        DeviceEffect::Grab(id) => {
            let outcome = match nodes.grab(id) {
                Ok(()) => confirm_grab(id, nodes),
                Err(GrabError::Busy) => {
                    eprintln!("keycap: device {} is held by another program", id.0);
                    GrabOutcome::Busy
                }
                Err(GrabError::Other(e)) => {
                    eprintln!("keycap: grabbing device {} failed: {e}", id.0);
                    GrabOutcome::Failed
                }
            };
            devices.grab_result(id, outcome, now)
        }
        DeviceEffect::Ungrab(id) => {
            nodes.ungrab(id);
            (devices, Vec::new())
        }
        DeviceEffect::Close(id) => {
            nodes.close(id);
            (devices, Vec::new())
        }
    }
}
