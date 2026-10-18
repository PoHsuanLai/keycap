//! keycap: the Mac keyboard mapping daemon.
//!
//! The pure remap core lives in `keycap-core` and is re-exported here, so the daemon, its tests
//! and its examples name one set of types. This crate adds what touches the system: the system
//! config file and its watcher, the daemon, and the `keycap` command.

pub mod command;
mod config;
mod daemon;
mod keymap;

pub use config::{DEBOUNCE, FILE_NAME, spawn, system};
pub use keycap_core::{
    Backoff, ChordError, ConfigLayer, ConfigSource, DEFAULTS_TOML, Decision, DeviceId, Edge,
    Escape, EscapeChord, GrabGate, Held, Hold, KEYS_DOWN_BYTES, Key, KeysDown, Layout,
    LayoutOverrides, Logical, MAX_PROFILES, ModMap, Mono, OutEvent, OutMod, OutMods, Oversize,
    Physical, Profile, Profiles, Record, RemapInput, RemapOut, Remapper, Role, Roles, RuleAction,
    Sequences, Stroke, Trigger, builtin, layout_overrides, resolve, role, within_limits,
};
