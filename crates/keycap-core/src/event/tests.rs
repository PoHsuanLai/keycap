use super::*;
use crate::key::Key;

#[test]
fn edge_from_value_table() {
    const CASES: &[(i32, Option<Edge>)] = &[
        (0, Some(Edge::Release)),
        (1, Some(Edge::Press)),
        (2, Some(Edge::Repeat)),
        (3, None),
        (-1, None),
    ];
    for &(value, want) in CASES {
        assert_eq!(Edge::from_value(value), want, "{value}");
        if let Some(edge) = want {
            assert_eq!(edge.value(), value);
        }
    }
}

#[test]
fn out_events_compare_by_key_and_edge() {
    let press = OutEvent::Key {
        key: Logical(Key::ESC),
        edge: Edge::Press,
    };
    let release = OutEvent::Key {
        key: Logical(Key::ESC),
        edge: Edge::Release,
    };
    assert_ne!(press, release);
    assert_ne!(press, OutEvent::Sync);
    assert_eq!(press, press);
}
