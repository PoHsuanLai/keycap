//! Which connection is the desktop's shell. Every program of the person's can reach the socket,
//! so a connection is only obeyed when the kernel says it runs in the shell's own unit. That is
//! a speed bump and not a wall: a program of the same user can put itself in a control group
//! of that name. What it gets for doing so is bounded by what the user layer may say
//! (`ConfigLayer::confined`), which is the part that holds.
//!
//! keycap is a dynamic user without privileges, so it cannot read another user's image path;
//! the unit name in the control group is all there is to go on, and the process is held by its
//! pidfd so the name cannot belong to a process that has since been replaced.

use super::procs::Procs;
use peephole::{Alive, Pinned, Prober, ProcFs, RealProc};
use peephole_core::roles::SHELL;
use peephole_core::{CgroupPath, PinNeed, RoleName, Standing, Table, judge};
use rustix::fd::OwnedFd;

/// The programs allowed to steer, as the rows of a caller table: the shell, and the Plasma focus
/// feed that the standalone install can run instead. The paths only matter when the image can
/// be read at all, which a dynamic user cannot do for another user's process.
pub(super) const STEERING_TABLE: &str = r#"
[[peer]]
app  = "org.quire.Shell"
unit = "sill.service"
exe  = [
  "/opt/quire/releases/*/bin/sill",
  "/opt/quire/current/bin/sill",
  "/usr/local/bin/sill",
  "/usr/bin/sill",
]
role = "shell"

[[peer]]
app  = "org.keycap.FocusFeed"
unit = "keycap-focus.service"
exe  = [
  "/usr/local/bin/keycap-focus",
  "/home/*/.local/bin/keycap-focus",
]
role = "focus_feed"
"#;

/// The role of the Plasma focus feed.
const FOCUS_FEED: RoleName = RoleName::from_static("focus_feed");

/// How firmly a connection's process must have been held. Peephole's socket pin is made after
/// the connect (`Reopened`), so nothing firmer can be asked yet.
const PIN_NEED: PinNeed = PinNeed::Any;

/// A process we hold, running as the shell, under the manager of `user`.
#[derive(Debug)]
pub(crate) struct Held {
    process: Pinned,
    user: u32,
}

/// What a connection showed itself to be when it connected.
#[derive(Debug)]
pub(crate) enum Origin {
    /// In the shell's unit, under this user's manager.
    Shell(Held),
    /// Anything else.
    Other,
}

/// What a connection claims right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Claim {
    /// The shell of this user, and its process is still the one we held.
    Shell {
        user: u32,
    },
    Other,
}

impl Origin {
    /// The claim as of now: a shell whose process has since exited (and whose socket a child
    /// kept open) claims nothing.
    pub(crate) fn claim(&self) -> Claim {
        match self {
            Origin::Shell(held) if held.process.alive() == Alive::Yes => {
                Claim::Shell { user: held.user }
            }
            Origin::Shell(_) | Origin::Other => Claim::Other,
        }
    }
}

/// Decides what a freshly connected stream is.
pub(crate) trait Vouch: Send + 'static {
    /// The origin of the process on the other end of `stream`.
    fn origin(&self, stream: &OwnedFd) -> Origin;
}

/// Who is trusted to steer, chosen by the unit's environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Steering {
    /// Only the shell's unit.
    #[default]
    ShellUnit,
    /// Every program of the seat's user, as before the shell was told apart. For tests whose
    /// client is not in a unit; only the unit's environment sets it.
    SeatUser,
}

impl Steering {
    /// The choice `KEYCAP_STEERING` names: only `seat-user` changes the default.
    pub(crate) fn from_env_value(v: Option<&str>) -> Self {
        match v {
            Some("seat-user") => Steering::SeatUser,
            _ => Steering::ShellUnit,
        }
    }

    pub(crate) fn vouch(self) -> Box<dyn Vouch> {
        match self {
            Steering::ShellUnit => Box::new(ShellCheck::system()),
            Steering::SeatUser => Box::new(SeatUser),
        }
    }
}

