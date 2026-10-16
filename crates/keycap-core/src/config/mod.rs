//! Layered configuration over strings: the compiled-in defaults and any text a caller hands in,
//! each read leniently into a `ConfigLayer` and folded into `Profiles`. Reading the system file
//! and watching it are the daemon's (`keycap::config`).

mod confine;
mod defaults;
mod devices;
mod layer;
mod lenient;
mod limits;
mod parse;
mod resolve;
#[cfg(test)]
mod tests;

pub use confine::{MAX_USER_RULES, Sequences};
pub use defaults::{DEFAULTS_TOML, builtin};
pub use devices::{LayoutOverrides, layout_overrides};
pub use layer::{ConfigLayer, ConfigSource};
pub use limits::{MAX_INHERIT_STEPS, MAX_PROFILES, MAX_RULES_PER_PROFILE, Oversize, within_limits};
pub use resolve::resolve;
