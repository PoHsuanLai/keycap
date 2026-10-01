//! What the reader does with each kind of message and each output of the core.

use super::Reader;
use crate::daemon::clock;
use crate::daemon::devices::{DeviceEffect, Devices, drive};
use crate::daemon::discover::{Hotplug, Identity, scan};
use crate::daemon::events::{InputEvent, encode};
use crate::daemon::inbox::{Message, Request};
use crate::daemon::leds;
use crate::daemon::settings::parse_user;
use crate::daemon::socket::ToSocket;
use crate::daemon::uinput::Emitter;
use crate::{DeviceId, RemapInput, RemapOut};
use keycap_protocol::Warning;

/// Whether the virtual keyboard still holds what was written to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Wire {
    Intact,
    /// A write failed and the device was recreated: it holds nothing, and whatever the core
    /// computed against the old state is stale.
    Recreated,
}

/// One config problem as a log line.
pub(super) fn log_warning(warning: &Warning) {
    eprintln!("keycap: config: {}: {}", warning.key, warning.problem);
}

impl<E: Emitter> Reader<E> {
    pub(super) fn handle(&mut self, message: Message) {
        match message {
            Message::Request(request) => self.request(request),
            Message::System(layer, warnings) => {
                warnings.iter().for_each(log_warning);
                self.layers = self.layers.with_system(layer);
                self.reload();
            }
            Message::Hotplug(Hotplug::Appeared(path)) => {
                let found = scan(&self.roots.sys, &self.roots.dev)
                    .into_iter()
                    .find(|c| c.identity.path == path);
                if let Some(candidate) = found {
                    self.appear(candidate.identity);
                }
            }
            Message::Hotplug(Hotplug::Gone(path)) => {
                if let Some(id) = self.devices.id_of(&path) {
                    self.lose(id);
                }
            }
        }
    }

    fn request(&mut self, request: Request) {
        match request {
            Request::Focus(profile) => {
                let (unknowns, notice) =
                    std::mem::take(&mut self.unknowns).note(&profile, &self.profiles);
                self.unknowns = unknowns;
                notice.line().iter().for_each(|line| eprintln!("{line}"));
                self.feed(RemapInput::Focus(profile));
            }
            Request::UserConfig { client, toml } => {
                let (layer, mut warnings) = parse_user(&toml);
                self.layers = self.layers.with_user(layer);
                warnings.extend(self.reload());
                self.socket.send(ToSocket::Warnings { client, warnings });
            }
            Request::ClearUserConfig => {
                self.layers = self.layers.without_user();
                self.reload();
            }
            Request::Resume => {
                let step = std::mem::take(&mut self.devices).resumed();
                self.carry_out(step);
                self.feed(RemapInput::Resume);
            }
        }
    }

    /// Resolves the layers again; logs what is wrong and returns it.
    fn reload(&mut self) -> Vec<Warning> {
        let (profiles, warnings) = self.layers.resolve();
        warnings.iter().for_each(log_warning);
        self.profiles = profiles;
        self.unknowns = std::mem::take(&mut self.unknowns).reset();
        warnings
    }

    pub(super) fn appear(&mut self, identity: Identity) {
        let step = std::mem::take(&mut self.devices).appeared(identity);
        self.carry_out(step);
    }

    /// Carries out what the device state machine asked for, then acts on any device that was
    /// grabbed or let go as a result.
    pub(super) fn carry_out(&mut self, (devices, effects): (Devices, Vec<DeviceEffect>)) {
        self.devices = drive(devices, effects, &mut self.nodes, clock::now());
        self.sync_grabs();
    }

    /// One input through the remapper, and what it asks for carried out.
    pub(super) fn feed(&mut self, input: RemapInput) {
        let (remapper, outs) = std::mem::take(&mut self.remapper).step(input, &self.profiles);
        self.remapper = remapper;
        let mut wire = Wire::Intact;
        for out in outs {
            wire = self.dispatch(out, wire);
        }
    }

