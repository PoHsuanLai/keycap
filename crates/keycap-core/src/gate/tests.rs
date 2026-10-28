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
fn gate_check_table() {
    let none = bitmap(&[]);
    let first = bitmap(&[(0, 0b10)]);
    let last = bitmap(&[(KEYS_DOWN_BYTES - 1, 0x80)]);
    let cases = [
        ("pending, clear", false, none, Some(Decision::Grab)),
        ("pending, key down", false, first, Some(Decision::Wait)),
        ("pending, last bit down", false, last, Some(Decision::Wait)),
        ("grabbed, clear", true, none, None),
        ("grabbed, key down", true, first, None),
    ];
    for (name, past_gate, keys, decision) in cases {
        assert_eq!(check_gate(past_gate, keys), decision, "{name}");
    }
}

#[test]
fn any_sees_every_byte() {
    assert!(!bitmap(&[]).any());
    for i in 0..KEYS_DOWN_BYTES {
        assert!(bitmap(&[(i, 1)]).any(), "byte {i}");
    }
}
