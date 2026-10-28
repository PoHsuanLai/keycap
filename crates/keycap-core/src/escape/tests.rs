use super::*;
use crate::event::{DeviceId, Edge, Physical};
use crate::key::Key;

const LS: Physical = Physical(Key::LEFTSHIFT);
const RS: Physical = Physical(Key::RIGHTSHIFT);
const ESC: Physical = Physical(Key::ESC);
const A: Physical = Physical(Key::from_code(30));

const D0: DeviceId = DeviceId(0);
const D1: DeviceId = DeviceId(1);

use Edge::{Press as P, Release as R, Repeat as T};
// Whether the step fired the chord.
const F: bool = true;
const Q: bool = false;

#[test]
fn escape_script_table() {
    type Script = &'static [(Physical, Edge, bool)];
    let cases: &[(&str, Script)] = &[
        (
            "shift, shift, esc fires",
            &[(LS, P, Q), (RS, P, Q), (ESC, P, F)],
        ),
        (
            "right shift first also fires",
            &[(RS, P, Q), (LS, P, Q), (ESC, P, F)],
        ),
        (
            "esc first never fires",
            &[(ESC, P, Q), (LS, P, Q), (RS, P, Q)],
        ),
        (
            "esc between the shifts is quiet",
            &[(LS, P, Q), (ESC, P, Q), (RS, P, Q)],
        ),
        ("one shift is not enough", &[(LS, P, Q), (ESC, P, Q)]),
        (
            "released shift disarms",
            &[(LS, P, Q), (RS, P, Q), (RS, R, Q), (ESC, P, Q)],
        ),
        (
            "repeat of esc is quiet",
            &[(LS, P, Q), (RS, P, Q), (ESC, P, F), (ESC, T, Q)],
        ),
        (
            "repeat does not press",
            &[(LS, P, Q), (RS, T, Q), (ESC, P, Q)],
        ),
        (
            "other keys are ignored",
            &[(LS, P, Q), (RS, P, Q), (A, P, Q), (ESC, P, F)],
        ),
        (
            "second esc press fires again",
            &[
                (LS, P, Q),
                (RS, P, Q),
                (ESC, P, F),
                (ESC, R, Q),
                (ESC, P, F),
            ],
        ),
    ];
    for (name, script) in cases {
        let mut state = Escape::new();
        for (i, &(key, edge, want)) in script.iter().enumerate() {
            let (next, got) = state.step(D0, key, edge);
            assert_eq!(got, want, "{name}: step {i}");
            state = next;
        }
    }
}

#[test]
fn shifts_across_keyboards_arm_the_chord_until_a_keyboard_goes_or_lets_go() {
    // Step 1: shifts on two keyboards arm the chord.
    let (state, _) = Escape::new().step(D0, LS, P);
    let (armed, _) = state.step(D1, RS, P);
    assert_eq!(armed.step(D1, ESC, P).1, F, "two keyboards arm");

    // Step 2: a vanished keyboard's keys no longer count.
    let (state, _) = Escape::new().step(D0, LS, P);
    let (state, _) = state.step(D1, RS, P);
    let gone = state.device_gone(D0);
    assert_eq!(gone.step(D1, ESC, P).1, Q, "vanished keyboard");

    // Step 3: a release on one keyboard leaves the other keyboard's shift down.
    let (state, _) = Escape::new().step(D0, LS, P);
    let (state, _) = state.step(D1, LS, P);
    let (state, _) = state.step(D1, RS, P);
    let (state, _) = state.step(D0, LS, R);
    assert_eq!(
        state.step(D1, ESC, P).1,
        F,
        "release on one keyboard: D1's left shift is still down"
    );
}
