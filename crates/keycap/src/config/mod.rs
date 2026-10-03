//! The system side of configuration: the system file and its watcher. The layers, their parsing
//! and their folding into `Profiles` are `keycap-core`'s, over strings.

mod load;
#[cfg(test)]
mod tests;
mod watch;

pub use load::{FILE_NAME, system};
pub use watch::{DEBOUNCE, spawn};
