//! A combo device's pointer twin and the reader's other grab-time state, on a recording
//! emitter: no node is opened and no device is created.

use super::tests::Rig;
use super::unknown::Unknowns;
use crate::DeviceId;
use crate::Edge;
use crate::daemon::events::{EV_ABS, EV_KEY, InputEvent};
use crate::daemon::inbox::Request;
use crate::daemon::leds::{Lamp, Led, LedEvent};
use crate::daemon::pointer::{Masks, PointerCaps};
use crate::daemon::route::Routed;
use crate::daemon::uinput::Emitter;
use keycap_protocol::ProfileName;

const PAD: DeviceId = DeviceId(0);
const BTN_LEFT: u16 = 0x110;

const fn ev(kind: u16, code: u16, value: i32) -> InputEvent {
    InputEvent { kind, code, value }
}

/// A rig whose device 0 is grabbed and has a twin.
fn rig_with_twin() -> Rig {
    let mut rig = Rig::new();
    let caps = PointerCaps::new("Combo", 1, 2, &Masks::default(), vec![]);
    let twin = rig.reader.emitter.twin(&caps).unwrap();
    rig.reader.twins.insert(PAD, twin);
    rig.reader.held.insert(PAD);
    rig
}

fn pass(rig: &mut Rig, events: &[InputEvent]) {
    rig.reader.apply(Routed::Pass {
        device: PAD,
        events: events.to_vec(),
    });
}

fn release_frame() -> Vec<InputEvent> {
    vec![ev(EV_KEY, BTN_LEFT, 0), InputEvent::SYNC]
}

#[test]
fn a_twinned_devices_pointer_goes_to_its_twin_and_nothing_reaches_the_keyboard() {
    let mut rig = rig_with_twin();
    let frame = [ev(EV_ABS, 0, 40), ev(EV_KEY, BTN_LEFT, 1), InputEvent::SYNC];
    pass(&mut rig, &frame);
    assert_eq!(
        rig.reader.emitter.twins[0].borrow().batches,
        [frame.to_vec()]
    );
    assert_eq!(rig.reader.emitter.batches, Vec::<Vec<InputEvent>>::new());
}

#[test]
fn keys_of_a_twinned_device_still_reach_the_remapper() {
    let mut rig = rig_with_twin();
    rig.key("a", Edge::Press);
    assert_eq!(
        rig.reader.emitter.batches.len(),
        1,
        "the key is written by the keyboard"
    );
    assert_eq!(
        rig.reader.emitter.twins[0].borrow().batches,
        Vec::<Vec<InputEvent>>::new()
    );
}

#[test]
fn an_unplugged_device_releases_its_buttons_on_the_twin_and_destroys_it() {
    let mut rig = rig_with_twin();
    pass(&mut rig, &[ev(EV_KEY, BTN_LEFT, 1), InputEvent::SYNC]);
    rig.reader.lose(PAD);
    let twin = rig.reader.emitter.twins[0].borrow();
    assert_eq!(
        twin.batches.last(),
        Some(&release_frame()),
        "BTN_LEFT up on the twin"
    );
    assert!(twin.destroyed);
    assert_eq!(
        rig.reader.emitter.batches,
        Vec::<Vec<InputEvent>>::new(),
        "the keyboard was never told of the button"
    );
}

#[test]
fn the_escape_chord_releases_the_twins_buttons_and_gives_the_twin_up() {
    let mut rig = rig_with_twin();
    pass(&mut rig, &[ev(EV_KEY, BTN_LEFT, 1), InputEvent::SYNC]);
    rig.key("leftshift", Edge::Press);
    rig.key("rightshift", Edge::Press);
    rig.key("esc", Edge::Press);
    let twin = rig.reader.emitter.twins[0].borrow();
    assert_eq!(twin.batches.last(), Some(&release_frame()));
    assert!(twin.destroyed, "the device is let go, so is its twin");
}

#[test]
fn a_twin_that_fails_hands_its_pointer_to_the_keyboard() {
    let mut rig = rig_with_twin();
    rig.reader.emitter.twins[0].borrow_mut().failing = 1;
    pass(&mut rig, &[ev(EV_KEY, BTN_LEFT, 1), InputEvent::SYNC]);
    assert!(rig.reader.emitter.twins[0].borrow().destroyed);
    let frame = [ev(EV_KEY, BTN_LEFT, 0), InputEvent::SYNC];
    pass(&mut rig, &frame);
    assert_eq!(rig.reader.emitter.batches, [frame.to_vec()]);
}

#[test]
fn the_lamps_the_desktop_set_are_remembered_for_the_next_grab() {
    let mut rig = Rig::new();
    assert_eq!(rig.reader.lamps.events(), [], "nothing set yet");
    rig.reader.emitter.queued_leds = vec![LedEvent {
        led: Led::Caps,
        lamp: Lamp::On,
    }];
    rig.reader.forward_leds();
    assert_eq!(rig.reader.lamps.events(), [ev(0x11, 1, 1)], "Caps Lock on");
    rig.reader.emitter.queued_leds = vec![LedEvent {
        led: Led::Caps,
        lamp: Lamp::Off,
    }];
    rig.reader.forward_leds();
    assert_eq!(
        rig.reader.lamps.events(),
        [ev(0x11, 1, 0)],
        "Caps Lock off again"
    );
}

#[test]
fn an_undefined_focus_is_remembered_until_the_config_changes() {
    let mut rig = Rig::new();
    let focus = |rig: &mut Rig, name: &str| {
        rig.request(Request::Focus(ProfileName::parse(name).unwrap()));
    };
    focus(&mut rig, "terminal");
    assert_eq!(
        rig.reader.unknowns,
        Unknowns::default(),
        "a defined profile is not noted"
    );
    focus(&mut rig, "mystery");
    assert_ne!(rig.reader.unknowns, Unknowns::default());
    rig.request(Request::ClearUserConfig);
    assert_eq!(
        rig.reader.unknowns,
        Unknowns::default(),
        "a reload forgets it"
    );
}
