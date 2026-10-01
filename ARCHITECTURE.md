# Architecture

keycap is the Mac keyboard mapping daemon: it grabs keyboards, remaps by profile and writes one
virtual keyboard. `README.md` has the data flow, the socket protocol, the lifecycle and the
safety rules; `CONVENTIONS.md` holds the rules every repo shares; `FINDINGS.md` the open items
and standing facts. This file is the map: where each thing lives, what may depend on what, and
how to add the next thing. keycap depends on no desktop crate and runs on any Linux.

## 1. Crate map

| Crate | Path | Purpose | May depend on |
|---|---|---|---|
| `keycap-protocol` | `crates/keycap-protocol` | the socket contract: `ToDaemon`, `ToClient`, `Status`, `ProfileName`, `PROTOCOL_VERSION`, `MAX_PACKET`, `encode`, `decode`, `compat` | `serde`, `serde_json`, `thiserror` |
| `keycap` | `crates/keycap` | the pure core and the daemon; library plus the `keycap` binary | `keycap-protocol`, `serde`, `serde_json`, `thiserror`, `toml`, `notify`, `rustix` |

Edges: `keycap` -> `keycap-protocol` only. `keycap-protocol` depends on no other workspace
crate, and has no `toml`, `notify` or `rustix`, so a desktop takes it without pulling the
daemon. sill imports it as a path dependency. There is no boundary script;
`cargo tree -p keycap-protocol` showing only serde, serde_json and thiserror is the check, run
in review.

The library surface of `keycap` is the pure core. The daemon, `sys` and the CLI verbs are
`pub(crate)`; the one public entry to them is `command::run`.

## 2. Modules and layers

`crates/keycap/src`. A module imports from its own layer or a lower one only. Core modules
never name `daemon` modules.

| Layer | Modules | Nature | Visibility |
|---|---|---|---|
| L0 vocabulary | `key` (`Key`, `Modifier`, `Side`, `Mods`), `event` (`Edge`, `KeyEvent`, `Physical`, `Logical`), `time` (`Mono`) | newtypes | `pub` |
| L1 layout | `layout` (`Layout`, `role`) | pure tables | `pub` |
| L2 profiles | `profile` (`Trigger`, `Stroke`, `RuleAction`, `Profile`, `Profiles`), `config` (`ConfigLayer`, `resolve`, `lenient`, `load`, `watch`) | pure except `load` and `watch` (file reads, inotify) | `pub` |
| L3 state | `held`, `gate`, `escape`, `backoff` | pure state machines | `pub` |
| L4 remapper | `remap` (`Remapper::step`, `RemapInput`, `RemapOut`) | pure composition: `Remapper::step(self, RemapInput, &Profiles) -> (Self, Vec<RemapOut>)` | `pub` |
| L5 effects | `daemon` (`reader`, `socket`, `discover`, `uinput`, `sdnotify`, `env`, `sys`) | threads, sockets, devices, clocks, ioctls | `pub(crate)` |
| L6 entry | `command` | argument parsing, one `run(args)`; reads `std::env` once into `daemon::Env`; `--print-config`, `--check-uinput`, `replay FILE` (a pure script parser, `script.rs`) | `pub` (one fn) |

Daemon modules:

