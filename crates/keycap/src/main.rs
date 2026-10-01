//! Stub entry point; a later wave replaces it with `keycap::command::run`.

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!("usage: keycap [run | --print-config | replay FILE]");
    ExitCode::from(2)
}
