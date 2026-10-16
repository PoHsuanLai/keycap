//! Live reload: a directory watch with a 30 ms debounce, for the system file, the vendor file
//! and, through `spawn_reading`, for any other file in the directory (the keymap).
//!
//! The directory, not the file, is watched: editors and atomic writers replace the inode by
//! rename, which a file watch would lose.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::Duration;

use keycap_protocol::Warning;
use notify::{RecursiveMode, Watcher};

use super::load::{FILE_NAME, system};
use super::vendor::vendor;
use keycap_core::ConfigLayer;

/// Quiet time after the last change before the file is re-read.
pub const DEBOUNCE: Duration = Duration::from_millis(30);

/// Whether a changed path is the file called `name`.
fn concerns(p: &Path, name: &str) -> bool {
    p.file_name().is_some_and(|n| n == name)
}

/// Watches `dir` on a thread of its own and sends every settled reload of the system layer to
/// `out`. Returns the watcher, which must be kept alive; dropping it ends the thread.
pub fn spawn(
    dir: PathBuf,
    out: Sender<(ConfigLayer, Vec<Warning>)>,
) -> notify::Result<notify::RecommendedWatcher> {
    spawn_reading(dir, FILE_NAME, out, system)
}

/// Like `spawn` for the vendor file in `dir`, which must exist: a folder that is not there has
/// nothing to watch and is no layer.
pub fn spawn_vendor(
    dir: PathBuf,
    out: Sender<(ConfigLayer, Vec<Warning>)>,
) -> notify::Result<notify::RecommendedWatcher> {
    spawn_reading(dir, FILE_NAME, out, vendor)
}

/// Like `spawn` for the file `name` in `dir`: `read` is called on the directory after each
/// settled change and its result sent to `out`.
pub fn spawn_reading<T: Send + 'static>(
    dir: PathBuf,
    name: &'static str,
    out: Sender<T>,
    read: fn(&Path) -> T,
) -> notify::Result<notify::RecommendedWatcher> {
    let (tx, rx) = mpsc::channel::<()>();
    let mut watcher = notify::recommended_watcher(move |ev: notify::Result<notify::Event>| {
        if let Ok(ev) = ev
            && ev.paths.iter().any(|p| concerns(p, name))
        {
            let _ = tx.send(());
        }
    })?;
    watcher.watch(&dir, RecursiveMode::NonRecursive)?;
    std::thread::Builder::new()
        .name(format!("keycap-watch-{name}"))
        .spawn(move || debounce_loop(&rx, &dir, &out, read))
        .map_err(notify::Error::io)?;
    Ok(watcher)
}

fn debounce_loop<T>(rx: &mpsc::Receiver<()>, dir: &Path, out: &Sender<T>, read: fn(&Path) -> T) {
    while rx.recv().is_ok() {
        loop {
            match rx.recv_timeout(DEBOUNCE) {
                Ok(()) => continue,
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        if out.send(read(dir)).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_named_file_matters() {
        const CASES: &[(&str, bool)] = &[
            ("/etc/keycap/keycap.toml", true),
            ("/etc/keycap/other.toml", false),
            ("/etc/keycap/keycap.toml.swp", false),
            ("/etc/keycap/.keycap.toml.tmp", false),
            ("/etc/keycap/keymap", false),
        ];
        for &(path, want) in CASES {
            assert_eq!(concerns(Path::new(path), FILE_NAME), want, "{path}");
        }
    }
}
