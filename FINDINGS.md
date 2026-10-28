FINDINGS
========

Open items and standing facts.

## Type rules (2026-10-28)

CONVENTIONS now follows the plain-Rust-first type rule: `bool` for fields, returns and private
functions; enums only for public `f(true, false)` arguments, a realistic third state, or disk/bus
values; newtypes only with a rule; two-way outcomes are `Result`/`Option`. The two-state private
enums that existed only because `bool` was banned are gone.

## Keymap-aware apps

An app that resolves Cmd itself through chordkit (temor, any quire app) must receive Super, not
the `terminal` profile's Ctrl+Shift+letter: Cmd+D and Cmd+Shift+D collapse to one chord there.
Foreign terminals (foot, kitty, ...) keep the rewrite.

**Mechanism: a profile, `keymap-aware`, chosen by the desktop from the app id.** Registration
(`KeymapCall::Register`) was considered and rejected: keycap only ever learns the focused window as
a profile name (`Focus`), never an app id, and a registration is tied to a connection, not a
window, so the daemon cannot tell which focused window the registering app owns. A guess
(any registration flips the focused terminal) would let a foreign terminal lose its rewrite while
temor runs in the background. The desktop does know the app id, and already maps it to a profile
(`ghostty` is the precedent), so the new profile needs no protocol change.

The profile inherits `default`, writes Command as Super and Control as Control, and restates the
editing rules `default` rewrites (Cmd+arrows, Option+arrows, the Backspace/Delete family,
Control+A/E) as themselves, because a rule cannot be deleted down an `inherits` chain. The
desktop's own chords (Cmd+Tab, Cmd+Space, Cmd+`, Cmd+Q) stay rewritten. A unit test fails if a
new `default` rule is neither a desktop chord nor restated.

Wiring: temor's app id is `org.quire.Temor` (its desktop entry, `StartupWMClass`). Plasma:
`OWN_PROFILES` in `dist/plasma/keycap-focus` maps it to `terminal`, not `keymap-aware` (changed
2026-10-14): KWin binds Super+T, Super+W, Super+D and Super+V globally, so Command as Super never
reached temor there, and chordkit's KDE keymap expects the Ctrl+Shift+letter forms `terminal`
writes. The cost on Plasma is that Cmd+D and Cmd+Shift+D arrive as one chord. sill: temor lists `TerminalEmulator`, so
sill's category table would give it `terminal`; sill's `DeclaredProfile` (`X-Keycap-Profile`) or
`OwnProfiles` must name `keymap-aware` for it (both rank above the category table). Quire apps
are not `TerminalEmulator`, so they never get the terminal rewrite; they get `default` (Cmd as
Ctrl) unless they too declare `keymap-aware`. A desktop-wide rule set in `[profile.default]` is
inherited, but one the desktop restates for a chord (sill's Cmd+H to Ctrl+H) must be restated in
`[profile.keymap-aware]` too.

## Open items

**Deferred from the 2026-10-16 production audit (input).**
- *Caller identity (INPUT-03, INPUT-14)* is done in the second wave, see "Who steers" below.
- *uinput access (INPUT-02)* is done: the desktop's root service owns `/dev/uinput` and keycap is handed
  its devices (see "Where the virtual devices come from"). The standalone install keeps `GROUP="input"`.
- *Unit hardening (INPUT-15), part.* `PrivateUsers=` may unmap the `input` supplementary group,
  which waits for a `systemd-analyze security` run. `/etc/keycap` stays root's under `DynamicUser=`:
  systemd.exec(5) hands the innermost folder to the service's user for `State`/`Cache`/`Logs`/`RuntimeDirectory=`
  but not for `ConfigurationDirectory=`, so the root-only machine file (see "Vendor layer") is read.
- *Supply chain (INPUT-16)* closed 2026-10-17: CI runs `cargo deny check licenses` and `advisories bans sources` (deny.toml allows crates.io and the three git sources of Cargo.lock); advisories read a live database, so a new one can fail an unchanged tree.
- *chordkit's own readers (INPUT-12)* closed: chordkit-kde and chordkit-gnome read their files through
  `chordkit-file`'s bounded regular-file read.

**Vendor layer (2026-10-17).** The desktop's Caps Lock rule must ship with every install and release, but a release
must never write the administrator's `/etc/keycap`. So keycap has a fourth layer, `/usr/lib/keycap/keycap.toml`
(`$KEYCAP_VENDOR_DIR`), between the built-in mapping and the machine file: built-in < vendor < machine < person.
It has the machine file's trust (lone-key rules allowed; `[user_layer] sequences` counts when the machine file sets
none). A missing folder is not watched, so a vendor file installed while the daemon runs is read at its next start.
Tests: `config/tests.rs` (`trusted_files`), `daemon/settings.rs` (order, bare key, `sequences`).

