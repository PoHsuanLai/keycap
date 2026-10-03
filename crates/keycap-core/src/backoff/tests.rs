use std::time::Duration;

use super::*;

fn secs(mut b: Backoff, n: usize) -> Vec<u64> {
    (0..n)
        .map(|_| {
            let (d, next) = b.step();
            b = next;
            d.as_secs()
        })
        .collect()
}

#[test]
fn backoff_sequence_table() {
    let cases: &[(&str, Backoff, &[u64])] = &[
        ("device", Backoff::DEVICE, &[1, 2, 4, 8, 16, 30, 30, 30]),
        (
            "short cap",
            Backoff::new(Duration::from_secs(1), Duration::from_secs(5)),
            &[1, 2, 4, 5, 5],
        ),
        (
            "cap below first",
            Backoff::new(Duration::from_secs(3), Duration::from_secs(2)),
            &[3, 2, 2],
        ),
    ];
    for (name, backoff, want) in cases {
        assert_eq!(secs(*backoff, want.len()), *want, "{name}");
    }
}

#[test]
fn reset_returns_to_the_first_delay() {
    let (_, b) = Backoff::DEVICE.step();
    let (_, b) = b.step();
    assert_eq!(b.step().0, Duration::from_secs(4));
    assert_eq!(b.reset(), Backoff::DEVICE);
}
