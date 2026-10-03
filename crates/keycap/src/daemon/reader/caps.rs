//! A Caps Lock the user did not ask for, wired to the reader: the lamp read at a grab, the LEDs
//! the desktop sets, the Caps Lock presses keycap writes, and the tap written before the next
//! typed key. The decisions are `daemon::caps`.

use super::Reader;
use crate::daemon::caps::CapsKey;
use crate::daemon::events::encode;
use crate::daemon::leds::{Lamp, Led, LedEvent};
use crate::daemon::uinput::Emitter;
use crate::{DeviceId, OutEvent, RemapInput};

impl<E: Emitter> Reader<E> {
    /// Notes the Caps Lock lamp of the keyboard `id` just grabbed. Called after its LEDs were
    /// brought in line with the desktop's, so the lamp is the desktop's idea of the lock; it
    /// stands in only until the desktop reports the LED itself.
    pub(super) fn arm_caps(&mut self, id: DeviceId) {
        match self.nodes.leds_lit(id) {
            Ok(lit) => self.caps_grabbed(lit.lamp(Led::Caps)),
            Err(e) => eprintln!("keycap: reading the LEDs of device {} failed: {e}", id.0),
        }
    }

    /// The decision of `arm_caps`, given what it read.
    pub(super) fn caps_grabbed(&mut self, lamp: Lamp) {
        self.caps = self.caps.grabbed(lamp);
    }

    /// Follows the Caps Lock LED through the changes the desktop made on the virtual keyboard.
    pub(super) fn caps_reported(&mut self, changes: &[LedEvent]) {
        self.caps = changes
            .iter()
            .filter(|change| change.led == Led::Caps)
            .fold(self.caps, |caps, change| caps.reported(change.lamp));
    }

    /// Notes the Caps Lock presses in `batch`, just written: the user asked for that lock.
    pub(super) fn caps_wrote(&mut self, batch: &[OutEvent]) {
        self.caps = self.caps.wrote(batch);
    }

    /// Writes the tap that clears a stray lock, if `input` is the key it waits for.
    pub(super) fn settle_caps(&mut self, input: &RemapInput) {
        let key = CapsKey::in_profile(self.profiles.default_profile());
        let (caps, tap) = self.caps.before(input, key);
        self.caps = caps;
        if !tap.is_empty() {
            eprintln!(
                "keycap: turned Caps Lock off: the desktop had it on, nothing typed through \
                 keycap asked for it, and the Caps Lock key is remapped so pressing it cannot \
                 (Shift+Caps Lock sets it on purpose)"
            );
            self.write(&encode(&tap));
        }
    }
}
