# keycap

The Mac keyboard mapping daemon. A system service that grabs the keyboards, rewrites keys by
profile (Command acts as Control, Option as Alt, Cmd+Left is Home, and so on) and writes the
result to one virtual keyboard. It runs from boot, so the mapping also holds at the login
screen and on a text console. The engine knows profiles, not apps: any desktop drives it with
one socket message, `Focus { profile }`. sill is the first. Portable to any Linux; no desktop
dependency.

## Architecture

```
evdev (grabbed) ──► GrabGate ──► Remapper::step ──► Vec<RemapOut> ──► dispatch ──► uinput keyboard
   EVIOCGRAB       (no key held)   (pure state)                  │
   └─ touchpad axes of a combo device ──────────────────────────┴─────────────────► "<name> (keycap pointer)"
socket ── Focus ───────────────────► Remapper::step (profile)    └─ ReleaseGrabs ─► ungrab, Paused
socket ── UserConfig ──► config merge ──► Profiles
/etc/keycap/keycap.toml ── inotify ──► config merge
```

Two threads and no async runtime: the **reader** (poll over every grabbed fd, an eventfd and
the timeout; pets the systemd watchdog, drains the socket channel, then feeds the core) and the
**socket** thread (accept, peer classification, fan-out of `Status`), woken by an eventfd so
the reader never waits on a client.

The pure core takes events and instants and returns values; nothing in it reads a clock or
touches a device, so a script replays to exactly the outputs the live daemon produced.

