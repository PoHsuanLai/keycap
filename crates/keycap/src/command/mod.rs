//! The `keycap` command line: the daemon and its small tools. `run` is the one way in, and the
//! only place that reads the process environment.

mod caps_lamp;
mod replay;
mod script;
mod status;

use crate::DEFAULTS_TOML;
use crate::daemon::source::Source;
use crate::daemon::uinput::{self, UINPUT};
use crate::daemon::{self, Env};
use keycap_protocol::Keyboard;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use turnstile_client::DeviceSource;

const USAGE: &str = "usage:
  keycap                       run the daemon
  keycap --print-config        print the built-in defaults.toml
  keycap --check-uinput        can this user create virtual devices? (needs
                               KEYCAP_ALLOW_UINPUT_PROBE=1; opens and closes only)
  keycap status                what the running daemon is doing (exit 0 only when a keyboard
                               is grabbed)
  keycap resume                grab the keyboards again after the escape chord
  keycap replay FILE [--config FILE]
                               replay a script of inputs through the remapper and print
                               what it writes; --config adds a user config layer
  keycap --help | --version

a replay script has one input per line, `#` starts a comment:
  <ms> press|release|repeat <dev> <apple|pc> <keyname>
  <ms> focus <profile>
  <ms> gone <dev>
  <ms> release-all
  <ms> resume
  <ms> led caps|num|scroll on|off
                               the desktop sets a LED of the virtual keyboard; a Caps Lock
                               that comes on while the Caps Lock key is remapped and nothing
                               keycap wrote asked for it is tapped off before the next key";

/// One invocation of the command.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Daemon,
    PrintConfig,
    CheckUinput,
    Status,
    Resume,
    Replay {
        file: PathBuf,
        config: Option<PathBuf>,
    },
    Help,
    Version,
}

/// The command `args` (without the program name) ask for; `None` is a usage error.
fn parse(args: &[&str]) -> Option<Command> {
    match args {
        [] | ["run"] => Some(Command::Daemon),
        ["--print-config"] => Some(Command::PrintConfig),
        ["--check-uinput"] => Some(Command::CheckUinput),
        ["status"] => Some(Command::Status),
        ["resume"] => Some(Command::Resume),
        ["replay", file] => Some(Command::Replay {
            file: PathBuf::from(file),
            config: None,
        }),
        ["replay", file, "--config", config] | ["replay", "--config", config, file] => {
            Some(Command::Replay {
                file: PathBuf::from(file),
                config: Some(PathBuf::from(config)),
            })
        }
        ["--help" | "-h"] => Some(Command::Help),
        ["--version"] => Some(Command::Version),
        _ => None,
    }
}

/// Runs the command `args` (the program name first) name, with the process environment.
pub fn run(args: impl IntoIterator<Item = String>) -> ExitCode {
    let args: Vec<String> = args.into_iter().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let env = Env::from_lookup(|k| std::env::var(k).ok());
    match parse(&args) {
        None => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
        Some(command) => match execute(command, env) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("keycap: {e}");
                ExitCode::FAILURE
            }
        },
    }
}

fn execute(command: Command, env: Env) -> Result<(), String> {
    match command {
        Command::Daemon => daemon::run(env),
        Command::PrintConfig => print_out(DEFAULTS_TOML),
        Command::CheckUinput => check_devices(&env),
        Command::Status => status::read_status(&env.runtime_dir).and_then(|s| {
            let lamp =
                |keyboard: &Keyboard| caps_lamp::lamp_of(&env.sys_input, &keyboard.info.path);
            print_out(&format!("{}\n", status::render(&s, lamp)))?;
            status::is_remapping(&s)
                .then_some(())
                .ok_or_else(|| "no keyboard is being remapped".to_owned())
        }),
        Command::Resume => status::send_resume(&env.runtime_dir),
        Command::Replay { file, config } => replay::replay_file(&file, config.as_deref())
            .and_then(|lines| print_out(&lines.join("\n")))
            .and_then(|()| print_out("\n")),
        Command::Help => print_out(&format!("{USAGE}\n")),
        Command::Version => print_out(&format!("keycap {}\n", env!("CARGO_PKG_VERSION"))),
    }
}

/// Can devices be had: from the device service when its socket is there (connecting creates
/// nothing), else from the node (opening and closing it creates nothing either).
fn check_devices(env: &Env) -> Result<(), String> {
    match Source::detect(&env.device_service) {
        Source::Service(broker) => broker
            .open()
            .map(drop)
            .map_err(|e| e.to_string())
            .and_then(|()| print_out("keycap: the device service answers; devices can be made\n")),
        _ => uinput::probe(env.probe, Path::new(UINPUT))
            .map_err(|e| e.to_string())
            .and_then(|()| {
                print_out(&format!(
                    "keycap: {UINPUT} can be opened; this user can create devices\n"
                ))
            }),
    }
}

/// Writes `text` to stdout. A closed pipe (`keycap replay f | head`) is the reader's choice,
/// not a failure.
fn print_out(text: &str) -> Result<(), String> {
    match std::io::stdout().lock().write_all(text.as_bytes()) {
        Err(e) if e.kind() != std::io::ErrorKind::BrokenPipe => Err(format!("stdout: {e}")),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argument_table() {
        let replay = |file: &str, config: Option<&str>| {
            Some(Command::Replay {
                file: PathBuf::from(file),
                config: config.map(PathBuf::from),
            })
        };
        let cases: &[(&str, &[&str], Option<Command>)] = &[
            ("no arguments run the daemon", &[], Some(Command::Daemon)),
            ("explicit run", &["run"], Some(Command::Daemon)),
            (
                "print config",
                &["--print-config"],
                Some(Command::PrintConfig),
            ),
            (
                "check uinput",
                &["--check-uinput"],
                Some(Command::CheckUinput),
            ),
            ("status", &["status"], Some(Command::Status)),
            ("resume", &["resume"], Some(Command::Resume)),
            ("status takes nothing", &["status", "x"], None),
            ("replay", &["replay", "a.txt"], replay("a.txt", None)),
            (
                "replay with a config after",
                &["replay", "a.txt", "--config", "u.toml"],
                replay("a.txt", Some("u.toml")),
            ),
            (
                "replay with a config before",
                &["replay", "--config", "u.toml", "a.txt"],
                replay("a.txt", Some("u.toml")),
            ),
            ("help", &["--help"], Some(Command::Help)),
            ("short help", &["-h"], Some(Command::Help)),
            ("version", &["--version"], Some(Command::Version)),
            ("replay needs a file", &["replay"], None),
            (
                "config needs a path",
                &["replay", "a.txt", "--config"],
                None,
            ),
            ("unknown verb", &["record", "a.txt"], None),
            ("unknown flag", &["--nope"], None),
            ("run takes nothing", &["run", "extra"], None),
        ];
        for (name, args, want) in cases {
            assert_eq!(&parse(args), want, "{name}");
        }
    }

    #[test]
    fn the_usage_names_every_verb_and_the_script_format() {
        for word in [
            "--print-config",
            "--check-uinput",
            "status",
            "resume",
            "replay",
            "--help",
            "release-all",
            "apple|pc",
        ] {
            assert!(USAGE.contains(word), "{word}");
        }
    }
}
