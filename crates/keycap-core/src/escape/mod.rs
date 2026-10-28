//! The escape chord: both Shift keys held, then Esc pressed (physical keys).

mod model;
mod step;
#[cfg(test)]
mod tests;

pub use model::Escape;
