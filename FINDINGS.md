FINDINGS
========

Open items and standing facts.

## Open items

**The profile mechanism cannot see inside a window.** A terminal embedded in another app (an
editor's terminal panel) gets the window's profile, not `terminal`: the desktop knows the
focused window, keycap knows only the name it is sent. The user override (the per-app profile
in the seat owner's config) is the escape hatch. Unproven whether a finer signal exists.

**Does the compositor see the uinput keyboard like the physical one? Unverified.** libxkbcommon
and the compositor keep a keymap per device. The virtual "keycap keyboard" should get the
session's layout, but whether the layout, its options and per-device settings carry over is
untested on hardware.

**Combo devices pass pointer events through the virtual device. Unverified on hardware.**
A receiver that is a keyboard and a pointer in one node is grabbed whole, so its pointer events
are forwarded through the keycap virtual device. Motion, clicks and scroll through that path,
and their latency, are untested on a real receiver.

**The daemon's device I/O has never run against a kernel. Unverified on hardware.** The reader's
`poll` over real evdev fds, `read` until `EAGAIN`, `EVIOCGKEY` and `EVIOCGRAB` through the real
`EvdevGrabber`, hotplug from `notify` on `/dev/input`, and the uinput device itself are compiled
and reviewed; the tests drive the same logic through fakes, a recording emitter and socket pairs.
Needs a machine with a keyboard: boot with the unit, plug and unplug, grab with a key held.

**`SO_PEERCRED` and the seat file on a real logind are untested.** The socket tests use the
test's own uid and a fake `seat0` file; the real `ACTIVE_UID=` line, a user switch and a
Bluetooth-era session change are covered by the parser table only.

**The systemd watchdog loop is proven against a datagram socket, not systemd.** The reader pets
`WATCHDOG=1` on every pass and sleeps at most half the interval; whether `WatchdogSec=1` holds
under load (a burst of events, a slow uinput write) is a manual check.

**A uinput write that fails is logged and dropped.** A failed or short write to the virtual
device leaves the remapper's `Held` records believing keys are down that never reached the
wire. Whether `/dev/uinput` can fail mid-stream in practice (nonblocking `EAGAIN` under load) is
unknown; a pause or `ReleaseAll` releases the records either way.

**Manual acceptance, waiting for the user and hardware.** The list is in the README, "Manual
checks".

## Standing facts

**The grab is released by the kernel when the fd closes.** A crash, a panic (`panic = "abort"`)
or `kill -9` closes every fd, so the keyboard returns to the kernel at once; systemd restarts
keycap and the grab gate waits for all keys to be up before grabbing again.

**Repeat is the compositor's job.** keycap drops `Repeat` events and sends only presses and
releases; the compositor generates repeat from the held virtual key, so a repeating mapped
chord repeats its output.

**`rustix` is outside the pinned block**, as in palmrest: poll, eventfd, `SOCK_SEQPACKET`,
`CLOCK_MONOTONIC` and the ioctl plumbing without libc's unsafe surface.

**`unsafe` lives in `daemon/sys.rs` only.** Each block has a SAFETY comment; ioctl opcodes are
checked by a test.
