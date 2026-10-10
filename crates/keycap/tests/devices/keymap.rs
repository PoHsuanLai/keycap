//! The keymap file through the real daemon: served, watched, a bad edit keeping the last good
//! copy, registrations ending with their connection.

use crate::support::Scene;
use chordkit::{AppAction, AppId, ChangeCallback, DefaultChord, KeymapSource};
use keycap_client::{KeycapSource, Origin};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const WITHIN: Duration = Duration::from_secs(10);

fn wait_for(what: &str, ready: impl Fn() -> bool) {
    let deadline = Instant::now() + WITHIN;
    while !ready() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn counter() -> (Arc<AtomicUsize>, ChangeCallback) {
    let count = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&count);
    let callback: ChangeCallback = Box::new(move || {
        seen.fetch_add(1, Ordering::SeqCst);
    });
    (count, callback)
}

#[test]
#[ignore = "creates input devices; run in CI with KEYCAP_DEVICE_TESTS=1"]
fn the_keymap_file_is_served_watched_and_a_bad_edit_keeps_the_last_good_copy() {
    let scene = Scene::new("keymap-file");
    let config = scene.dir().join("config");
    std::fs::create_dir_all(&config).expect("config dir");
    let file = config.join("keymap");
    std::fs::write(&file, "copy = Primary+Alt+C\n").expect("write");

    let daemon = scene.daemon();
    let source = KeycapSource::new(daemon.socket_path(), &file, "");
    let first = source.machine();
    assert_eq!(first.origin, Origin::Daemon);
    assert_eq!(first.overrides.entries().len(), 1);
    assert_eq!(first.problems, Vec::<String>::new());

    let (count, callback) = counter();
    let _watch = source.watch(callback).expect("watches");
    std::thread::sleep(Duration::from_millis(500));
    std::fs::write(&file, "paste = Primary+Alt+V\n").expect("write");
    wait_for("the change", || count.load(Ordering::SeqCst) >= 1);
    let second = source.machine();
    assert_eq!(second.overrides.entries()[0].action.id(), "paste");

    let before = count.load(Ordering::SeqCst);
    std::fs::write(&file, "this is not a keymap\n").expect("write");
    wait_for("the problem report", || {
        count.load(Ordering::SeqCst) > before
    });
    let kept = source.machine();
    assert_eq!(kept.overrides, second.overrides, "the last good copy stays");
    assert!(!kept.problems.is_empty());
    assert_eq!(kept.origin, Origin::Daemon);

    // It loads as a keymap too, with the person's change in it.
    let keymap = source
        .load(chordkit::Platform::Linux {
            desktop: chordkit::Desktop::Ours,
        })
        .expect("loads");
    assert!(!keymap.bindings(chordkit::Context::Normal).is_empty());
}

#[test]
#[ignore = "creates input devices; run in CI with KEYCAP_DEVICE_TESTS=1"]
fn registrations_are_listed_and_end_when_the_app_lets_go() {
    let scene = Scene::new("keymap-register");
    let daemon = scene.daemon();
    let source = KeycapSource::new(daemon.socket_path(), scene.dir().join("config/keymap"), "");
    let app = AppId::new("mail").expect("app");
    let compose = AppAction::new("mail.compose").expect("action");
    let chord: DefaultChord = "Primary+Shift+N".parse().expect("chord");

    let kept = source
        .register(&app, &[(compose, chord)])
        .expect("registers");
    wait_for("the registration", || {
        source.registered().is_ok_and(|all| all.len() == 1)
    });
    drop(kept);
    wait_for("the registration to end", || {
        source.registered().is_ok_and(|all| all.is_empty())
    });
}
