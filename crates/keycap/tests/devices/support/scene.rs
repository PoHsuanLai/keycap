//! One device test's surroundings: its scratch directory, the daemon, the client and the output
//! reader built on it, and the failure report written when the test panics.

use keycap_devtest::{Client, Daemon, DaemonSpec, Failure, Output, Scratch};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The name every fake keyboard registers under, which the daemon is told it may grab.
pub const FAKE_NAME: &str = "keycap-ci-keyboard";
/// The `WATCHDOG_USEC` the daemon runs with.
pub const WATCHDOG: Duration = Duration::from_secs(2);
/// How long a test waits for the daemon to announce `READY=1`.
const READY_WITHIN: Duration = Duration::from_secs(5);

/// The environment variable a device test requires, so a plain `cargo test --ignored` on a
/// developer's machine never creates a device.
const GATE: &str = "KEYCAP_DEVICE_TESTS";
/// Where a failed test's logs are copied, when set.
const ARTIFACTS: &str = "KEYCAP_TEST_ARTIFACTS";

/// Unwraps a harness result, naming the step.
pub fn ok<T>(result: Result<T, Failure>, step: &str) -> T {
    result.unwrap_or_else(|e| panic!("{step}: {e}"))
}

/// A device test's scratch space. Declared first in a test so it drops last, after the daemon
/// is dead and its stderr is complete.
pub struct Scene {
    name: String,
    scratch: Scratch,
}

impl Scene {
    /// Opens the scene, refusing to run unless `KEYCAP_DEVICE_TESTS=1`.
    pub fn new(name: &str) -> Self {
        assert_eq!(
            std::env::var(GATE).as_deref(),
            Ok("1"),
            "this test creates input devices; set {GATE}=1 (CI does) and run it with --ignored"
        );
        Self {
            name: name.to_owned(),
            scratch: Scratch::new(name),
        }
    }

    /// The scratch directory.
    pub fn dir(&self) -> &Path {
        self.scratch.path()
    }

    /// The built `keycap` binary.
    pub fn binary() -> &'static Path {
        Path::new(env!("CARGO_BIN_EXE_keycap"))
    }

    /// What starts a daemon here.
    pub fn spec(&self) -> DaemonSpec<'_> {
        DaemonSpec {
            binary: Self::binary(),
            scratch: self.scratch.path(),
            grab_virtual: FAKE_NAME,
            watchdog: WATCHDOG,
        }
    }

    /// Starts the daemon and waits for it to be ready.
    pub fn daemon(&self) -> Daemon {
        let mut daemon = ok(Daemon::start(&self.spec()), "starting the daemon");
        ok(daemon.wait_ready(READY_WITHIN), "waiting for the daemon");
        daemon
    }

    /// A client of `daemon`'s socket.
    pub fn client(&self, daemon: &Daemon) -> Client {
        let log = self.dir().join("client.log");
        ok(
            Client::connect(&daemon.socket_path(), Some(&log)),
            "connecting a client",
        )
    }

    /// A reader on the daemon's virtual keyboard.
    pub fn output(&self) -> Output {
        let trace = self.dir().join("output.trace");
        ok(Output::find(Some(&trace)), "opening the output keyboard")
    }

    /// The failure report: the daemon's stderr, what the output device produced and what the
    /// client heard.
    fn report(&self) -> String {
        let read = |file: &str| {
            std::fs::read_to_string(self.dir().join(file)).unwrap_or_else(|_| "(none)\n".into())
        };
        format!(
            "==== {name}: keycap stderr ====\n{}\n==== {name}: output device trace ====\n{}\n\
             ==== {name}: client log ====\n{}",
            read("keycap.stderr"),
            read("output.trace"),
            read("client.log"),
            name = self.name
        )
    }

    /// Copies the logs into the artifacts directory, when one is configured.
    fn archive(&self) {
        let Ok(dir) = std::env::var(ARTIFACTS) else {
            return;
        };
        let dest = PathBuf::from(dir).join(&self.name);
        if std::fs::create_dir_all(&dest).is_err() {
            return;
        }
        for file in ["keycap.stderr", "output.trace", "client.log"] {
            let _ = std::fs::copy(self.dir().join(file), dest.join(file));
        }
    }
}

impl Drop for Scene {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("{}", self.report());
            self.archive();
        }
    }
}
