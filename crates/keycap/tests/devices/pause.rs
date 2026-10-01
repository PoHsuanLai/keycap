//! The emergency chord and `Resume`, end to end.

use crate::support::{Rig, ok};
use keycap_devtest::ExternalGrab;
use keycap_protocol::Mode;
use std::time::Duration;

const QUIET: Duration = Duration::from_millis(500);
const STATUS_WITHIN: Duration = Duration::from_secs(5);

#[test]
#[ignore = "creates input devices; run in CI with KEYCAP_DEVICE_TESTS=1"]
fn the_escape_chord_gives_the_keyboard_back_and_resume_takes_it_again() {
    let mut r = Rig::start("escape-chord");
    r.press("leftshift", "+leftshift SYN");
    r.press("rightshift", "");
    r.press("esc", "-leftshift SYN");
    ok(
        r.client
            .wait_status("Paused", STATUS_WITHIN, |s| s.mode == Mode::Paused),
        "status after the chord",
    );
    r.assert_nothing_held();

    // Everything is up, and nothing grabs the keyboard: another reader may take it now.
    ok(r.fake.release("esc"), "releasing Esc");
    ok(r.fake.release("rightshift"), "releasing right Shift");
    ok(r.fake.release("leftshift"), "releasing left Shift");
    ok(
        ExternalGrab::take(r.fake.node()).and_then(ExternalGrab::release),
        "grabbing the fake keyboard while paused",
    );
    ok(r.out.expect_silence(QUIET), "output after the releases");

    // Paused: Command+C goes nowhere through keycap.
    ok(r.fake.chord(&["leftmeta", "c"]), "typing Command+C");
    ok(r.out.expect_silence(QUIET), "output while paused");

    ok(r.client.resume(), "sending Resume");
    ok(
        r.client
            .wait_status("Running", STATUS_WITHIN, |s| s.mode == Mode::Running),
        "status after Resume",
    );
    r.wait_grab(keycap_protocol::Grab::Active);

    r.press("leftmeta", "+leftctrl SYN");
    r.press("c", "+c SYN");
    r.release("c", "-c SYN");
    r.release("leftmeta", "-leftctrl SYN");
    r.assert_nothing_held();
}