    fn dispatch(&mut self, out: RemapOut, wire: Wire) -> Wire {
        match (out, wire) {
            // Computed against held keys the recreated device never had: writing it would
            // press a key nothing records.
            (RemapOut::Write(_), Wire::Recreated) => Wire::Recreated,
            (RemapOut::Write(batch), Wire::Intact) => self.write(&encode(&batch)),
            (RemapOut::ReleaseGrabs, wire) => {
                self.release_grabs();
                let (buttons, events) = std::mem::take(&mut self.buttons).release_all();
                self.buttons = buttons;
                self.release_twins();
                match self.write(&events) {
                    Wire::Intact => wire,
                    Wire::Recreated => Wire::Recreated,
                }
            }
            // The status is read back from the remapper when it is published.
            (RemapOut::Status(_), wire) => wire,
        }
    }

    /// Gives every grabbed keyboard back to the kernel.
    fn release_grabs(&mut self) {
        let step = std::mem::take(&mut self.devices).released();
        self.carry_out(step);
    }

    /// Releases every button held on a pointer twin.
    fn release_twins(&mut self) {
        for (id, e) in self.twins.release_all() {
            eprintln!(
                "keycap: releasing the pointer of device {} failed: {e}",
                id.0
            );
        }
    }

    /// Writes `events` to the virtual keyboard. A failed write leaves `Held` out of step with
    /// the wire, so the device is replaced by a fresh one, where every key is up.
    pub(super) fn write(&mut self, events: &[InputEvent]) -> Wire {
        match self.emitter.write(events) {
            Ok(()) => Wire::Intact,
            Err(e) => {
                eprintln!("keycap: writing to the virtual keyboard failed: {e}");
                self.recreate_output();
                Wire::Recreated
            }
        }
    }

    /// A new virtual keyboard and a core that believes nothing is down on it. The keyboards
    /// are given back when no new device can be made: grabbed with nowhere to write, they
    /// would leave the machine without keys.
    fn recreate_output(&mut self) {
        match self.emitter.recreate() {
            Ok(()) => eprintln!("keycap: recreated the virtual keyboard"),
            Err(e) => {
                eprintln!(
                    "keycap: cannot recreate the virtual keyboard: {e}; releasing the keyboards"
                );
                self.release_grabs();
            }
        }
        // The new device has nothing down, so what the releases would write is not written.
        let (remapper, _unwritten) =
            std::mem::take(&mut self.remapper).step(RemapInput::ReleaseAll, &self.profiles);
        self.remapper = remapper;
        self.buttons = std::mem::take(&mut self.buttons).forget();
    }

    /// `id`'s keys and buttons are released on the wire: it was unplugged, or its queue
    /// overflowed and what it holds is no longer known.
    pub(super) fn reset_device(&mut self, id: DeviceId) {
        self.feed(RemapInput::DeviceGone(id));
        let (buttons, events) = std::mem::take(&mut self.buttons).release_device(id);
        self.buttons = buttons;
        self.write(&events);
        if let Err(e) = self.twins.release(id) {
            eprintln!(
                "keycap: releasing the pointer of device {} failed: {e}",
                id.0
            );
        }
    }

    /// Copies the LED changes the desktop made on the virtual keyboard onto every grabbed
    /// physical keyboard: the kernel no longer drives their LEDs once they are grabbed.
    pub(super) fn forward_leds(&mut self) {
        let changes = self.emitter.leds();
        self.lamps = std::mem::take(&mut self.lamps).observe(&changes);
        let events: Vec<InputEvent> = changes.into_iter().map(leds::encode).collect();
        if events.is_empty() {
            return;
        }
        for id in self.devices.grabbed() {
            if let Err(e) = self.nodes.write(id, &events) {
                eprintln!("keycap: setting LEDs on device {} failed: {e}", id.0);
            }
        }
    }
}
