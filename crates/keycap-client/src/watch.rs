//! Telling a program its keymap changed: the daemon's message, or, with no daemon, the file.

use crate::connection::Heard;
use crate::fetch::subscribe;
use chordkit::ChangeCallback;
use keycap_protocol::{KeymapState, ToClient};
use std::io;
use std::path::{Path, PathBuf};
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

fn fingerprint(path: &Path) -> Fingerprint {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// Starts the watch thread.
pub(crate) fn spawn(
    socket: PathBuf,
    machine_file: PathBuf,
    person_file: PathBuf,
    on_change: ChangeCallback,
) -> io::Result<Stop> {
    let stopped = Arc::new(AtomicBool::new(false));
    let seen = Arc::clone(&stopped);
    // The files are looked at before the thread starts, so a change made right after `spawn`
    // returns is a change, not the baseline.
    let files = Files {
        machine: fingerprint(&machine_file),
        person: fingerprint(&person_file),
        machine_file,
        person_file,
    };
    std::thread::Builder::new()
        .name("keycap-keymap-watch".to_owned())
        .spawn(move || run(&socket, files, &on_change, &seen))?;
    Ok(Stop(stopped))
}

/// The two keymap files and how they looked when last checked.
struct Files {
    machine_file: PathBuf,
    person_file: PathBuf,
    machine: Fingerprint,
    person: Fingerprint,
}

impl Files {
    /// Whether the person's file changed since the last look. Polled at a modest interval: the
    /// person's file lives in a home directory the daemon knows nothing about.
    fn person_changed(&mut self) -> bool {
        let now = fingerprint(&self.person_file);
        std::mem::replace(&mut self.person, now) != now
    }

    /// Whether the machine's file changed since the last look; only asked while no daemon
    /// answers, because the daemon says so itself when it does.
    fn machine_changed(&mut self) -> bool {
        let now = fingerprint(&self.machine_file);
        std::mem::replace(&mut self.machine, now) != now
    }
}

fn run(socket: &Path, mut files: Files, on_change: &ChangeCallback, stopped: &AtomicBool) {
    let mut last: Option<KeymapState> = None;
    while !stopped.load(Ordering::Relaxed) {
        match subscribe(socket) {
            Ok(connection) => {
                while !stopped.load(Ordering::Relaxed) {
                    match connection.recv(TICK) {
                        Ok(Heard::Message(ToClient::Keymap(state))) => {
                            if last.as_ref().is_some_and(|before| *before != state) {
                                on_change();
                            }
                            last = Some(state);
                        }
                        Ok(Heard::Message(_) | Heard::Silence) => {}
                        Ok(Heard::Closed) | Err(_) => break,
                    }
                    if files.person_changed() {
                        on_change();
                    }
                }
                files.machine = fingerprint(&files.machine_file);
            }
            Err(_) => {
                let machine = files.machine_changed();
                let person = files.person_changed();
                if machine || person {
                    on_change();
                }
            }
        }
        std::thread::sleep(RETRY);
    }
}
