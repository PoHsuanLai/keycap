use super::model::KEYS_DOWN_BYTES;
use super::*;

fn bitmap(set: &[(usize, u8)]) -> KeysDown {
    let mut bytes = [0u8; KEYS_DOWN_BYTES];
    for &(i, v) in set {
        bytes[i] = v;
    }
    KeysDown::from_bytes(bytes)
}

#[test]
fn gate_step_table() {
    let none = bitmap(&[]);
    let first = bitmap(&[(0, 0b10)]);
    let last = bitmap(&[(KEYS_DOWN_BYTES - 1, 0x80)]);
    let cases = [
        (
            "pending, clear",
            GrabGate::Pending,
            none,
            GrabGate::Grabbed,
            Decision::Grab,
        ),
        (
            "pending, key down",
            GrabGate::Pending,
            first,
            GrabGate::Pending,
            Decision::Wait,
        ),
        (
            "pending, last bit down",
            GrabGate::Pending,
            last,
            GrabGate::Pending,
            Decision::Wait,
        ),
        (
            "grabbed, clear",
            GrabGate::Grabbed,
            none,
            GrabGate::Grabbed,
            Decision::Keep,
        ),
        (
            "grabbed, key down",
            GrabGate::Grabbed,
            first,
            GrabGate::Grabbed,
            Decision::Keep,
        ),
    ];
    for (name, gate, keys, next, decision) in cases {
        assert_eq!(gate.step(keys), (next, decision), "{name}");
    }
}

#[test]
fn any_sees_every_byte() {
    assert!(!bitmap(&[]).any());
    for i in 0..KEYS_DOWN_BYTES {
        assert!(bitmap(&[(i, 1)]).any(), "byte {i}");
    }
}