| Module | What it is |
| --- | --- |
| `key` | `Key(u16)` from the name table, `Modifier`, `Side`, `Mods` |
| `event` | `Edge`, `KeyEvent`, the `Physical` and `Logical` key newtypes |
| `layout` | `Layout { Apple, Pc }` from the vendor; `role` by key position |
| `profile` | `Trigger`, `Stroke`, `RuleAction`, `Profile`, `Profiles` (`default` always exists) |
| `config` | lenient TOML into `ConfigLayer`s (built-in defaults, system file, the seat owner's `UserConfig`), folded by `resolve` |
| `remap` | `Remapper::step(self, RemapInput, &Profiles) -> (Self, Vec<RemapOut>)` |
| `held` | what each physical press emitted, so its release sends exactly that |
| `gate` | `GrabGate`: a keyboard is grabbed only once no key is down |
| `escape` | the emergency chord, both Shifts + Esc |
| `backoff` | the pure reconnect backoff |
| `keycap-protocol` (crate) | the socket messages and their codec |
| `daemon` | threads, socket, discovery, the uinput keyboard, sd_notify, and `sys`: the ioctls, the only module allowed `unsafe` (besides the test-only `keycap-devtest`) |

## The socket protocol

`/run/keycap/keycap.sock`, `SOCK_SEQPACKET`, mode 0666, one JSON message per packet, adjacently
tagged (`{"kind": "...", "v": {...}}`). The first packet on a connection is `Hello`; a client
checks `version` (`PROTOCOL_VERSION = 1`) before reading on. `MAX_PACKET` is 64 KiB, so a
config fits in one packet.

```rust
pub enum ToDaemon {
    Focus { profile: ProfileName },   // the whole desktop-facing contract
    UserConfig { toml: String },      // the seat owner's settings, parsed leniently
    Resume,                           // after the escape chord or a pause
}
pub enum ToClient {
    Hello { version: u16, status: Status },
    Status(Status),
    ConfigWarnings(Vec<Warning>),
}
pub struct Status { mode: Mode, keyboards: Vec<Keyboard> }  // empty = no keyboards
pub enum Mode { Running, Paused }                           // Paused = escape chord
pub struct Keyboard { info: KeyboardInfo, grab: Grab }
pub enum Grab {
    Active,
    WaitingForRelease,                // grab gate: a key is held
    Conflict,                         // EVIOCGRAB -> EBUSY: another remapper holds it
}
// ProfileName: validated [a-z0-9_-]+, parsed once at the boundary
```

`SO_PEERCRED` sorts each client into `Peer { SeatOwner, Observer }`, using `ACTIVE_UID` from
`/run/systemd/seats/seat0`. Only the seat owner's `Focus`, `UserConfig` and `Resume` are
honoured; every client receives `Status`.

## Lifecycle

keycap's lifetime is the machine's, not the session's.

| Moment | What keycap does |
| --- | --- |
| Boot | Starts from `multi-user.target`, before the display manager, and grabs keyboards once no keys are held. The login screen and text consoles already have Mac keys, with the `default` profile. |
| Login | Nothing restarts. The desktop connects and starts sending `Focus`. The seat owner's `UserConfig` applies within one message. |
| Logout or desktop crash | The client's socket closes, and keycap drops back to `default` and the system config. The keyboards stay grabbed throughout. |
| User switch | `ACTIVE_UID` in `/run/systemd/seats/seat0` changes. keycap re-reads it when that file changes and only listens to the new owner. |
| Suspend/resume, keyboard reconnect | Each device is re-grabbed through the grab gate. The daemon itself keeps running. |

Only per-app profiles and the user's own settings wait for a session.

## Failure behaviour

| Failure | What keycap does | Proven by |
| --- | --- | --- |
| Crash or panic (`panic = "abort"`) | The kernel releases the grab, systemd restarts keycap within 200 ms, and the grab gate waits for keys to be released before grabbing again | gate table tests, `Held::drain` tests, device test (`kill -9`, restart) |
| Hang | Watchdog kill after 2 s (the reader wakes at least every second), then the crash path | device test (ping cadence), systemd tier (`kill -STOP`) |
| Crash loop | The unit becomes failed after 5 restarts; the desktop notices | systemd tier (six `kill -9`s, `reset-failed`) |
| Killed mid-chord | The kernel drops the grab and closes the virtual keyboard with the process, so nothing stays down; the restarted daemon starts with a fresh `Remapper` | gate table tests, device test |
| Keyboard unplugged mid-chord | `DeviceGone` releases exactly what that keyboard's keys wrote | core tests, device test |
| Profile changed mid-chord | `Focus` applies from the next press; held keys keep what they wrote | core tests |
| Combo device (keyboard and touchpad in one node) | Its keys are remapped as usual; its absolute axes, buttons and motion go to a second virtual device, `<name> (keycap pointer)`, cloned from the node's axes, ranges and properties, made when the device is grabbed and destroyed when it is let go, unplugged or paused, after its held buttons are released | table tests of the capability value, recording twins in the reader tests; hardware unverified |
| Focus on a profile the config does not define | The default profile applies and the log says so once per name until the config changes | `Unknowns` table test |
| Another remapper holds the keyboard | `Grab::Conflict`; the grab is retried after 1 s, doubling to 30 s, never in a loop | devices tests with a fake grabber, device test with a second grabber |
| Emergency chord | `RemapOut::ReleaseGrabs` and `Status::Paused` | core test, device test (`Resume` included) |

`RemapInput::ReleaseAll` is the core's drain entry point, used by the tests and by `keycap replay`
(`release-all`); the daemon never sends it, because a process that dies takes its grab with it.

## Safety rules

keycap grabs the user's only keyboard, so:

- **Tests create no input device unless they are the gated device tests.** The ordinary suite
  drives the core with scripts and a recording sink and never constructs the uinput device. The
  device tests (`crates/keycap/tests/devices`) are the one exception: each is `#[ignore]`d and
  asserts `KEYCAP_DEVICE_TESTS=1`, so neither `cargo test` nor `cargo test -- --ignored` alone
  creates a device. CI runs them (see Tests).
- **Virtual devices are never grabbed**, so another remapper's output cannot loop back through
  keycap. The one exception is `KEYCAP_GRAB_VIRTUAL=<exact device name>`, which makes the virtual
  device with that name eligible like a hardware keyboard, for tests whose keyboard is itself a
  uinput device. Only root, or the unit's `Environment=`, can set it; it admits
  exactly one named device and makes it the only device grabbed (no hardware keyboard is touched),
  and keycap's own keyboard and palmrest's stay excluded even when named. Leave it unset on a
  real machine.
- **No `record` command.** A tool that writes every keystroke to a file is a keylogger. Test
  scripts are synthesised by hand; `keycap replay FILE` feeds one through the core.
- `keycap --check-uinput` opens and closes `/dev/uinput` without `UI_DEV_CREATE`, and only when
  `KEYCAP_ALLOW_UINPUT_PROBE=1` is set.
- The emergency chord (both Shifts + Esc) releases every grab at once; `Resume` grabs again.
- keycap never changes kernel module parameters or `/etc` files; `install.sh` does, with sudo,
  and only when asked (`--dry-run` prints the same commands and runs none).
- The device tests make "keycap keyboard" write real key events to the desktop session. Run them
  on a machine you are not using (CI), not in your own session: a mapped Cmd+Q is Alt+F4.

## Install

```bash
./install.sh --dry-run          # print every command, change nothing
./install.sh                    # build, install, enable and start
./install.sh --desktop plasma --plasma-focus
./uninstall.sh [--purge] [--plasma-focus] [--restore-toshy]
```

`install.sh` builds a release binary and installs, with sudo, only what differs from what is
already there (run it twice and the second run changes nothing): the binary to
`/usr/local/bin`, `dist/keycap.service`, `dist/70-keycap.rules` (the `input` group may write
`/dev/uinput`), `dist/keycap.polkit.rules` (the active session user may restart and reset the
unit), then reloads systemd and udev, restores SELinux labels and enables the service. A running
keycap is restarted when its binary or unit changed; a changed `/etc/keycap/keycap.toml` needs no
restart, keycap reloads it. With
`--desktop plasma` it also installs `dist/keycap.plasma.toml` as `/etc/keycap/keycap.toml` when
that file is absent (never over an existing one). `--plasma-focus` adds the focus feed below,
and restarts the helper on every run (enable, then restart), so it runs the script just installed.
It starts keycap next to any running remapper, which then shows as `Grab::Conflict` until the
other lets go. `DESTDIR=dir` stages the files under `dir` and skips systemd, udev and sudo.
`docs/switch-from-toshy.md` has the full switch and rollback.

### The Plasma focus feed

Plasma has no way to ask "which window has focus" over a socket, so `dist/plasma/` has two
small parts: a KWin script (`kwin-script/`) reports the focused window's resource class over the
session bus, and `keycap-focus` (Python, needs `python3-gobject`) owns `org.keycap.Focus` and
turns it into `Focus { profile }`: `ghostty` for Ghostty, `terminal` for other terminal
emulators, `filemanager` for file managers, `default` for everything else. It reopens the socket when keycap restarts. The
lists are at the top of the script. Limits: a terminal embedded in another window gets that
window's profile; after restarting `keycap-focus`, focus another window once, since KWin only
reports changes.

### Per-app profiles

The daemon never sees an app id: the desktop maps the focused window to a profile name and sends
`Focus { profile }`, so any app can have a profile of its own with no keycap change. Define
`[profile.NAME]` with `inherits = "terminal"` (or another profile) and have the desktop send NAME;
a name nobody defined is `default`. On Plasma the feed's `OWN_PROFILES` list (top of
`dist/plasma/keycap-focus`) maps a part of the resource class to a profile; Ghostty is the first
entry.

