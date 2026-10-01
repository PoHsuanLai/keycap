//! Another program holding the keyboard.

use crate::support::{FAKE_NAME, GRAB_WITHIN, Scene, ok};
use keycap_devtest::{ExternalGrab, FakeKeyboard, FakeSpec};
use keycap_protocol::Grab;
use std::time::Duration;

const QUIET: Duration = Duration::from_millis(500);
/// The retry backs off from one second; a few rounds fit in this.
const RETRY_WITHIN: Duration = Duration::from_secs(20);

#[test]
#[ignore = "creates input devices; run in CI with KEYCAP_DEVICE_TESTS=1"]
fn a_keyboard_another_program_grabbed_is_a_conflict_until_it_lets_go() {
    let scene = Scene::new("grab-conflict");
    let fake = ok(
        FakeKeyboard::create(&FakeSpec::apple(FAKE_NAME)),
        "creating the fake keyboard",
    );
    let other = ok(
        ExternalGrab::take(fake.node()),
        "grabbing the fake keyboard before keycap",
    );

    let daemon = scene.daemon();
    let mut client = scene.client(&daemon);
    ok(
        client.wait_grab(FAKE_NAME, Grab::Conflict, GRAB_WITHIN),
        "status while another program holds the keyboard",
    );
    let mut out = scene.output();
    ok(fake.chord(&["leftmeta", "c"]), "typing Command+C");
    ok(out.expect_silence(QUIET), "output during the conflict");

    ok(other.release(), "letting go");
    ok(
        client.wait_grab(FAKE_NAME, Grab::Active, RETRY_WITHIN),
        "status after the other program let go",
    );
    ok(fake.press("leftmeta"), "pressing Command");
    ok(out.expect("+leftctrl SYN"), "Command after the retry");
    ok(fake.release("leftmeta"), "releasing Command");
    ok(out.expect("-leftctrl SYN"), "Command released");
}
