# keycap

The Mac keyboard mapping daemon. A system service that grabs the keyboards, rewrites keys by
profile (Command acts as Control, Option as Alt, Cmd+Left is Home, and so on) and writes the
result to one virtual keyboard. It runs from boot, so the mapping also holds at the login
screen and on a text console. The engine knows profiles, not apps: any desktop drives it with
one socket message, `Focus { profile }`. sill is the first. Portable to any Linux; no desktop
dependency.

## Architecture

```
evdev (grabbed) ──► GrabGate ──► Remapper::step ──► Vec<Out> ──► dispatch ──► uinput keyboard
   EVIOCGRAB       (no key held)   (pure state)                  │
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
| `profile` | `Trigger`, `Stroke`, `Action`, `Profile`, `Profiles` (`default` always exists) |
| `config` | lenient TOML into `Layer`s (built-in defaults, system file, the seat owner's `UserConfig`), folded by `resolve` |
| `remap` | `Remapper::step(self, Input, &Profiles) -> (Self, Vec<Out>)` |
| `held` | what each physical press emitted, so its release sends exactly that |
| `gate` | `GrabGate`: a keyboard is grabbed only once no key is down |
| `escape` | the emergency chord, both Shifts + Esc |
| `backoff` | the pure reconnect backoff |
| `keycap-protocol` (crate) | the socket messages and their codec |
| `daemon` | threads, socket, discovery, the uinput keyboard, sd_notify, and `sys`: the ioctls, the only module allowed `unsafe` |

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
| Crash or panic (`panic = "abort"`) | The kernel releases the grab, systemd restarts keycap within 200 ms, and the grab gate waits for keys to be released before grabbing again | gate table tests, `Held::drain` tests |
| Hang | Watchdog kill after 1 s, then the crash path | manual check |
| Crash loop | The unit becomes failed after 5 restarts; the desktop notices | manual check |
| Killed mid-chord, keyboard unplugged mid-chord, profile changed mid-chord | `ReleaseAll` releases exactly what `Held` recorded | core tests |
| Another remapper holds the keyboard | `Grab::Conflict`; the grab is retried after 1 s, doubling to 30 s, never in a loop | devices tests with a fake grabber |
| Emergency chord | `Out::ReleaseGrabs` and `Status::Paused` | core test |

## Safety rules

keycap grabs the user's only keyboard, so:

- **Tests never create an input device.** Every test drives the core with scripts and a
  recording sink. The uinput device is compiled and reviewed, and its event encoding is tested,
  but nothing in the suite constructs it.
- **No `record` command.** A tool that writes every keystroke to a file is a keylogger. Test
  scripts are synthesised by hand; `keycap replay FILE` feeds one through the core.
- `keycap --check-uinput` opens and closes `/dev/uinput` without `UI_DEV_CREATE`, and only when
  `KEYCAP_ALLOW_UINPUT_PROBE=1` is set.
- The emergency chord (both Shifts + Esc) releases every grab at once; `Resume` grabs again.
- keycap never changes kernel module parameters or `/etc` files; the install script prints the
  root steps instead of running them.

## Install

```bash
./install.sh
```

`install.sh` builds a release binary and installs it to `/usr/local/bin`. It prints, and does
not run, the commands that need root:

- `dist/keycap.service` to `/etc/systemd/system/`, then `systemctl enable --now keycap`.
- `dist/70-keycap.rules` to `/etc/udev/rules.d/`: the `input` group may write `/dev/uinput`.
- `dist/keycap.polkit.rules` to `/etc/polkit-1/rules.d/`: the active session user may restart
  and reset `keycap.service`.

## Tools

```
keycap                          run the daemon
keycap --print-config           print the built-in defaults.toml
keycap --check-uinput           can this user create virtual devices?
keycap replay FILE [--config FILE]
keycap --help | --version
```

`--check-uinput` opens and closes `/dev/uinput` and creates nothing; it refuses to run unless
`KEYCAP_ALLOW_UINPUT_PROBE=1` is set.

`keycap replay FILE` feeds a script through the remapper with the built-in profiles (and
`--config FILE` laid over them as the user layer) and prints each input with what it writes.
It never touches a device. A script has one input per line; `#` starts a comment:

```
<ms> press|release|repeat <dev> <apple|pc> <keyname>
<ms> focus <profile>
<ms> gone <dev>
<ms> release-all
<ms> resume
```

`<dev>` is a number naming a keyboard, `apple` or `pc` its layout, `<keyname>` a name from the
key table (`leftmeta`, `c`, `left`). Output is `+key` for a press, `-key` for a release, `SYN`
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
| `default` | Cmd+Tab, Cmd+Space | the desktop's own shortcuts (sill's) |
| `default` | Cmd+Q | Alt+F4 |
| `default` | Ctrl+A, Ctrl+E | Home, End |
| `terminal` | inherits `default`; Cmd+C, V, T, W, N, F | Ctrl+Shift+C, V, T, W, N, F |
| `terminal` | physical Control | stays Control (modmap, and Ctrl+A, Ctrl+E map to themselves), so Ctrl+C still stops the program and readline keeps its keys |

Modifiers are mapped at press time, so Cmd+click works although the mouse does not pass through
keycap. When a chord matches a rule, keycap changes only the modifiers that need to change.

## Manual checks

Waiting for the user and real hardware:

- Install the unit and check the mapping at the login screen.
- Cmd+C in a terminal and in a GUI app.
- `kill -9` keycap mid-chord: no stuck keys.
- `kill -STOP` keycap: the watchdog restarts it.
- Run keyd at the same time: the `Conflict` notification appears.
- The escape chord, then `Resume`.
- Bluetooth keyboard reconnect.

## Tests

`cargo test` runs the unit tables and the core scenario suite: every v1 mapping in both
profiles, Cmd held across a profile switch, unplug mid-chord, kill mid-chord then `ReleaseAll`,
the escape chord, the grab gate with a key held, Apple vs PC layout, and a property test that
every emitted press has exactly one release. Socket tests run on a private socket in a temp
directory. Scripts are synthesised in `tests/support`.
