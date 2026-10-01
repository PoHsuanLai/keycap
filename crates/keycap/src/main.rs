//! The `keycap` command: the daemon and its tools.

use std::process::ExitCode;

fn main() -> ExitCode {
    keycap::command::run(std::env::args())
}