/// The real check: hold the process, read its control group, judge it against the steering table.
#[derive(Debug)]
pub(crate) struct ShellCheck<P: ProcFs = Procs> {
    prober: Prober<P>,
    table: Table,
}

impl ShellCheck {
    /// The check of this machine. A table that does not parse leaves nobody steering, which
    /// is the safe way to fail.
    pub(crate) fn system() -> Self {
        let table = Table::from_toml(STEERING_TABLE).unwrap_or_else(|e| {
            eprintln!("keycap: nobody may steer: the steering table is unusable: {e}");
            Table::empty()
        });
        Self::new(Prober::new(Procs(RealProc::system())), table)
    }
}

impl<P: ProcFs> ShellCheck<P> {
    pub(crate) fn new(prober: Prober<P>, table: Table) -> Self {
        Self { prober, table }
    }

    /// What the held process `process` is.
    pub(crate) fn origin_of(&self, process: Pinned) -> Origin {
        let Ok(facts) = self.prober.probe(&process, None) else {
            return Origin::Other;
        };
        let user = match judge(&facts, &self.table) {
            Standing::Service { role, pin, .. }
                if (role == SHELL || role == FOCUS_FEED) && PIN_NEED.satisfied_by(pin) =>
            {
                manager_user(&facts.cgroup)
            }
            _ => None,
        };
        user.map_or(Origin::Other, |user| Origin::Shell(Held { process, user }))
    }
}

impl<P: ProcFs + 'static> Vouch for ShellCheck<P> {
    fn origin(&self, stream: &OwnedFd) -> Origin {
        Pinned::of_stream(stream).map_or(Origin::Other, |process| self.origin_of(process))
    }
}

/// Every connection is the shell of the user it runs as.
#[derive(Debug)]
struct SeatUser;

impl Vouch for SeatUser {
    fn origin(&self, stream: &OwnedFd) -> Origin {
        Pinned::of_stream(stream).map_or(Origin::Other, |process| {
            let user = process.uid().get();
            Origin::Shell(Held { process, user })
        })
    }
}

