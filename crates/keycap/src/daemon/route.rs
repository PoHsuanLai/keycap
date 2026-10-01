//! From one read of a grabbed device to what the reader does with it. Pure.

use super::events::{Decoded, InputEvent, decode};
use crate::{DeviceId, Layout, Mono, RemapInput};

/// What one device event asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Routed {
    /// A key transition for the remapper.
    Remap(RemapInput),
    /// Pointer events of a combo device, ending in their `SYN_REPORT`, written unchanged.
    Pass {
        device: DeviceId,
        events: Vec<InputEvent>,
    },
    /// The device's queue overflowed: what keycap wrote for it is no longer known to match
    /// what it holds, so its state is released.
    Resync(DeviceId),
}

/// Whether the events being read are the fragment that follows a `SYN_DROPPED`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Skip {
    Nothing,
    /// Discard events up to and including the next `SYN_REPORT`; the fragment may continue in
    /// the next read.
    UntilSync,
}

/// The pointer events gathered since the last frame boundary, and what was routed so far.
struct Frame {
    device: DeviceId,
    skip: Skip,
    routed: Vec<Routed>,
    pointer: Vec<InputEvent>,
}

impl Frame {
    fn new(device: DeviceId, skip: Skip) -> Self {
        Self {
            device,
            skip,
            routed: Vec::new(),
            pointer: Vec::new(),
        }
    }

    /// Closes the pointer frame, if any, with a `SYN_REPORT`.
    fn flush(mut self) -> Self {
        if !self.pointer.is_empty() {
            let mut events = std::mem::take(&mut self.pointer);
            events.push(InputEvent::SYNC);
            self.routed.push(Routed::Pass {
                device: self.device,
                events,
            });
        }
        self
    }

    /// The queue overflowed: close what is open, ask for a resync and discard up to the next
    /// `SYN_REPORT`.
    fn dropped(mut self) -> Self {
        self = self.flush();
        self.routed.push(Routed::Resync(self.device));
        Self {
            skip: Skip::UntilSync,
            ..self
        }
    }

    fn remap(mut self, input: RemapInput) -> Self {
        self.routed.push(Routed::Remap(input));
        self
    }

    fn point(mut self, event: InputEvent) -> Self {
        self.pointer.push(event);
        self
    }
}