## Tools

```
keycap                          run the daemon
keycap --print-config           print the built-in defaults.toml
keycap --check-uinput           can this user create virtual devices?
keycap status                   what the running daemon is doing; exit 0 only if a keyboard is grabbed
keycap resume                   grab the keyboards again after the escape chord
keycap replay FILE [--config FILE]
keycap --help | --version
```

`--check-uinput` opens and closes `/dev/uinput` and creates nothing; it refuses to run unless
`KEYCAP_ALLOW_UINPUT_PROBE=1` is set.

The daemon reads its environment once, at start:

| Variable | Meaning | Default |
| --- | --- | --- |
| `NOTIFY_SOCKET`, `WATCHDOG_USEC` | systemd's `sd_notify` socket and watchdog interval | none |
| `RUNTIME_DIRECTORY` | where `keycap.sock` lives | `/run/keycap` |
| `KEYCAP_CONFIG_DIR` | where `keycap.toml` is read | `/etc/keycap` |
| `KEYCAP_SEAT_FILE` | logind's seat state file, for the active uid | `/run/systemd/seats/seat0` |
| `KEYCAP_SYS_INPUT`, `KEYCAP_DEV_INPUT` | where nodes are listed and opened | `/sys/class/input`, `/dev/input` |
| `KEYCAP_GRAB_VIRTUAL` | one virtual device, by exact name, that may be grabbed (see Safety rules) | none |
| `KEYCAP_ALLOW_UINPUT_PROBE` | lets `--check-uinput` open the node | none |

