//! The socket thread: who may talk to keycap, and what each of them hears.

#[cfg(test)]
pub(crate) mod client;
mod handle;
mod inbound;
mod limits;
mod seat;
mod server;
#[cfg(test)]
mod tests;

pub(crate) use handle::{SocketHandle, ToSocket};
pub(crate) use seat::{SEAT_FILE, SeatFile};
pub(crate) use server::{bind, socket_path, spawn};
