//! What the daemon takes from the process environment, read once in `command::run` so nothing
//! below it reads `std::env`.

use super::discover::{DEV_INPUT, SYS_INPUT, UDEV_DATA, VirtualAllowance};
use super::socket::SEAT_FILE;
use super::uinput::{PROBE_PERMIT, ProbePermit};
use std::path::PathBuf;
use std::time::Duration;

/// Where the config directory is when nothing overrides it.
const DEFAULT_CONFIG_DIR: &str = keycap_protocol::CONFIG_DIR;
/// Where the socket directory is when systemd does not provide one.
const DEFAULT_RUNTIME_DIR: &str = "/run/keycap";

/// Where logind records the seat's active user.
const DEFAULT_SEAT_FILE: &str = SEAT_FILE;

/// The process environment as the daemon uses it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Env {
    /// `$NOTIFY_SOCKET`: where `READY=1` goes; none outside systemd. A leading `@` is an
    /// abstract socket.
    pub notify_socket: Option<String>,
    /// `$KEYCAP_CONFIG_DIR`, else `/etc/keycap`.
    pub config_dir: PathBuf,
    /// `$RUNTIME_DIRECTORY` (systemd's), else `/run/keycap`: where the socket lives.
    pub runtime_dir: PathBuf,
    /// `$KEYCAP_SEAT_FILE`, else logind's `seat0` state file: where the active user is read
    /// from; injectable so tests need no logind.
    pub seat_file: PathBuf,
    /// `$KEYCAP_SYS_INPUT`, else `/sys/class/input`: where discovery lists the nodes.
    pub sys_input: PathBuf,
    /// `$KEYCAP_DEV_INPUT`, else `/dev/input`: where the nodes are opened and watched.
    pub dev_input: PathBuf,
    /// `$KEYCAP_UDEV_DATA`, else `/run/udev/data`: where discovery reads a node's seat.
    pub udev_data: PathBuf,
    /// `$KEYCAP_GRAB_VIRTUAL`: the one virtual device, by exact name, discovery may grab. Only
    /// the unit's environment sets it; it exists for tests whose keyboard is a uinput device.
    pub grab_virtual: VirtualAllowance,
    /// Whether `--check-uinput` may open the uinput node.
    pub probe: ProbePermit,
    /// `$WATCHDOG_USEC` as a duration: how often systemd expects a ping at the latest.
    pub watchdog: Option<Duration>,
}