The path variables exist so a test can run the real binary against scratch directories.

`keycap replay FILE` feeds a script through the remapper with the built-in profiles (and
`--config FILE` laid over them as the user layer) and prints each input with what it writes.
It never touches a device. A script has one input per line; `#` starts a comment:

```
<ms> press|release|repeat <dev> <apple|pc> <keyname>
<ms> focus <profile>
<ms> gone <dev>
<ms> release-all
<ms> resume
<ms> led caps|num|scroll on|off
```

`<dev>` is a number naming a keyboard, `apple` or `pc` its layout, `<keyname>` a name from the
key table (`leftmeta`, `c`, `left`). `led` is the desktop setting a LED of the virtual keyboard;
with the Caps Lock key remapped, a `led caps on` nothing in the script asked for is answered by
a `+capslock SYN -capslock SYN` tap before the next typed key, as the daemon does. Output is `+key` for a press, `-key` for a release, `SYN`
for the sync, then `release-grabs` and `status paused` for the escape chord:

```
$ cat copy.txt
0 press 1 apple leftmeta
10 press 1 apple c
20 release 1 apple c
30 release 1 apple leftmeta
$ keycap replay copy.txt
0 press 1 apple leftmeta -> +leftctrl SYN
10 press 1 apple c -> +c SYN
20 release 1 apple c -> -c SYN
30 release 1 apple leftmeta -> -leftctrl SYN
```

## Configuration

Three layers, each overriding the one before:

1. `defaults.toml`, compiled in: the Mac mapping below.
2. `/etc/keycap/keycap.toml`: the machine's settings, re-read when the file changes.
3. The seat owner's `UserConfig`, sent over the socket (a desktop forwards
   `~/.config/keycap/keycap.toml`).

`keycap --print-config` prints the built-in `defaults.toml`, the file to copy from. A bad value costs only its own key (logged,
the layer below used and a `ConfigWarnings` sent); an unknown key is ignored. A rule with
`then = []` swallows its chord, which removes a rule a profile inherits. Profiles are open:
a name the config does not define resolves to `default`.

## The default v1 mapping

