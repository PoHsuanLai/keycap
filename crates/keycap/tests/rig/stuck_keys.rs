//! Random keyboards through the real profiles: the written stream must always be playable on a
//! device, and nothing may stay down once everything is released.

use crate::support::{
    Rig, focus, gone, pc_press, pc_release, press, release, release_all, repeat, resume,
};

use keycap::RemapInput;

struct Xorshift(u64);

impl Xorshift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

/// Role keys of both layouts, letters that have rules and some that do not, arrows, Tab,
/// Backspace and Esc (which fires the escape chord when both Shifts are down).
const KEYS: &[&str] = &[
    "leftmeta",
    "rightmeta",
    "leftalt",
    "rightalt",
    "leftctrl",
    "rightctrl",
    "leftshift",
    "rightshift",
    "a",
    "c",
    "v",
    "e",
    "q",
    "left",
    "right",
    "up",
    "down",
    "tab",
    "backspace",
    "delete",
    "space",
    "esc",
];
const PROFILES: &[&str] = &["default", "terminal", "unknown"];

/// Device 0 is an Apple keyboard, device 1 a PC one.
fn key_input(rng: &mut Xorshift) -> RemapInput {
    let dev = rng.below(2) as u32;
    let key = *rng.pick(KEYS);
    match (dev, rng.below(10)) {
        (0, 0..=4) => press(dev, key),
        (0, 5..=8) => release(dev, key),
        (0, _) => repeat(dev, key),
        (_, 0..=4) => pc_press(dev, key),
        _ => pc_release(dev, key),
    }
}

fn input(rng: &mut Xorshift) -> RemapInput {
    match rng.below(40) {
        0 => focus(rng.pick(PROFILES)),
        1 => gone(rng.below(2) as u32),
        2 => release_all(),
        3 => resume(),
        _ => key_input(rng),
    }
}

#[test]
fn random_inputs_never_stick_or_double_write() {
    let (mut lifts, mut escapes) = (0, 0);
    for seed in 1..=400u64 {
        let mut rng = Xorshift(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut rig = Rig::new();
        for step in 0..250 {
            let input = input(&mut rng);
            let label = format!("seed {seed} step {step}: {input:?}");
            let (_, text) = rig
                .try_feed(input)
                .unwrap_or_else(|why| panic!("{label}: {why}"));
            lifts += usize::from(text.contains("+home"));
            escapes += usize::from(text.contains("GRABS-OFF"));
        }
        rig.feed(release_all());
        assert_eq!(
            rig.down(),
            Vec::<&str>::new(),
            "seed {seed}: keys left down"
        );
    }
    assert!(lifts > 100, "the generator rarely reaches a lift: {lifts}");
    assert!(
        escapes > 5,
        "the generator rarely fires the escape chord: {escapes}"
    );
}
