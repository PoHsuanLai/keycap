//! Telling a program its keymap changed: the daemon's message, or, with no daemon, the file.

use crate::fetch::subscribe;
use chordkit::ChangeCallback;
use keycap_protocol::{KeymapState, ToClient};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

/// How long a receive waits before the thread looks at whether it should stop.
const TICK: Duration = Duration::from_millis(250);
/// How long before a lost or absent daemon is tried again.
const RETRY: Duration = Duration::from_millis(500);

/// Ends the watch when dropped; the thread notices within a retry or a tick.
#[derive(Debug)]
pub(crate) struct Stop(Arc<AtomicBool>);

impl Drop for Stop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// What a file looked like: its modification time and size.
type Fingerprint = Option<(SystemTime, u64)>;

fn fingerprint(path: &std::path::Path) -> Fingerprint {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// Starts the watch thread.
pub(crate) fn spawn(
    socket: PathBuf,
    keymap_file: PathBuf,
    on_change: ChangeCallback,
) -> io::Result<Stop> {
    let stopped = Arc::new(AtomicBool::new(false));
    let seen = Arc::clone(&stopped);
    std::thread::Builder::new()
        .name("keycap-keymap-watch".to_owned())
        .spawn(move || run(&socket, &keymap_file, &on_change, &seen))?;
    Ok(Stop(stopped))
}

fn run(
    socket: &std::path::Path,
    keymap_file: &std::path::Path,
    on_change: &ChangeCallback,
    stopped: &AtomicBool,
) {
    let mut last: Option<KeymapState> = None;
    let mut file = fingerprint(keymap_file);
    while !stopped.load(Ordering::Relaxed) {
        match subscribe(socket) {
            Ok(connection) => {
                while !stopped.load(Ordering::Relaxed) {
                    match connection.recv(TICK) {
                        Ok(crate::Heard::Message(ToClient::Keymap(state))) => {
                            if last.as_ref().is_some_and(|before| *before != state) {
                                on_change();
                            }
                            last = Some(state);
                        }
                        Ok(crate::Heard::Message(_) | crate::Heard::Silence) => {}
                        Ok(crate::Heard::Closed) | Err(_) => break,
                    }
                }
            }
            Err(_) => {
                let now = fingerprint(keymap_file);
                if now != file {
                    file = now;
                    on_change();
                }
            }
        }
        std::thread::sleep(RETRY);
    }
}
