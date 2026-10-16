//! `run`: brings the daemon up and hands the main thread to the reader.

use super::devices::status_of;
use super::discover::{Hotplug, spawn as watch_nodes};
use super::env::Env;
use super::inbox::{Inbox, Message, forward, mailbox};
use super::reader::{Reader, Roots, Wiring};
use super::sdnotify::Notifier;
use super::settings::Layers;
use super::socket::{self, SeatFile, SocketHandle, ToSocket, bind, socket_path};
use super::source::Source;
use super::uinput::Device;
use crate::ConfigLayer;
use crate::CoreStatus;
use crate::config::{self, system, vendor};
use crate::keymap::{self, Reading};
use keycap_protocol::Warning;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

/// Runs the daemon: returns only when setup or the reader's `poll` fails.
pub(crate) fn run(env: Env) -> Result<(), String> {
    let notifier =
        Notifier::new(env.notify_socket.as_deref()).map_err(|e| format!("notify socket: {e}"))?;
    let source = Source::detect(&env.device_service);
    eprintln!("keycap: virtual devices from {}", source.describe());
    let emitter = create_output(&source)?;
    let (inbox, mailbox) = mailbox().map_err(|e| format!("inbox: {e}"))?;
    let path = socket_path(&env.runtime_dir);
    let listener = bind(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let initial = status_of(&Default::default(), CoreStatus::Running);
    let first_keymap = keymap::read(&env.config_dir);
    keymap::settle(&Default::default(), first_keymap.clone())
        .problems
        .iter()
        .for_each(|p| eprintln!("keycap: keymap: line {}: {}", p.line, p.what));
    let socket = socket::spawn(
        listener,
        inbox.clone(),
        SeatFile(env.seat_file.clone()),
        env.steering.vouch(),
        initial,
        first_keymap,
    )
    .map_err(|e| format!("socket thread: {e}"))?;
    eprintln!("keycap: listening on {}", path.display());
    let _devices = watch_devices(&env.dev_input, &inbox)?;
    let (layers, _config) = load_config(&env, &inbox);
    let _keymap = watch_keymap(&env.config_dir, &socket);
    Reader::new(Wiring {
        mailbox,
        socket,
        emitter,
        notifier,
        watchdog: env.watchdog,
        roots: Roots::of(&env),
        layers,
    })
    .run()
}

/// How long `/dev/uinput` is waited for before the daemon gives up. The unit is `Type=notify`
/// and ordered before the display manager, so a wait with no end holds the whole boot.
const UINPUT_PATIENCE: Duration = Duration::from_secs(5);
/// How long between attempts.
const UINPUT_RETRY: Duration = Duration::from_millis(500);

/// What to do after a failed attempt to create the virtual keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Patience {
    TryAgainAfter(Duration),
    GiveUp,
}

/// The decision after `waited` in total: try again until `UINPUT_PATIENCE` is used up.
fn patience(waited: Duration) -> Patience {
    if waited >= UINPUT_PATIENCE {
        Patience::GiveUp
    } else {
        Patience::TryAgainAfter(UINPUT_RETRY)
    }
}

/// Creates the virtual keyboard, retrying for a few seconds while its source is not yet
/// accessible (early boot, a udev rule not yet applied, the device service not yet started).
/// Nothing can be remapped without it, so past that the daemon exits non-zero and systemd's
/// restart policy takes over.
fn create_output(source: &Source) -> Result<Device, String> {
    let mut waited = Duration::ZERO;
    loop {
        match Device::create(source) {
            Ok(device) => return Ok(device),
            Err(e) => match patience(waited) {
                Patience::GiveUp => {
                    return Err(format!(
                        "no virtual keyboard after {}s: {e} (is the device service running, or \
                         the uinput module loaded and /dev/uinput writable by the keycap unit?)",
                        waited.as_secs()
                    ));
                }
                Patience::TryAgainAfter(delay) => {
                    eprintln!("keycap: {e}; retrying in {}ms", delay.as_millis());
                    std::thread::sleep(delay);
                    waited += delay;
                }
            },
        }
    }
}

/// Watches `/dev/input` for keyboards coming and going. The watcher must be kept alive.
fn watch_devices(dev_input: &Path, inbox: &Inbox) -> Result<notify::RecommendedWatcher, String> {
    let (tx, rx) = mpsc::channel::<Hotplug>();
    let watcher =
        watch_nodes(dev_input, tx).map_err(|e| format!("watching {}: {e}", dev_input.display()))?;
    forward("hotplug", rx, inbox.clone(), Message::Hotplug)
        .map_err(|e| format!("hotplug thread: {e}"))?;
    Ok(watcher)
}

