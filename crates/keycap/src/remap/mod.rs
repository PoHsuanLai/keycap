//! The remapper: key events in, written events out, composed from `Held`, `Escape` and the
//! profiles.

mod action;
mod batch;
mod model;
mod press;
mod step;
#[cfg(test)]
mod tests;

pub use model::{CoreStatus, Input, Out, Remapper};
