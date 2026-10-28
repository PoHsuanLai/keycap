//! `struct input_event` as bytes, and the rendering of a stream the way `keycap replay` prints
//! it: `+leftctrl +c SYN`.

use crate::keys::name_of;
use keycap_sys::EVENT_BYTES;

pub const EV_SYN: u16 = 0x00;
pub const EV_KEY: u16 = 0x01;
pub const EV_REL: u16 = 0x02;
pub const EV_ABS: u16 = 0x03;
pub const EV_LED: u16 = 0x11;
pub const SYN_REPORT: u16 = 0;
pub const LED_CAPSL: u16 = 0x01;

/// One input event without its timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    pub kind: u16,
    pub code: u16,
    pub value: i32,
}

impl Event {
    /// A key press (`value` 1) or release (0).
    pub const fn key(code: u16, value: i32) -> Self {
        Self {
            kind: EV_KEY,
            code,
            value,
        }
    }

    /// The `SYN_REPORT` that ends a frame.
    pub const SYNC: Event = Event {
        kind: EV_SYN,
        code: SYN_REPORT,
        value: 0,
    };

    fn to_bytes(self) -> [u8; EVENT_BYTES] {
        let mut b = [0u8; EVENT_BYTES];
        b[16..18].copy_from_slice(&self.kind.to_le_bytes());
        b[18..20].copy_from_slice(&self.code.to_le_bytes());
        b[20..24].copy_from_slice(&self.value.to_le_bytes());
        b
    }

    pub(crate) fn from_bytes(b: &[u8; EVENT_BYTES]) -> Self {
        Self {
            kind: u16::from_le_bytes([b[16], b[17]]),
            code: u16::from_le_bytes([b[18], b[19]]),
            value: i32::from_le_bytes([b[20], b[21], b[22], b[23]]),
        }
    }
}

/// The bytes a write of `events` carries.
pub(crate) fn encode(events: &[Event]) -> Vec<u8> {
    events.iter().flat_map(|e| e.to_bytes()).collect()
}

/// One event as a token: `+key`, `-key`, `~key` (repeat), `SYN`, `SYN_<n>` for the other sync
/// codes (a `SYN_DROPPED` must show), and `ev<type>:<code>=<value>` for the rest.
pub(crate) fn token(e: Event) -> String {
    match (e.kind, e.value) {
        (EV_KEY, 1) => format!("+{}", name_of(e.code)),
        (EV_KEY, 0) => format!("-{}", name_of(e.code)),
        (EV_KEY, 2) => format!("~{}", name_of(e.code)),
        (EV_SYN, _) if e.code == SYN_REPORT => "SYN".to_owned(),
        (EV_SYN, _) => format!("SYN_{}", e.code),
        _ => format!("ev{}:{}={}", e.kind, e.code, e.value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_read_like_the_replay_tool() {
        const CASES: &[(&str, Event, &str)] = &[
            ("press", Event::key(29, 1), "+leftctrl"),
            ("release", Event::key(46, 0), "-c"),
            ("repeat", Event::key(46, 2), "~c"),
            ("sync", Event::SYNC, "SYN"),
            (
                "dropped",
                Event {
                    kind: EV_SYN,
                    code: 3,
                    value: 0,
                },
                "SYN_3",
            ),
            (
                "led",
                Event {
                    kind: EV_LED,
                    code: 1,
                    value: 1,
                },
                "ev17:1=1",
            ),
        ];
        for &(name, e, want) in CASES {
            assert_eq!(token(e), want, "{name}");
        }
    }

    #[test]
    fn events_survive_the_byte_form() {
        let e = Event {
            kind: EV_KEY,
            code: 0x2ff,
            value: -2,
        };
        assert_eq!(Event::from_bytes(&e.to_bytes()), e);
        assert_eq!(encode(&[e, Event::SYNC]).len(), 2 * EVENT_BYTES);
    }
}
