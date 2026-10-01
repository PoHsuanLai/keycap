//! `Backoff`: exponential retry delays as a value.

mod model;
#[cfg(test)]
mod tests;

pub use model::Backoff;
