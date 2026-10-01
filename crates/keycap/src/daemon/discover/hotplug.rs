//! Hotplug: a watch on `/dev/input` that reports `event*` nodes appearing and going.

use notify::event::{EventKind, ModifyKind, RenameMode};
use notify::{RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

/// A device node came or went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Hotplug {
    Appeared(PathBuf),
    Gone(PathBuf),
}

/// Whether a path is an evdev node (`eventN`), the only names that matter in `/dev/input`
/// and `/sys/class/input`.
pub(crate) fn concerns_us(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_prefix("event"))
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

/// What a filesystem event means for one path. A permission change counts as an appearance:
/// udev creates a node as root-only and opens it to the `input` group a moment later, so the
/// first sight may not yet be readable. The consumer treats a repeated `Appeared` as a no-op.
pub(crate) fn hotplug_of(kind: &EventKind, path: &Path) -> Option<Hotplug> {
    if !concerns_us(path) {
        return None;
    }
    let path = path.to_path_buf();
    match kind {
        EventKind::Create(_)
        | EventKind::Modify(ModifyKind::Metadata(_) | ModifyKind::Name(RenameMode::To)) => {
            Some(Hotplug::Appeared(path))
        }
        EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
            Some(Hotplug::Gone(path))
        }
        _ => None,
    }
}

/// Watches `dir` (not its subdirectories) and sends each device node coming or going to `out`.
/// Returns the watcher, which must be kept alive.
pub(crate) fn spawn(
    dir: &Path,
    out: Sender<Hotplug>,
) -> notify::Result<notify::RecommendedWatcher> {
    let mut watcher = notify::recommended_watcher(move |ev: notify::Result<notify::Event>| {
        if let Ok(ev) = ev {
            ev.paths
                .iter()
                .filter_map(|p| hotplug_of(&ev.kind, p))
                .for_each(|h| {
                    let _ = out.send(h);
                });
        }
    })?;
    watcher.watch(dir, RecursiveMode::NonRecursive)?;
    Ok(watcher)
}
