//! The "keycap keyboard" as a reader sees it: what keycap actually wrote to the kernel.

use crate::event::{EV_LED, Event, SYN_REPORT, encode, token};
use crate::failure::{Failure, errno};
use crate::keys::name_of;
use crate::sysfs::{OUTPUT_NAME, find_node, name_registered, wait_for};
use keycap_sys::{End, drain};
use rustix::fd::OwnedFd;
use rustix::fs::{Mode, OFlags};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long `expect` waits for the events it wants.
const WAIT: Duration = Duration::from_secs(3);
/// How long it keeps listening after they arrive, to catch events that should not follow.
const SETTLE: Duration = Duration::from_millis(200);
/// How long `Output::find` waits for the device to register.
const FIND_WAIT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(5);

/// A read-only, nonblocking reader on keycap's output node. It sees only what is written after
/// it opened, so open it before the action under test.
#[derive(Debug)]
pub struct Output {
    fd: OwnedFd,
    node: PathBuf,
    down: BTreeSet<u16>,
    trace: Option<std::fs::File>,
    opened: Instant,
    gone: bool,
}

impl Output {
    /// Waits for "keycap keyboard" to register and opens its node. Every event read is
    /// appended to `trace`, a file the failure report includes.
    pub fn find(trace: Option<&Path>) -> Result<Self, Failure> {
        let node = wait_for(FIND_WAIT, || find_node(OUTPUT_NAME)).ok_or_else(|| {
            Failure::new(format!(
                "no {OUTPUT_NAME:?} under /sys/class/input after {FIND_WAIT:?}: did the daemon \
                 create its virtual keyboard?"
            ))
        })?;
        let fd = rustix::fs::open(
            &node,
            OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(errno(format!("opening {}", node.display())))?;
        Ok(Self::on(fd, node, trace))
    }

    fn on(fd: OwnedFd, node: PathBuf, trace: Option<&Path>) -> Self {
        let trace = trace.and_then(|p| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
                .ok()
        });
        Self {
            fd,
            node,
            down: BTreeSet::new(),
            trace,
            opened: Instant::now(),
            gone: false,
        }
    }

    /// Reads what is queued: its tokens, and the keys down afterwards.
    fn read(&mut self) -> Vec<String> {
        let drained = drain(&self.fd, Event::from_bytes);
        if drained.end == End::Gone && !self.gone {
            self.gone = true;
            self.log("the node is gone (ENODEV or end of file)");
        }
        for e in &drained.events {
            match (e.kind, e.value) {
                (1, 1 | 2) => {
                    self.down.insert(e.code);
                }
                (1, 0) => {
                    self.down.remove(&e.code);
                }
                _ => {}
            }
        }
        let tokens: Vec<String> = drained.events.into_iter().map(token).collect();
        if !tokens.is_empty() {
            self.log(&tokens.join(" "));
        }
        tokens
    }

    fn log(&mut self, line: &str) {
        let at = self.opened.elapsed().as_secs_f64();
        if let Some(file) = &mut self.trace {
            let _ = writeln!(file, "{at:9.3}s {line}");
        }
    }

    /// Listens for `want` (tokens like `+leftctrl +c SYN`; empty for silence). It fails as
    /// soon as what arrived stops being a prefix of `want`, when `WAIT` passes without all of
    /// it, or when anything follows it within `SETTLE`. The failure shows both streams.
    pub fn expect(&mut self, want: &str) -> Result<(), Failure> {
        self.expect_within(want, WAIT)
    }

    /// `expect` with the wait given.
    pub fn expect_within(&mut self, want: &str, wait: Duration) -> Result<(), Failure> {
        let want: Vec<String> = want.split_whitespace().map(str::to_owned).collect();
        let shown = |tokens: &[String]| format!("[{}]", tokens.join(" "));
        self.log(&format!("expect {}", shown(&want)));
        let mut got: Vec<String> = Vec::new();
        let deadline = Instant::now() + wait;
        while got != want {
            got.extend(self.read());
            if !want.starts_with(&got) {
                return Err(Failure::new(format!(
                    "output differs: want {} got {}",
                    shown(&want),
                    shown(&got)
                )));
            }
            if got != want {
                if Instant::now() >= deadline {
                    return Err(Failure::new(format!(
                        "output incomplete after {wait:?}: want {} got {}",
                        shown(&want),
                        shown(&got)
                    )));
                }
                std::thread::sleep(POLL);
            }
        }
        let settled = Instant::now() + SETTLE;
        while Instant::now() < settled {
            let extra = self.read();
            if !extra.is_empty() {
                return Err(Failure::new(format!(
                    "output continued past {}: extra {}",
                    shown(&want),
                    shown(&extra)
                )));
            }
            std::thread::sleep(POLL);
        }
        Ok(())
    }

    /// Reads for `quiet` and fails if anything at all arrives.
    pub fn expect_silence(&mut self, quiet: Duration) -> Result<(), Failure> {
        self.log("expect silence");
        let end = Instant::now() + quiet;
        while Instant::now() < end {
            let got = self.read();
            if !got.is_empty() {
                return Err(Failure::new(format!(
                    "expected silence for {quiet:?}, got [{}]",
                    got.join(" ")
                )));
            }
            std::thread::sleep(POLL);
        }
        Ok(())
    }

