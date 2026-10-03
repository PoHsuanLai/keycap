//! Keyboard layouts and what each modifier position means on them.

mod model;
mod role;
#[cfg(test)]
mod tests;

pub use model::Layout;
pub use role::role;
