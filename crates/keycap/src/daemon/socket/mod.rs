//! The socket thread: who may talk to keycap, and what each of them hears.

#[cfg(test)]
pub(crate) mod client;
mod handle;
mod inbound;
mod limits;
mod peer;
mod seat;
mod server;
mod shelf;
mod shell;
#[cfg(test)]
mod tests;

pub(crate) use handle::{SocketHandle, ToSocket};
pub(crate) use latchkey::guard::{SEAT_FILE, SeatFile};
pub(crate) use server::{bind, socket_path, spawn};
pub(crate) use shell::Steering;
