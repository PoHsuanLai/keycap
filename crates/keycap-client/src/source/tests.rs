//! `KeycapSource` against a fake daemon, with the file fallback and without any file.

use super::*;
use crate::testing::{FakeDaemon, Scratch};
use chordkit::{Action, Context, StandardAction};
use keycap_protocol::{KeymapState, PROTOCOL_VERSION};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const WAIT: Duration = Duration::from_secs(5);

fn state(overrides: &str) -> KeymapState {
    KeymapState {
        overrides: overrides.to_owned(),
        problems: Vec::new(),
    }
}

fn chords(keymap: &Keymap, action: StandardAction) -> Vec<chordkit::Chord> {
    keymap.chords_of(&Action::Standard(action))
}

fn alt_copy() -> chordkit::Chord {
    "Primary+Alt+C"
        .parse::<chordkit::DefaultChord>()
        .expect("reads")
        .resolve(OURS, Context::Normal)
        .expect("resolves")
}

/// A source with no daemon and the keymap file in `dir` holding `text` (none: no file).
fn file_only(dir: &Scratch, text: Option<&str>) -> KeycapSource {
    let file = dir.path().join("keymap");
    if let Some(text) = text {
        std::fs::write(&file, text).expect("write");
    }
    KeycapSource::new(dir.path().join("no-daemon.sock"), file, "")
}

fn wait_for(what: &str, ready: impl Fn() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !ready() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn with_a_daemon_the_persons_keymap_comes_from_it_and_not_from_the_file() {
    let daemon = FakeDaemon::start(PROTOCOL_VERSION, state("copy = Primary+Alt+C\n"));
    let dir = Scratch::new("source-daemon");
    let file = dir.path().join("keymap");
    std::fs::write(&file, "copy = Primary+Alt+X\n").expect("write");
    let source = KeycapSource::new(daemon.socket.clone(), file, "");

    assert_eq!(source.machine().origin, Origin::Daemon);
    let keymap = source.load(OURS).expect("loads");
    assert_eq!(chords(&keymap, StandardAction::Copy), [alt_copy()]);
}

#[test]
fn without_a_daemon_the_file_is_read_directly() {
    let dir = Scratch::new("source-file");
    let source = file_only(&dir, Some("copy = Primary+Alt+C\n"));
    assert_eq!(source.machine().origin, Origin::MachineFile);
    let keymap = source.load(OURS).expect("loads");
    assert_eq!(chords(&keymap, StandardAction::Copy), [alt_copy()]);
}

#[test]
fn a_daemon_too_old_for_the_keymap_means_the_file_too() {
    let daemon = FakeDaemon::start(1, state("copy = Primary+Alt+X\n"));
    let dir = Scratch::new("source-old");
    let file = dir.path().join("keymap");
    std::fs::write(&file, "copy = Primary+Alt+C\n").expect("write");
    let source = KeycapSource::new(daemon.socket.clone(), file, "");
    assert_eq!(source.machine().origin, Origin::MachineFile);
    assert_eq!(
        chords(&source.load(OURS).expect("loads"), StandardAction::Copy),
        [alt_copy()]
    );
}

#[test]
fn with_no_daemon_and_no_file_the_keymap_is_the_conventions_and_the_desktops_bindings() {
    let dir = Scratch::new("source-nothing");
    let keymap = file_only(&dir, None).load(OURS).expect("loads");
    let expected = Keymap::conventional(OURS).with_system(system::system_bindings());
    assert_eq!(
        keymap.bindings(Context::Normal),
        expected.bindings(Context::Normal)
    );
    assert_eq!(chords(&keymap, StandardAction::Launcher).len(), 2);
    assert_eq!(chords(&keymap, StandardAction::Copy).len(), 1);
}

#[test]
fn the_desktops_bindings_only_apply_on_our_desktop() {
    let dir = Scratch::new("source-windows");
    let keymap = file_only(&dir, None)
        .load(Platform::Windows)
        .expect("loads");
    let plain = Keymap::conventional(Platform::Windows);
    assert_eq!(
        keymap.bindings(Context::Normal),
        plain.bindings(Context::Normal)
    );
}

#[test]
fn a_chord_the_desktop_keeps_is_refused_once_and_a_bad_line_is_reported() {
    let dir = Scratch::new("source-problems");
    let source = file_only(&dir, Some("copy = Ctrl+1\nteleport = F5\n"));
    let (keymap, problems) = source.load_reporting(OURS);
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(problems.iter().any(|p| p.contains("teleport")));
    assert!(problems.iter().any(|p| p.contains("Space 1")));
    assert!(!chords(&keymap, StandardAction::Copy).is_empty());
}

#[test]
fn problems_the_daemon_reports_are_passed_on_without_repeating_the_refusals() {
    let served = KeymapState {
        overrides: "copy = Ctrl+1\n".to_owned(),
        problems: vec![keycap_protocol::KeymapProblem {
            line: 4,
            what: "Something was wrong.".to_owned(),
        }],
    };
    let daemon = FakeDaemon::start(PROTOCOL_VERSION, served);
    let source = KeycapSource::new(daemon.socket.clone(), "/nonexistent/keymap", "");
    let (_, problems) = source.load_reporting(OURS);
    assert_eq!(problems[0], "System keymap: line 4: Something was wrong.");
    assert_eq!(problems.len(), 2, "{problems:?}");
}

#[test]
fn the_machine_source_reads_its_paths_from_the_environment() {
    let pairs = [
        ("KEYCAP_CONFIG_DIR", "/tmp/cfg"),
        ("KEYCAP_SOCKET", "/tmp/k.sock"),
        ("XDG_CONFIG_HOME", "/tmp/xdg"),
        ("HOME", "/home/me"),
    ];
    let lookup = |name: &str| {
        pairs
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| (*v).to_owned())
    };
    let source = KeycapSource::from_lookup(lookup);
    assert_eq!(source.socket(), Path::new("/tmp/k.sock"));
    assert_eq!(source.machine_file, Path::new("/tmp/cfg/keymap"));
    assert_eq!(source.person_file(), Path::new("/tmp/xdg/keycap/keymap"));
    let home_only =
        KeycapSource::from_lookup(|name| (name == "HOME").then(|| "/home/me".to_owned()));
    assert_eq!(
        home_only.person_file(),
        Path::new("/home/me/.config/keycap/keymap")
    );
    let default = KeycapSource::from_lookup(|_| None);
    assert_eq!(default.person_file(), Path::new(""));
    assert_eq!(default.socket(), Path::new(SOCKET_PATH));
    assert_eq!(default.machine_file, Path::new("/etc/keycap/keymap"));
}