| Module | Role |
|---|---|
| `daemon/sys.rs` | the only `unsafe`: typed ioctls (`EVIOCGRAB`, `EVIOCGKEY`, `EVIOCGBIT`, `EVIOCGPROP`, `EVIOCGABS`, uinput incl. `UI_ABS_SETUP`), each with a `SAFETY` comment, and a test pinning every opcode's number |
| `daemon/uinput.rs` | the one "keycap keyboard" virtual device, opened read-write, destroyed on `Drop`; passes the pointer events of plain combo devices through and carries the lock LEDs back; the `Emitter` and `PointerSink` traits and their recording fakes |
| `daemon/twin.rs` | the "<name> (keycap pointer)" virtual device of a combo device whose pointer the keyboard cannot carry (absolute axes): registered from a `PointerCaps`, destroyed on `Drop` |
| `daemon/pointer.rs` | pure: `PointerCaps`, the pointer half of a device as a value (axes with their `absinfo`, relative axes, buttons, input properties, the twin's name), built from the kernel's masks; `needs_twin` |
| `daemon/buttons.rs` | pure: the mouse buttons each combo device holds, released on unplug, overflow and pause; the keyboard's record and each twin's are separate values |
| `daemon/leds.rs` | pure: the lock-LED events the compositor writes to the virtual device, forwarded to every grabbed keyboard, and `LedState`, the last lamps it set, written to a keyboard the moment it is grabbed |
| `daemon/readiness.rs` | which polled fds are ready, as an enum rather than flags |
| `daemon/events.rs` | `struct input_event` as bytes, `decode` (what a device sent) and `encode` (what keycap writes), pure |
| `daemon/discover/` | the /sys scan and `notify` on `/dev/input`; `classify(&Caps, &Identity, Origin) -> Eligibility` is pure and table-tested (virtual devices are never grabbed, which rules out loops with other remappers), sysfs roots injectable |
| `daemon/devices/` | every keyboard as a pure state machine: `{ identity, layout, gate, grab }` plus the conflict `Backoff`; transitions return `(Devices, Vec<DeviceEffect>)`, `drive` carries the effects out through `Nodes`, `status_of` makes the `Status` a desktop sees |
| `daemon/grabber.rs` | `Grabber` (grab, ungrab) and `Nodes` (open, close, keys down): the real evdev fds, and `FakeGrabber` in tests |
| `daemon/reader/` | the main thread: one `poll` over the nodes and the inbox's eventfd, timeout from the next gate or backoff deadline and half the watchdog; the reader waits on nothing but `poll`. `grabs.rs` acts on a device just grabbed or let go (LEDs, twin), `twins.rs` holds the twins and the pure `grab_change`, `unknown.rs` the pure `Unknowns` set behind the once-per-generation log of an undefined focused profile |
| `daemon/route.rs`, `daemon/evdev.rs` | one read of a node (drained until `EAGAIN`) to `RemapInput`s and pointer frames, pure `route` over the drained events |
| `daemon/inbox.rs`, `daemon/waker.rs` | what other threads send the reader (`Request`, system config, hotplug) over one channel and one eventfd |
| `daemon/socket/` | the socket thread; `seat.rs` holds the pure parts (`active_uid`, `peer_of` to `Peer { SeatOwner, Observer }`, `request_of`, `Seat::look` for owner leaving or user switch); `limits.rs` the bounds against hostile clients (32 clients, the seat owner exempt from that cap; 8 per uid; 16 messages per client per pass; listener paused 1 s on EMFILE; complaints logged once a minute per uid); `inbound.rs` the bounded read |
| `daemon/settings.rs` | the config layers in force (built-in, system, user) and their resolution |
| `daemon/service.rs` | `run(env)`: the virtual keyboard, socket thread, watchers, then the reader |
| `daemon/sdnotify.rs` | `READY=1`, `WATCHDOG=1` and `STATUS=` to `$NOTIFY_SOCKET` |
| `daemon/env.rs` | `Env::from_lookup`: environment, directories, the seat file |
| `daemon/clock.rs` | the one `CLOCK_MONOTONIC` read |

`lib.rs` holds declarations and root re-exports only.

## 3. One home

| Concept | Home |
|---|---|
| Evdev key codes and names | `key::Key` (the name table) |
| Pressed key vs sent key | `event::{Physical, Logical}` |
| Modifier positions on Apple and PC keyboards | `layout` |
| What a chord becomes | `profile::{Trigger, Stroke, RuleAction}` |
| Profile inheritance and lookup | `profile::Profiles` |
| Every default mapping | `defaults.toml` |
| Lenient per-key reading, config layers, inheritance flattening | `config::{lenient, ConfigLayer, resolve}` |
| What each press emitted, and which written keys are lifted | `held::Held` |
| The pointer half of a combo device | `daemon::pointer::PointerCaps` |
| Which lamps the desktop last set | `daemon::leds::LedState` |
| Which undefined profile names were already logged | `daemon::reader::unknown::Unknowns` |
| When a keyboard may be grabbed | `gate::GrabGate` |
| The emergency chord | `escape` |
| The mapping state machine | `remap::Remapper` |
| Socket messages | `keycap-protocol` |
| Threads, sockets, devices, clocks | `daemon` |
| ioctls and all `unsafe` | `daemon::sys` |
| Environment, clock, directories | `daemon::Env`, built once in `command::run` |
| Command-line verbs | `command` |

## 4. Traits

| Trait | Signature | Implementors |
|---|---|---|
| `Grabber` (`daemon::grabber`) | grab and ungrab one device, returning a typed error | the real evdev grab, a fake in tests |
| `Nodes: Grabber` (`daemon::grabber`) | open, close and read the keys down of a device by id | the open evdev fds, a fake in tests |
| `Emitter` (`daemon::uinput`) | `write` one batch of events to the virtual keyboard, `recreate` it after a failed write, expose `led_fd` / `leds` for LED forwarding, and `twin` a `PointerSink` for a combo device | the uinput device, a recording sink in tests |
| `PointerSink` (`daemon::uinput`) | `write` one batch to a pointer twin; the device is destroyed when dropped | the uinput `PointerTwin`, a recording twin in tests |
| `Setting` (`config::lenient`) | `fn read(v: &toml::Value, key: &str, warn: &mut Vec<Warning>) -> Option<Self>` | every unit newtype and enum |

A trait appears only when two real implementations are swapped at a boundary. **Closed enums
that stay enums:** `RemapInput`, `RemapOut`, `RuleAction`, `Edge`, `Layout`, `ToDaemon`, `ToClient`,
`Peer`, `Eligibility`, `Mode`, `Grab`.

## 5. Recipes

### Add a mapping

1. `defaults.toml`: a `[[profile.NAME.rules]]` entry with `when` and `then` chords, in the
   profile it belongs to. Keys are names from the table in `key`, never numbers.
2. A scenario test beside the others in `tests/`: a synthesised script of key events, asserting
   the `RemapOut`s, including the release order and that nothing is left held.
3. The README mapping table gains a row.

### Add a profile

1. `defaults.toml`: a `[profile.NAME]` table with `inherits = "default"` (or another profile)
   and its rules. A name is `[a-z0-9_-]+`.
2. A scenario test that focuses the profile and drives its rules, and one that switches into
   and out of it with a modifier held.
3. The desktop must send `Focus { profile: NAME }`; keycap needs no change for that.
4. The README mapping table gains a row.

### Add a socket message

1. `keycap-protocol`: the variant in `message.rs`, its vocabulary in `vocab.rs`, the round-trip
   test; bump `PROTOCOL_VERSION` when incompatible.
2. `daemon/socket/seat.rs`: the arm of `request_of`, with its `Peer` check, and the `Request` it becomes in `daemon/inbox.rs`; the reader's `request` handles it.
3. The core input or output it maps to, and the desktop, in the same change.

## 6. Test harness

| Kind of thing | Test |
|---|---|
| Key names, layout, classify, opcodes, protocol | table tests beside the code |
| Whole-core behaviour | scripts of `RemapInput`s through `Remapper::step`, asserting `RemapOut`s, in `tests/` |
| Stuck keys | a property test: every emitted press has exactly one release |
| Socket | a private socket in a temp dir and a fake seat file, in `daemon/socket/tests.rs` |
| Grab conflicts, pause and resume | `daemon/devices` tests: the state machine driven through `FakeGrabber` |
| The reader's decisions | `daemon/reader/tests.rs` and `pointer_tests.rs`: a recording `Emitter` whose twins log what they are told and whether they were dropped, a private socket, no node opened |
| Replay | `command/script.rs` table tests and `command/replay.rs` on a script in a temp dir |

No test creates an input device, opens a real node for writing, or reads a real config
directory or a running daemon.

## 7. Repo rules

- **Only `daemon` owns threads, sockets, devices, clocks and the environment.** When the core
  seems to need an effect, it returns an `RemapOut` for the daemon to carry out.
- **`unsafe` lives only in `daemon/sys.rs`**, each block with a `SAFETY` comment.
- **The layering rule:** core modules never name daemon modules.
- **Units and ids are newtypes:** `Key`, `Physical`, `Logical`, `ProfileName`, `Mono`. `Key` is
  the one bare generic root export, kept deliberately: it is the evdev code vocabulary and every
  other name here is built on it. Every other root export carries its concept in its name
  (`RemapInput`, `RemapOut`, `ConfigSource`, `ConfigLayer`, `RuleAction`).
- **No `bool` in a field, parameter or key.** A choice is an enum.
- **Release is exact:** a key's release sends what its press sent (`Held`), and `drain` empties
  it; a stuck modifier cannot be produced.
- **`panic = "abort"` in release only**, so a panic ends the process and the kernel drops the
  grab.
- **The protocol is a crate:** a change to a message is a change to `keycap-protocol`, its
  tests, `PROTOCOL_VERSION` when incompatible, and the desktop, in one change.
- **Tests never create an input device** (README, "Safety rules").
- **Gate** (no boundary script):

  ```bash
  cargo fmt --all --check
  cargo clippy --workspace --all-targets --all-features -- -D warnings
  cargo test --workspace --all-features
  cargo deny check licenses
  ```

## 8. The core vocabulary and its semantics

The contract every core module builds on. Input and output speak different vocabularies, and
the types keep them apart.

| Side | Type | Values |
|---|---|---|
| In | `Physical(Key)` | the evdev code the keyboard sent |
| In | `Role` | `Command`, `Option`, `Control`, `Shift`: what a modifier key *means*, by position |
| In | `Roles` | bitset of held roles |
| Out | `Logical(Key)` | the evdev code keycap writes |
| Out | `OutMod` | `Ctrl`, `Alt`, `Super`, `Shift`: written as the left-hand key |
| Out | `OutMods` | bitset of written modifiers |
| Both | `Key(u16)` | an evdev code, constructed only from the name table |
| Both | `DeviceId(u32)`, `Mono(u64)` ns, `Edge { Press, Release, Repeat }` | |

`layout::role(Layout, Physical) -> Option<Role>` works by position: the key beside the space
bar is `Command` on both layouts (Apple: Meta; PC: Alt), the next one out is `Option` (Apple:
Alt; PC: Meta), Control is `Control`, Shift is `Shift`. `Layout` comes from the device's vendor.

### Profiles

```rust
pub struct ModMap { pub command: OutMod, pub option: OutMod, pub control: OutMod } // Shift -> Shift
pub struct Trigger { pub roles: Roles, pub key: Key }        // exact match on held roles
pub struct Stroke { pub mods: OutMods, pub key: Key }
pub enum Hold { Restore, WhileHeld }
pub enum RuleAction {
    Map(Stroke, Hold),       // held while the physical key is held (so it repeats)
    Sequence(Vec<Stroke>),   // tapped in order on press; the release sends nothing
    Swallow,
}
pub struct Profile { pub modmap: ModMap, pub rules: BTreeMap<Trigger, RuleAction> }
pub struct Profiles { default: Profile, named: BTreeMap<ProfileName, Profile> }
```

Inheritance is resolved at load: a `Profile` is always flat, and `Profiles::resolve(&name)`
returns `default` for an unknown name (the daemon logs `profile "<name>" is not defined; using default` once per name until the config changes). `defaults.toml` is compiled
in with `include_str!`; `/etc/keycap/keycap.toml` and the seat owner's `UserConfig` have the
same shape and override per profile, per modmap field and per trigger.

```toml
[profile.default]
modmap = { command = "ctrl", option = "alt", control = "super" }
rules = [
  { when = "command+left", then = "home" },
  { when = "command+tab", then = "alt+tab", hold = "while_held" },
  { when = "command+backspace", then = ["shift+home", "delete"] },
]

[profile.terminal]
inherits = "default"
modmap = { control = "ctrl" }
rules = [{ when = "command+c", then = "ctrl+shift+c" }]
```

`when` uses role names (`command`, `option`, `control`, `shift`), `then` uses output names
(`ctrl`, `alt`, `super`, `shift`); the parser rejects the other vocabulary on each side. A string
`then` is `Map`, a list is `Sequence`; `hold` defaults to `restore`.
`then = []` is `Swallow`: the chord is consumed and nothing is written, which is also how a
profile removes a rule it inherits. `hold` belongs to a single stroke; on a list it is reported
and ignored. Shift is the one name both vocabularies share.

Config is read in layers. `ConfigLayer::parse(text, ConfigSource)` turns one file into a `ConfigLayer` (per profile:
optional `inherits`, the modmap fields it sets, its rules by trigger) and warnings keyed by dotted
path (`profile.terminal.rules[2].when`), each problem prefixed with the `ConfigSource` (`Builtin`,
`System`, `User`). A bad value costs its own key, or its own rule for a bad `when` or `then`;
unknown keys are ignored; text that is not TOML is an empty layer and one warning (key `file`).
Within one file a trigger given twice keeps the later rule.

`config::resolve(&[ConfigLayer]) -> (Profiles, Vec<Warning>)` folds the layers (later over earlier, per
profile, per modmap field, per trigger), then flattens inheritance, child over parent,
transitively. A profile with no `inherits` inherits `default`; `default` inherits nothing. An
unknown parent, or a parent chain that leads back to the profile, is a warning and the profile
inherits `default` instead. The base under `default` is `ModMap::IDENTITY` (each role writes the
modifier it is named after) and no rules, so `resolve(&[])` still yields a `default`.
`Profiles::new` is `pub(crate)`: only resolution builds one. `Ord` on `Trigger` and `Stroke`
(and on `ProfileName`, in `keycap-protocol`) is for `BTreeMap` keys and carries no other meaning.


### `Remapper::step`

```rust
pub enum RemapInput {
    Key { device: DeviceId, layout: Layout, key: Physical, edge: Edge, at: Mono },
    Focus(ProfileName),
    DeviceGone(DeviceId),
    ReleaseAll,
    Resume,
}
pub enum RemapOut { Write(Vec<OutEvent>), ReleaseGrabs, Status(CoreStatus) }
```

- **Modifiers are written eagerly.** A role press writes its `ModMap` output at once, so
  Cmd+click reaches the pointer's client as Ctrl+click. The remapper keeps a
  `(DeviceId, Physical) -> Role` map beside `Held`; the held roles are read through `Held`'s
  records, so a role key `Held` has let go is no longer a role.
- **An unmatched key** is written as itself; the modifiers already down apply.
- **Lifts.** A role's modifier belongs to the role key's record in `Held` and stays there while
  a stroke needs it off the wire. `Held::lift(owner, keys)` takes keys some record holds off
  the wire (a release is written if they were down) and remembers `owner` as the one who lifted
  them; the records keep them. A key is down on the wire exactly while some record holds it and
  no owner has lifted it. The owner's release (`Held::release`, or `unlift`) presses back each
  key that is still held and that no other owner keeps lifted. Releasing a record whose key is
  lifted writes nothing for that key, and the lift forgets it. A deliberate press of a lifted
  key (`press`, `extend`) writes it and ends every lift of it, so the held and written views
  cannot disagree. `drain` releases only what is down and presses nothing back; `device_gone`
  releases the device's records first, then ends its lifts, so only keys another device holds
  come back.
- **A matched `Map`** writes the minimum change. With `Restore` the physical key owns the
  lift: it lifts every written modifier the stroke does not want, presses the wanted ones that
  are not down, and presses the key, all in one record at the physical key. On the physical
  release the record goes in reverse order and then the lift ends, so the modifiers come back as
  the held roles say. A role key released meanwhile just leaves its record; no write, and the
  lift forgets the key.
- **`WhileHeld` ownership.** Cmd+Tab must keep Alt down after Tab is released, until Command
  is. So the lift, and the stroke's modifiers that were not already down (`Held::extend`), belong
  to the trigger's non-Shift role key, the lowest `(device, key)` if several, and only the
  stroke's own key belongs to the physical key. Tab release writes only the Tab release; a
  second Tab matches again with Alt already down. Command's release ends Alt (it is in Command's
  record) and does not press Control back, since Command's own record is gone. A trigger with
  no non-Shift role to own the stroke behaves as `Restore`.