| Profile | Input | Output |
| --- | --- | --- |
| `default` (modmap) | Command, Option, Control | Control, Alt, Super |
| `default` | Cmd+Left, Cmd+Right | Home, End |
| `default` | Cmd+Up, Cmd+Down | Ctrl+Home, Ctrl+End |
| `default` | Option+Left, Option+Right | Ctrl+Left, Ctrl+Right |
| `default` | Option+Backspace | Ctrl+Backspace |
| `default` | Cmd+Backspace | Shift+Home, Delete |
| `default` | Cmd+Tab, Cmd+Shift+Tab | Alt+Tab, Alt+Shift+Tab, Alt held until Cmd is released |
| `default` | Cmd+`, Cmd+Shift+` | Alt+`, Alt+Shift+` (windows of one app), Alt held likewise |
| `default` | Cmd+Space | Super+Space (sill's launcher on COSMIC; Plasma's file, below, makes it Alt+Space) |
| `default` | Ctrl+Space, Ctrl+Shift+Space | Ctrl+Space, Ctrl+Shift+Space (input method switch, not Super+Space) |
| `default` | Cmd+Q | Alt+F4 |
| `default` | Ctrl+A, Ctrl+E | Home, End |
| `terminal` | inherits `default`; every Cmd+letter except Q (Cmd+C, V, T, W, N, F, D, Z, K, ...) | Ctrl+Shift+letter: never a bare Ctrl+letter, which in a terminal is a control character (Ctrl+D is end-of-file, Ctrl+Z suspends); an unlisted Cmd+letter does nothing harmful |
| `terminal` | Cmd+Backspace, Cmd+Delete | Ctrl+U, Ctrl+K (kill to line start, to line end) |
| `terminal` | Cmd+Shift+Left, Cmd+Shift+Right | Ctrl+PageUp, Ctrl+PageDown (previous, next tab) |
| `terminal` | Cmd+Minus, Cmd+=, Cmd+., Cmd+[, Cmd+], Cmd+Backslash | Ctrl+Minus, Ctrl+Shift+=, Ctrl+C, Ctrl+Shift+[, Ctrl+Shift+], Ctrl+Shift+Backslash |
| `terminal` | Option+Backspace, Option+Delete | Ctrl+W, Esc then D (delete word left, right) |
| `ghostty` | inherits `terminal`; Cmd+D, Cmd+Shift+D | Ctrl+Shift+O, Ctrl+Shift+E (split right, down) |
| `ghostty` | Cmd+K, Cmd+Option+I, Cmd+= | Ctrl+L (clear), Ctrl+Shift+I (inspector), Ctrl+= (zoom in) |
| `filemanager` | inherits `default`; Cmd+Backspace | Delete (trash the selection; not "select to the start, delete") |
| `filemanager` | Cmd+Left, Right, Up, Down; Cmd+I; Cmd+Shift+. | Alt+Left, Right, Up, Enter; Alt+Enter; Ctrl+H |
| `terminal` | physical Control | stays Control (modmap, and Ctrl+A, Ctrl+E map to themselves), so Ctrl+C still stops the program and readline keeps its keys |

