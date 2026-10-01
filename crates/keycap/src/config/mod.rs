//! Layered configuration: the compiled-in defaults, the system file and the seat owner's
//! settings, each read leniently into a `Layer` and folded into `Profiles`.

mod defaults;
mod layer;
mod lenient;
mod load;
mod parse;
mod resolve;
#[cfg(test)]
mod tests;
mod watch;

pub use defaults::builtin;
pub use layer::{Layer, Source};
pub use load::{FILE_NAME, system};
pub use resolve::resolve;
pub use watch::{DEBOUNCE, spawn};
