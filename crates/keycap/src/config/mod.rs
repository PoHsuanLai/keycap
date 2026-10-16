//! The system side of configuration: the system file, the vendor file under it, and their
//! watchers. The layers, their parsing and their folding into `Profiles` are `keycap-core`'s,
//! over strings.

mod load;
#[cfg(test)]
mod tests;
mod vendor;
mod watch;

pub use load::{FILE_NAME, system};
pub use vendor::{VENDOR_DIR, vendor};
pub use watch::{DEBOUNCE, spawn, spawn_reading, spawn_vendor};