/// Watches the keymap file and hands every settled re-read to the socket thread, which serves
/// the keymap. A directory that cannot be watched costs live reload only. The watcher must be
/// kept alive.
fn watch_keymap(dir: &Path, socket: &SocketHandle) -> Option<notify::RecommendedWatcher> {
    let (tx, rx) = mpsc::channel::<Reading>();
    let relay = socket.clone();
    let thread = std::thread::Builder::new()
        .name("keycap-keymap".to_owned())
        .spawn(move || rx.iter().for_each(|r| relay.send(ToSocket::Keymap(r))));
    let watcher = thread
        .map_err(|e| format!("keymap thread: {e}"))
        .and_then(|_| {
            keymap::spawn(dir.to_path_buf(), tx)
                .map_err(|e| format!("not watching the keymap in {}: {e}", dir.display()))
        });
    watcher.map_err(|e| eprintln!("keycap: {e}")).ok()
}

/// How a config file is watched: the function that starts the watch and the message its
/// re-reads arrive as.
type Watch = fn(PathBuf, mpsc::Sender<Reload>) -> notify::Result<notify::RecommendedWatcher>;

/// A re-read config layer and what was wrong with it.
type Reload = (ConfigLayer, Vec<Warning>);

/// Watches `dir` with `start` and hands every settled re-read to the reader as `wrap` says. A
/// directory that cannot be watched costs live reload only: the keyboard still works.
fn watch_layer(
    dir: &Path,
    inbox: &Inbox,
    start: Watch,
    wrap: fn(Reload) -> Message,
) -> Option<notify::RecommendedWatcher> {
    let (tx, rx) = mpsc::channel();
    let watcher = start(dir.to_path_buf(), tx)
        .map_err(|e| format!("not watching {}: {e}", dir.display()))
        .and_then(|watcher| {
            forward("config", rx, inbox.clone(), wrap)
                .map(|()| watcher)
                .map_err(|e| format!("config thread: {e}"))
        });
    watcher.map_err(|e| eprintln!("keycap: {e}")).ok()
}

/// The built-in, vendor and system layers, and the watches that re-read the vendor and system
/// files. A folder that is not there has no layer to watch (the vendor folder on a machine
/// without the desktop). The watchers must be kept alive.
fn load_config(env: &Env, inbox: &Inbox) -> (Layers, Vec<notify::RecommendedWatcher>) {
    let (vendor_layer, vendor_warnings) = vendor(&env.vendor_dir);
    let (layer, warnings) = system(&env.config_dir);
    vendor_warnings
        .iter()
        .chain(&warnings)
        .for_each(|w| eprintln!("keycap: config: {}: {}", w.key, w.problem));
    let layers = Layers::builtin_only()
        .with_vendor(vendor_layer)
        .with_system(layer);
    let vendor_watch = env
        .vendor_dir
        .is_dir()
        .then(|| {
            watch_layer(
                &env.vendor_dir,
                inbox,
                config::spawn_vendor,
                |(layer, warnings)| Message::Vendor(layer, warnings),
            )
        })
        .flatten();
    let system_watch = watch_layer(
        &env.config_dir,
        inbox,
        config::spawn,
        |(layer, warnings)| Message::System(layer, warnings),
    );
    (
        layers,
        vendor_watch.into_iter().chain(system_watch).collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patience_runs_out_after_five_seconds_of_waiting() {
        let ms = Duration::from_millis;
        let cases: &[(&str, Duration, Patience)] = &[
            ("first failure", ms(0), Patience::TryAgainAfter(ms(500))),
            (
                "most of the way",
                ms(4_500),
                Patience::TryAgainAfter(ms(500)),
            ),
            ("just under", ms(4_999), Patience::TryAgainAfter(ms(500))),
            ("used up", ms(5_000), Patience::GiveUp),
            ("long past", ms(90_000), Patience::GiveUp),
        ];
        for &(name, waited, want) in cases {
            assert_eq!(patience(waited), want, "{name}");
        }
    }

    #[test]
    fn the_whole_wait_fits_inside_the_units_start_timeout() {
        // dist/keycap.service sets TimeoutStartSec=10.
        assert!(UINPUT_PATIENCE + UINPUT_RETRY < Duration::from_secs(10));
    }
}
