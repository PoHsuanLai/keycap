//! What happens to a device the moment it is grabbed or let go: its LEDs are brought in line
//! with the desktop's and, for a combo device, its pointer twin is made or destroyed.

use super::Reader;
use super::twins::grab_change;
use crate::DeviceId;
use crate::daemon::uinput::Emitter;
use std::collections::BTreeSet;

impl<E: Emitter> Reader<E> {
    /// Compares the devices grabbed now with the ones seen grabbed last, and acts on the
    /// difference. Called after every step that can grab or release a device.
    pub(super) fn sync_grabs(&mut self) {
        let now: BTreeSet<DeviceId> = self.devices.grabbed().into_iter().collect();
        let change = grab_change(&self.held, &now);
        for id in change.given {
            if let Err(e) = self.twins.retire(id) {
                eprintln!(
                    "keycap: releasing the pointer of device {} failed: {e}",
                    id.0
                );
            }
        }
        for id in change.taken {
            self.light(id);
            self.arm_caps(id);
            self.make_twin(id);
        }
        self.held = now;
    }

    /// Writes the lamps the desktop last set to the freshly grabbed `id`: the kernel stops
    /// driving them once the device is grabbed, and the desktop only tells keycap about changes.
    fn light(&self, id: DeviceId) {
        let events = self.lamps.events();
        if events.is_empty() {
            return;
        }
        if let Err(e) = self.nodes.write(id, &events) {
            eprintln!("keycap: setting LEDs on device {} failed: {e}", id.0);
        }
    }

    /// A pointer twin for `id` when the keyboard cannot carry its pointer. Without one the
    /// pointer events still go through the keyboard, which drops what it has no axis for.
    fn make_twin(&mut self, id: DeviceId) {
        let Some(identity) = self.devices.identity(id) else {
            return;
        };
        let caps = match self.nodes.pointer_caps(id, identity) {
            Ok(caps) if caps.needs_twin() => caps,
            Ok(_) => return,
            Err(e) => {
                eprintln!("keycap: reading the pointer of device {} failed: {e}", id.0);
                return;
            }
        };
        match self.emitter.twin(&caps) {
            Ok(twin) => {
                eprintln!("keycap: created \"{}\"", caps.name);
                self.twins.insert(id, twin);
            }
            Err(e) => eprintln!("keycap: cannot create \"{}\": {e}", caps.name),
        }
    }
}
