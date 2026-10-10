//! Test support: a scratch directory and a fake keycap daemon on a private socket that speaks
//! the keymap part of the protocol. It needs no devices, so these tests run everywhere.

use keycap_protocol::{
    self as protocol, KeymapState, Mode, Registration, Status, ToClient, ToDaemon,
};
use rustix::fd::OwnedFd;
use rustix::net::{AddressFamily, RecvFlags, SendFlags, SocketAddrUnix, SocketType};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

static NEXT: AtomicU32 = AtomicU32::new(0);

/// An empty directory under the system temp dir, removed on drop.
pub(crate) struct Scratch(PathBuf);

impl Scratch {
    pub(crate) fn new(label: &str) -> Scratch {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("keycap-client-{label}-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("test scratch directory");
        Scratch(dir)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
struct Shared {
    keymap: KeymapState,
    watchers: Vec<Arc<OwnedFd>>,
    registered: Vec<(usize, Registration)>,
    next_id: usize,
}

/// A daemon that serves one keymap, watchers, and registrations by connection.
pub(crate) struct FakeDaemon {
    pub(crate) socket: PathBuf,
    shared: Arc<Mutex<Shared>>,
    _dir: Scratch,
}

fn send(fd: &OwnedFd, message: &ToClient) {
    let _ = rustix::net::send(fd, &protocol::encode(message), SendFlags::NOSIGNAL);
}

impl FakeDaemon {
    /// A daemon whose `Hello` says `version`, holding `keymap`.
    pub(crate) fn start(version: u16, keymap: KeymapState) -> FakeDaemon {
        let dir = Scratch::new("fake-daemon");
        let socket = dir.path().join("keycap.sock");
        let listener =
            rustix::net::socket(AddressFamily::UNIX, SocketType::SEQPACKET, None).expect("socket");
        rustix::net::bind(&listener, &SocketAddrUnix::new(&socket).expect("path")).expect("bind");
        rustix::net::listen(&listener, 16).expect("listen");
        let shared = Arc::new(Mutex::new(Shared {
            keymap,
            ..Shared::default()
        }));
        let accepting = Arc::clone(&shared);
        std::thread::spawn(move || {
            while let Ok(fd) = rustix::net::accept(&listener) {
                let shared = Arc::clone(&accepting);
                std::thread::spawn(move || serve(Arc::new(fd), version, &shared));
            }
        });
        FakeDaemon {
            socket,
            shared,
            _dir: dir,
        }
    }

    /// Replaces the keymap and tells every watcher.
    pub(crate) fn change(&self, keymap: KeymapState) {
        let mut shared = self.shared.lock().expect("lock");
        shared.keymap = keymap.clone();
        for watcher in &shared.watchers {
            send(watcher, &ToClient::Keymap(keymap.clone()));
        }
    }

    /// How many connections have asked to hear the keymap.
    pub(crate) fn watchers(&self) -> usize {
        self.shared.lock().expect("lock").watchers.len()
    }
}

fn serve(fd: Arc<OwnedFd>, version: u16, shared: &Mutex<Shared>) {
    let me = {
        let mut shared = shared.lock().expect("lock");
        shared.next_id += 1;
        shared.next_id
    };
    send(
        &fd,
        &ToClient::Hello {
            version,
            status: Status {
                mode: Mode::Running,
                keyboards: vec![],
            },
        },
    );
    let mut buf = vec![0u8; protocol::MAX_PACKET];
    while let Ok((n, _)) = rustix::net::recv(&*fd, &mut buf, RecvFlags::empty()) {
        if n == 0 {
            break;
        }
        let Ok(message) = protocol::decode::<ToDaemon>(&buf[..n]) else {
            break;
        };
        let mut shared = shared.lock().expect("lock");
        match message {
            ToDaemon::WatchKeymap => {
                shared.watchers.push(Arc::clone(&fd));
                send(&fd, &ToClient::Keymap(shared.keymap.clone()));
            }
            ToDaemon::Register(registration) => shared.registered.push((me, registration)),
            ToDaemon::ListRegistered => {
                let all = shared.registered.iter().map(|(_, r)| r.clone()).collect();
                send(&fd, &ToClient::Registered(all));
            }
            ToDaemon::Focus { .. } | ToDaemon::UserConfig { .. } | ToDaemon::Resume => {}
        }
    }
    let mut shared = shared.lock().expect("lock");
    shared.registered.retain(|(owner, _)| *owner != me);
    shared.watchers.retain(|w| !Arc::ptr_eq(w, &fd));
}
