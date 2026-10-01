//! Every keyboard keycap has seen: whether it may be grabbed yet, whether it is, and when a
//! refused grab is tried again. The state machine is pure; `drive` carries its effects out.

mod drive;
mod model;
mod status;
mod step;
#[cfg(test)]
mod tests;

pub(crate) use drive::{drive, poll_gates};
pub(crate) use model::Devices;
pub(crate) use status::{status_of, summary};