/// Routes the events read from keyboard `device`, in order, and says whether the next read
/// starts inside a fragment to discard. Pointer events keep their place relative to keys: a
/// pending pointer frame is closed before the next key, and one still open at the end of the
/// read is closed there, so no event waits on a `SYN_REPORT` that may arrive in a later read.
/// After a `SYN_DROPPED` everything up to the next `SYN_REPORT` is discarded, because it is the
/// tail of a frame whose start was lost.
pub(crate) fn route(
    device: DeviceId,
    layout: Layout,
    events: &[InputEvent],
    at: Mono,
    skip: Skip,
) -> (Vec<Routed>, Skip) {
    let frame = events
        .iter()
        .fold(Frame::new(device, skip), |frame, &event| {
            match (frame.skip, decode(event)) {
                (_, Decoded::Dropped) => frame.dropped(),
                (Skip::UntilSync, Decoded::Sync) => Frame {
                    skip: Skip::Nothing,
                    ..frame
                },
                (Skip::UntilSync, _) => frame,
                (Skip::Nothing, Decoded::Key(key, edge)) => frame.flush().remap(RemapInput::Key {
                    device,
                    layout,
                    key,
                    edge,
                    at,
                }),
                (Skip::Nothing, Decoded::Pointer(pointer)) => frame.point(pointer),
                (Skip::Nothing, Decoded::Sync) => frame.flush(),
                (Skip::Nothing, Decoded::Other) => frame,
            }
        })
        .flush();
    (frame.routed, frame.skip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::events::{EV_KEY, EV_REL, EV_SYN, REL_X, SYN_DROPPED, SYN_REPORT};
    use crate::{Edge, Key, Physical};

    const DEV: DeviceId = DeviceId(7);
    const AT: Mono = Mono(5);

    const fn ev(kind: u16, code: u16, value: i32) -> InputEvent {
        InputEvent { kind, code, value }
    }

    const SYN: InputEvent = ev(EV_SYN, SYN_REPORT, 0);
    const DROPPED: InputEvent = ev(EV_SYN, SYN_DROPPED, 0);
    const MOVE: InputEvent = ev(EV_REL, REL_X, 4);
    const CLICK: InputEvent = ev(EV_KEY, 0x110, 1);

    fn key(code: u16, edge: Edge) -> Routed {
        Routed::Remap(RemapInput::Key {
            device: DEV,
            layout: Layout::Pc,
            key: Physical(Key::from_code(code)),
            edge,
            at: AT,
        })
    }

    fn pass(events: &[InputEvent]) -> Routed {
        Routed::Pass {
            device: DEV,
            events: events.to_vec(),
        }
    }

    #[test]
    fn routing_table() {
        let cases: &[(&str, &[InputEvent], Vec<Routed>)] = &[
            ("nothing", &[], vec![]),
            (
                "a key frame: the sync is the remapper's to write",
                &[ev(EV_KEY, 30, 1), SYN],
                vec![key(30, Edge::Press)],
            ),
            (
                "a repeat still goes to the remapper, which drops it",
                &[ev(EV_KEY, 30, 2), SYN],
                vec![key(30, Edge::Repeat)],
            ),
            (
                "pointer motion passes with its own sync",
                &[MOVE, SYN],
                vec![pass(&[MOVE, SYN])],
            ),
            (
                "a click and motion share a frame",
                &[CLICK, MOVE, SYN],
                vec![pass(&[CLICK, MOVE, SYN])],
            ),
            (
                "an open pointer frame is closed at the end of the read",
                &[MOVE],
                vec![pass(&[MOVE, SYN])],
            ),
            (
                "a key closes the pointer frame before it",
                &[MOVE, ev(EV_KEY, 30, 1), SYN],
                vec![pass(&[MOVE, SYN]), key(30, Edge::Press)],
            ),
            (
                "unrelated events vanish",
                &[ev(0x04, 4, 458_756), ev(0x11, 1, 1)],
                vec![],
            ),
        ];
        for (name, events, want) in cases {
            let (got, skip) = route(DEV, Layout::Pc, events, AT, Skip::Nothing);
            assert_eq!(&got, want, "{name}");
            assert_eq!(skip, Skip::Nothing, "{name}");
        }
    }

    #[test]
    fn an_overflow_asks_for_a_resync_and_discards_up_to_the_next_report() {
        // (name, skip on entry, events, routed, skip on exit)
        type Case<'a> = (&'a str, Skip, &'a [InputEvent], Vec<Routed>, Skip);
        let cases: [Case<'_>; 6] = [
            (
                "the fragment after the marker is discarded, the next frame is routed",
                Skip::Nothing,
                &[
                    ev(EV_KEY, 30, 1),
                    DROPPED,
                    ev(EV_KEY, 31, 1),
                    SYN,
                    ev(EV_KEY, 32, 1),
                    SYN,
                ],
                vec![
                    key(30, Edge::Press),
                    Routed::Resync(DEV),
                    key(32, Edge::Press),
                ],
                Skip::Nothing,
            ),
            (
                "a fragment that runs into the next read",
                Skip::Nothing,
                &[DROPPED, ev(EV_KEY, 31, 1)],
                vec![Routed::Resync(DEV)],
                Skip::UntilSync,
            ),
            (
                "the next read finishes the fragment and resumes",
                Skip::UntilSync,
                &[ev(EV_KEY, 31, 0), SYN, ev(EV_KEY, 32, 1), SYN],
                vec![key(32, Edge::Press)],
                Skip::Nothing,
            ),
            (
                "a read of only the fragment's middle keeps discarding",
                Skip::UntilSync,
                &[ev(EV_KEY, 31, 0)],
                vec![],
                Skip::UntilSync,
            ),
            (
                "pointer motion before the marker is closed and kept",
                Skip::Nothing,
                &[MOVE, DROPPED, SYN],
                vec![pass(&[MOVE, SYN]), Routed::Resync(DEV)],
                Skip::Nothing,
            ),
            (
                "a second marker inside the fragment asks again",
                Skip::Nothing,
                &[DROPPED, ev(EV_KEY, 31, 1), DROPPED, SYN],
                vec![Routed::Resync(DEV), Routed::Resync(DEV)],
                Skip::Nothing,
            ),
        ];
        for (name, skip, events, want, want_skip) in cases {
            let (got, after) = route(DEV, Layout::Pc, events, AT, skip);
            assert_eq!(got, want, "{name}");
            assert_eq!(after, want_skip, "{name}: skip afterwards");
        }
    }
}