- **A matched `Sequence`** taps each stroke in order with the minimum-change rule: lift what
  the stroke does not want, press its modifiers and key, release them. The lifts are kept until
  the last stroke, then ended. All strokes go out in one `Write` with one `Sync` at the end.
  Nothing is left in `Held` for the key, so its release writes nothing.
- **`Swallow`** records nothing and writes nothing; the release writes nothing either.
- **`Held`** records, per `(DeviceId, Physical)`, exactly what the press wrote, and a release
  writes the inverse of that record and nothing else (plus the restore of its lifts). `DeviceGone`
  releases that device's records; `ReleaseAll` releases everything. A key never written is
  never released, so the release of a key pressed before the grab writes nothing.
- **`Focus`** takes effect at the next press; held keys keep what they wrote.
- **Repeat** events are dropped: the compositor repeats.
- **The escape chord** (both Shift keys + Esc, physical) releases everything and yields
  `RemapOut::ReleaseGrabs` and `CoreStatus::Paused`, in that order after the one draining `Write`; the
  Esc press is not written. Keys are ignored until `Resume`, which yields `Status(Running)`.
  The detector tracks the three chord keys per `(DeviceId, Physical)`. The grabs are given back
  at the chord, so the releases that follow never arrive: `Resume` and `ReleaseAll` start it
  over empty, and `DeviceGone(d)` forgets that keyboard's keys, so a lone Esc after a resume
  is an ordinary key and a Shift on an unplugged keyboard arms nothing.
- **Bare Alt and Super.** A compositor reads Alt or Super released with no key between as a tap
  (menu bar, launcher). `Held` tracks the written Alt and Super keys that have had no other key
  written since their press. A lift of such a key, and the release of one a lift put back, is
  led by a tap of `F24` (press and release in the same write, never recorded in `Held`), so the
  desktop sees a key between. A person's own tap with no lift is left alone, and a Control
  written for Command needs none: a Ctrl tap does nothing.
- Every `Write` ends with exactly one `SYN_REPORT` (`OutEvent::Sync`), and a step that writes
  nothing emits no `Write`.
