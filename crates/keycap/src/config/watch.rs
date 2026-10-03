//! Live reload: a directory watch with a 30 ms debounce.
//!
//! The directory, not the file, is watched: editors and atomic writers replace the inode by
//! rename, which a file watch would lose.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::Duration;

use keycap_protocol::Warning;
use notify::{RecursiveMode, Watcher};

use super::load::{FILE_NAME, system};
use keycap_core::ConfigLayer;

/// Quiet time after the last change before the file is re-read.
pub const DEBOUNCE: Duration = Duration::from_millis(30);

/// Whether a changed path is our file.
fn concerns_us(p: &Path) -> bool {
    p.file_name().is_some_and(|n| n == FILE_NAME)
}

/// Watches `dir` on a thread of its own and sends every settled reload of the system layer to
/// `out`. Returns the watcher, which must be kept alive; dropping it ends the thread.
pub fn spawn(
    dir: PathBuf,
    out: Sender<(ConfigLayer, Vec<Warning>)>,
) -> notify::Result<notify::RecommendedWatcher> {
    let (tx, rx) = mpsc::channel::<()>();
    let mut watcher = notify::recommended_watcher(move |ev: notify::Result<notify::Event>| {
        if let Ok(ev) = ev
            && ev.paths.iter().any(|p| concerns_us(p))
        {
            let _ = tx.send(());
        }
    })?;
    watcher.watch(&dir, RecursiveMode::NonRecursive)?;
    std::thread::Builder::new()
        .name("keycap-config".to_owned())
        .spawn(move || debounce_loop(&rx, &dir, &out))
        .map_err(notify::Error::io)?;
    Ok(watcher)
}

fn debounce_loop(rx: &mpsc::Receiver<()>, dir: &Path, out: &Sender<(ConfigLayer, Vec<Warning>)>) {
    while rx.recv().is_ok() {
        loop {
            match rx.recv_timeout(DEBOUNCE) {
                Ok(()) => continue,
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        if out.send(system(dir)).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_our_file_name_matters() {
        const CASES: &[(&str, bool)] = &[
            ("/etc/keycap/keycap.toml", true),
            ("/etc/keycap/other.toml", false),
            ("/etc/keycap/keycap.toml.swp", false),
            ("/etc/keycap/.keycap.toml.tmp", false),
        ];
        for &(path, want) in CASES {
            assert_eq!(concerns_us(Path::new(path)), want, "{path}");
        }
    }
}
