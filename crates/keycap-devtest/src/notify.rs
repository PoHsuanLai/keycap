//! A stand-in for systemd's notify socket: a datagram socket that records every `sd_notify`
//! message with the time it arrived.

use std::os::unix::net::UnixDatagram;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How long the listener blocks before it looks at the stop flag.
const TICK: Duration = Duration::from_millis(50);

/// One message the daemon sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub at: Instant,
    pub text: String,
}

/// Listens on a path until dropped.
#[derive(Debug)]
pub struct Notifications {
    notes: Arc<Mutex<Vec<Note>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Notifications {
    /// Binds `path` (replacing a stale file) and starts recording.
    pub fn listen(path: &Path) -> std::io::Result<Self> {
        let _ = std::fs::remove_file(path);
        let socket = UnixDatagram::bind(path)?;
        socket.set_read_timeout(Some(TICK))?;
        let notes = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (notes, stop) = (Arc::clone(&notes), Arc::clone(&stop));
            std::thread::Builder::new()
                .name("notify-listener".to_owned())
                .spawn(move || {
                    let mut buf = [0u8; 4096];
                    while !stop.load(Ordering::Relaxed) {
                        if let Ok(n) = socket.recv(&mut buf) {
                            let text = String::from_utf8_lossy(&buf[..n]).into_owned();
                            if let Ok(mut notes) = notes.lock() {
                                notes.push(Note {
                                    at: Instant::now(),
                                    text,
                                });
                            }
                        }
                    }
                })?
        };
        Ok(Self {
            notes,
            stop,
            thread: Some(thread),
        })
    }

    /// Every message so far, oldest first.
    pub fn all(&self) -> Vec<Note> {
        self.notes.lock().map(|n| n.clone()).unwrap_or_default()
    }

    /// The arrival times of the messages that are exactly `text`.
    pub fn times_of(&self, text: &str) -> Vec<Instant> {
        self.all()
            .into_iter()
            .filter(|n| n.text == text)
            .map(|n| n.at)
            .collect()
    }
}

impl Drop for Notifications {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch::Scratch;

    #[test]
    fn messages_are_recorded_in_order_with_arrival_times() {
        let dir = Scratch::new("notify");
        let path = dir.path().join("notify.sock");
        let listener = Notifications::listen(&path).unwrap();
        let sender = UnixDatagram::unbound().unwrap();
        for text in ["READY=1", "WATCHDOG=1", "WATCHDOG=1"] {
            sender.send_to(text.as_bytes(), &path).unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while listener.all().len() < 3 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let texts: Vec<String> = listener.all().into_iter().map(|n| n.text).collect();
        assert_eq!(texts, ["READY=1", "WATCHDOG=1", "WATCHDOG=1"]);
        assert_eq!(listener.times_of("WATCHDOG=1").len(), 2);
        assert_eq!(listener.times_of("READY=1").len(), 1);
    }
}
