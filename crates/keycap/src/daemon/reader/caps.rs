//! The one-time check of a Caps Lock left on, wired to the reader: read the lamp at the first
//! grab, write the tap before the first typed key. The decisions are `daemon::caps`.

use super::Reader;
use crate::daemon::caps::{CapsFix, CapsKey};
use crate::daemon::leds::Led;
use crate::daemon::uinput::Emitter;
use crate::{DeviceId, Edge, RemapInput};

impl<E: Emitter> Reader<E> {
    /// Looks at Caps Lock on the keyboard `id` just grabbed. Called after its LEDs were brought
    /// in line with the desktop's, so the lamp is the desktop's idea of the lock.
    pub(super) fn arm_caps(&mut self, id: DeviceId) {
        let lamp = match self.nodes.leds_lit(id) {
            Ok(lit) => lit.lamp(Led::Caps),
            Err(e) => {
                eprintln!("keycap: reading the LEDs of device {} failed: {e}", id.0);
                return;
            }
        };
        let key = CapsKey::in_profile(self.profiles.default_profile());
        self.arm_caps_with(lamp, key);
    }

    /// The decision of `arm_caps`, given what it read.
    pub(super) fn arm_caps_with(&mut self, lamp: crate::daemon::leds::Lamp, key: CapsKey) {
        let before = self.caps;
        self.caps = before.grabbed(lamp, key);
        if before != CapsFix::Armed && self.caps == CapsFix::Armed {
            eprintln!(
                "keycap: Caps Lock is on and the Caps Lock key is remapped, so it cannot be \
                 turned off by pressing it; keycap taps it once before the first key you type"
            );
        }
    }

    /// Writes the tap that clears the lock, if one is armed and `input` is the key it waits for.
    pub(super) fn settle_caps(&mut self, input: &RemapInput) {
        let RemapInput::Key {
            layout,
            key,
            edge: Edge::Press,
            ..
        } = input
        else {
            return;
        };
        let (caps, tap) = self.caps.pressed(*layout, *key);
        self.caps = caps;
        if !tap.is_empty() {
            eprintln!("keycap: turned Caps Lock off (it was on when keycap took over)");
            self.write(&tap);
        }
    }
}
