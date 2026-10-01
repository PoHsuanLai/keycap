//! The usual setup: a fake keyboard, the daemon with it grabbed, a client and an output reader.

use super::scene::{FAKE_NAME, Scene, ok};
use keycap_devtest::{Client, Daemon, FakeKeyboard, FakeSpec, Output};
use keycap_protocol::Grab;
use std::time::Duration;

/// How long a grab may take to show as active: the gate polls, and a conflict backs off.
pub const GRAB_WITHIN: Duration = Duration::from_secs(10);

/// Everything one mapping test drives. Fields drop in order, so the keyboard goes first, the
/// daemon is killed next and the scene reports last.
pub struct Rig {
    pub fake: FakeKeyboard,
    pub client: Client,
    pub out: Output,
    pub daemon: Daemon,
    scene: Scene,
}

impl Rig {
    /// Starts with an Apple keyboard.
    pub fn start(test: &str) -> Self {
        Self::start_with(test, &FakeSpec::apple(FAKE_NAME))
    }

    /// Creates `spec`, starts the daemon and waits until the daemon holds the keyboard.
    pub fn start_with(test: &str, spec: &FakeSpec) -> Self {
        let scene = Scene::new(test);
        let fake = ok(FakeKeyboard::create(spec), "creating the fake keyboard");
        let daemon = scene.daemon();
        let mut client = scene.client(&daemon);
        ok(
            client.wait_grab(FAKE_NAME, Grab::Active, GRAB_WITHIN),
            "waiting for the grab",
        );
        let out = scene.output();
        Self {
            fake,
            client,
            out,
            daemon,
            scene,
        }
    }

    /// The scene the rig runs in.
    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Replaces a dead daemon with a fresh one and waits for it to hold the fake keyboard.
    pub fn restart(&mut self) {
        self.daemon = self.scene.daemon();
        self.client = self.scene.client(&self.daemon);
        self.reopen_output();
    }

    /// Opens a new reader on the output keyboard, for after the daemon recreated it.
    pub fn reopen_output(&mut self) {
        self.out = self.scene.output();
    }

    /// Presses `key` on the fake keyboard and expects `want` on the output.
    pub fn press(&mut self, key: &str, want: &str) {
        ok(self.fake.press(key), &format!("pressing {key}"));
        ok(self.out.expect(want), &format!("after pressing {key}"));
    }

    /// Releases `key` on the fake keyboard and expects `want` on the output.
    pub fn release(&mut self, key: &str, want: &str) {
        ok(self.fake.release(key), &format!("releasing {key}"));
        ok(self.out.expect(want), &format!("after releasing {key}"));
    }

    /// Fails if the output left a key down.
    pub fn assert_nothing_held(&self) {
        assert_eq!(
            self.out.held(),
            Vec::<String>::new(),
            "keys left down on the output keyboard"
        );
    }

    /// Waits until the daemon reports `grab` for the fake keyboard.
    pub fn wait_grab(&mut self, grab: Grab) {
        ok(
            self.client.wait_grab(FAKE_NAME, grab, GRAB_WITHIN),
            "waiting for the grab",
        );
    }
}