    /// Reads everything for `quiet` and returns it, for a step whose exact output a test does
    /// not pin (the trace file still has it).
    pub fn collect(&mut self, quiet: Duration) -> Vec<String> {
        let end = Instant::now() + quiet;
        let mut all = Vec::new();
        while Instant::now() < end {
            all.extend(self.read());
            std::thread::sleep(POLL);
        }
        all
    }

    /// The keys the stream read so far leaves down, by name; a stuck key shows here.
    pub fn held(&self) -> Vec<String> {
        self.down.iter().map(|&c| name_of(c)).collect()
    }

    /// Waits until the device is no longer registered, which is what a dead daemon leaves.
    pub fn expect_gone(&mut self, wait: Duration) -> Result<(), Failure> {
        let gone = wait_for(wait, || {
            self.read();
            (!name_registered(OUTPUT_NAME)).then_some(())
        });
        gone.ok_or_else(|| {
            Failure::new(format!(
                "{OUTPUT_NAME:?} was still registered {wait:?} after the daemon died"
            ))
        })
    }

    /// Writes a LED event to the node, as a compositor does when Caps Lock turns on.
    pub fn write_led(&self, led: u16, value: i32) -> Result<(), Failure> {
        let fd = rustix::fs::open(
            &self.node,
            OFlags::WRONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(errno(format!(
            "opening {} for writing",
            self.node.display()
        )))?;
        let bytes = encode(&[
            Event {
                kind: EV_LED,
                code: led,
                value,
            },
            Event {
                kind: 0,
                code: SYN_REPORT,
                value: 0,
            },
        ]);
        rustix::io::write(&fd, &bytes)
            .map(drop)
            .map_err(errno("writing the LED event"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EV_KEY;

    /// An `Output` reading a pipe, with `send` as the kernel.
    fn rig() -> (Output, impl Fn(&[Event])) {
        let (r, w) = rustix::pipe::pipe_with(rustix::pipe::PipeFlags::NONBLOCK).unwrap();
        let out = Output::on(r, PathBuf::from("/dev/null"), None);
        let send = move |events: &[Event]| {
            rustix::io::write(&w, &encode(events)).unwrap();
        };
        (out, send)
    }

    fn press(name: &str) -> [Event; 2] {
        [
            Event::key(crate::keys::code_of(name).unwrap(), 1),
            Event::SYNC,
        ]
    }

    fn release(name: &str) -> [Event; 2] {
        [
            Event::key(crate::keys::code_of(name).unwrap(), 0),
            Event::SYNC,
        ]
    }

    #[test]
    fn expect_matches_across_reads_and_tracks_held_keys() {
        let (mut out, send) = rig();
        send(&press("leftctrl"));
        out.expect("+leftctrl SYN").unwrap();
        assert_eq!(out.held(), ["leftctrl"]);
        send(&release("leftctrl"));
        out.expect("-leftctrl SYN").unwrap();
        assert!(out.held().is_empty());
    }

    #[test]
    fn expect_fails_on_a_wrong_token_with_both_streams_shown() {
        let (mut out, send) = rig();
        send(&press("a"));
        let e = out.expect("+b SYN").unwrap_err().to_string();
        assert!(e.contains("want [+b SYN] got [+a SYN]"), "{e}");
    }

    #[test]
    fn expect_is_token_exact_so_a_longer_name_is_not_a_prefix_match() {
        let (mut out, send) = rig();
        send(&press("leftalt"));
        let e = out.expect_within("+left SYN", Duration::from_millis(100));
        assert!(e.is_err(), "+leftalt must not satisfy +left");
    }

    #[test]
    fn expect_fails_when_events_follow_the_wanted_ones() {
        let (mut out, send) = rig();
        send(&press("a"));
        let e = std::thread::scope(|s| {
            s.spawn(|| {
                std::thread::sleep(Duration::from_millis(80));
                send(&press("b"));
            });
            out.expect("+a SYN").unwrap_err().to_string()
        });
        assert!(e.contains("extra [+b SYN]"), "{e}");
    }

    #[test]
    fn expect_times_out_with_what_it_has() {
        let (mut out, send) = rig();
        send(&[Event::key(crate::keys::code_of("a").unwrap(), 1)]);
        let e = out
            .expect_within("+a SYN", Duration::from_millis(60))
            .unwrap_err()
            .to_string();
        assert!(e.contains("incomplete") && e.contains("got [+a]"), "{e}");
    }

    #[test]
    fn silence_passes_when_quiet_and_fails_on_any_event() {
        let (mut out, send) = rig();
        out.expect_silence(Duration::from_millis(50)).unwrap();
        out.expect("").unwrap();
        send(&[Event {
            kind: EV_KEY,
            code: 30,
            value: 2,
        }]);
        let e = out.expect_silence(Duration::from_millis(50)).unwrap_err();
        assert!(e.to_string().contains("[~a]"), "{e}");
    }
}
