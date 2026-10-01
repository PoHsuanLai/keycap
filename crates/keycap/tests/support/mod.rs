//! The scenario harness: scripts of inputs through `Remapper::step`, a model wire that rejects
//! impossible writes, and a compact rendering of what was written ("+leftctrl +c SYN").

#![allow(dead_code)]

use std::collections::BTreeSet;

use keycap::{
    CoreStatus, DeviceId, Edge, Key, Layout, Logical, Mono, OutEvent, Physical, Profiles,
    RemapInput, RemapOut, Remapper, builtin, resolve,
};
use keycap_protocol::ProfileName;

/// The shipped profiles, resolved; the built-in layer must parse without a warning.
pub fn real_profiles() -> Profiles {
    let (layer, warnings) = builtin();
    assert!(warnings.is_empty(), "built-in warnings: {warnings:?}");
    resolve(&[layer]).0
}

fn physical(name: &str) -> Physical {
    Physical(Key::named(name).unwrap_or_else(|| panic!("no key named {name:?}")))
}

fn key_input(layout: Layout, dev: u32, name: &str, edge: Edge) -> RemapInput {
    RemapInput::Key {
        device: DeviceId(dev),
        layout,
        key: physical(name),
        edge,
        at: Mono::default(),
    }
}

/// `name` pressed on the Apple keyboard `dev`.
pub fn press(dev: u32, name: &str) -> RemapInput {
    key_input(Layout::Apple, dev, name, Edge::Press)
}

/// `name` released on the Apple keyboard `dev`.
pub fn release(dev: u32, name: &str) -> RemapInput {
    key_input(Layout::Apple, dev, name, Edge::Release)
}

/// `name` auto-repeating on the Apple keyboard `dev`.
pub fn repeat(dev: u32, name: &str) -> RemapInput {
    key_input(Layout::Apple, dev, name, Edge::Repeat)
}

/// `name` pressed on the PC keyboard `dev`.
pub fn pc_press(dev: u32, name: &str) -> RemapInput {
    key_input(Layout::Pc, dev, name, Edge::Press)
}

/// `name` released on the PC keyboard `dev`.
pub fn pc_release(dev: u32, name: &str) -> RemapInput {
    key_input(Layout::Pc, dev, name, Edge::Release)
}

/// The desktop focusing the profile `name`.
pub fn focus(name: &str) -> RemapInput {
    RemapInput::Focus(ProfileName::parse(name).expect("a valid profile name"))
}

/// A keyboard unplugged.
pub fn gone(dev: u32) -> RemapInput {
    RemapInput::DeviceGone(DeviceId(dev))
}

/// The daemon's reset.
pub fn release_all() -> RemapInput {
    RemapInput::ReleaseAll
}

/// Leaving the escape pause.
pub fn resume() -> RemapInput {
    RemapInput::Resume
}

/// The outputs of one step as text: `+key` and `-key` for presses and releases, `SYN` for the
/// sync, `GRABS-OFF`, `PAUSED` and `RUNNING` for the rest, separated by `; ` between outputs.
pub fn render(outs: &[RemapOut]) -> String {
    let one = |out: &RemapOut| match out {
        RemapOut::Write(events) => events
            .iter()
            .map(render_event)
            .collect::<Vec<_>>()
            .join(" "),
        RemapOut::ReleaseGrabs => "GRABS-OFF".to_owned(),
        RemapOut::Status(CoreStatus::Paused) => "PAUSED".to_owned(),
        RemapOut::Status(CoreStatus::Running) => "RUNNING".to_owned(),
    };
    outs.iter().map(one).collect::<Vec<_>>().join("; ")
}

fn render_event(event: &OutEvent) -> String {
    match event {
        OutEvent::Key {
            key,
            edge: Edge::Press,
        } => format!("+{}", key.0.name()),
        OutEvent::Key { key, .. } => format!("-{}", key.0.name()),
        OutEvent::Sync => "SYN".to_owned(),
    }
}

/// A remapper over the real profiles, with the written stream replayed on a model wire.
pub struct Rig {
    remapper: Remapper,
    profiles: Profiles,
    wire: BTreeSet<Logical>,
}

impl Default for Rig {
    fn default() -> Self {
        Self::new()
    }
}

impl Rig {
    pub fn new() -> Self {
        Rig {
            remapper: Remapper::new(),
            profiles: real_profiles(),
            wire: BTreeSet::new(),
        }
    }

    /// Feed `input`, check the writes against the wire, return them rendered.
    pub fn feed(&mut self, input: RemapInput) -> String {
        self.feed_outs(input).1
    }

    /// Feed `input` and return the raw outputs with their rendering.
    pub fn feed_outs(&mut self, input: RemapInput) -> (Vec<RemapOut>, String) {
        self.try_feed(input).unwrap_or_else(|why| panic!("{why}"))
    }

    /// Feed `input`; an impossible write is an `Err` saying what it was.
    pub fn try_feed(&mut self, input: RemapInput) -> Result<(Vec<RemapOut>, String), String> {
        let remapper = std::mem::take(&mut self.remapper);
        let (next, outs) = remapper.step(input, &self.profiles);
        self.remapper = next;
        outs.iter().try_for_each(|out| self.apply(out))?;
        let text = render(&outs);
        Ok((outs, text))
    }

    /// Feed each input and compare its rendering with the expectation beside it.
    pub fn expect(&mut self, script: Vec<(RemapInput, &str)>) {
        for (i, (input, want)) in script.into_iter().enumerate() {
            let label = format!("{input:?}");
            assert_eq!(self.feed(input), want, "step {i}: {label}");
        }
    }

    /// The written keys down on the wire, by name, in key order.
    pub fn down(&self) -> Vec<&'static str> {
        self.wire.iter().map(|k| k.0.name()).collect()
    }

    pub fn status(&self) -> CoreStatus {
        self.remapper.status()
    }

    fn apply(&mut self, out: &RemapOut) -> Result<(), String> {
        let RemapOut::Write(events) = out else {
            return Ok(());
        };
        let Some((OutEvent::Sync, body)) = events.split_last() else {
            return Err(format!("a write must end with Sync: {events:?}"));
        };
        if body.is_empty() {
            return Err("an empty write was emitted".to_owned());
        }
        body.iter().try_for_each(|event| match *event {
            OutEvent::Key {
                key,
                edge: Edge::Press,
            } if !self.wire.insert(key) => Err(format!("pressed {key:?} while down")),
            OutEvent::Key {
                key,
                edge: Edge::Release,
            } if !self.wire.remove(&key) => Err(format!("released {key:?} while up")),
            OutEvent::Sync => Err(format!("Sync inside a write: {events:?}")),
            OutEvent::Key { .. } => Ok(()),
        })
    }
}
