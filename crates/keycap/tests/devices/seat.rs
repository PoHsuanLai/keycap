//! Who may steer keycap: the seat's owner, not any other local user.

use crate::support::{Rig, Scene, ok};
use keycap_devtest::FOCUS_SETTLE;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

/// The uid the observer client runs as: `nobody`, which owns no seat.
const OBSERVER_UID: u32 = 65534;

/// The built `socket_client` example, copied into the scratch directory so the observer's uid
/// can execute it whatever the permissions of the build directory above it.
fn observer_client(scene: &Scene) -> PathBuf {
    let exe = std::env::current_exe().expect("the test binary's own path");
    let built = exe
        .parent()
        .and_then(|deps| deps.parent())
        .map(|profile| profile.join("examples").join("socket_client"))
        .expect("the test binary lives in target/<profile>/deps");
    assert!(
        built.exists(),
        "{} is missing: build the examples first (`cargo build -p keycap --examples`)",
        built.display()
    );
    let copy = scene.dir().join("socket_client");
    std::fs::copy(&built, &copy).expect("copying socket_client into the scratch directory");
    std::fs::set_permissions(&copy, std::fs::Permissions::from_mode(0o755))
        .expect("making the copy executable");
    copy
}

#[test]
#[ignore = "creates input devices; run in CI with KEYCAP_DEVICE_TESTS=1"]
fn a_client_that_does_not_own_the_seat_cannot_change_the_profile() {
    if rustix::process::getuid().as_raw() != 0 {
        eprintln!(
            "SKIPPED: running the observer client as uid {OBSERVER_UID} needs root; this run is \
             uid {}",
            rustix::process::getuid().as_raw()
        );
        return;
    }
    let mut r = Rig::start("seat-observer");
    let client = observer_client(r.scene());

    let output = Command::new(&client)
        .arg(r.daemon.socket_path())
        .args(["focus", "terminal"])
        .env_clear()
        .uid(OBSERVER_UID)
        .gid(OBSERVER_UID)
        .output()
        .expect("running the observer client");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "the observer client failed: {}\nstdout: {stdout}\nstderr: {stderr}",
        output.status
    );
    assert_eq!(
        stdout.lines().last(),
        Some("closed"),
        "the daemon should hang up on an observer that sends a request.\nstdout: {stdout}\n\
         stderr: {stderr}"
    );
    std::thread::sleep(FOCUS_SETTLE);

    // The observer's Focus did nothing: Command+C is still plain Control+C.
    r.press("leftmeta", "+leftctrl SYN");
    r.press("c", "+c SYN");
    r.release("c", "-c SYN");
    r.release("leftmeta", "-leftctrl SYN");

    // The seat owner's own Focus, as a control, does change it.
    ok(r.client.focus("terminal"), "the owner's Focus");
    std::thread::sleep(FOCUS_SETTLE);
    r.press("leftmeta", "+leftctrl SYN");
    r.press("c", "+leftshift +c SYN");
    r.release("c", "-c -leftshift SYN");
    r.release("leftmeta", "-leftctrl SYN");
}
