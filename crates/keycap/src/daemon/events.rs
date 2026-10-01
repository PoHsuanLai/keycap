//! `struct input_event` and the two directions of translation: what a grabbed device sent
//! (`decode`) and what keycap writes (`encode`). Pure.

use crate::key::Key;
use crate::{Edge, Logical, OutEvent, Physical};

pub(crate) const EV_SYN: u16 = 0x00;
pub(crate) const EV_KEY: u16 = 0x01;
pub(crate) const EV_REL: u16 = 0x02;
pub(crate) const EV_LED: u16 = 0x11;
pub(crate) const SYN_REPORT: u16 = 0;
/// The kernel's marker that a client's event queue overflowed and events were lost.
pub(crate) const SYN_DROPPED: u16 = 3;
pub(crate) const REL_X: u16 = 0x00;
pub(crate) const REL_Y: u16 = 0x01;
pub(crate) const REL_HWHEEL: u16 = 0x06;
pub(crate) const REL_WHEEL: u16 = 0x08;
pub(crate) const REL_WHEEL_HI_RES: u16 = 0x0b;
pub(crate) const REL_HWHEEL_HI_RES: u16 = 0x0c;
/// The highest key or button code keycap's virtual keyboard registers.
pub(crate) const MAX_CODE: u16 = 0x2ff;

/// Bytes of a `struct input_event` on a 64-bit kernel: a 16-byte timestamp, then type, code
/// and value.
pub(crate) const EVENT_BYTES: usize = 24;

/// One `struct input_event` without its timestamp (the kernel stamps uinput writes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InputEvent {
    pub kind: u16,
    pub code: u16,
    pub value: i32,
}

impl InputEvent {
    /// The `SYN_REPORT` that ends a frame.
    pub(crate) const SYNC: InputEvent = InputEvent {
        kind: EV_SYN,
        code: SYN_REPORT,
        value: 0,
    };

    /// The event as written to uinput, with a zero timestamp.
    pub(crate) fn to_bytes(self) -> [u8; EVENT_BYTES] {
        let mut b = [0u8; EVENT_BYTES];
        b[16..18].copy_from_slice(&self.kind.to_le_bytes());
        b[18..20].copy_from_slice(&self.code.to_le_bytes());
        b[20..24].copy_from_slice(&self.value.to_le_bytes());
        b
    }

    /// The event as read from evdev; the timestamp is dropped.
    pub(crate) fn from_bytes(b: &[u8; EVENT_BYTES]) -> Self {
        Self {
            kind: u16::from_le_bytes([b[16], b[17]]),
            code: u16::from_le_bytes([b[18], b[19]]),
            value: i32::from_le_bytes([b[20], b[21], b[22], b[23]]),
        }
    }
}

/// The events in a buffer read from an evdev node: whole 24-byte records, a trailing partial
/// record ignored (the kernel never returns one).
pub(crate) fn parse(bytes: &[u8]) -> Vec<InputEvent> {
    let (records, _partial) = bytes.as_chunks::<EVENT_BYTES>();
    records.iter().map(InputEvent::from_bytes).collect()
}

/// What a device event means to keycap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Decoded {
    /// A keyboard key transition.
    Key(Physical, Edge),
    /// A pointer event (relative motion or a mouse button) on a combo device: written back
    /// unchanged.
    Pointer(InputEvent),
    /// `SYN_REPORT`.
    Sync,
    /// `SYN_DROPPED`: events were lost before this point and what follows, up to the next
    /// `SYN_REPORT`, is a fragment.
    Dropped,
    /// Anything else (`MSC_SCAN`, LEDs, a key with no valid value).
    Other,
}

/// Whether `code` names a button (`BTN_*`) rather than a key. Buttons occupy 0x100..0x160
/// (misc, mouse, joystick, gamepad, digitiser, wheel) and the trigger-happy block at 0x2c0;
/// the codes between them, from `KEY_OK` on, are keys.
pub(crate) const fn is_button(code: u16) -> bool {
    matches!(code, 0x100..=0x15f | 0x2c0..=0x2ff)
}

/// Classifies one event read from a grabbed device.
pub(crate) fn decode(e: InputEvent) -> Decoded {
    match (e.kind, e.code) {
        (EV_SYN, SYN_REPORT) => Decoded::Sync,
        (EV_SYN, SYN_DROPPED) => Decoded::Dropped,
        (EV_REL, _) => Decoded::Pointer(e),
        (EV_KEY, code) if is_button(code) => Decoded::Pointer(e),
        (EV_KEY, 0) => Decoded::Other,
        (EV_KEY, code) => Edge::from_value(e.value).map_or(Decoded::Other, |edge| {
            Decoded::Key(Physical(Key::from_code(code)), edge)
        }),
        _ => Decoded::Other,
    }
}