fn counter() -> (Arc<AtomicUsize>, ChangeCallback) {
    let count = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&count);
    (
        count,
        Box::new(move || {
            seen.fetch_add(1, Ordering::SeqCst);
        }),
    )
}

#[test]
fn watch_fires_when_the_daemon_says_the_keymap_changed_and_not_for_the_first_state() {
    let daemon = FakeDaemon::start(PROTOCOL_VERSION, state("copy = Primary+Alt+C\n"));
    let source = KeycapSource::new(daemon.socket.clone(), "/nonexistent/keymap", "");
    let (count, callback) = counter();
    let _watch = source.watch(callback).expect("watches");
    wait_for("the watcher to subscribe", || daemon.watchers() == 1);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        count.load(Ordering::SeqCst),
        0,
        "the first state is not a change"
    );

    daemon.change(state("paste = Primary+Alt+V\n"));
    wait_for("the callback", || count.load(Ordering::SeqCst) == 1);
}

#[test]
fn with_no_daemon_watch_fires_when_the_file_changes() {
    let dir = Scratch::new("source-watch-file");
    let source = file_only(&dir, Some("copy = Primary+Alt+C\n"));
    let (count, callback) = counter();
    let _watch = source.watch(callback).expect("watches");
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(count.load(Ordering::SeqCst), 0);

    std::fs::write(
        dir.path().join("keymap"),
        "copy = Primary+Alt+C\nredo = none\n",
    )
    .expect("write");
    wait_for("the callback", || count.load(Ordering::SeqCst) >= 1);
}

