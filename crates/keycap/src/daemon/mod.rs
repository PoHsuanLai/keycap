//! Threads, sockets, devices, clocks and the environment: everything with an effect.

pub(crate) mod clock;
pub(crate) mod devices;
pub(crate) mod discover;
pub(crate) mod env;
pub(crate) mod evdev;
pub(crate) mod events;
pub(crate) mod grabber;
pub(crate) mod inbox;
pub(crate) mod reader;
pub(crate) mod route;
#[cfg(test)]
pub(crate) mod scratch;
pub(crate) mod sdnotify;
pub(crate) mod service;
pub(crate) mod settings;
pub(crate) mod socket;
pub(crate) mod sys;
pub(crate) mod uinput;
pub(crate) mod waker;

pub(crate) use env::Env;
pub(crate) use service::run;