`dist/keycap.plasma.toml` (installed as `/etc/keycap/keycap.toml` by `install.sh --desktop plasma`)
lays Plasma's equivalents of macOS shortcuts over this: Cmd+Space is Alt+Space (KRunner),
Cmd+Ctrl+Space is Super+. (emoji), Cmd+Ctrl+F is Super+Shift+F (full screen), Cmd+H minimises,
Cmd+F3 shows the desktop, Cmd+Shift+3/4/5 are Spectacle's, Ctrl+Left/Right switch desktops
(in terminals too, as in macOS Terminal). Personal choices (Caps Lock as the input switch, a
keyboard's layout) go in your own config: `docs/personal-config.md`.

A keyboard's layout (which key beside the space bar is Command) comes from its vendor id: Apple
(`05ac`, `004c`) is Apple, everything else PC. A keyboard with a Mac/Windows switch (Keychron, ...)
reports the same ids in both positions, so the host cannot see the switch; name it with a
`[[device]]` entry (`match = "vendor:product"` or an exact name, `layout = "apple"|"pc"`) in the
config, see `docs/personal-config.md`.

Modifiers are mapped at press time, so Cmd+click works although the mouse does not pass through
keycap. When a chord matches a rule, keycap changes only the modifiers that need to change.

## Manual checks

What CI cannot reach, waiting for the user and real hardware:

- A real Apple keyboard, and a Bluetooth keyboard reconnecting (a uhid device, not a fake).
- Install the unit and check the mapping at the login screen, in a real compositor session
  (the COSMIC session, sill's `Focus` messages, Cmd+C in a terminal and in a GUI app).
- That the compositor gives the virtual keyboard the session's layout.
- A user switch under logind: `ACTIVE_UID` changing, the old owner no longer obeyed.
- Run keyd at the same time: the `Conflict` notification appears.
- The escape chord, then `Resume`.
- Bluetooth keyboard reconnect.
- Grab a keyboard (or restart keycap) while Caps Lock is on: its light matches the desktop's.
  With the Caps Lock key remapped, a lock nothing typed through keycap asked for is tapped off
  before the next key typed (the journal says `turned Caps Lock off`), and `keycap status` shows
  `Caps Lock: ON` until then. keycap follows the lock from the LEDs the desktop sets on its
  virtual keyboard, so a lock the desktop reports a moment after the grab is caught too; one set
  with Shift+Caps Lock stays, also across a VT switch. The tap waits for a key because the
  desktop may not have opened the new virtual keyboard yet. `keycap replay` plays it with
  `<ms> led caps on|off` lines.
- A receiver with a touchpad: the pointer keeps working while it is grabbed, and stops with
  the escape chord only because the device is given back.

CI covers the rest: the mapping through the kernel, the grab gate, unplug and `kill -9` mid-chord,
the escape chord and `Resume`, LED forwarding, a second grabber, the seat check, the watchdog and
the unit's restart and start-limit behaviour.

## Tests

Three tiers, each in its own CI job (`.github/workflows/ci.yml`).

1. **The gate** (`gate`). `cargo test --workspace --all-features` runs the unit tables and the
   core scenario suite: every v1 mapping in both profiles, Cmd held across a profile switch,
   unplug mid-chord, `ReleaseAll` and `Resume` after the escape chord, the escape chord, the
   grab gate with a key held, Apple vs PC layout, and a property test that every emitted press
   has exactly one release. Socket tests run on a private socket in a temp directory. Scripts
   are synthesised in `tests/support`. No device is touched.
2. **The device tests** (`devices`). `crates/keycap/tests/devices` runs the built `keycap` binary
   against a fake keyboard on `/dev/uinput` and reads the "keycap keyboard" it writes back from
   `/dev/input`, through the real evdev grab, the real uinput device and the real socket. The
   harness is the `keycap-devtest` crate. Every test is `#[ignore]`d and asserts
   `KEYCAP_DEVICE_TESTS=1`. On a Linux machine you may use as a test bed (they create and grab
   a keyboard for a few seconds each; the fake is grabbed, your real keyboards are not):

   ```bash
   sudo modprobe uinput
   cargo build -p keycap --bins --examples
   cargo test -p keycap --test devices --no-run     # build as yourself
   sudo -E env "PATH=$PATH" KEYCAP_DEVICE_TESTS=1 \
     target/debug/deps/devices-<hash> --ignored --test-threads=1
   ```

   They need root (the nodes are `root:input`), run one at a time (one fake name, one daemon),
   and dump the daemon's stderr, the output device's event trace and the status history when a
   test fails. Set `KEYCAP_TEST_ARTIFACTS=DIR` to keep those logs in files.
3. **The systemd tier** (`systemd`). `ci/systemd-test.sh` installs the release binary, the unit,
   the udev rule and the polkit rule on a runner whose PID 1 is systemd, adds a drop-in that
   sets `KEYCAP_GRAB_VIRTUAL`, plugs in a fake keyboard and checks that the unit starts under
   its hardening, that `kill -STOP` makes the watchdog restart it, that six quick `kill -9`s end
   in `failed` (start limit), and that `reset-failed` and `start` recover. It installs files, so
   it refuses to run outside CI unless `KEYCAP_CI_ALLOW_INSTALL=1`.
