//! `/proc` as keycap's unit can read it, for peephole's prober.

use peephole::{ProcFs, RealProc};
use peephole_core::FileTrust;
use std::io;
use std::path::{Path, PathBuf};

/// Reads `/proc` as a process without privileges can: `<pid>/root` of another user's process
/// is behind a check it does not pass, which only means the process's sandbox cannot be
/// looked at. That is "not found" to the prober, as for a process with no sandbox; a
/// sandboxed program has a control group of its own and never the shell's.
#[derive(Debug)]
pub(crate) struct Procs<P = RealProc>(pub P);

/// The proc-relative file that tells a Flatpak sandbox.
const SANDBOX_MARK: &str = "root/.flatpak-info";

impl<P: ProcFs> ProcFs for Procs<P> {
    fn read_capped(&self, rel: &Path, cap: u64) -> io::Result<Vec<u8>> {
        self.0.read_capped(rel, cap).map_err(|e| match e.kind() {
            io::ErrorKind::PermissionDenied if rel.ends_with(SANDBOX_MARK) => {
                io::ErrorKind::NotFound.into()
            }
            _ => e,
        })
    }

    fn readlink(&self, rel: &Path) -> io::Result<PathBuf> {
        self.0.readlink(rel)
    }

    fn trust_of(&self, abs: &Path) -> io::Result<FileTrust> {
        self.0.trust_of(abs)
    }

    fn trust_of_followed(&self, rel: &Path) -> io::Result<FileTrust> {
        self.0.trust_of_followed(rel)
    }

    fn read_file_capped(&self, abs: &Path, cap: u64) -> io::Result<Vec<u8>> {
        self.0.read_file_capped(abs, cap)
    }

    fn list_dir(&self, abs: &Path) -> io::Result<Vec<PathBuf>> {
        self.0.list_dir(abs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::socket::shell::{Claim, STEERING_TABLE, ShellCheck};
    use peephole::{FixtureImage, FixtureProc, FixtureProcess, Pinned, Prober};
    use peephole_core::{Pid, Pin, Table, Uid};

    const PID: u32 = 4242;
    const MANAGER_1000: &str = "/user.slice/user-1000.slice/user@1000.service";

    fn held(uid: u32) -> Pinned {
        Pinned::introduced(Pid::new(PID).unwrap(), Uid::new(uid), Pin::Reopened)
    }

    /// `/proc` that refuses `<pid>/root`, as it does to a process without privileges looking at
    /// another user's process.
    #[derive(Debug)]
    struct Refusing(FixtureProc);

    impl ProcFs for Refusing {
        fn read_capped(&self, rel: &Path, cap: u64) -> io::Result<Vec<u8>> {
            match rel.ends_with(SANDBOX_MARK) {
                true => Err(io::ErrorKind::PermissionDenied.into()),
                false => self.0.read_capped(rel, cap),
            }
        }
        fn readlink(&self, rel: &Path) -> io::Result<PathBuf> {
            self.0.readlink(rel)
        }
        fn trust_of(&self, abs: &Path) -> io::Result<FileTrust> {
            self.0.trust_of(abs)
        }
        fn trust_of_followed(&self, rel: &Path) -> io::Result<FileTrust> {
            self.0.trust_of_followed(rel)
        }
        fn read_file_capped(&self, abs: &Path, cap: u64) -> io::Result<Vec<u8>> {
            self.0.read_file_capped(abs, cap)
        }
        fn list_dir(&self, abs: &Path) -> io::Result<Vec<PathBuf>> {
            self.0.list_dir(abs)
        }
    }

    #[test]
    fn another_users_hidden_sandbox_metadata_does_not_hide_the_shell() {
        let proc = FixtureProc::new().process(FixtureProcess {
            pid: Pid::new(PID).unwrap(),
            cgroup: &format!("0::{MANAGER_1000}/session.slice/sill.service\n"),
            image: FixtureImage::Hidden,
            flatpak_info: None,
        });
        let table = Table::from_toml(STEERING_TABLE).unwrap();
        let bare = ShellCheck::new(Prober::new(Refusing(proc.clone())), table.clone());
        assert_eq!(
            bare.origin_of(held(1000)).claim(),
            Claim::Other,
            "peephole alone gives up"
        );
        let wrapped = ShellCheck::new(Prober::new(Procs(Refusing(proc))), table);
        assert_eq!(
            wrapped.origin_of(held(1000)).claim(),
            Claim::Shell { user: 1000 }
        );
    }
}
