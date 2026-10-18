//! Finding input nodes by the name their device registered under.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Where input class devices are listed.
const SYS_INPUT: &str = "/sys/class/input";
/// Where their nodes are.
const DEV_INPUT: &str = "/dev/input";
/// How often a wait looks again.
pub(crate) const POLL: Duration = Duration::from_millis(10);

/// The name keycap's virtual keyboard registers under.
pub const OUTPUT_NAME: &str = "keycap keyboard";

/// `event<N>` as a number, for ordering.
fn event_number(name: &str) -> Option<u32> {
    name.strip_prefix("event")?.parse().ok()
}

/// Every node under `class_dir` (an `eventN` entry per node, here or one level down) whose
/// device is called `name`, as `(N, node path)`, oldest first.
fn nodes_in(class_dir: &Path, dev_dir: &Path, name: &str) -> Vec<(u32, PathBuf)> {
    let mut found: Vec<(u32, PathBuf)> = std::fs::read_dir(class_dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|e| {
            let file = e.file_name().into_string().ok()?;
            let n = event_number(&file)?;
            let registered = std::fs::read_to_string(e.path().join("device/name")).ok()?;
            (registered.trim() == name).then(|| (n, dev_dir.join(file)))
        })
        .collect();
    found.sort();
    found
}

/// The newest node whose device is called `name`, if its device node exists.
pub(crate) fn find_node(name: &str) -> Option<PathBuf> {
    nodes_in(Path::new(SYS_INPUT), Path::new(DEV_INPUT), name)
        .into_iter()
        .rev()
        .map(|(_, path)| path)
        .find(|path| path.exists())
}

/// The node of the input device `sysname` (`inputN`), if it has appeared.
pub(crate) fn node_of_sysname(sysname: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(Path::new(SYS_INPUT).join(sysname)).ok()?;
    entries
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|f| event_number(f).is_some())
        .map(|f| Path::new(DEV_INPUT).join(f))
        .find(|path| path.exists())
}

/// Whether the device `sysname` is still registered.
pub(crate) fn registered(sysname: &str) -> bool {
    Path::new(SYS_INPUT).join(sysname).exists()
}

/// Calls `probe` every `POLL` until it returns `Some` or `timeout` passes.
pub(crate) fn wait_for<T>(timeout: Duration, mut probe: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(found) = probe() {
            return Some(found);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(POLL);
    }
}

/// Whether any node is currently registered under `name`.
pub(crate) fn name_registered(name: &str) -> bool {
    !nodes_in(Path::new(SYS_INPUT), Path::new(DEV_INPUT), name).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch::Scratch;

    fn node(sys: &Path, entry: &str, name: &str) {
        let dir = sys.join(entry).join("device");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("name"), format!("{name}\n")).unwrap();
    }

    #[test]
    fn nodes_are_found_by_exact_name_oldest_first() {
        let root = Scratch::new("sysfs-find");
        let sys = root.path().join("sys");
        node(&sys, "event10", "kb");
        node(&sys, "event2", "kb");
        node(&sys, "event3", "kb two");
        node(&sys, "mouse0", "kb");
        node(&sys, "eventX", "kb");
        let found = nodes_in(&sys, Path::new("/dev/input"), "kb");
        assert_eq!(
            found,
            [
                (2, PathBuf::from("/dev/input/event2")),
                (10, PathBuf::from("/dev/input/event10")),
            ]
        );
    }

    #[test]
    fn wait_for_returns_the_first_answer_or_none_at_the_deadline() {
        let mut calls = 0;
        let got = wait_for(Duration::from_secs(2), || {
            calls += 1;
            (calls == 3).then_some(calls)
        });
        assert_eq!(got, Some(3));
        assert_eq!(wait_for(Duration::from_millis(30), || None::<u8>), None);
    }
}
