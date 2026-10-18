//! What the devices look like to a desktop.

use super::model::Devices;
use keycap_protocol::{Grab, Keyboard, KeyboardInfo, Mode, Status};

/// The status a desktop sees: the remapper's mode and each keyboard's grab, by id.
pub(crate) fn status_of(devices: &Devices, mode: Mode) -> Status {
    Status {
        mode,
        keyboards: devices
            .tracked
            .values()
            .map(|t| Keyboard {
                info: KeyboardInfo {
                    name: t.identity.name.clone(),
                    path: t.identity.path.display().to_string(),
                    vendor: t.identity.vendor,
                    product: t.identity.product,
                },
                grab: t.grab,
            })
            .collect(),
    }
}

/// The one line `systemctl status` shows.
pub(crate) fn summary(status: &Status) -> String {
    let count = |grab: Grab| status.keyboards.iter().filter(|k| k.grab == grab).count();
    match status.mode {
        Mode::Paused => "paused: keyboards released".to_owned(),
        Mode::Running => format!(
            "{} grabbed, {} waiting, {} in conflict",
            count(Grab::Active),
            count(Grab::WaitingForRelease),
            count(Grab::Conflict)
        ),
    }
}