impl Env {
    /// Reads every variable through `lookup`.
    pub(crate) fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let non_empty = |k: &str| lookup(k).filter(|s| !s.is_empty());
        let dir =
            |k: &str, default: &str| PathBuf::from(non_empty(k).as_deref().unwrap_or(default));
        Self {
            notify_socket: non_empty("NOTIFY_SOCKET"),
            config_dir: dir("KEYCAP_CONFIG_DIR", DEFAULT_CONFIG_DIR),
            runtime_dir: dir("RUNTIME_DIRECTORY", DEFAULT_RUNTIME_DIR),
            seat_file: dir("KEYCAP_SEAT_FILE", DEFAULT_SEAT_FILE),
            sys_input: dir("KEYCAP_SYS_INPUT", SYS_INPUT),
            dev_input: dir("KEYCAP_DEV_INPUT", DEV_INPUT),
            udev_data: dir("KEYCAP_UDEV_DATA", UDEV_DATA),
            grab_virtual: VirtualAllowance::from_env_value(
                lookup("KEYCAP_GRAB_VIRTUAL").as_deref(),
            ),
            probe: ProbePermit::from_env_value(lookup(PROBE_PERMIT).as_deref()),
            watchdog: non_empty("WATCHDOG_USEC")
                .and_then(|s| s.parse::<u64>().ok())
                .filter(|&us| us > 0)
                .map(Duration::from_micros),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(name, _)| *name == k)
                .map(|&(_, v)| v.to_owned())
        }
    }

    #[test]
    fn reads_each_variable_from_the_lookup() {
        let env = Env::from_lookup(lookup(&[
            ("NOTIFY_SOCKET", "/run/systemd/notify"),
            ("KEYCAP_CONFIG_DIR", "/tmp/cfg"),
            ("RUNTIME_DIRECTORY", "/run/keycap"),
            (PROBE_PERMIT, "1"),
            ("WATCHDOG_USEC", "1000000"),
            ("KEYCAP_SEAT_FILE", "/tmp/seat0"),
            ("KEYCAP_SYS_INPUT", "/tmp/sys"),
            ("KEYCAP_DEV_INPUT", "/tmp/dev"),
            ("KEYCAP_UDEV_DATA", "/tmp/udev"),
            ("KEYCAP_GRAB_VIRTUAL", "test keyboard"),
        ]));
        assert_eq!(env.notify_socket.as_deref(), Some("/run/systemd/notify"));
        assert_eq!(env.config_dir, PathBuf::from("/tmp/cfg"));
        assert_eq!(env.runtime_dir, PathBuf::from("/run/keycap"));
        assert_eq!(env.seat_file, PathBuf::from("/tmp/seat0"));
        assert_eq!(env.sys_input, PathBuf::from("/tmp/sys"));
        assert_eq!(env.dev_input, PathBuf::from("/tmp/dev"));
        assert_eq!(env.udev_data, PathBuf::from("/tmp/udev"));
        assert_eq!(
            env.grab_virtual,
            VirtualAllowance::Named("test keyboard".to_owned())
        );
        assert_eq!(env.probe, ProbePermit::Granted);
        assert_eq!(env.watchdog, Some(Duration::from_secs(1)));
    }

    #[test]
    fn an_empty_environment_has_the_defaults() {
        let env = Env::from_lookup(lookup(&[]));
        assert_eq!(env.notify_socket, None);
        assert_eq!(env.config_dir, PathBuf::from("/etc/keycap"));
        assert_eq!(env.runtime_dir, PathBuf::from("/run/keycap"));
        assert_eq!(env.seat_file, PathBuf::from("/run/systemd/seats/seat0"));
        assert_eq!(env.sys_input, PathBuf::from("/sys/class/input"));
        assert_eq!(env.dev_input, PathBuf::from("/dev/input"));
        assert_eq!(env.udev_data, PathBuf::from("/run/udev/data"));
        assert_eq!(env.grab_virtual, VirtualAllowance::Nobody);
        assert_eq!(env.probe, ProbePermit::Refused);
        assert_eq!(env.watchdog, None);
    }

    #[test]
    fn systemds_runtime_directory_wins_and_odd_values_fall_back() {
        type Case = (
            &'static str,
            &'static [(&'static str, &'static str)],
            &'static str,
            Option<Duration>,
        );
        const CASES: &[Case] = &[
            (
                "runtime directory from systemd",
                &[("RUNTIME_DIRECTORY", "/run/custom")],
                "/run/custom",
                None,
            ),
            (
                "empty runtime directory",
                &[("RUNTIME_DIRECTORY", "")],
                "/run/keycap",
                None,
            ),
            (
                "unparsable watchdog",
                &[("WATCHDOG_USEC", "soon")],
                "/run/keycap",
                None,
            ),
            (
                "zero watchdog",
                &[("WATCHDOG_USEC", "0")],
                "/run/keycap",
                None,
            ),
            (
                "500 ms watchdog",
                &[("WATCHDOG_USEC", "500000")],
                "/run/keycap",
                Some(Duration::from_millis(500)),
            ),
        ];
        for &(name, vars, runtime, watchdog) in CASES {
            let env = Env::from_lookup(lookup(vars));
            assert_eq!(env.runtime_dir, PathBuf::from(runtime), "{name}");
            assert_eq!(env.watchdog, watchdog, "{name}");
        }
    }
}
