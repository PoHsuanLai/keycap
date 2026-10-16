//! The real `keycap` binary as a child process, with every path it touches in a scratch
//! directory and `sd_notify` going to a socket the harness reads.

use crate::failure::{Failure, io};
use crate::notify::Notifications;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// How a daemon is started.
#[derive(Debug, Clone)]
pub struct DaemonSpec<'a> {
    /// The built binary (`env!("CARGO_BIN_EXE_keycap")` in keycap's own tests).
    pub binary: &'a Path,
    /// Where the runtime directory, config directory, seat file, notify socket and stderr log go.
    pub scratch: &'a Path,
    /// The value of `KEYCAP_GRAB_VIRTUAL`: the fake keyboard's name.
    pub grab_virtual: &'a str,
    /// `WATCHDOG_USEC`; the daemon pings at least every half of it.
    pub watchdog: Duration,
}

/// A running daemon; killed on drop.
#[derive(Debug)]
pub struct Daemon {
    child: Child,
    notifications: Notifications,
    started: Instant,
    runtime_dir: PathBuf,
    stderr_log: PathBuf,
}

impl Daemon {
    /// Spawns the daemon. The seat file names the uid this process runs as, and the daemon is
    /// told to steer for the seat's user, so the harness's own client is the seat owner. The
    /// stderr log is appended to, so a restart keeps the first run's lines.
    pub fn start(spec: &DaemonSpec<'_>) -> Result<Self, Failure> {
        let dir = |name: &str| -> Result<PathBuf, Failure> {
            let path = spec.scratch.join(name);
            std::fs::create_dir_all(&path).map_err(io(format!("creating {}", path.display())))?;
            Ok(path)
        };
        let (runtime_dir, config_dir) = (dir("run")?, dir("config")?);
        let seat_file = spec.scratch.join("seat0");
        let uid = rustix::process::getuid().as_raw();
        std::fs::write(&seat_file, format!("IS_SEAT0=1\nACTIVE_UID={uid}\n"))
            .map_err(io("writing the seat file"))?;
        let notify_path = spec.scratch.join("notify.sock");
        let notifications =
            Notifications::listen(&notify_path).map_err(io("binding the notify socket"))?;
        let stderr_log = spec.scratch.join("keycap.stderr");
        let stderr = File::options()
            .create(true)
            .append(true)
            .open(&stderr_log)
            .map_err(io("opening the stderr log"))?;
        let child = Command::new(spec.binary)
            .env_clear()
            .env("KEYCAP_GRAB_VIRTUAL", spec.grab_virtual)
            // No device service here: the harness makes devices on /dev/uinput itself, and its
            // client is not in the shell's unit, so it steers as the seat's user.
            .env(
                "KEYCAP_DEVICE_SERVICE",
                spec.scratch.join("no-device-service"),
            )
            .env("KEYCAP_STEERING", "seat-user")
            // The scratch folder is the test user's, so a machine file written here is refused
            // (the daemon reads that file only when root-only); a run has no machine layer.
            .env("KEYCAP_CONFIG_DIR", &config_dir)
            // A folder that is not there is no vendor layer, so a run never reads the machine's.
            .env("KEYCAP_VENDOR_DIR", spec.scratch.join("no-vendor-config"))
            .env("KEYCAP_SEAT_FILE", &seat_file)
            .env("RUNTIME_DIRECTORY", &runtime_dir)
            .env("NOTIFY_SOCKET", &notify_path)
            .env("WATCHDOG_USEC", spec.watchdog.as_micros().to_string())
            .env("RUST_BACKTRACE", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(stderr))
            .spawn()
            .map_err(io(format!("spawning {}", spec.binary.display())))?;
        Ok(Self {
            child,
            notifications,
            started: Instant::now(),
            runtime_dir,
            stderr_log,
        })
    }

    /// The control socket.
    pub fn socket_path(&self) -> PathBuf {
        self.runtime_dir.join("keycap.sock")
    }

    /// The process id.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Everything the daemon wrote to stderr, all runs.
    pub fn stderr(&self) -> String {
        std::fs::read_to_string(&self.stderr_log).unwrap_or_default()
    }

    /// The `sd_notify` messages received.
    pub fn notifications(&self) -> &Notifications {
        &self.notifications
    }

    /// Time since the spawn.
    pub fn uptime(&self) -> Duration {
        self.started.elapsed()
    }

    /// Fails if the process has exited, naming its status.
    pub fn assert_alive(&mut self) -> Result<(), Failure> {
        match self.child.try_wait() {
            Ok(None) => Ok(()),
            Ok(Some(status)) => Err(Failure::new(format!("the daemon exited: {status}"))),
            Err(e) => Err(Failure::new(format!("waiting for the daemon: {e}"))),
        }
    }

    /// Waits for `READY=1`; it must arrive within `within` of the spawn.
    pub fn wait_ready(&mut self, within: Duration) -> Result<Duration, Failure> {
        let deadline = self.started + within;
        loop {
            if let Some(at) = self.notifications.times_of("READY=1").first() {
                return Ok(at.duration_since(self.started));
            }
            self.assert_alive()
                .map_err(|e| e.during("waiting for READY=1"))?;
            if Instant::now() >= deadline {
                return Err(Failure::new(format!(
                    "no READY=1 within {within:?} of the spawn; notifications: {:?}",
                    self.notifications
                        .all()
                        .iter()
                        .map(|n| n.text.as_str())
                        .collect::<Vec<_>>()
                )));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// `kill -9`: the daemon gets no chance to clean up, as in a crash.
    pub fn kill(&mut self) -> Result<(), Failure> {
        self.child.kill().map_err(io("killing the daemon"))?;
        self.child
            .wait()
            .map(drop)
            .map_err(io("reaping the daemon"))
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
