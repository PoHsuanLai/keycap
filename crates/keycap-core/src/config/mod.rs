//! Layered configuration over strings: the compiled-in defaults and any text a caller hands in,
//! each read leniently into a `ConfigLayer` and folded into `Profiles`. Reading the system file
//! and watching it are the daemon's (`keycap::config`).

mod defaults;
mod devices;
mod layer;
mod lenient;
mod parse;
mod resolve;
#[cfg(test)]
mod tests;

pub use defaults::{DEFAULTS_TOML, builtin};
pub use devices::{LayoutOverrides, layout_overrides};
pub use layer::{ConfigLayer, ConfigSource};
pub use resolve::resolve;
