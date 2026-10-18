//! Which connection is the desktop's shell. Every program of the person's can reach the socket,
//! so a connection is only obeyed when the kernel says it runs in the shell's own unit. That is
//! a speed bump and not a wall: a program of the same user can put itself in a control group
//! of that name. What it gets for doing so is bounded by what the user layer may say
//! (`ConfigLayer::confined`), which is the part that holds.
//!
//! keycap is a dynamic user without privileges, so it cannot read another user's image path;
//! the unit name in the control group is all there is to go on, and the process is held by its
//! pidfd so the name cannot belong to a process that has since been replaced. The check itself is
//! `latchkey::guard`; this file is keycap's table and its choice of who is believed.

use latchkey::guard::{SeatUser, ShellCheck, Vouch};
use peephole_core::roles::SHELL;
use peephole_core::{RoleName, Table};

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
            Steering::ShellUnit => Box::new(steering_check()),
            Steering::SeatUser => Box::new(SeatUser),
        }
    }
}

/// The check of this machine against the steering table. A table that does not parse leaves
/// nobody steering, which is the safe way to fail.
fn steering_check() -> ShellCheck {
    let table = Table::from_toml(STEERING_TABLE).unwrap_or_else(|e| {
        eprintln!("keycap: nobody may steer: the steering table is unusable: {e}");
        Table::empty()
    });
    ShellCheck::system(table, vec![SHELL, FOCUS_FEED])
}

#[cfg(test)]
mod tests {
    use super::*;
    use latchkey::guard::Claim;
    use peephole::{FixtureImage, FixtureProc, FixtureProcess, Pinned, Prober};
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
        ShellCheck::new(
            Prober::new(proc),
            Table::from_toml(STEERING_TABLE).unwrap(),
            vec![SHELL, FOCUS_FEED],
        )
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
    fn the_shell_and_the_focus_feed_claim_the_shell_and_no_other_unit_does() {
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
