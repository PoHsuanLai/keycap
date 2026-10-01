//! A keyboard's and the daemon's life: held at start, unplugged mid-chord, killed mid-chord.

use crate::support::{FAKE_NAME, GRAB_WITHIN, Rig, Scene, ok};
use keycap_devtest::{ExternalGrab, FakeKeyboard, FakeSpec};
use keycap_protocol::Grab;
use std::time::Duration;

/// How long silence must last to count as silence.
const QUIET: Duration = Duration::from_millis(500);

#[test]
#[ignore = "creates input devices; run in CI with KEYCAP_DEVICE_TESTS=1"]
fn a_key_held_before_the_daemon_starts_delays_the_grab_until_it_is_released() {
    let scene = Scene::new("grab-gate");
    let fake = ok(
        FakeKeyboard::create(&FakeSpec::apple(FAKE_NAME)),
        "creating the fake keyboard",
    );
    ok(fake.press("a"), "holding a");

    let daemon = scene.daemon();
    let mut client = scene.client(&daemon);
    ok(
        client.wait_grab(FAKE_NAME, Grab::WaitingForRelease, GRAB_WITHIN),
        "status while a key is held",
    );
    let mut out = scene.output();
    ok(out.expect_silence(QUIET), "output while the key is held");
    // The gate only looks; nothing holds the keyboard, so another reader can grab it now.
    ok(
        ExternalGrab::take(fake.node()).and_then(ExternalGrab::release),
        "grabbing the fake keyboard while keycap waits for the key",
    );

    ok(fake.release("a"), "releasing a");
    ok(
        client.wait_grab(FAKE_NAME, Grab::Active, GRAB_WITHIN),
        "status after the release",
    );
    ok(out.expect_silence(QUIET), "output after the grab");

    ok(fake.press("leftmeta"), "pressing Command");
    ok(out.expect("+leftctrl SYN"), "Command after the grab");
    ok(fake.press("c"), "pressing c");
    ok(out.expect("+c SYN"), "c after the grab");
    ok(fake.release("c"), "releasing c");
    ok(fake.release("leftmeta"), "releasing Command");
    ok(out.expect("-c SYN -leftctrl SYN"), "the releases");
}

#[test]
#[ignore = "creates input devices; run in CI with KEYCAP_DEVICE_TESTS=1"]
fn unplugging_the_keyboard_mid_chord_releases_what_it_wrote() {
    let mut r = Rig::start("unplug-mid-chord");
    r.press("leftmeta", "+leftctrl SYN");
    assert_eq!(
        r.out.held(),
        ["leftctrl"],
        "Control is down before the unplug"
    );

    let Rig {
        fake,
        mut out,
        mut client,
        ..
    } = r;
    fake.destroy();
    ok(out.expect("-leftctrl SYN"), "after the unplug");
    assert!(out.held().is_empty(), "keys left down: {:?}", out.held());
    ok(
        client.wait_status("no keyboards", GRAB_WITHIN, |s| s.keyboards.is_empty()),
        "status after the unplug",
    );
}

#[test]
#[ignore = "creates input devices; run in CI with KEYCAP_DEVICE_TESTS=1"]
fn killing_the_daemon_mid_chord_removes_its_keyboard_and_a_restart_grabs_afresh() {
    let mut r = Rig::start("kill-mid-chord");
    r.press("leftmeta", "+leftctrl SYN");

    ok(r.daemon.kill(), "kill -9");
    ok(
        r.out.expect_gone(Duration::from_secs(5)),
        "the output keyboard after the kill",
    );
    // The grab went with the process: nobody holds the fake keyboard, and Command is still
    // down on it.
    ok(
        ExternalGrab::take(r.fake.node()).and_then(ExternalGrab::release),
        "grabbing the fake keyboard after the kill",
    );

    r.restart();
    r.wait_grab(Grab::WaitingForRelease);
    ok(
        r.out.expect_silence(QUIET),
        "output while Command is still held",
    );
    ok(r.fake.release("leftmeta"), "releasing Command");
    r.wait_grab(Grab::Active);
    ok(r.out.expect_silence(QUIET), "output after the new grab");

    r.press("leftmeta", "+leftctrl SYN");
    r.press("c", "+c SYN");
    r.release("c", "-c SYN");
    r.release("leftmeta", "-leftctrl SYN");
    r.assert_nothing_held();
}
