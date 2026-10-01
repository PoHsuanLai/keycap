//! Lock LEDs written to the virtual keyboard, as a compositor does, reach the physical one.

use crate::support::{Rig, ok};
use keycap_devtest::LED_CAPSL;
use std::time::Duration;

const WITHIN: Duration = Duration::from_secs(3);

#[test]
#[ignore = "creates input devices; run in CI with KEYCAP_DEVICE_TESTS=1"]
fn caps_lock_written_to_the_virtual_keyboard_lights_the_physical_one() {
    let r = Rig::start("led-forwarding");
    // Whatever the kernel's console handler and the daemon wrote at the grab is not the answer.
    let _ = r.fake.leds();

    ok(r.out.write_led(LED_CAPSL, 1), "lighting Caps Lock");
    ok(
        r.fake.expect_led(LED_CAPSL, 1, WITHIN),
        "Caps Lock on the fake keyboard",
    );

    let _ = r.fake.leds();
    ok(r.out.write_led(LED_CAPSL, 0), "putting Caps Lock out");
    ok(
        r.fake.expect_led(LED_CAPSL, 0, WITHIN),
        "Caps Lock off on the fake keyboard",
    );
}