/// The events written for a batch of output.
pub(crate) fn encode(batch: &[OutEvent]) -> Vec<InputEvent> {
    batch
        .iter()
        .map(|e| match *e {
            OutEvent::Key {
                key: Logical(key),
                edge,
            } => InputEvent {
                kind: EV_KEY,
                code: key.code(),
                value: edge.value(),
            },
            OutEvent::Sync => InputEvent {
                kind: EV_SYN,
                code: SYN_REPORT,
                value: 0,
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn ev(kind: u16, code: u16, value: i32) -> InputEvent {
        InputEvent { kind, code, value }
    }

    #[test]
    fn bytes_round_trip_and_sit_after_the_timestamp() {
        const CASES: &[InputEvent] = &[
            ev(EV_KEY, 30, 1),
            ev(EV_KEY, 0x2ff, 0),
            ev(EV_REL, REL_WHEEL, -1),
            ev(EV_SYN, SYN_REPORT, 0),
            ev(0xffff, 0xffff, i32::MIN),
        ];
        for &e in CASES {
            let bytes = e.to_bytes();
            assert_eq!(InputEvent::from_bytes(&bytes), e, "{e:?}");
            assert_eq!(bytes[..16], [0u8; 16], "{e:?}: zero timestamp");
        }
        assert_eq!(
            ev(EV_KEY, 0x1e, 1).to_bytes()[16..],
            [1, 0, 0x1e, 0, 1, 0, 0, 0]
        );
    }

    #[test]
    fn parse_splits_a_read_into_events_and_drops_a_partial_tail() {
        let events = [
            ev(EV_KEY, 30, 1),
            ev(EV_KEY, 30, 0),
            ev(EV_SYN, SYN_REPORT, 0),
        ];
        let mut bytes: Vec<u8> = events.iter().flat_map(|e| e.to_bytes()).collect();
        assert_eq!(parse(&bytes), events);
        bytes.extend([1, 2, 3]);
        assert_eq!(parse(&bytes), events, "partial record dropped");
        assert_eq!(parse(&[]), []);
        assert_eq!(InputEvent::SYNC, ev(EV_SYN, SYN_REPORT, 0));
    }

    #[test]
    fn from_bytes_ignores_the_timestamp() {
        let mut b = ev(EV_KEY, 57, 2).to_bytes();
        b[..16].fill(0xaa);
        assert_eq!(InputEvent::from_bytes(&b), ev(EV_KEY, 57, 2));
    }

    #[test]
    fn decode_table() {
        let key = |code, edge| Decoded::Key(Physical(Key::from_code(code)), edge);
        let ptr = |kind, code, value| Decoded::Pointer(ev(kind, code, value));
        let cases: &[(&str, InputEvent, Decoded)] = &[
            ("a pressed", ev(EV_KEY, 30, 1), key(30, Edge::Press)),
            ("a released", ev(EV_KEY, 30, 0), key(30, Edge::Release)),
            ("a repeating", ev(EV_KEY, 30, 2), key(30, Edge::Repeat)),
            ("key with a bad value", ev(EV_KEY, 30, 3), Decoded::Other),
            ("reserved code 0", ev(EV_KEY, 0, 1), Decoded::Other),
            (
                "KEY_OK is a key",
                ev(EV_KEY, 0x160, 1),
                key(0x160, Edge::Press),
            ),
            ("KEY_MICMUTE", ev(EV_KEY, 248, 1), key(248, Edge::Press)),
            ("BTN_MISC", ev(EV_KEY, 0x100, 1), ptr(EV_KEY, 0x100, 1)),
            ("BTN_LEFT", ev(EV_KEY, 0x110, 1), ptr(EV_KEY, 0x110, 1)),
            ("BTN_TOUCH", ev(EV_KEY, 0x14a, 0), ptr(EV_KEY, 0x14a, 0)),
            (
                "last low button",
                ev(EV_KEY, 0x15f, 1),
                ptr(EV_KEY, 0x15f, 1),
            ),
            (
                "BTN_TRIGGER_HAPPY1",
                ev(EV_KEY, 0x2c0, 1),
                ptr(EV_KEY, 0x2c0, 1),
            ),
            ("REL_X", ev(EV_REL, REL_X, 5), ptr(EV_REL, REL_X, 5)),
            (
                "REL_HWHEEL_HI_RES",
                ev(EV_REL, REL_HWHEEL_HI_RES, -120),
                ptr(EV_REL, REL_HWHEEL_HI_RES, -120),
            ),
            ("SYN_REPORT", ev(EV_SYN, SYN_REPORT, 0), Decoded::Sync),
            ("SYN_DROPPED", ev(EV_SYN, 3, 0), Decoded::Dropped),
            ("SYN_CONFIG", ev(EV_SYN, 1, 0), Decoded::Other),
            ("MSC_SCAN", ev(0x04, 4, 458_756), Decoded::Other),
            ("LED", ev(0x11, 1, 1), Decoded::Other),
        ];
        for &(name, input, want) in cases {
            assert_eq!(decode(input), want, "{name}");
        }
    }

    #[test]
    fn encode_writes_keys_with_their_edge_and_sync_as_report() {
        let key = |code, edge| OutEvent::Key {
            key: Logical(Key::from_code(code)),
            edge,
        };
        let batch = [
            key(29, Edge::Press),
            key(46, Edge::Press),
            key(46, Edge::Release),
            key(29, Edge::Repeat),
            OutEvent::Sync,
        ];
        assert_eq!(
            encode(&batch),
            [
                ev(EV_KEY, 29, 1),
                ev(EV_KEY, 46, 1),
                ev(EV_KEY, 46, 0),
                ev(EV_KEY, 29, 2),
                ev(EV_SYN, SYN_REPORT, 0),
            ]
        );
        assert_eq!(encode(&[]), []);
    }

    #[test]
    fn keys_survive_encode_then_decode() {
        for code in [1u16, 30, 57, 125, 248, 0x160, 0x2bf] {
            for edge in [Edge::Press, Edge::Release, Edge::Repeat] {
                let out = OutEvent::Key {
                    key: Logical(Key::from_code(code)),
                    edge,
                };
                let wire = encode(&[out])[0];
                let back = decode(InputEvent::from_bytes(&wire.to_bytes()));
                assert_eq!(back, Decoded::Key(Physical(Key::from_code(code)), edge));
            }
        }
        let sync = encode(&[OutEvent::Sync])[0];
        assert_eq!(decode(sync), Decoded::Sync);
    }
}