*Decided (owner, 2026-10-17): the machine file is checked too.* Both files are read by one reader,
`config/trusted.rs`, through peephole's own check (`dirs_trust` + `weaker` over `RealProc`): the file and every
folder above it root's, not group/other-writable, and a link never passes. Otherwise the file is no layer and one
warning in plain words ("installed defaults: not used, because someone other than an administrator could change this
file or a folder it is in"; the machine file's says "machine settings: ..."). A missing file or folder is no layer and
no warning. Both files are capped at 64 KiB (`LAYER_FILE_CAP`). Consequences:
- An administrator's group-writable `/etc/keycap/keycap.toml` stops applying, and the log says why. That is the point:
  the user-layer boundary (a rule on a key pressed alone only from a root-owned file) no longer leans on `/etc`
  alone being root's.
- A test that points `$KEYCAP_CONFIG_DIR` at a scratch folder gets no machine layer from a file written there (the
  scratch folder is the test user's). `keycap-devtest`'s daemon writes none today, so it is unaffected; a test that
  needs a machine layer needs a different seam, not a looser check. `config/tests.rs` proves the refusal on the real
  file system.
- `/etc/keycap` is root's under `DynamicUser=` with `ConfigurationDirectory=` (systemd.exec(5): that one folder
  type is not handed to the service's user), so the shipped unit reads a root-only machine file.

**Who steers (INPUT-03, INPUT-14, wave 2).** The owner decided keycap gets a light identity check and a reduced
user layer, and no root identity service and no new capability. The shell is the one connection obeyed for
`Focus` and `UserConfig`: its process sits in `sill.service` (or, for the Plasma feed,
`keycap-focus.service`) under `user@<active uid>.service`, held by pidfd (`Proof::Cgroup`; peephole's socket
pin is `Reopened`, and any pin is accepted). Every other process of the active uid is `Plain`: it hears status,
watches the keymap, registers actions and may `Resume`; any other request drops its connection. This is a speed bump,
not a boundary: a same-user program can move itself into a control group of that name. The boundary is the
reduced user layer: no rule on a key with no modifier held (Shift counts as one), no `then` that is a list
or empty, at most 256 rules (the layer is refused whole), unless the root-owned `/etc/keycap/keycap.toml` (or the vendor file, see "Vendor layer") says
`[user_layer] sequences = "allow"` (that lifts the list ban only; the machine file itself is unrestricted).
A `Focus` naming a profile nobody defined writes as `default` and is logged once, as before: that is the
refusal for (d), because a defined profile is the only thing a name can select.
- *Owner decision: `Resume` is allowed from `Plain`* (the active user's own processes, not `Observer`): it
  only re-grabs and types nothing, and `keycap resume` from a terminal is the escape-chord recovery.
- *Consequence: user rules on a bare key move to the machine file.* sill's shipped layer has `capslock` ->
  `ctrl+space`; it is reported and left out of the user layer, and the desktop's packaging now ships it in the vendor file
  `/usr/lib/keycap/keycap.toml` (see "Vendor layer"). `shift+capslock` moves with it (NEW-PI-5, 2026-10-17): Shift is held for every capital letter and every shifted symbol, so it is not a role that makes a key a shortcut. A user rule needs Command, Option or Control; `shift+a`, `shift+7`, `shift+;` and `shift+capslock` are left out of the user layer with a warning. The machine and vendor files may hold them.
- *The user layer is cut when it arrives.* Changing `[user_layer]` in the machine file later does not re-cut a
  layer already in force; the shell sends its file again on change or restart.
- *Probing needs `ProtectProc=default`.* A process without privileges cannot read another user's
  `/proc/<pid>/root` (peephole reads it for sandbox metadata) or image path. peephole v0.1.3 reads a
  refused `<pid>/root/.flatpak-info` as "no sandbox", as it reads a refused `exe` as hidden, so the probe
  reaches the control group; keycap's own `Procs` wrapper for this is gone.
- *INPUT-14.* The Plasma `keycap-focus` helper (Python under Gio, not JavaScript) answers `Focus` only to
  the connection that owns `org.kde.KWin` now (`GetNameOwner` per call) and owns its own name with
  `DO_NOT_QUEUE`, exiting if it is taken.

**Where the virtual devices come from (INPUT-02, wave 2).** With `/run/turnstile/input.sock` present, keycap
asks the root service for a `KeycapKeyboard` and, per combo device, a `KeycapPointer` (the original's name
as the label; the service adds the suffix), and uses the handed descriptor exactly as it used its own:
writes, `led_fd` and `leds` are unchanged. Without the socket, or if nothing answers on it, keycap opens
`/dev/uinput` as before (the standalone install, and a developer's machine). A refusal is the answer.
`dist/keycap.service` therefore has no `DeviceAllow=/dev/uinput`; `install.sh` adds it as the drop-in
`keycap.service.d/10-uinput.conf` for the standalone install, with `70-keycap.rules` as before. The desktop's
packaging ships neither the drop-in nor the udev line. `--check-uinput` connects to the service when the socket
is there (it creates nothing) and otherwise opens the node as before. Unverified: the service's behaviour
against a real keycap (needs the desktop gate), and `After=turnstile.socket` ordering on a real boot.

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

**Pointer twins and grab-time LEDs are untested on hardware.** A grabbed combo device whose node
has EV_ABS or relative axes the keyboard does not register gets a second virtual device,
`<name> (keycap pointer)`, cloned from the node (axes with `absinfo`, buttons, `INPUT_PROP_*`)
and carrying every non-key event of that device. Unverified: which Apple or third-party
receivers expose EV_ABS on the keyboard node; that libinput classes the twin as a touchpad
(gestures, palm rejection, tap-to-click from the cloned properties); that the twin keeps the
source's per-device settings; and that writing the remembered lock LEDs to a freshly grabbed
keyboard needs only the `input` group's write access to the node. A device that reports
`BTN_TOUCH` or `BTN_TOOL_*` with no EV_ABS is carried by the keyboard as before. LED forwarding from the virtual keyboard to a grabbed one is covered by a CI device test once green.

**Manual acceptance, waiting for the user and hardware.** What remains manual after CI is a real
Apple or Bluetooth keyboard, a real compositor session (sill's `Focus`), and a logind
user switch. The list is in docs/development.md, "Manual checks".

**Pre-switch review of 2026-10-03 (the user's K5 Max over Bluetooth).** What was checked on the
real machine's sysfs masks, and what stays open.
- The K5 Max keyboard node (`3434:0a53`, under `uhid`) reports `EV_ABS` (`ABS_VOLUME`, a
  consumer-control axis) and wheel axes, no `REL_X`. It used to classify as a keyboard *and
  pointer* and would have got a pointless "(keycap pointer)" twin; a pointer now needs
  `REL_X` or an absolute position axis (`ABS_X`, `ABS_MT_POSITION_X`). Its `Mouse` node and the
  Magic Mouse (`004c:0269`) have no letter keys and are never grabbed (table tests with the real
  masks, `discover/uhid_devices.rs`).
- A Keychron's Mac/Windows switch is not visible to the host: the firmware (QMK) swaps the
  keycodes it sends, with the same vendor and product id, the same capability masks and the same
  HID descriptor in both positions (checked on the K5 Max node's masks; reading the switch would
  need VIA's raw HID writes, which keycap will not do). So such keyboards are `Pc` by default and
  a `[[device]]` entry in the config sets the layout by id or name.
- **The `devices` suite cannot be run in a live desktop session.** Its "keycap keyboard" output
  reaches the compositor (a mapped Cmd+Q is Alt+F4), and before this review its daemon scanned
  the real `/sys/class/input`, so it would also have tried to grab the real keyboard when nobody
  else held it. A named `KEYCAP_GRAB_VIRTUAL` now excludes every other device, but the output
  problem remains: run the suite in CI or on a spare machine. It has still not run anywhere.
- Hotplug, a keyboard that wakes with a key down, and a keyboard that appears while paused are
  covered by `devices` tests with the fake grabber; the real wake and reconnect of a Bluetooth
  keyboard is a manual check (`docs/switch-from-toshy.md`, step 5.9).
- The Plasma focus feed (`dist/plasma/`) is unverified inside KWin: the D-Bus service, the wire
  message and the reconnect were run against a private session bus and a real daemon with no
  keyboard; the KWin script (`workspace.windowActivated`, `resourceClass`) is copied from Toshy's
  working script and untested here. KWin reports changes only, so after restarting the service
  one window change is needed before the profile is right.

## The desktop keymap

**Who owns what.** chordkit holds every keymap noun (actions, chords, overrides, the resolver,
display) and the platform sources. keycap is this desktop's source and remapper, and defines no
such type. It stores the person's overrides, serves them, tells apps when they change, and
supplies the desktop's own bindings.

**Two files, the person's last.** The machine's `keymap` (`/etc/keycap/keymap`, admin-owned,
optional) is read by the daemon; the person's own `$XDG_CONFIG_HOME/keycap/keymap` (else
`~/.config/keycap/keymap`) is read by `keycap-client` in each program and never by the daemon.
Both use chordkit's overrides line format. The person's file wins per action. Settings ›
Keyboard writes the person's file (the whole file, to a temporary name and renamed over it) and
needs no privilege; programs notice by polling its size and time every quarter second, so no
message to the daemon is involved. `KeycapSource::person_file()` says where it is.

**Judging.** The daemon (`keymap` module, watched with `config::watch::spawn_reading`) rereads
the machine file after 30 ms of quiet and judges it with the pure `settle`: a missing file is no
overrides; bad lines are skipped and reported with their line; an unreadable, oversized or
non-text file, or one in which no line reads, is a bad edit, so the last good copy stays and the
problem is reported. Chords the desktop keeps for itself are reported too. The person's file is
read leniently by the client: a bad line is skipped and reported, and an unusable file counts as
empty with one problem (no last-good copy is kept for it). Problems from both files are returned
by `load_reporting`, each labelled "System keymap:" or "Your keymap:".

**How apps get it.** Through quire, which takes a `chordkit::KeymapSource`: it passes
`KeycapSource::standard()`, whose `load(platform)` is chordkit's conventions for our desktop, the
desktop's own bindings (`keycap_client::system_bindings`, only what chordkit's conventions leave to
the source), the machine's overrides (asked of the daemon with `KeymapCall::Watch`, or, when no daemon
answers, read from `/etc/keycap/keymap`) and then the person's.
`watch(callback)` fires on the daemon's next `Keymap` message that differs from the last one, on a
change of the person's file, and, while there is no daemon, of the machine's file. Apps may
`register` their actions so Settings can list them (`registered()`); resolution stays in each
app, in-process, through chordkit, and registrations end with the connection.

**Seat standing of keymap clients.** A connection that only sends keymap messages is an observer
for the seat even when it runs as the seat owner's uid: it is classed on its first keymap message
(`Role::KeymapOnly`), is not counted as the owner being present, and its leaving never resets the
reader. It counts against its own per-uid limit (64), not the owner's (8).

**What the remap layer still does.** The keyboard is still remapped by profiles (`keycap.toml`):
Command acts as Control in apps that only know Control, Cmd+Space becomes Super+Space, and so on.
That layer works on keys, below the keymap: it does not read `keymap`. An app that resolves
through chordkit sees keys after that layer, so a shortcut in `keymap` is a shortcut as the
app's keymap names it, and the remap layer can still change which physical key produces it.
Moving the desktop's own bindings (launcher, switcher) off the remap rules and onto this keymap
is not done.

**Open.**
- `system_bindings` has one row (the launcher also on Ctrl+Alt+Space, from sill's custom
  shortcuts). chordkit's conventions for our desktop already carry the rest; confirm no other
  desktop-owned chord differs from them.
- A connection that connects and leaves without sending anything is still counted as an owner
  until it goes, so it can trigger the reset; keymap clients always send first.
- Registrations are not checked against chordkit's conflicts; they are only listed.

## Standing facts

**A pointer twin's life follows the grab.** It is created right after a device is grabbed (the
reader compares the grabbed set after every step that can change it) and destroyed, after its
held buttons are released, when the device is let go: unplugged, paused by the escape chord, or
a failed recreation of the keyboard. A twin that cannot be created, or whose write fails, is
logged and dropped; the device's pointer events then pass through "keycap keyboard", which
registers no absolute axes, so a touchpad stops until the next grab while a mouse keeps working.
`MSC_*` events are not forwarded: `MSC_SCAN` accompanies every keystroke and the twin registers
no `EV_MSC`. Absolute-axis state is not tracked: destroying the twin ends any touch the desktop
saw.

**The lamps are written once per grab.** keycap remembers the last LED state the desktop set on
the virtual keyboard and writes it to a device when it is grabbed; a lamp the desktop never set
is not written. Later changes are forwarded as before.

**An undefined focused profile is logged once per config generation.** A `Focus` naming a
profile the config does not define uses `default`, logs one line per name (64 names, then one
line saying there are more), and logs it again after any config reload.

**A configuration has size limits.** At most 64 profiles, 8 `inherits` steps and 512 rules in one
profile after the layers are folded (`keycap-core` `config/limits.rs`). A system file or a user
config over them is refused whole with a plain warning and the earlier settings stay. Flattening
builds each profile once from its parent's.

**Devices are looked at again.** A `notify` error or an overflow flag sends `Hotplug::Rescan`;
a burst of appearances is one scan; every node is looked at every 30 seconds in case something
was missed. Only nodes whose udev entry (`/run/udev/data/c<major>:<minor>`) does not name another
seat than `seat0` are candidates.

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
request is disconnected (so is a program of the seat owner's that is not the shell); log lines about clients are limited to one per uid per minute. A client
that asks the reader for more than 100 things a second, or whose requests fill the reader's inbox
(256), is disconnected. A reply never exceeds one packet: the keymap's problem list is 50 lines of
200 characters and a count, an app registers at most 128 actions, and a `Registered` reply is cut
at 48 KiB. A message too large for a packet, or a full queue, never closes a client: a status or
keymap a client missed is sent again, in its latest form, as soon as there is room. Only the
seat owner's programs, the shell or not, may register actions.

**The grab is released by the kernel when the fd closes.** A crash, a panic (`panic = "abort"`)
or `kill -9` closes every fd, so the keyboard returns to the kernel at once; systemd restarts
keycap and the grab gate waits for all keys to be up before grabbing again.

**Repeat is the compositor's job.** keycap drops `Repeat` events and sends only presses and
releases; the compositor generates repeat from the held virtual key, so a repeating mapped
chord repeats its output.

**`rustix` is outside the pinned block**, as in palmrest: poll, eventfd, `SOCK_SEQPACKET`,
`CLOCK_MONOTONIC` and file opens without libc's unsafe surface.

**No `unsafe` anywhere; the workspace lint is `forbid` (2026-10-17).** The ioctls in `daemon/sys.rs` and,
test-only, `keycap-devtest/src/sys.rs` used to be rustix `Ioctl` blocks (including a hand-written
`unsafe impl Ioctl` for `EVIOCGABS`). They now go through `input-linux` 0.7 (MIT, `default-features = false`,
the same line as turnstile): `EvdevHandle` for `EVIOCGRAB`, `EVIOCGKEY`, `EVIOCGLED`, `EVIOCGBIT`, `EVIOCGPROP`,
`EVIOCGABS` and `UInputHandle` for the `UI_SET_*BIT` calls, `UI_DEV_SETUP`, `UI_ABS_SETUP`, `UI_DEV_CREATE`,
`UI_DEV_DESTROY`, `UI_GET_SYSNAME`. No site stays unsafe. What changed in behaviour: a raw code is now checked
against its kind's range before the ioctl (`InvalidInput`); `UInputHandle::create` does setup, the axis setups and
create in one call, so the axes are passed at creation, not as the bits are set; on `EINVAL` from `UI_DEV_SETUP`
(a kernel before 4.5) it falls back to the legacy `uinput_user_dev` write. The crate brings `nix` 0.29 beside
the `nix` 0.31 that `notify` uses. The opcode and struct-size tests are gone with the opcodes; the manual check is
that keycap grabs the keyboard and remaps, and releases on exit (the gated device tests cover it).

## Re-audit rows (2026-10-17)

- *INPUT-15, `PrivateUsers=` assessed and not set.* The daemon compares the uid in `SO_PEERCRED` with the
  seat's active user (`socket/seat.rs`). With `PrivateUsers=yes` every process outside the unit's user namespace
  reads as the overflow uid, so no client would ever match the seat owner and the shell could not steer. The other
  deferred half, `ProtectProc=invisible`, stays dropped for the steering check (WAVE2-DESIGN 4.8). The unit now also
  unsets the 22 loader variables so that turnstile can prove it `Full`. Closes if steering moves to a mechanism that
  does not read uids.
- *INPUT-02, the standalone install keeps its `input` group rule.* `install.sh` and `70-keycap.rules` (a node open
  to the `input` group) are for a machine with no turnstile, where keycap opens `/dev/uinput` itself. The desktop's
  packaging ships neither. Closes when no supported standalone target remains.
- *INPUT-03, steering identity.* Owner decision: no root identity check for keycap's own socket; the bound is the
  user layer's confinement, now including Shift (NEW-PI-5).
- *INPUT-20, traced by reading, not run.* When keycap dies, the kernel destroys "keycap keyboard". In casement the
  keyboard reaches the seat only through libinput (`casement-backend/src/input/libinput_real.rs`): libinput's evdev
  backend releases every key a device holds before it reports the device removed, those releases arrive as ordinary key
  events from a device `convert` still knows, and `DeviceRemoved` then only drops the device from the roster
  (`FedInput::Gone` reaches `present.gone`, `tty/mod.rs`), with nothing else keyed on it. So a Command or Control held
  at a `kill -9` is released by libinput's own events. The only place keys are released "silently" is the test double
  `VirtualInput::unplug`, which is not the real path. What remains unproven is libinput's behaviour on this kernel and
  version: the device test (hold a modifier, `kill -9` keycap, read the seat's pressed set) belongs to casement's
  suite and stays on the manual queue.
