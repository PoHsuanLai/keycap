use std::collections::BTreeSet;

use super::*;
use crate::event::{DeviceId, Edge, Logical, OutEvent, Physical};
use crate::key::Key;

const D0: DeviceId = DeviceId(0);
const D1: DeviceId = DeviceId(1);

fn at(device: DeviceId, code: u16) -> (DeviceId, Physical) {
    (device, Physical(Key::from_code(code)))
}

fn l(code: u16) -> Logical {
    Logical(Key::from_code(code))
}

fn press(code: u16) -> OutEvent {
    OutEvent::Key {
        key: l(code),
        edge: Edge::Press,
    }
}

fn release(code: u16) -> OutEvent {
    OutEvent::Key {
        key: l(code),
        edge: Edge::Release,
    }
}

#[test]
fn press_and_release_table() {
    // (case, wants, expected press events, expected release events)
    type Case = (&'static str, &'static [u16], &'static [u16], &'static [u16]);
    const CASES: &[Case] = &[
        ("single key", &[30], &[30], &[30]),
        (
            "modifier then key releases in reverse",
            &[29, 30],
            &[29, 30],
            &[30, 29],
        ),
        (
            "duplicate wants collapse",
            &[29, 29, 30],
            &[29, 30],
            &[30, 29],
        ),
        ("nothing wanted writes nothing", &[], &[], &[]),
    ];
    for &(name, wants, presses, releases) in CASES {
        let wants: Vec<Logical> = wants.iter().map(|&c| l(c)).collect();
        let (held, out) = Held::new().press(at(D0, 1), wants);
        assert_eq!(
            out,
            presses.iter().map(|&c| press(c)).collect::<Vec<_>>(),
            "{name}: press"
        );
        let (held, out) = held.release(at(D0, 1));
        assert_eq!(
            out,
            releases.iter().map(|&c| release(c)).collect::<Vec<_>>(),
            "{name}: release"
        );
        assert!(held.is_empty(), "{name}");
    }
}

#[test]
fn release_of_unheld_key_writes_nothing() {
    let (held, out) = Held::new().release(at(D0, 5));
    assert!(out.is_empty());
    assert!(held.is_empty());
}

#[test]
fn shared_key_is_pressed_once_and_released_by_last_holder() {
    let (held, out) = Held::new().press(at(D0, 125), vec![l(29)]);
    assert_eq!(out, vec![press(29)]);
    let (held, out) = held.press(at(D0, 126), vec![l(29), l(30)]);
    assert_eq!(out, vec![press(30)], "29 already down");
    let (held, out) = held.release(at(D0, 125));
    assert!(out.is_empty(), "29 still held by the other record");
    assert!(held.is_down(l(29)));
    let (held, out) = held.release(at(D0, 126));
    assert_eq!(out, vec![release(30), release(29)]);
    assert!(!held.is_down(l(29)));
}

#[test]
fn second_press_of_held_key_releases_old_record_first() {
    let (held, _) = Held::new().press(at(D0, 1), vec![l(29), l(30)]);
    let (held, out) = held.press(at(D0, 1), vec![l(31)]);
    assert_eq!(out, vec![release(30), release(29), press(31)]);
    assert_eq!(held.down().collect::<Vec<_>>(), vec![l(31)]);
}

#[test]
fn repress_with_same_keys_rewrites_them() {
    let (held, _) = Held::new().press(at(D0, 1), vec![l(30)]);
    let (_, out) = held.press(at(D0, 1), vec![l(30)]);
    assert_eq!(out, vec![release(30), press(30)]);
}

#[test]
fn device_gone_releases_only_that_device() {
    let (held, _) = Held::new().press(at(D0, 1), vec![l(29)]);
    let (held, _) = held.press(at(D1, 1), vec![l(30)]);
    let (held, out) = held.device_gone(D0);
    assert_eq!(out, vec![release(29)]);
    assert_eq!(held.down().collect::<Vec<_>>(), vec![l(30)]);
    assert!(held.is_held(at(D1, 1)));
    assert!(!held.is_held(at(D0, 1)));
}