#[test]
fn dropping_the_watch_ends_it() {
    let dir = Scratch::new("source-watch-drop");
    let source = file_only(&dir, Some("copy = Primary+Alt+C\n"));
    let (count, callback) = counter();
    let watch = source.watch(callback).expect("watches");
    drop(watch);
    std::thread::sleep(Duration::from_millis(1200));
    std::fs::write(dir.path().join("keymap"), "redo = none\n").expect("write");
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

#[test]
fn registered_actions_are_listed_and_end_with_their_registration() {
    let daemon = FakeDaemon::start(PROTOCOL_VERSION, KeymapState::default());
    let source = KeycapSource::new(daemon.socket.clone(), "/nonexistent/keymap", "");
    let app = chordkit::AppId::new("mail").expect("app");
    let compose = chordkit::AppAction::new("mail.compose").expect("action");
    let chord: chordkit::DefaultChord = "Primary+Shift+N".parse().expect("chord");

    let kept = source
        .register(&app, &[(compose, chord)])
        .expect("registers");
    wait_for("the registration", || {
        source.registered().is_ok_and(|all| all.len() == 1)
    });
    let all = source.registered().expect("lists");
    assert_eq!(all[0].app, "mail");
    assert_eq!(all[0].actions[0].id, "mail.compose");
    assert_eq!(all[0].actions[0].chord, chord.canonical());

    drop(kept);
    wait_for("the registration to end", || {
        source.registered().is_ok_and(|all| all.is_empty())
    });
}

#[test]
fn registering_with_an_absent_daemon_says_so() {
    let app = chordkit::AppId::new("mail").expect("app");
    let absent = KeycapSource::new("/nonexistent/keycap.sock", "/nonexistent/keymap", "");
    assert!(matches!(
        absent.register(&app, &[]),
        Err(crate::ClientError::Connect { .. })
    ));
}

#[test]
fn the_persons_file_wins_over_the_machines_and_problems_say_whose_they_are() {
    let dir = Scratch::new("source-person");
    let machine = dir.path().join("machine");
    let person = dir.path().join("person");
    std::fs::write(&machine, "copy = Primary+Alt+X\nundo = none\nbogus = F5\n").expect("write");
    std::fs::write(&person, "copy = Primary+Alt+C\nteleport = F5\n").expect("write");
    let source = KeycapSource::new(dir.path().join("no-daemon.sock"), &machine, &person);

    let (keymap, problems) = source.load_reporting(OURS);
    assert_eq!(chords(&keymap, StandardAction::Copy), [alt_copy()]);
    assert!(
        chords(&keymap, StandardAction::Undo).is_empty(),
        "the machine's change stands"
    );
    let mine: Vec<_> = problems
        .iter()
        .filter(|p| p.starts_with("Your keymap: "))
        .collect();
    let theirs: Vec<_> = problems
        .iter()
        .filter(|p| p.starts_with("System keymap: "))
        .collect();
    assert_eq!((mine.len(), theirs.len()), (1, 1), "{problems:?}");
    assert!(mine[0].contains("teleport") && theirs[0].contains("bogus"));
    assert_eq!(source.person().origin, Origin::PersonFile);
}

#[test]
fn watch_fires_when_the_persons_file_changes_with_or_without_a_daemon() {
    let daemon = FakeDaemon::start(PROTOCOL_VERSION, KeymapState::default());
    let dir = Scratch::new("source-person-watch");
    let person = dir.path().join("person");
    std::fs::write(&person, "copy = Primary+Alt+C\n").expect("write");
    let source = KeycapSource::new(daemon.socket.clone(), "/nonexistent/keymap", &person);
    let (count, callback) = counter();
    let _watch = source.watch(callback).expect("watches");
    wait_for("the watcher to subscribe", || daemon.watchers() == 1);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(count.load(Ordering::SeqCst), 0);

    std::fs::write(&person, "copy = Primary+Alt+C\nredo = none\n").expect("write");
    wait_for("the callback", || count.load(Ordering::SeqCst) >= 1);

    let alone = Scratch::new("source-person-watch-alone");
    let file = alone.path().join("person");
    let source = KeycapSource::new(
        alone.path().join("no-daemon.sock"),
        "/nonexistent/keymap",
        &file,
    );
    let (count, callback) = counter();
    let _watch = source.watch(callback).expect("watches");
    std::thread::sleep(Duration::from_millis(100));
    std::fs::write(&file, "redo = none\n").expect("write");
    wait_for("the callback", || count.load(Ordering::SeqCst) >= 1);
}
