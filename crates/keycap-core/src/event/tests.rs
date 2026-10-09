use super::*;

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
