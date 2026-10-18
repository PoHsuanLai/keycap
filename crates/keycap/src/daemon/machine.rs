//! The remapper and the Caps Lock rule composed: the one place the two meet. The reader runs it on
//! the real keyboards and `keycap replay` runs it on a script, so the replay cannot drift from the
//! daemon. Pure; the reads and the writes belong to the reader, which tells the machine what it
//! wrote (`wrote`) only for what reached the wire.

use super::caps::{CapsFix, CapsKey};
use super::leds::{Lamp, Led, LedEvent};
use crate::{OutEvent, Profiles, RemapInput, RemapOut, Remapper};
use keycap_protocol::Mode;

/// The remapper and what keycap knows of the Caps Lock lock.
#[derive(Debug, Clone, Default)]
pub(crate) struct Machine {
    remapper: Remapper,
    caps: CapsFix,
}

impl Machine {
    /// Whether keys are being mapped.
    pub(crate) fn status(&self) -> Mode {
        self.remapper.status()
    }

    /// The machine after a keyboard was grabbed with its Caps Lock lamp `lamp`.
    pub(crate) fn grabbed(self, lamp: Lamp) -> Self {
        Self {
            caps: self.caps.grabbed(lamp),
            ..self
        }
    }

    /// The machine after the desktop made `changes` to the LEDs of the virtual keyboard.
    pub(crate) fn reported(self, changes: &[LedEvent]) -> Self {
        let caps = changes
            .iter()
            .filter(|change| change.led == Led::Caps)
            .fold(self.caps, |caps, change| caps.reported(change.lamp));
        Self { caps, ..self }
    }

    /// The machine after `batch` reached the virtual keyboard.
    pub(crate) fn wrote(self, batch: &[OutEvent]) -> Self {
        Self {
            caps: self.caps.wrote(batch),
            ..self
        }
    }

    /// The machine before `input` goes to the remapper, and the Caps Lock tap to write ahead of
    /// it when a stray lock is due to be cleared (empty otherwise). The tap is written before
    /// `step`, because a write that fails replaces the virtual keyboard and resets the remapper.
    pub(crate) fn settled(self, input: &RemapInput, profiles: &Profiles) -> (Self, Vec<OutEvent>) {
        let key = CapsKey::in_profile(profiles.default_profile());
        let (caps, tap) = self.caps.before(input, key);
        (Self { caps, ..self }, tap)
    }

    /// The machine after `input`, and what the remapper asks to be written. The Caps Lock presses
    /// in the outputs are noted by `wrote`, once they are written.
    pub(crate) fn step(self, input: RemapInput, profiles: &Profiles) -> (Self, Vec<RemapOut>) {
        let (remapper, outs) = self.remapper.step(input, profiles);
        (Self { remapper, ..self }, outs)
    }

    /// The machine after everything held was released. The releases are not written: this is for
    /// a new virtual keyboard that has nothing down.
    pub(crate) fn forgotten(self, profiles: &Profiles) -> Self {
        let (remapper, _unwritten) = self.remapper.step(RemapInput::ReleaseAll, profiles);
        Self { remapper, ..self }
    }
}
