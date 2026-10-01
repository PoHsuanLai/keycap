//! `Held`: exactly what each physical key's press left down, so its release can undo it.

mod model;
mod step;
#[cfg(test)]
mod tests;

pub use model::{Held, Record};
