//! `sd_notify`: readiness, then a ping often enough for `WatchdogSec`.

use crate::support::{Scene, WATCHDOG, ok};
use keycap_devtest::Daemon;
use std::time::Duration;

/// READY=1 must come within this of the spawn.
const READY_WITHIN: Duration = Duration::from_secs(2);
/// How long pings are watched.
const WATCH_FOR: Duration = Duration::from_secs(5);
/// Slack on top of half the watchdog interval for a loaded runner; systemd itself would kill
/// the daemon only at the whole interval.
const SLACK: Duration = Duration::from_millis(500);

#[test]
#[ignore = "creates input devices; run in CI with KEYCAP_DEVICE_TESTS=1"]
fn ready_comes_quickly_and_the_watchdog_is_pinged_at_least_every_half_interval() {
    let scene = Scene::new("watchdog");
    let mut daemon = ok(Daemon::start(&scene.spec()), "starting the daemon");
    let ready = ok(daemon.wait_ready(READY_WITHIN), "READY=1");
    eprintln!("READY=1 after {ready:?}");

    while daemon.uptime() < ready + WATCH_FOR {
        ok(daemon.assert_alive(), "watching the watchdog");
        std::thread::sleep(Duration::from_millis(100));
    }

    let notes = daemon.notifications();
    let ready_at = notes.times_of("READY=1")[0];
    let pings = notes.times_of("WATCHDOG=1");
    let limit = WATCHDOG / 2 + SLACK;
    assert!(
        pings.len() >= 4,
        "{} WATCHDOG=1 pings in {WATCH_FOR:?}, want at least 4 (one per half interval); all \
         notifications: {:?}",
        pings.len(),
        notes.all().iter().map(|n| &n.text).collect::<Vec<_>>()
    );
    let mut previous = ready_at;
    for (i, &at) in pings.iter().enumerate() {
        let gap = at.duration_since(previous);
        assert!(
            gap <= limit,
            "ping {i} came {gap:?} after the one before, over {limit:?} (WATCHDOG_USEC/2 plus \
             slack)"
        );
        previous = at;
    }
}
