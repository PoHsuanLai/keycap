//! A desktop's end of keycap's socket, for scripts and the device tests.
//!
//! `socket_client SOCKET status` prints the status the daemon says hello with.
//! `socket_client SOCKET focus PROFILE` sends `Focus` and prints what follows: each message,
//! then `closed` when the daemon hangs up (what it does to a client that is not the seat's
//! owner) or `open` when it stays silent for three seconds.

use keycap_client::Heard;
use keycap_devtest::{Client, summary};
use keycap_protocol::ToClient;
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

const LISTEN: Duration = Duration::from_secs(3);

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("socket_client: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[&str]) -> Result<(), String> {
    match args {
        [socket, "status"] => {
            let client = Client::connect(Path::new(socket), None).map_err(|e| e.to_string())?;
            println!("status {}", summary(client.status()));
            Ok(())
        }
        [socket, "focus", profile] => {
            let mut client = Client::connect(Path::new(socket), None).map_err(|e| e.to_string())?;
            println!("hello {}", summary(client.status()));
            client.focus(profile).map_err(|e| e.to_string())?;
            println!("{}", listen(&mut client)?);
            Ok(())
        }
        _ => Err("usage: socket_client SOCKET status | SOCKET focus PROFILE".to_owned()),
    }
}

/// Prints what the daemon sends until it hangs up (`closed`) or goes quiet (`open`).
fn listen(client: &mut Client) -> Result<&'static str, String> {
    loop {
        match client.recv(LISTEN).map_err(|e| e.to_string())? {
            Heard::Message(ToClient::Status(s)) => println!("status {}", summary(&s)),
            Heard::Message(other) => println!("message {other:?}"),
            Heard::Silence => return Ok("open"),
            Heard::Closed => return Ok("closed"),
        }
    }
}
