//! `run`: brings the daemon up and hands the main thread to the reader.

use super::devices::status_of;
use super::discover::{DEV_INPUT, Hotplug, spawn as watch_nodes};
use super::env::Env;
use super::inbox::{Inbox, Message, forward, mailbox};
use super::reader::{Reader, Roots, Wiring};
use super::sdnotify::Notifier;
use super::settings::Layers;
use super::socket::{self, SeatFile, bind, socket_path};
use super::uinput::Device;
use crate::config::{self, system};
use crate::{Backoff, CoreStatus};
use std::path::Path;
use std::sync::mpsc;

/// Runs the daemon: returns only when setup or the reader's `poll` fails.
pub(crate) fn run(env: Env) -> Result<(), String> {
    let notifier =
        Notifier::new(env.notify_socket.as_deref()).map_err(|e| format!("notify socket: {e}"))?;
    let emitter = create_output();
    let (inbox, mailbox) = mailbox().map_err(|e| format!("inbox: {e}"))?;
    let path = socket_path(&env.runtime_dir);
    let listener = bind(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let initial = status_of(&Default::default(), CoreStatus::Running);
    let socket = socket::spawn(
        listener,
        inbox.clone(),
        SeatFile(env.seat_file.clone()),
        initial,
    )
    .map_err(|e| format!("socket thread: {e}"))?;
    eprintln!("keycap: listening on {}", path.display());
    let _devices = watch_devices(&inbox)?;
    let (layers, _config) = load_config(&env, &inbox);
    Reader::new(Wiring {
        mailbox,
        socket,
        emitter,
        notifier,
        watchdog: env.watchdog,
        roots: Roots::system(),
        layers,
    })
    .run()
}

/// Creates the virtual keyboard, retrying with backoff while `/dev/uinput` is not yet
/// accessible (early boot, a udev rule not yet applied). Nothing can be remapped without it, so
/// there is nothing else to do meanwhile.
fn create_output() -> Device {
    let mut backoff = Backoff::DEVICE;
    loop {
        match Device::create() {
            Ok(device) => return device,
            Err(e) => {
                let (delay, next) = backoff.step();
                eprintln!("keycap: {e}; retrying in {}s", delay.as_secs());
                std::thread::sleep(delay);
                backoff = next;
            }
        }
    }
}

/// Watches `/dev/input` for keyboards coming and going. The watcher must be kept alive.
fn watch_devices(inbox: &Inbox) -> Result<notify::RecommendedWatcher, String> {
    let (tx, rx) = mpsc::channel::<Hotplug>();
    let watcher =
        watch_nodes(Path::new(DEV_INPUT), tx).map_err(|e| format!("watching {DEV_INPUT}: {e}"))?;
    forward("hotplug", rx, inbox.clone(), Message::Hotplug)
        .map_err(|e| format!("hotplug thread: {e}"))?;
    Ok(watcher)
}

/// The built-in and system layers, and the watch that re-reads the system file. A config
/// directory that cannot be watched costs live reload only: the keyboard still works.
fn load_config(env: &Env, inbox: &Inbox) -> (Layers, Option<notify::RecommendedWatcher>) {
    let (layer, warnings) = system(&env.config_dir);
    warnings
        .iter()
        .for_each(|w| eprintln!("keycap: config: {}: {}", w.key, w.problem));
    let layers = Layers::builtin_only().with_system(layer);
    let (tx, rx) = mpsc::channel();
    let watcher = config::spawn(env.config_dir.clone(), tx)
        .map_err(|e| format!("not watching {}: {e}", env.config_dir.display()))
        .and_then(|watcher| {
            forward("config", rx, inbox.clone(), |(layer, warnings)| {
                Message::System(layer, warnings)
            })
            .map(|()| watcher)
            .map_err(|e| format!("config thread: {e}"))
        });
    match watcher {
        Ok(watcher) => (layers, Some(watcher)),
        Err(e) => {
            eprintln!("keycap: {e}");
            (layers, None)
        }
    }
}
