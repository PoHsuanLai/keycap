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

**The daemon's device I/O is covered by CI device tests once green; not yet run.** The reader's
`poll` over real evdev fds, `read` until `EAGAIN`, `EVIOCGKEY` and `EVIOCGRAB` through the real
`EvdevGrabber`, hotplug from `notify` on `/dev/input`, and the uinput device itself are driven by
`crates/keycap/tests/devices` against a fake keyboard on `/dev/uinput`: mapping, the grab gate,
unplug and `kill -9` mid-chord, the escape chord and `Resume`, a second grabber. They have not run
yet; this stays open until the `devices` job has passed on GitHub's runner. What CI cannot show: a
real keyboard (an Apple or Bluetooth one, which registers under `uhid`) and a physical unplug.

**`SO_PEERCRED` is covered by CI device tests once green; the seat file on a real logind is
untested.** The device tests run the real binary with a fake seat file naming the test's uid and
check that a client running as another uid is an observer whose `Focus` is refused. The real
`ACTIVE_UID=` line, a user switch and a session change under logind are covered by the parser table
only; they stay manual.

**The systemd watchdog loop is covered by CI once green.** The reader pets `WATCHDOG=1` on every
pass and sleeps at most half the interval (one second under `WatchdogSec=2`). The device tests
check the ping cadence against a datagram socket, and `ci/systemd-test.sh` checks, under real
systemd, that `kill -STOP` gets the daemon restarted, that six quick `kill -9`s trip the start
limit and that `reset-failed` recovers. Neither has run yet. Whether the cadence holds under load
(a burst of events, a slow uinput write) is a manual check.

**Combo devices with a touchpad lose their pointer while grabbed.** A receiver that is a
keyboard and a touchpad in one node (EV_ABS: absolute axes, multitouch slots, `BTN_TOUCH`,
`BTN_TOOL_*`) is grabbed whole, and the virtual keyboard forwards only EV_KEY and EV_REL, so its
pointer stops working for as long as the grab holds. Options, none chosen: (1) leave such a
device ungrabbed (`Eligibility::NotKeyboard` when EV_ABS is present), which keeps the pointer
and loses the mapping on that keyboard; (2) register the device's axes on the virtual device
(`UI_ABS_SETUP` per axis, slots and tool bits copied from the source), which needs one virtual
pointer per source because the axis ranges differ and a keyboard device with a touchscreen's
axes confuses libinput's device classification; (3) a second virtual device per such source
that carries only the pointer half, with the keyboard half staying on "keycap keyboard".
Option 3 is the faithful one and the largest. Unverified on hardware which Apple or third-party
receivers expose EV_ABS on the keyboard node.

**LEDs are copied only as the desktop changes them.** The compositor sets Caps Lock on the
virtual keyboard and keycap writes the same change to every grabbed physical keyboard. A
keyboard grabbed while a lock is already on is not brought in line until the next change; the
nodes are opened read-write for this, which needs the unit's `input` group to have write access.
The forwarding itself (a LED written to the virtual keyboard reaches the grabbed one) is covered
by a CI device test once green; the real keyboard's LED is unverified on hardware.

**Manual acceptance, waiting for the user and hardware.** What remains manual after CI is a real
Apple or Bluetooth keyboard, a real compositor session (COSMIC, sill's `Focus`), and a logind
user switch. The list is in the README, "Manual checks".

## Standing facts

**A key held across a `SYN_DROPPED` is re-pressed only on its next press.** When a device's
queue overflows, keycap discards events up to the next `SYN_REPORT` and treats the device as
gone for the remapper: everything written for it is released and its mouse buttons are
released. A key still down on the device keeps no record, so its release writes nothing and
holding it produces no repeat until it is pressed again. The alternative, re-reading
`EVIOCGKEY` and replaying presses, would type keys the user pressed before the overflow.

**A failed uinput write replaces the virtual device.** A failed or short write leaves the
remapper's `Held` records out of step with the wire, so keycap destroys "keycap keyboard",
creates a fresh one (every key up) and resets the held state without writing. If the new device
cannot be made, the keyboards are given back as for the escape chord; `Resume` regrabs. The
batch that failed is lost.

**Virtual devices are never grabbed, except one named in `KEYCAP_GRAB_VIRTUAL`.** Every node
under `/sys/devices/virtual/input` is left alone, so another remapper's output is not fed back
through keycap. A keyboard that registers there on purpose (a remote-desktop input, a virtual
keyboard to be remapped) is not remapped. Bluetooth keyboards register under `uhid` and are not
affected. The CI keyboard is a uinput device, so the device tests start the daemon with
`KEYCAP_GRAB_VIRTUAL=<its exact name>`; the allowance is exact-match, one device, and never covers
keycap's own keyboard or palmrest's. Only root or the unit's environment can set it.

**A grab that succeeds is checked again.** A key that went down between the gate's check and
the grab reached the desktop but its release would not, so keycap re-reads `EVIOCGKEY` after
`EVIOCGRAB`, gives the grab back if any key is down and returns the keyboard to the gate.

**The socket bounds every client.** 32 clients in all and 8 per uid, 16 messages per client and
16 accepts per pass, and a pause of one second after `accept` fails for any reason but
`EAGAIN` (`EMFILE` leaves the connection pending, which would spin). An observer that sends a
request is disconnected; log lines about clients are limited to one per uid per minute.

**The grab is released by the kernel when the fd closes.** A crash, a panic (`panic = "abort"`)
or `kill -9` closes every fd, so the keyboard returns to the kernel at once; systemd restarts
keycap and the grab gate waits for all keys to be up before grabbing again.

**Repeat is the compositor's job.** keycap drops `Repeat` events and sends only presses and
releases; the compositor generates repeat from the held virtual key, so a repeating mapped
chord repeats its output.

**`rustix` is outside the pinned block**, as in palmrest: poll, eventfd, `SOCK_SEQPACKET`,
`CLOCK_MONOTONIC` and the ioctl plumbing without libc's unsafe surface.

**`unsafe` lives in `daemon/sys.rs` and, test-only, `keycap-devtest/src/sys.rs`.** Each block has
a SAFETY comment; ioctl opcodes are checked by a test in each.
