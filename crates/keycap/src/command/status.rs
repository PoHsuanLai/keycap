//! `keycap status` and `keycap resume`: the daemon's socket from the command line. Status reads
//! the `Hello` every client gets; resume sends the one request that ends the escape-chord pause.

use crate::daemon::leds::Lamp;
use keycap_client::{ClientError, Connection};
use keycap_protocol::{Grab, Keyboard, Mode, Status, ToDaemon};
use std::path::Path;
use std::time::Duration;

/// The socket's file name inside the runtime directory.
const SOCKET: &str = "keycap.sock";
/// How long a reply may take; the daemon answers a connect at once.
const REPLY_WAIT: Duration = Duration::from_secs(2);

/// What the daemon said about itself.
pub(super) fn read_status(runtime_dir: &Path) -> Result<Status, String> {
    connect(runtime_dir).map(|connection| connection.status().clone())
}

/// Asks the daemon to grab the keyboards again after the escape chord. Only the seat owner is
/// obeyed, so from another account this does nothing.
pub(super) fn send_resume(runtime_dir: &Path) -> Result<(), String> {
    connect(runtime_dir)?
        .send(&ToDaemon::Resume)
        .map_err(|e| e.to_string())
}

fn connect(runtime_dir: &Path) -> Result<Connection, String> {
    Connection::open(&runtime_dir.join(SOCKET), REPLY_WAIT).map_err(|e| match e {
        ClientError::Connect { .. } => {
            format!("{e}; is the keycap service running? (systemctl status keycap)")
        }
        other => other.to_string(),
    })
}

/// A status as the lines `keycap status` prints; `caps` is the Caps Lock lamp of a keyboard,
/// when it has one, and goes on a line of its own under it.
pub(super) fn render(status: &Status, caps: impl Fn(&Keyboard) -> Option<Lamp>) -> String {
    let mode = match status.mode {
        Mode::Running => "running",
        Mode::Paused => "paused by the escape chord; run `keycap resume` to grab again",
    };
    let mut lines = vec![format!("keycap: {mode}")];
    if status.keyboards.is_empty() {
        lines.push("  no keyboards".to_owned());
    }
    for k in &status.keyboards {
        let hold = match k.grab {
            Grab::Active => "grabbed, remapping",
            Grab::WaitingForRelease => "waiting for its keys to be released",
            Grab::Conflict => "held by another remapper (Toshy, keyd, ...)",
        };
        lines.push(format!(
            "  {} ({}, {:04x}:{:04x}): {hold}",
            printable(&k.info.name),
            printable(&k.info.path),
            k.info.vendor,
            k.info.product
        ));
        lines.extend(caps(k).map(caps_line));
    }
    lines.join("\n")
}

/// `text` with each control character written out (`\u{1b}`), so a keyboard that calls itself
/// an escape sequence cannot change what the person's terminal shows.
fn printable(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if c.is_control() {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

fn caps_line(lamp: Lamp) -> String {
    match lamp {
        Lamp::Off => "    Caps Lock: off".to_owned(),
        Lamp::On => "    Caps Lock: ON (letters come out in capitals; press Shift+Caps Lock, or \
                     Caps Lock if your config does not remap it, to turn it off)"
            .to_owned(),
    }
}

/// Whether the daemon is remapping at least one keyboard: what the exit status of
/// `keycap status` says.
pub(super) fn is_remapping(status: &Status) -> bool {
    status.mode == Mode::Running && status.keyboards.iter().any(|k| k.grab == Grab::Active)
}

#[cfg(test)]
mod tests {
    use super::*;
    use keycap_protocol::KeyboardInfo;

    fn keyboard(name: &str, grab: Grab) -> Keyboard {
        Keyboard {
            info: KeyboardInfo {
                name: name.to_owned(),
                path: "/dev/input/event16".to_owned(),
                vendor: 0x3434,
                product: 0x0a53,
            },
            grab,
        }
    }

    #[test]
    fn render_table() {
        let cases: Vec<(&str, Status, &str)> = vec![
            (
                "no keyboards",
                Status {
                    mode: Mode::Running,
                    keyboards: vec![],
                },
                "keycap: running\n  no keyboards",
            ),
            (
                "grabbed",
                Status {
                    mode: Mode::Running,
                    keyboards: vec![keyboard("K5", Grab::Active)],
                },
                "keycap: running\n  K5 (/dev/input/event16, 3434:0a53): grabbed, remapping",
            ),
            (
                "conflict",
                Status {
                    mode: Mode::Running,
                    keyboards: vec![keyboard("K5", Grab::Conflict)],
                },
                "keycap: running\n  K5 (/dev/input/event16, 3434:0a53): held by another remapper (Toshy, keyd, ...)",
            ),
        ];
        for (name, status, want) in cases {
            assert_eq!(render(&status, |_| None), want, "{name}");
        }
    }

    #[test]
    fn a_device_name_cannot_carry_terminal_control_codes() {
        let status = Status {
            mode: Mode::Running,
            keyboards: vec![keyboard("Evil\u{1b}[2J\u{7}\nKB é", Grab::Active)],
        };
        let text = render(&status, |_| None);
        assert!(
            !text.chars().any(|c| c.is_control() && c != '\n'),
            "{text:?}"
        );
        assert_eq!(
            text.lines().count(),
            2,
            "the newline in the name is written out"
        );
        assert!(text.contains("Evil\\u{1b}[2J\\u{7}\\nKB é"), "{text}");
    }

    #[test]
    fn the_caps_lock_lamp_goes_under_its_keyboard() {
        let status = Status {
            mode: Mode::Running,
            keyboards: vec![
                keyboard("K5", Grab::Active),
                keyboard("Other", Grab::Active),
            ],
        };
        let lamp = |k: &Keyboard| (k.info.name == "K5").then_some(Lamp::On);
        let text = render(&status, lamp);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 4, "{text}");
        assert!(lines[2].starts_with("    Caps Lock: ON (letters come out in capitals"));
        assert!(lines[2].contains("Shift+Caps Lock"), "{}", lines[2]);
        assert!(
            lines[3].starts_with("  Other"),
            "a keyboard without a lamp gets no line"
        );
        let off = render(&status, |_| Some(Lamp::Off));
        assert_eq!(off.matches("    Caps Lock: off").count(), 2);
    }

    #[test]
    fn only_an_active_grab_in_running_mode_counts_as_remapping() {
        let one = |mode, grab| Status {
            mode,
            keyboards: vec![keyboard("K5", grab)],
        };
        assert!(is_remapping(&one(Mode::Running, Grab::Active)));
        assert!(!is_remapping(&one(Mode::Running, Grab::Conflict)));
        assert!(!is_remapping(&one(Mode::Running, Grab::WaitingForRelease)));
        assert!(!is_remapping(&one(Mode::Paused, Grab::Active)));
        assert!(!is_remapping(&Status {
            mode: Mode::Running,
            keyboards: vec![]
        }));
    }
}
