//! Mappings through the whole path: the kernel's evdev node in, the kernel's uinput device out.
//! The traces are the ones `tests/scenarios.rs` asserts on the pure core.

use crate::support::{Rig, ok};
use keycap_devtest::FOCUS_SETTLE;

#[test]
#[ignore = "creates input devices; run in CI with KEYCAP_DEVICE_TESTS=1"]
fn command_left_is_home_and_control_comes_back_on_release() {
    let mut r = Rig::start("command-left");
    r.press("leftmeta", "+leftctrl SYN");
    r.press("left", "-leftctrl +home SYN");
    r.release("left", "-home +leftctrl SYN");
    r.release("leftmeta", "-leftctrl SYN");
    r.assert_nothing_held();
}

#[test]
#[ignore = "creates input devices; run in CI with KEYCAP_DEVICE_TESTS=1"]
fn option_left_taps_a_neutral_key_before_it_lifts_alt() {
    let mut r = Rig::start("option-left");
    r.press("leftalt", "+leftalt SYN");
    r.press("left", "+f24 -f24 -leftalt +leftctrl +left SYN");
    r.release("left", "-left -leftctrl +leftalt SYN");
    r.release("leftalt", "+f24 -f24 -leftalt SYN");
    r.assert_nothing_held();
}

#[test]
#[ignore = "creates input devices; run in CI with KEYCAP_DEVICE_TESTS=1"]
fn focusing_terminal_over_the_socket_turns_command_c_into_control_shift_c() {
    let mut r = Rig::start("focus-terminal");
    // Before the focus, Command+C is plain Control+C.
    r.press("leftmeta", "+leftctrl SYN");
    r.press("c", "+c SYN");
    r.release("c", "-c SYN");
    r.release("leftmeta", "-leftctrl SYN");

    ok(r.client.focus("terminal"), "sending Focus");
    std::thread::sleep(FOCUS_SETTLE);

    r.press("leftmeta", "+leftctrl SYN");
    r.press("c", "+leftshift +c SYN");
    r.release("c", "-c -leftshift SYN");
    r.release("leftmeta", "-leftctrl SYN");
    r.assert_nothing_held();
}
