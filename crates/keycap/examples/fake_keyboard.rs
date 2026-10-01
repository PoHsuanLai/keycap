//! A fake keyboard on `/dev/uinput`, for the systemd tier of CI.
//!
//! `fake_keyboard hold SECONDS` creates an Apple keyboard called `$KEYCAP_FAKE_NAME` (default
//! `keycap-ci-keyboard`), prints `ready <node>` and keeps it plugged in for that long. It
//! creates an input device: run it only where a throwaway keyboard is welcome.

use keycap_devtest::{FakeKeyboard, FakeSpec};
use std::io::Write;
use std::process::ExitCode;
use std::time::Duration;

const DEFAULT_NAME: &str = "keycap-ci-keyboard";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [verb, seconds] = args.as_slice() else {
        eprintln!("usage: fake_keyboard hold SECONDS");
        return ExitCode::from(2);
    };
    let Ok(seconds) = seconds.parse::<u64>() else {
        eprintln!("fake_keyboard: {seconds:?} is not a number of seconds");
        return ExitCode::from(2);
    };
    if verb != "hold" {
        eprintln!("usage: fake_keyboard hold SECONDS");
        return ExitCode::from(2);
    }
    let name = std::env::var("KEYCAP_FAKE_NAME").unwrap_or_else(|_| DEFAULT_NAME.to_owned());
    match FakeKeyboard::create(&FakeSpec::apple(&name)) {
        Ok(fake) => {
            println!("ready {}", fake.node().display());
            let _ = std::io::stdout().flush();
            std::thread::sleep(Duration::from_secs(seconds));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("fake_keyboard: {e}");
            ExitCode::FAILURE
        }
    }
}
