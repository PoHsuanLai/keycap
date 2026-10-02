//! Layered configuration: the compiled-in defaults, the system file and the seat owner's
//! settings, each read leniently into a `ConfigLayer` and folded into `Profiles`.

mod defaults;
mod devices;
mod layer;
mod lenient;
mod load;
mod parse;
mod resolve;
#[cfg(test)]
mod tests;
mod watch;

pub(crate) use defaults::DEFAULTS_TOML;
pub use defaults::builtin;
pub use devices::{LayoutOverrides, layout_overrides};
pub use layer::{ConfigLayer, ConfigSource};
pub use load::{FILE_NAME, system};
pub use resolve::resolve;
pub use watch::{DEBOUNCE, spawn};
