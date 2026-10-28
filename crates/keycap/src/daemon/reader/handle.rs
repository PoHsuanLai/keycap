//! What the reader does with each kind of message and each output of the core.

use super::Reader;
use super::rescan::Lookout;
use crate::daemon::clock;
use crate::daemon::devices::{DeviceEffect, Devices, drive};
use crate::daemon::discover::{Hotplug, Identity, scan};
use crate::daemon::events::{InputEvent, encode};
use crate::daemon::inbox::{Message, Request};
use crate::daemon::leds;
use crate::daemon::settings::Layers;
use crate::daemon::socket::ToSocket;
use crate::daemon::uinput::Emitter;
use crate::{DeviceId, RemapInput, RemapOut};
use keycap_protocol::Warning;

/// One config problem as a log line.
pub(super) fn log_warning(warning: &Warning) {
    eprintln!("keycap: config: {}: {}", warning.key, warning.problem);
}

impl<E: Emitter> Reader<E> {
    pub(super) fn handle(&mut self, message: Message) {
        match message {
            Message::Request(request) => self.request(request),
            Message::Vendor(layer, warnings) => {
                warnings.iter().for_each(log_warning);
                let changed = self.layers.with_vendor(layer);
                self.take_up(changed);
            }
            Message::System(layer, warnings) => {
                warnings.iter().for_each(log_warning);
                let changed = self.layers.with_system(layer);
                self.take_up(changed);
            }
            Message::Hotplug(Hotplug::Appeared(path)) => {
                self.looking = std::mem::take(&mut self.looking).appeared(path);
            }
            Message::Hotplug(Hotplug::Rescan) => {
                eprintln!("keycap: device events were missed; looking at every keyboard again");
                self.looking = std::mem::take(&mut self.looking).everything();
            }
            Message::Hotplug(Hotplug::Gone(path)) => {
                if let Some(id) = self.devices.id_of(&path) {
                    self.lose(id);
                }
            }
        }
    }

    /// Puts `changed` in force if it folds within the limits; otherwise says why not and leaves
    /// the layers as they were.
    fn take_up(&mut self, changed: Layers) {
        match changed.accepted() {
            Ok(layers) => {
                self.layers = layers;
                self.reload();
            }
            Err(over) => log_warning(&over.warning()),
        }
    }

    fn request(&mut self, request: Request) {
        match request {
            Request::Focus(profile) => {
                let (unknowns, notice) =
                    std::mem::take(&mut self.unknowns).note(&profile, &self.profiles);
                self.unknowns = unknowns;
                notice
                    .iter()
                    .for_each(|notice| eprintln!("{}", notice.line()));
                // A name the settings do not define writes as `default` (profiles.resolve), is
                // logged once, and is taken up if a later config defines it.
                self.feed(RemapInput::Focus(profile));
            }
            Request::UserConfig { client, toml } => {
                let (layer, mut warnings) = match self.layers.offered(&toml) {
                    Ok(offer) => offer,
                    Err(over) => {
                        let refusal = over.warning();
                        log_warning(&refusal);
                        self.socket.send(ToSocket::Warnings {
                            client,
                            warnings: vec![refusal],
                        });
                        return;
                    }
                };
                warnings.iter().for_each(log_warning);
                match self.layers.with_user(layer).accepted() {
                    Ok(layers) => {
                        self.layers = layers;
                        warnings.extend(self.reload());
                    }
                    Err(over) => {
                        let refusal = over.warning();
                        log_warning(&refusal);
                        warnings.push(refusal);
                    }
                }
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
        self.devices = std::mem::take(&mut self.devices).with_overrides(self.layers.overrides());
        self.unknowns = std::mem::take(&mut self.unknowns).reset();
        warnings
    }

    /// Looks at the nodes the messages of this pass said to: one scan for the whole burst.
    /// Keyboards already known are left as they are.
    pub(super) fn look(&mut self) {
        let looking = std::mem::take(&mut self.looking);
        if looking == Lookout::Nothing {
            return;
        }
        for candidate in scan(
            &self.roots.sys,
            &self.roots.dev,
            &self.roots.udev,
            &self.roots.allow,
        ) {
            if looking.wants(&candidate.identity.path) {
                self.appear(candidate.identity);
            }
        }
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

    /// One input through the remapper, and what it asks for carried out: the Caps Lock tap first
    /// when a stray lock is due to be cleared.
    pub(super) fn feed(&mut self, input: RemapInput) {
        let (machine, tap) = std::mem::take(&mut self.machine).settled(&input, &self.profiles);
        self.machine = machine;
        self.write_caps_tap(&tap);
        let (machine, outs) = std::mem::take(&mut self.machine).step(input, &self.profiles);
        self.machine = machine;
        let mut recreated = false;
        for out in outs {
            recreated = self.dispatch(out, recreated);
        }
    }

    /// Carries out `out`; `recreated` is whether an earlier write of this input replaced the
    /// device, and the answer whether one has by now.
    fn dispatch(&mut self, out: RemapOut, recreated: bool) -> bool {
        match (out, recreated) {
            // Computed against held keys the recreated device never had: writing it would
            // press a key nothing records.
            (RemapOut::Write(_), true) => true,
            (RemapOut::Write(batch), false) => {
                self.caps_wrote(&batch);
                self.write(&encode(&batch))
            }
            (RemapOut::ReleaseGrabs, recreated) => {
                self.release_grabs();
                let (buttons, events) = std::mem::take(&mut self.buttons).release_all();
                self.buttons = buttons;
                self.release_twins();
                self.write(&events) || recreated
            }
            // The status is read back from the remapper when it is published.
            (RemapOut::Status(_), recreated) => recreated,
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
    /// the wire, so the device is replaced by a fresh one, where every key is up. True when it
    /// was replaced: it holds nothing, and whatever the core computed against the old state is
    /// stale.
    pub(super) fn write(&mut self, events: &[InputEvent]) -> bool {
        match self.emitter.write(events) {
            Ok(()) => false,
            Err(e) => {
                eprintln!("keycap: writing to the virtual keyboard failed: {e}");
                self.recreate_output();
                true
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
        self.machine = std::mem::take(&mut self.machine).forgotten(&self.profiles);
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
        self.caps_reported(&changes);
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
