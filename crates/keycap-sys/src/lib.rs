//! The low-level device calls the keycap daemon and its device-test harness both make: the
//! evdev and uinput ioctls behind safe handles (`sys`), and reading a nonblocking event node
//! until it has nothing queued (`drain`). No `unsafe`, no keycap concepts: raw codes and bytes
//! in, `io::Error` out.

mod drain;
mod sys;

pub use drain::{Drained, EVENT_BYTES, End, drain};
pub use sys::{
    AbsInfo, AbsSetup, BitSet, Capability, GrabError, InputId, KEY_STATE_BYTES, LED_STATE_BYTES,
    NAME_BYTES, UinputSetup, abs_info, capability_bits, dev_create, dev_destroy, grab, key_state,
    leds_lit, release, set_bit, sysname,
};