/// The user whose manager runs the unit at `cgroup`: the path starts
/// `/user.slice/user-<N>.slice/user@<N>.service/`, and `N` is the same twice.
fn manager_user(cgroup: &CgroupPath) -> Option<u32> {
    let mut segments = cgroup.segments();
    let (slice, user_slice, manager) = (segments.next()?, segments.next()?, segments.next()?);
    let from_slice = user_slice.strip_prefix("user-")?.strip_suffix(".slice")?;
    let from_manager = manager.strip_prefix("user@")?.strip_suffix(".service")?;
    match slice == "user.slice" && from_slice == from_manager {
        true => from_manager.parse().ok(),
        false => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peephole::{FixtureImage, FixtureProc, FixtureProcess};
    use peephole_core::{FileTrust, Pid, Pin, Uid, Writer};

    const PID: u32 = 4242;
    const MANAGER_1000: &str = "/user.slice/user-1000.slice/user@1000.service";

    fn check(cgroup: &str, image: FixtureImage<'_>) -> ShellCheck<FixtureProc> {
        let pid = Pid::new(PID).unwrap();
        let proc = FixtureProc::new().process(FixtureProcess {
            pid,
            cgroup: &format!("0::{cgroup}\n"),
            image,
            flatpak_info: None,
        });
        ShellCheck::new(Prober::new(proc), Table::from_toml(STEERING_TABLE).unwrap())
    }

    fn held(uid: u32) -> Pinned {
        Pinned::introduced(Pid::new(PID).unwrap(), Uid::new(uid), Pin::Reopened)
    }

    fn claim_of(cgroup: &str, image: FixtureImage<'_>) -> Claim {
        check(cgroup, image).origin_of(held(1000)).claim()
    }

    #[test]
    fn the_steering_table_parses_and_names_both_programs() {
        let table = Table::from_toml(STEERING_TABLE).unwrap();
        assert_eq!(table.rows().len(), 2);
        assert!(table.has_role(&SHELL) && table.has_role(&FOCUS_FEED));
    }

    #[test]
    fn only_the_shells_unit_under_the_users_manager_claims_the_shell() {
        let shell = Claim::Shell { user: 1000 };
        let cases: &[(&str, String, Claim)] = &[
            (
                "the shell",
                format!("{MANAGER_1000}/session.slice/sill.service"),
                shell,
            ),
            (
                "the focus feed",
                format!("{MANAGER_1000}/app.slice/keycap-focus.service"),
                shell,
            ),
            (
                "another unit",
                format!("{MANAGER_1000}/app.slice/mailo.service"),
                Claim::Other,
            ),
            (
                "a scope",
                format!("{MANAGER_1000}/app.slice/app-sill-1.scope"),
                Claim::Other,
            ),
            (
                "a system service of that name",
                "/system.slice/sill.service".to_owned(),
                Claim::Other,
            ),
            (
                "a manager of another user than the slice",
                "/user.slice/user-999.slice/user@1000.service/session.slice/sill.service"
                    .to_owned(),
                Claim::Other,
            ),
            (
                "not under user.slice",
                "/machine.slice/user-1000.slice/user@1000.service/sill.service".to_owned(),
                Claim::Other,
            ),
        ];
        for (name, cgroup, want) in cases {
            assert_eq!(claim_of(cgroup, FixtureImage::Hidden), *want, "{name}");
        }
    }

    #[test]
    fn a_readable_image_must_be_the_shells_and_root_owned() {
        let cgroup = format!("{MANAGER_1000}/session.slice/sill.service");
        let shell = Claim::Shell { user: 1000 };
        let cases: &[(&str, FixtureImage<'_>, Claim)] = &[
            (
                "the installed shell",
                FixtureImage::Present("/usr/bin/sill", FileTrust::RootOnly),
                shell,
            ),
            (
                "another program in its unit",
                FixtureImage::Present("/usr/bin/sh", FileTrust::RootOnly),
                Claim::Other,
            ),
            (
                "the shell's path, but writable",
                FixtureImage::Present("/usr/bin/sill", FileTrust::Writable(Writer::Group)),
                Claim::Other,
            ),
        ];
        for (name, image, want) in cases {
            assert_eq!(claim_of(&cgroup, *image), *want, "{name}");
        }
    }

    #[test]
    fn a_process_that_has_exited_claims_nothing_and_never_gets_the_claim() {
        let cgroup = format!("{MANAGER_1000}/session.slice/sill.service");
        let gone = Pinned::introduced_gone(Pid::new(PID).unwrap(), Uid::new(1000), Pin::Reopened);
        let origin = check(&cgroup, FixtureImage::Hidden).origin_of(gone);
        assert_eq!(origin.claim(), Claim::Other);
    }

    #[test]
    fn a_shell_whose_process_leaves_after_connecting_loses_the_claim() {
        let origin = Origin::Shell(Held {
            process: Pinned::introduced_gone(Pid::new(PID).unwrap(), Uid::new(1000), Pin::Reopened),
            user: 1000,
        });
        assert_eq!(origin.claim(), Claim::Other);
        let alive = Origin::Shell(Held {
            process: held(1000),
            user: 1000,
        });
        assert_eq!(alive.claim(), Claim::Shell { user: 1000 });
    }

    #[test]
    fn only_the_exact_word_asks_for_the_seat_user() {
        const CASES: &[(Option<&str>, Steering)] = &[
            (None, Steering::ShellUnit),
            (Some(""), Steering::ShellUnit),
            (Some("1"), Steering::ShellUnit),
            (Some("seat-user"), Steering::SeatUser),
        ];
        for &(value, want) in CASES {
            assert_eq!(Steering::from_env_value(value), want, "{value:?}");
        }
    }
}
