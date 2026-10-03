//! Keyboard LEDs: what the compositor writes to the virtual keyboard and keycap copies onto
//! the grabbed physical ones. Pure; the reads and writes belong to `uinput` and `grabber`.

use super::events::{EV_LED, InputEvent};
use std::collections::BTreeMap;

/// `LED_NUML`.
const LED_NUML: u16 = 0x00;
/// `LED_CAPSL`.
const LED_CAPSL: u16 = 0x01;
/// `LED_SCROLLL`.
const LED_SCROLLL: u16 = 0x02;

/// The LEDs a keyboard has and the virtual keyboard registers.
pub(crate) const LED_CODES: [u16; 3] = [LED_NUML, LED_CAPSL, LED_SCROLLL];

/// One keyboard LED.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Led {
    Num,
    Caps,
    Scroll,
}

/// Whether a LED is lit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lamp {
    Off,
    On,
}

/// A LED set to a state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LedEvent {
    pub led: Led,
    pub lamp: Lamp,
}

/// The lamps the desktop last set on the virtual keyboard, so a keyboard grabbed later can be
/// brought in line. A LED the desktop never set is absent.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct LedState(BTreeMap<Led, Lamp>);

impl LedState {
    /// The state after the desktop made `changes`, in order.
    pub(crate) fn observe(mut self, changes: &[LedEvent]) -> Self {
        self.0
            .extend(changes.iter().map(|change| (change.led, change.lamp)));
        self
    }

    /// The events that set every LED the desktop has set, in LED order; none if it set none.
    pub(crate) fn events(&self) -> Vec<InputEvent> {
        self.0
            .iter()
            .map(|(&led, &lamp)| encode(LedEvent { led, lamp }))
            .collect()
    }
}

/// The LEDs a keyboard reports lit (`EVIOCGLED`), as the kernel's bitmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LedsLit(u16);

impl LedsLit {
    /// The bitmap as the kernel wrote it: little-endian, bit `n` is LED `n`.
    pub(crate) const fn from_bytes(bytes: [u8; 2]) -> Self {
        Self(u16::from_le_bytes(bytes))
    }

    /// Whether `led` is lit.
    pub(crate) const fn lamp(self, led: Led) -> Lamp {
        if self.0 >> led.code() & 1 == 1 {
            Lamp::On
        } else {
            Lamp::Off
        }
    }
}

impl Led {
    const fn code(self) -> u16 {
        match self {
            Led::Num => LED_NUML,
            Led::Caps => LED_CAPSL,
            Led::Scroll => LED_SCROLLL,
        }
    }

    const fn from_code(code: u16) -> Option<Self> {
        match code {
            LED_NUML => Some(Led::Num),
            LED_CAPSL => Some(Led::Caps),
            LED_SCROLLL => Some(Led::Scroll),
            _ => None,
        }
    }
}

/// The LED change `e` asks for; `None` for anything else, including a LED keycap does not
/// register.
pub(crate) fn decode(e: InputEvent) -> Option<LedEvent> {
    if e.kind != EV_LED {
        return None;
    }
    Led::from_code(e.code).map(|led| LedEvent {
        led,
        lamp: if e.value == 0 { Lamp::Off } else { Lamp::On },
    })
}

