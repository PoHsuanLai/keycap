//! Modifier vocabulary: input roles by position, output modifiers by written key.

mod bitset;
mod input;
mod output;
#[cfg(test)]
mod tests;

pub use input::{Role, Roles};
pub use output::{OutMod, OutMods};