#[test]
fn drain_releases_everything_and_empties() {
    let (held, _) = Held::new().press(at(D0, 1), vec![l(29), l(30)]);
    let (held, _) = held.press(at(D1, 2), vec![l(29), l(31)]);
    let (held, out) = held.drain();
    assert_eq!(out.len(), 3, "each of 29, 30, 31 released once: {out:?}");
    assert!(held.is_empty());
    assert_eq!(held.down().count(), 0);
}

struct Xorshift(u64);

impl Xorshift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// The written stream as a device sees it: replays events and fails on an impossible one.
#[derive(Default)]
struct Wire {
    down: BTreeSet<Logical>,
    presses: usize,
    releases: usize,
}

impl Wire {
    fn apply(&mut self, events: &[OutEvent], context: &str) {
        for event in events {
            match *event {
                OutEvent::Key {
                    key,
                    edge: Edge::Press,
                } => {
                    assert!(
                        self.down.insert(key),
                        "{context}: pressed {key:?} while down"
                    );
                    self.presses += 1;
                }
                OutEvent::Key {
                    key,
                    edge: Edge::Release,
                } => {
                    assert!(
                        self.down.remove(&key),
                        "{context}: released {key:?} while up"
                    );
                    self.releases += 1;
                }
                other => panic!("{context}: unexpected {other:?}"),
            }
        }
    }
}

#[test]
fn random_sequences_never_stick_or_double_write() {
    const DEVICES: [DeviceId; 2] = [D0, D1];
    // The shared pool overlaps across physical keys and holds Alt (56) and Super (125), whose
    // releases may carry a neutral tap; 10.. are private to a physical key.
    const POOL: [u16; 4] = [1, 2, 56, 125];
    for seed in 1..=300u64 {
        let mut rng = Xorshift(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut held = Held::new();
        let mut wire = Wire::default();
        for step in 0..120 {
            let context = format!("seed {seed} step {step}");
            let device = DEVICES[rng.below(2) as usize];
            let phys = rng.below(6) as u16;
            let keys = |rng: &mut Xorshift| -> Vec<Logical> {
                (0..rng.below(4))
                    .map(|_| l(POOL[rng.below(4) as usize]))
                    .collect()
            };
            let (next, events) = match rng.below(16) {
                0..=4 => {
                    let n = rng.below(4);
                    let wants = (0..n)
                        .map(|_| {
                            if rng.below(3) == 0 {
                                l(10 + phys)
                            } else {
                                l(POOL[rng.below(4) as usize])
                            }
                        })
                        .collect();
                    held.press(at(device, phys), wants)
                }
                5..=6 => held.lift(at(device, phys), keys(&mut rng)),
                7 => held.extend(at(device, phys), keys(&mut rng)),
                8 => held.unlift(at(device, phys)),
                9 => held.release_record(at(device, phys)),
                10..=12 => held.release(at(device, phys)),
                13 => held.device_gone(device),
                14 => held.drain(),
                _ => held.release(at(device, phys)),
            };
            held = next;
            wire.apply(&events, &context);
            assert!(
                held.lifts.values().flatten().all(|&key| held.holds(key)),
                "{context}: a lift outlived its key"
            );
            assert!(
                held.bare.keys().all(|&key| held.is_down(key)),
                "{context}: a bare key is not down"
            );
            let held_down: BTreeSet<Logical> = held.down().collect();
            assert_eq!(
                held_down, wire.down,
                "{context}: Held and the wire disagree"
            );
        }
        let (held, events) = held.drain();
        wire.apply(&events, &format!("seed {seed} drain"));
        assert!(wire.down.is_empty(), "seed {seed}: keys left down");
        assert!(held.is_empty(), "seed {seed}");
        assert_eq!(wire.presses, wire.releases, "seed {seed}: unpaired press");
    }
}
