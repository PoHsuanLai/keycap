//! What the reader does with each kind of message and each output of the core.

use super::Reader;
use crate::daemon::clock;
use crate::daemon::devices::drive;
use crate::daemon::discover::{Hotplug, Identity, scan};
use crate::daemon::events::{InputEvent, encode};
use crate::daemon::inbox::{Message, Request};
use crate::daemon::settings::parse_user;
use crate::daemon::socket::ToSocket;
use crate::daemon::uinput::Emitter;
use crate::{Input, Out};
use keycap_protocol::Warning;

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
            Request::Focus(profile) => self.feed(Input::Focus(profile)),
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
                let (devices, effects) = std::mem::take(&mut self.devices).resumed();
                self.devices = drive(devices, effects, &mut self.nodes, clock::now());
                self.feed(Input::Resume);
            }
        }
    }

    /// Resolves the layers again; logs what is wrong and returns it.
    fn reload(&mut self) -> Vec<Warning> {
        let (profiles, warnings) = self.layers.resolve();
        warnings.iter().for_each(log_warning);
        self.profiles = profiles;
        warnings
    }

    pub(super) fn appear(&mut self, identity: Identity) {
        let (devices, effects) = std::mem::take(&mut self.devices).appeared(identity);
        self.devices = drive(devices, effects, &mut self.nodes, clock::now());
    }

    /// One input through the remapper, and what it asks for carried out.
    pub(super) fn feed(&mut self, input: Input) {
        let (remapper, outs) = std::mem::take(&mut self.remapper).step(input, &self.profiles);
        self.remapper = remapper;
        outs.into_iter().for_each(|out| self.dispatch(out));
    }

    fn dispatch(&mut self, out: Out) {
        match out {
            Out::Write(batch) => self.write(&encode(&batch)),
            Out::ReleaseGrabs => {
                let (devices, effects) = std::mem::take(&mut self.devices).released();
                self.devices = drive(devices, effects, &mut self.nodes, clock::now());
            }
            // The status is read back from the remapper when it is published.
            Out::Status(_) => {}
        }
    }

    pub(super) fn write(&mut self, events: &[InputEvent]) {
        if let Err(e) = self.emitter.write(events) {
            eprintln!("keycap: writing to the virtual keyboard failed: {e}");
        }
    }
}
