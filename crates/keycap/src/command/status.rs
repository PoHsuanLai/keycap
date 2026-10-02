//! `keycap status` and `keycap resume`: the daemon's socket from the command line. Status reads
//! the `Hello` every client gets; resume sends the one request that ends the escape-chord pause.

use keycap_protocol::{self as protocol, Grab, Mode, PROTOCOL_VERSION, Status, ToClient, ToDaemon};
use rustix::io::Errno;
use rustix::net::sockopt::{Timeout, set_socket_timeout};
use rustix::net::{AddressFamily, RecvFlags, SendFlags, SocketAddrUnix, SocketType};
use std::path::Path;
use std::time::Duration;

/// The socket's file name inside the runtime directory.
const SOCKET: &str = "keycap.sock";
/// How long a reply may take; the daemon answers a connect at once.
const REPLY_WAIT: Duration = Duration::from_secs(2);

/// What the daemon said about itself.
pub(super) fn read_status(runtime_dir: &Path) -> Result<Status, String> {
    let fd = connect(runtime_dir)?;
    set_socket_timeout(&fd, Timeout::Recv, Some(REPLY_WAIT)).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; protocol::MAX_PACKET];
    let n = match rustix::net::recv(&fd, &mut buf, RecvFlags::empty()) {
        Ok((0, _)) => return Err("the daemon closed the connection".to_owned()),
        Ok((n, _)) => n,
        Err(Errno::AGAIN) => return Err("the daemon sent no Hello".to_owned()),
        Err(e) => return Err(format!("receiving: {e}")),
    };
    match protocol::decode::<ToClient>(&buf[..n]).map_err(|e| e.to_string())? {
        ToClient::Hello { version, status } if version == PROTOCOL_VERSION => Ok(status),
        other => Err(format!("unexpected first packet: {other:?}")),
    }
}

/// Asks the daemon to grab the keyboards again after the escape chord. Only the seat owner is
/// obeyed, so from another account this does nothing.
pub(super) fn send_resume(runtime_dir: &Path) -> Result<(), String> {
    let fd = connect(runtime_dir)?;
    rustix::net::send(
        &fd,
        &protocol::encode(&ToDaemon::Resume),
        SendFlags::NOSIGNAL,
    )
    .map(drop)
    .map_err(|e| format!("sending: {e}"))
}

fn connect(runtime_dir: &Path) -> Result<rustix::fd::OwnedFd, String> {
    let path = runtime_dir.join(SOCKET);
    let fd = rustix::net::socket(AddressFamily::UNIX, SocketType::SEQPACKET, None)
        .map_err(|e| format!("creating a socket: {e}"))?;
    let addr = SocketAddrUnix::new(&path).map_err(|e| e.to_string())?;
    rustix::net::connect(&fd, &addr).map_err(|e| {
        format!(
            "cannot connect to {}: {e}; is the keycap service running? (systemctl status keycap)",
            path.display()
        )
    })?;
    Ok(fd)
}

/// A status as the lines `keycap status` prints.
pub(super) fn render(status: &Status) -> String {
    let mode = match status.mode {
        Mode::Running => "running",
        Mode::Paused => "paused by the escape chord; run `keycap resume` to grab again",
    };
    let mut lines = vec![format!("keycap: {mode}")];
    if status.keyboards.is_empty() {
        lines.push("  no keyboards".to_owned());
    }
    lines.extend(status.keyboards.iter().map(|k| {
        let hold = match k.grab {
            Grab::Active => "grabbed, remapping",
            Grab::WaitingForRelease => "waiting for its keys to be released",
            Grab::Conflict => "held by another remapper (Toshy, keyd, ...)",
        };
        format!(
            "  {} ({}, {:04x}:{:04x}): {hold}",
            k.info.name, k.info.path, k.info.vendor, k.info.product
        )
    }));
    lines.join("\n")
}

/// Whether the daemon is remapping at least one keyboard: what the exit status of
/// `keycap status` says.
pub(super) fn is_remapping(status: &Status) -> bool {
    status.mode == Mode::Running && status.keyboards.iter().any(|k| k.grab == Grab::Active)
}

#[cfg(test)]
mod tests {
    use super::*;
    use keycap_protocol::{Keyboard, KeyboardInfo};

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
            assert_eq!(render(&status), want, "{name}");
        }
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
