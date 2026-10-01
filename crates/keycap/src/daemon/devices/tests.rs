use super::model::DeviceEffect;
use super::*;
use crate::daemon::discover::Identity;
use crate::daemon::grabber::FakeGrabber;
use crate::{CoreStatus, DeviceId, Layout, Mono};
use keycap_protocol::{Grab, Mode};
use std::path::PathBuf;
use std::time::Duration;

const A: DeviceId = DeviceId(0);
const B: DeviceId = DeviceId(1);

fn at(ms: u64) -> Mono {
    Mono::from_millis(ms)
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn identity(n: u32, vendor: u16) -> Identity {
    Identity {
        name: format!("keyboard {n}"),
        vendor,
        product: 1,
        path: PathBuf::from(format!("/dev/input/event{n}")),
    }
}

/// The state machine and the fake nodes it drives, stepped the way the reader steps them.
struct Rig {
    devices: Devices,
    nodes: FakeGrabber,
}

impl Rig {
    fn new(nodes: FakeGrabber) -> Self {
        Rig {
            devices: Devices::new(),
            nodes,
        }
    }

    fn appear(&mut self, identity: Identity, now: Mono) {
        let (devices, effects) = std::mem::take(&mut self.devices).appeared(identity);
        self.devices = drive(devices, effects, &mut self.nodes, now);
    }

    fn poll(&mut self, now: Mono) {
        self.devices = poll_gates(std::mem::take(&mut self.devices), &mut self.nodes, now);
    }

    fn release(&mut self, now: Mono) {
        let (devices, effects) = std::mem::take(&mut self.devices).released();
        self.devices = drive(devices, effects, &mut self.nodes, now);
    }

    fn resume(&mut self) {
        self.devices = std::mem::take(&mut self.devices).resumed().0;
    }

    fn gone(&mut self, id: DeviceId, now: Mono) {
        let (devices, effects) = std::mem::take(&mut self.devices).gone(id);
        self.devices = drive(devices, effects, &mut self.nodes, now);
    }

    fn grabs(&self) -> Vec<Grab> {
        status_of(&self.devices, CoreStatus::Running)
            .keyboards
            .iter()
            .map(|k| k.grab)
            .collect()
    }
}

#[test]
fn an_idle_keyboard_is_opened_and_grabbed_at_once() {
    let mut rig = Rig::new(FakeGrabber::default());
    rig.appear(identity(3, 0x05ac), at(0));
    assert_eq!(rig.nodes.open_ids(), [A]);
    assert_eq!(rig.nodes.held(), [A]);
    assert_eq!(rig.grabs(), [Grab::Active]);
    assert_eq!(rig.devices.active(A), Some(Layout::Apple));
}

#[test]
fn appearing_twice_is_one_keyboard_and_no_second_effect() {
    let (devices, first) = Devices::new().appeared(identity(3, 0x05ac));
    let (devices, second) = devices.appeared(identity(3, 0x05ac));
    assert_eq!(first.len(), 1, "{first:?}");
    assert_eq!(second, []);
    let (devices, third) = devices.appeared(identity(4, 0x046d));
    assert_eq!(
        third,
        [DeviceEffect::Open {
            id: B,
            path: PathBuf::from("/dev/input/event4")
        }],
        "another path is another keyboard with the next id"
    );
    assert_eq!(status_of(&devices, CoreStatus::Running).keyboards.len(), 2);
}

#[test]
fn a_key_held_at_the_start_keeps_the_keyboard_waiting_until_it_is_up() {
    let mut nodes = FakeGrabber::default();
    nodes.press_key(A);
    let mut rig = Rig::new(nodes);
    rig.appear(identity(3, 0x046d), at(0));
    assert_eq!(rig.grabs(), [Grab::WaitingForRelease]);
    assert_eq!(rig.nodes.held(), [], "not grabbed while a key is down");
    assert_eq!(rig.devices.active(A), None, "its events are not remapped");
    assert_eq!(
        rig.devices.next_wake(at(0)),
        Some(ms(50)),
        "the gate is looked at again soon"
    );

    rig.poll(at(50));
    assert_eq!(rig.grabs(), [Grab::WaitingForRelease], "still held");

    rig.nodes.release_keys(A);
    rig.poll(at(100));
    assert_eq!(rig.grabs(), [Grab::Active]);
    assert_eq!(rig.nodes.held(), [A]);
    assert_eq!(
        rig.devices.next_wake(at(100)),
        None,
        "nothing left to watch"
    );
}

#[test]
fn a_busy_keyboard_is_a_conflict_retried_on_a_doubling_backoff_and_never_hammered() {
    let mut rig = Rig::new(FakeGrabber::with_busy([A]));
    rig.appear(identity(3, 0x046d), at(0));
    assert_eq!(rig.grabs(), [Grab::Conflict]);
    assert_eq!(
        rig.devices.next_wake(at(0)),
        Some(ms(1000)),
        "first wait is 1 s"
    );

    // Nothing is due before the deadline: no gate look, no grab attempt.
    assert_eq!(rig.devices.pending(at(999)), []);
    assert_eq!(rig.devices.next_wake(at(400)), Some(ms(600)));

    // Due at 1 s, refused again: the wait doubles.
    rig.poll(at(1000));
    assert_eq!(rig.grabs(), [Grab::Conflict]);
    assert_eq!(rig.devices.next_wake(at(1000)), Some(ms(2000)));
    assert_eq!(rig.devices.pending(at(2999)), []);

    // The other program lets go; the retry at 3 s succeeds.
    rig.nodes.free(A);
    rig.poll(at(3000));
    assert_eq!(rig.grabs(), [Grab::Active]);
    assert_eq!(rig.nodes.held(), [A]);
}

#[test]
fn the_conflict_backoff_caps_at_thirty_seconds_and_a_success_resets_it() {
    let mut rig = Rig::new(FakeGrabber::with_busy([A]));
    rig.appear(identity(3, 0x046d), at(0));
    let mut now = at(0);
    let mut waits = Vec::new();
    for _ in 0..7 {
        let wait = rig.devices.next_wake(now).expect("a retry is scheduled");
        waits.push(wait.as_secs());
        now = now.after(wait);
        rig.poll(now);
    }
    assert_eq!(waits, [1, 2, 4, 8, 16, 30, 30]);

    rig.nodes.free(A);
    rig.poll(now.after(ms(30_000)));
    assert_eq!(rig.grabs(), [Grab::Active]);

    // Another program grabs it again later: the wait starts over at 1 s.
    rig.nodes.make_busy(A);
    rig.release(now);
    rig.resume();
    rig.poll(now);
    assert_eq!(rig.grabs(), [Grab::Conflict]);
    assert_eq!(rig.devices.next_wake(now), Some(ms(1000)));
}

#[test]
fn a_pause_gives_every_grab_back_and_a_resume_regrabs_through_the_gate() {
    let mut nodes = FakeGrabber::with_busy([B]);
    nodes.press_key(B);
    let mut rig = Rig::new(nodes);
    rig.appear(identity(3, 0x05ac), at(0));
    rig.appear(identity(4, 0x046d), at(0));
    assert_eq!(rig.grabs(), [Grab::Active, Grab::WaitingForRelease]);
    assert_eq!(rig.nodes.held(), [A]);

    rig.release(at(10));
    assert_eq!(
        rig.nodes.held(),
        [],
        "ungrabbed: the kernel serves the keys again"
    );
    assert_eq!(
        rig.grabs(),
        [Grab::WaitingForRelease, Grab::WaitingForRelease]
    );
    assert_eq!(rig.devices.active(A), None);
    assert_eq!(
        rig.devices.pending(at(10_000)),
        [],
        "nothing is grabbed while paused"
    );
    assert_eq!(rig.devices.next_wake(at(10_000)), None);
    rig.poll(at(10_000));
    assert_eq!(
        rig.nodes.held(),
        [],
        "polling the gate while paused grabs nothing"
    );
    assert_eq!(rig.nodes.open_ids(), [A, B], "the nodes stay open");

    rig.resume();
    rig.nodes.free(B);
    rig.poll(at(10_020));
    assert_eq!(rig.nodes.held(), [A], "A has no key down; B still has one");
    assert_eq!(rig.grabs(), [Grab::Active, Grab::WaitingForRelease]);
    rig.nodes.release_keys(B);
    rig.poll(at(10_070));
    assert_eq!(rig.nodes.held(), [A, B]);
}

#[test]
fn an_unplugged_keyboard_is_closed_and_forgotten_and_a_second_unplug_does_nothing() {
    let mut rig = Rig::new(FakeGrabber::default());
    rig.appear(identity(3, 0x05ac), at(0));
    rig.appear(identity(4, 0x046d), at(0));
    rig.gone(A, at(5));
    assert_eq!(rig.nodes.open_ids(), [B]);
    assert_eq!(rig.nodes.held(), [B]);
    assert_eq!(rig.devices.id_of(&PathBuf::from("/dev/input/event3")), None);
    assert_eq!(
        rig.devices.id_of(&PathBuf::from("/dev/input/event4")),
        Some(B)
    );
    let (_, effects) = std::mem::take(&mut rig.devices).gone(A);
    assert_eq!(effects, []);
}

#[test]
fn a_node_that_cannot_be_opened_is_not_kept_so_the_next_sighting_tries_again() {
    let mut nodes = FakeGrabber::default();
    nodes.refuse_open(A);
    let mut rig = Rig::new(nodes);
    rig.appear(identity(3, 0x05ac), at(0));
    assert_eq!(status_of(&rig.devices, CoreStatus::Running).keyboards, []);
    assert_eq!(rig.nodes.open_ids(), []);
    rig.appear(identity(3, 0x05ac), at(1));
    assert_eq!(
        rig.devices.id_of(&PathBuf::from("/dev/input/event3")),
        Some(B),
        "a fresh id, a fresh attempt"
    );
}

#[test]
fn status_names_the_mode_and_lists_keyboards_in_id_order() {
    let mut rig = Rig::new(FakeGrabber::with_busy([B]));
    rig.appear(identity(3, 0x05ac), at(0));
    rig.appear(identity(4, 0x046d), at(0));
    let running = status_of(&rig.devices, CoreStatus::Running);
    assert_eq!(running.mode, Mode::Running);
    let seen: Vec<_> = running
        .keyboards
        .iter()
        .map(|k| {
            (
                k.info.name.as_str(),
                k.info.path.as_str(),
                k.info.vendor,
                k.grab,
            )
        })
        .collect();
    assert_eq!(
        seen,
        [
            ("keyboard 3", "/dev/input/event3", 0x05ac, Grab::Active),
            ("keyboard 4", "/dev/input/event4", 0x046d, Grab::Conflict),
        ]
    );
    assert_eq!(
        status_of(&rig.devices, CoreStatus::Paused).mode,
        Mode::Paused
    );
    assert_eq!(
        status_of(&Devices::new(), CoreStatus::Running).keyboards,
        []
    );
}

#[test]
fn summary_table() {
    let keyboard = |grab| keycap_protocol::Keyboard {
        info: keycap_protocol::KeyboardInfo {
            name: String::new(),
            path: String::new(),
            vendor: 0,
            product: 0,
        },
        grab,
    };
    let status = |mode, grabs: &[Grab]| keycap_protocol::Status {
        mode,
        keyboards: grabs.iter().copied().map(keyboard).collect(),
    };
    let cases = [
        (
            "none",
            status(Mode::Running, &[]),
            "0 grabbed, 0 waiting, 0 in conflict",
        ),
        (
            "mixed",
            status(
                Mode::Running,
                &[
                    Grab::Active,
                    Grab::Active,
                    Grab::Conflict,
                    Grab::WaitingForRelease,
                ],
            ),
            "2 grabbed, 1 waiting, 1 in conflict",
        ),
        (
            "paused",
            status(Mode::Paused, &[Grab::Active]),
            "paused: keyboards released",
        ),
    ];
    for (name, input, want) in cases {
        assert_eq!(summary(&input), want, "{name}");
    }
}
