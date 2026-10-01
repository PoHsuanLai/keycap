//! Threads, sockets, devices, clocks and the environment: everything with an effect.
//!
//! Nothing here is wired into the binary yet, so items are unused outside their tests until the
//! reader, socket and `command::run` modules call them.
#![allow(dead_code, unused_imports)]

pub(crate) mod discover;
pub(crate) mod env;
pub(crate) mod events;
pub(crate) mod grabber;
#[cfg(test)]
pub(crate) mod scratch;
pub(crate) mod sdnotify;
pub(crate) mod sys;
pub(crate) mod uinput;