/// The event that sets a LED, as written to a physical keyboard.
pub(crate) fn encode(e: LedEvent) -> InputEvent {
    InputEvent {
        kind: EV_LED,
        code: e.led.code(),
        value: match e.lamp {
            Lamp::Off => 0,
            Lamp::On => 1,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::events::{EV_KEY, EV_SYN, SYN_REPORT};

    const fn ev(kind: u16, code: u16, value: i32) -> InputEvent {
        InputEvent { kind, code, value }
    }

    const fn led(led: Led, lamp: Lamp) -> Option<LedEvent> {
        Some(LedEvent { led, lamp })
    }

    #[test]
    fn decode_table() {
        const CASES: &[(&str, InputEvent, Option<LedEvent>)] = &[
            (
                "caps lock on",
                ev(EV_LED, 0x01, 1),
                led(Led::Caps, Lamp::On),
            ),
            (
                "caps lock off",
                ev(EV_LED, 0x01, 0),
                led(Led::Caps, Lamp::Off),
            ),
            ("num lock on", ev(EV_LED, 0x00, 1), led(Led::Num, Lamp::On)),
            (
                "scroll lock on",
                ev(EV_LED, 0x02, 1),
                led(Led::Scroll, Lamp::On),
            ),
            (
                "any non-zero value lights it",
                ev(EV_LED, 0x01, 7),
                led(Led::Caps, Lamp::On),
            ),
            ("LED_MISC is not registered", ev(EV_LED, 0x03, 1), None),
            ("LED_MAIL is not registered", ev(EV_LED, 0x09, 1), None),
            ("a key with the caps lock code", ev(EV_KEY, 0x01, 1), None),
            ("a sync", ev(EV_SYN, SYN_REPORT, 0), None),
        ];
        for &(name, input, want) in CASES {
            assert_eq!(decode(input), want, "{name}");
        }
    }

    #[test]
    fn encode_writes_the_led_event_and_decode_reads_it_back() {
        for led in [Led::Num, Led::Caps, Led::Scroll] {
            for lamp in [Lamp::Off, Lamp::On] {
                let event = LedEvent { led, lamp };
                let wire = encode(event);
                assert_eq!(wire.kind, EV_LED);
                assert_eq!(decode(wire), Some(event), "{event:?}");
            }
        }
        assert_eq!(
            encode(LedEvent {
                led: Led::Caps,
                lamp: Lamp::On
            }),
            ev(EV_LED, 1, 1)
        );
    }

    #[test]
    fn leds_lit_reads_the_kernels_bitmap() {
        // (bytes, num, caps, scroll)
        const CASES: &[([u8; 2], Lamp, Lamp, Lamp)] = &[
            ([0, 0], Lamp::Off, Lamp::Off, Lamp::Off),
            ([1, 0], Lamp::On, Lamp::Off, Lamp::Off),
            ([2, 0], Lamp::Off, Lamp::On, Lamp::Off),
            ([4, 0], Lamp::Off, Lamp::Off, Lamp::On),
            ([7, 0xff], Lamp::On, Lamp::On, Lamp::On),
            ([0, 2], Lamp::Off, Lamp::Off, Lamp::Off),
        ];
        for &(bytes, num, caps, scroll) in CASES {
            let lit = LedsLit::from_bytes(bytes);
            assert_eq!(
                (
                    lit.lamp(Led::Num),
                    lit.lamp(Led::Caps),
                    lit.lamp(Led::Scroll)
                ),
                (num, caps, scroll),
                "{bytes:?}"
            );
        }
    }

    #[test]
    fn led_state_remembers_the_last_lamp_of_each_led() {
        let set = |led, lamp| LedEvent { led, lamp };
        type Case = (&'static str, &'static [(Led, Lamp)], &'static [InputEvent]);
        const CASES: &[Case] = &[
            ("nothing set, nothing to write", &[], &[]),
            ("caps on", &[(Led::Caps, Lamp::On)], &[ev(EV_LED, 1, 1)]),
            (
                "the last change of a LED wins",
                &[(Led::Caps, Lamp::On), (Led::Caps, Lamp::Off)],
                &[ev(EV_LED, 1, 0)],
            ),
            (
                "an off lamp is still written: the keyboard may have it lit",
                &[(Led::Num, Lamp::Off)],
                &[ev(EV_LED, 0, 0)],
            ),
            (
                "LEDs come out in LED order, not change order",
                &[(Led::Scroll, Lamp::On), (Led::Num, Lamp::On)],
                &[ev(EV_LED, 0, 1), ev(EV_LED, 2, 1)],
            ),
        ];
        for &(name, changes, want) in CASES {
            let changes: Vec<LedEvent> = changes.iter().map(|&(l, m)| set(l, m)).collect();
            let state = LedState::default().observe(&changes);
            assert_eq!(state.events(), want, "{name}");
        }
    }

    #[test]
    fn led_state_accumulates_across_observations() {
        let state = LedState::default()
            .observe(&[LedEvent {
                led: Led::Caps,
                lamp: Lamp::On,
            }])
            .observe(&[LedEvent {
                led: Led::Num,
                lamp: Lamp::On,
            }]);
        assert_eq!(state.events(), [ev(EV_LED, 0, 1), ev(EV_LED, 1, 1)]);
    }

    #[test]
    fn the_registered_codes_are_exactly_the_decodable_ones() {
        let decodable: Vec<u16> = (0..=0x0f)
            .filter(|&code| decode(ev(EV_LED, code, 1)).is_some())
            .collect();
        assert_eq!(decodable, LED_CODES);
    }
}
