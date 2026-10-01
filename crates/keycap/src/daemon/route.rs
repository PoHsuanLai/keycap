//! From one read of a grabbed device to what the reader does with it. Pure.

use super::events::{Decoded, InputEvent, decode};
use crate::{DeviceId, Input, Layout, Mono};

/// What one device event asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Routed {
    /// A key transition for the remapper.
    Remap(Input),
    /// Pointer events of a combo device, ending in their `SYN_REPORT`, written unchanged.
    Pass(Vec<InputEvent>),
}

/// The pointer events gathered since the last frame boundary, and what was routed so far.
#[derive(Default)]
struct Frame {
    routed: Vec<Routed>,
    pointer: Vec<InputEvent>,
}

impl Frame {
    /// Closes the pointer frame, if any, with a `SYN_REPORT`.
    fn flush(mut self) -> Self {
        if !self.pointer.is_empty() {
            let mut events = std::mem::take(&mut self.pointer);
            events.push(InputEvent::SYNC);
            self.routed.push(Routed::Pass(events));
        }
        self
    }

    fn remap(mut self, input: Input) -> Self {
        self.routed.push(Routed::Remap(input));
        self
    }

    fn point(mut self, event: InputEvent) -> Self {
        self.pointer.push(event);
        self
    }
}

/// Routes the events read from keyboard `device`, in order. Pointer events keep their place
/// relative to keys: a pending pointer frame is closed before the next key, and one still open
/// at the end of the read is closed there, so no event waits on a `SYN_REPORT` that may arrive
/// in a later read.
pub(crate) fn route(
    device: DeviceId,
    layout: Layout,
    events: &[InputEvent],
    at: Mono,
) -> Vec<Routed> {
    events
        .iter()
        .fold(Frame::default(), |frame, &event| match decode(event) {
            Decoded::Key(key, edge) => frame.flush().remap(Input::Key {
                device,
                layout,
                key,
                edge,
                at,
            }),
            Decoded::Pointer(pointer) => frame.point(pointer),
            Decoded::Sync => frame.flush(),
            Decoded::Other => frame,
        })
        .flush()
        .routed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::events::{EV_KEY, EV_REL, EV_SYN, REL_X, SYN_REPORT};
    use crate::{Edge, Key, Physical};

    const DEV: DeviceId = DeviceId(7);
    const AT: Mono = Mono(5);

    const fn ev(kind: u16, code: u16, value: i32) -> InputEvent {
        InputEvent { kind, code, value }
    }

    const SYN: InputEvent = ev(EV_SYN, SYN_REPORT, 0);
    const MOVE: InputEvent = ev(EV_REL, REL_X, 4);
    const CLICK: InputEvent = ev(EV_KEY, 0x110, 1);

    fn key(code: u16, edge: Edge) -> Routed {
        Routed::Remap(Input::Key {
            device: DEV,
            layout: Layout::Pc,
            key: Physical(Key::from_code(code)),
            edge,
            at: AT,
        })
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
                vec![Routed::Pass(vec![MOVE, SYN])],
            ),
            (
                "a click and motion share a frame",
                &[CLICK, MOVE, SYN],
                vec![Routed::Pass(vec![CLICK, MOVE, SYN])],
            ),
            (
                "an open pointer frame is closed at the end of the read",
                &[MOVE],
                vec![Routed::Pass(vec![MOVE, SYN])],
            ),
            (
                "a key closes the pointer frame before it",
                &[MOVE, ev(EV_KEY, 30, 1), SYN],
                vec![Routed::Pass(vec![MOVE, SYN]), key(30, Edge::Press)],
            ),
            (
                "unrelated events vanish",
                &[ev(0x04, 4, 458_756), ev(0x11, 1, 1)],
                vec![],
            ),
        ];
        for (name, events, want) in cases {
            assert_eq!(&route(DEV, Layout::Pc, events, AT), want, "{name}");
        }
    }
}
