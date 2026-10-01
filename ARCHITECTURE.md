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
| L2 profiles | `profile` (`Trigger`, `Stroke`, `Action`, `Profile`, `Profiles`), `config` (`Layer`, `resolve`, `lenient`, `load`, `watch`) | pure except `load` and `watch` (file reads, inotify) | `pub` |
| L3 state | `held`, `gate`, `escape`, `backoff` | pure state machines | `pub` |
| L4 remapper | `remap` (`Remapper::step`, `Input`, `Out`) | pure composition: `Remapper::step(self, Input, &Profiles) -> (Self, Vec<Out>)` | `pub` |
| L5 effects | `daemon` (`reader`, `socket`, `discover`, `uinput`, `sdnotify`, `env`, `sys`) | threads, sockets, devices, clocks, ioctls | `pub(crate)` |
| L6 entry | `command` | argument parsing, one `run(args)`; reads `std::env` once into `daemon::Env`; `replay FILE` | `pub` (one fn) |

Daemon modules:

| Module | Role |
|---|---|
| `daemon/sys.rs` | the only `unsafe`: typed ioctls (`EVIOCGRAB`, `EVIOCGBIT`, `EVIOCGKEY`, `EVIOCGID`, uinput), each with a `SAFETY` comment, and a test pinning every opcode's number |
| `daemon/uinput.rs` | the one "keycap keyboard" virtual device, destroyed on `Drop`; passes pointer events through for combo devices |
| `daemon/discover.rs` | the /sys scan and `notify` on `/dev/input`; `classify(&Caps) -> Eligibility` is pure and table-tested, sysfs roots injectable |
| `daemon/reader.rs` | one `poll` loop over the grabbed fds, an eventfd and the timeout; the reader waits on nothing but `poll` |
| `daemon/socket.rs` | the socket thread; `SO_PEERCRED` to `Peer { SeatOwner, Observer }` |
| `daemon/sdnotify.rs` | `READY=1` and `WATCHDOG=1` to `$NOTIFY_SOCKET` |
| `daemon/env.rs` | `Env::from_lookup`: environment, clock, directories |

`lib.rs` holds declarations and root re-exports only.

## 3. One home

| Concept | Home |
|---|---|
| Evdev key codes and names | `key::Key` (the name table) |
| Pressed key vs sent key | `event::{Physical, Logical}` |
| Modifier positions on Apple and PC keyboards | `layout` |
| What a chord becomes | `profile::{Trigger, Stroke, Action}` |
| Profile inheritance and lookup | `profile::Profiles` |
| Every default mapping | `defaults.toml` |
| Lenient per-key reading, config layers, inheritance flattening | `config::{lenient, Layer, resolve}` |
| What each press emitted | `held::Held` |
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
| `Grabber` (`daemon::discover`) | grab and ungrab one device, returning a typed error | the real evdev grab, a fake in tests |
| `Setting` (`config::lenient`) | `fn read(v: &toml::Value, key: &str, warn: &mut Vec<Warning>) -> Option<Self>` | every unit newtype and enum |

A trait appears only when two real implementations are swapped at a boundary. **Closed enums
that stay enums:** `Input`, `Out`, `Action`, `Edge`, `Layout`, `ToDaemon`, `ToClient`,
`Peer`, `Eligibility`, `Mode`, `Grab`.

## 5. Recipes

### Add a mapping

1. `defaults.toml`: a `[[profile.NAME.rules]]` entry with `when` and `then` chords, in the
   profile it belongs to. Keys are names from the table in `key`, never numbers.
2. A scenario test beside the others in `tests/`: a synthesised script of key events, asserting
   the `Out`s, including the release order and that nothing is left held.
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
2. `daemon/socket.rs`: the arm, with its `Peer` check.
3. The core input or output it maps to, and the desktop, in the same change.

## 6. Test harness

| Kind of thing | Test |
|---|---|
| Key names, layout, classify, opcodes, protocol | table tests beside the code |
| Whole-core behaviour | scripts of `Input`s through `Remapper::step`, asserting `Out`s, in `tests/` |
| Stuck keys | a property test: every emitted press has exactly one release |
| Socket | a private socket in a temp dir, beside `daemon/socket.rs` |
| Grab conflicts | `discover` and `gate` tests with a fake `Grabber` |

No test creates an input device, opens a real node for writing, or reads a real config
directory or a running daemon.

## 7. Repo rules

- **Only `daemon` owns threads, sockets, devices, clocks and the environment.** When the core
  seems to need an effect, it returns an `Out` for the daemon to carry out.
- **`unsafe` lives only in `daemon/sys.rs`**, each block with a `SAFETY` comment.
- **The layering rule:** core modules never name daemon modules.
- **Units and ids are newtypes:** `Key`, `Physical`, `Logical`, `ProfileName`, `Mono`.
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
pub enum Action {
    Map(Stroke, Hold),       // held while the physical key is held (so it repeats)
    Sequence(Vec<Stroke>),   // tapped in order on press; the release sends nothing
    Swallow,
}
pub struct Profile { pub modmap: ModMap, pub rules: BTreeMap<Trigger, Action> }
pub struct Profiles { default: Profile, named: BTreeMap<ProfileName, Profile> }
```

Inheritance is resolved at load: a `Profile` is always flat, and `Profiles::resolve(&name)`
returns `default` for an unknown name (the daemon logs it once). `defaults.toml` is compiled
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

Config is read in layers. `Layer::parse(text, Source)` turns one file into a `Layer` (per profile:
optional `inherits`, the modmap fields it sets, its rules by trigger) and warnings keyed by dotted
path (`profile.terminal.rules[2].when`), each problem prefixed with the `Source` (`Builtin`,
`System`, `User`). A bad value costs its own key, or its own rule for a bad `when` or `then`;
unknown keys are ignored; text that is not TOML is an empty layer and one warning (key `file`).
Within one file a trigger given twice keeps the later rule.

`config::resolve(&[Layer]) -> (Profiles, Vec<Warning>)` folds the layers (later over earlier, per
profile, per modmap field, per trigger), then flattens inheritance, child over parent,
transitively. A profile with no `inherits` inherits `default`; `default` inherits nothing. An
unknown parent, or a parent chain that leads back to the profile, is a warning and the profile
inherits `default` instead. The base under `default` is `ModMap::IDENTITY` (each role writes the
modifier it is named after) and no rules, so `resolve(&[])` still yields a `default`.
`Profiles::new` is `pub(crate)`: only resolution builds one. `Ord` on `Trigger` and `Stroke`
(and on `ProfileName`, in `keycap-protocol`) is for `BTreeMap` keys and carries no other meaning.


### `Remapper::step`

```rust
pub enum Input {
    Key { device: DeviceId, layout: Layout, key: Physical, edge: Edge, at: Mono },
    Focus(ProfileName),
    DeviceGone(DeviceId),
    ReleaseAll,
    Resume,
}
pub enum Out { Write(Vec<OutEvent>), ReleaseGrabs, Status(CoreStatus) }
```

- **Modifiers are written eagerly.** A role press writes its `ModMap` output at once, so
  Cmd+click reaches the pointer's client as Ctrl+click.
- **An unmatched key** is written as itself; the modifiers already down apply.
- **A matched `Map`** writes the minimum change: release written modifiers the stroke does not
  want, press the ones it wants and lacks, press the key. On the physical release: release the
  key, then (`Restore`) put the modifiers back as the held roles say. With `WhileHeld` the
  stroke's modifiers stay down until the last role of the trigger is released (Cmd+Tab keeps
  the switcher open while Command is held).
- **A matched `Sequence`** taps each stroke with the same minimum-change rule, then restores.
- **`Held`** records, per `(DeviceId, Physical)`, exactly what the press wrote. A release writes
  the inverse of that record and nothing else. `DeviceGone` releases that device's records;
  `ReleaseAll` releases everything. A key never written is never released.
- **`Focus`** takes effect at the next press; held keys keep what they wrote.
- **Repeat** events are dropped: the compositor repeats.
- **The escape chord** (both Shift keys + Esc, physical) releases everything and yields
  `Out::ReleaseGrabs` and `CoreStatus::Paused`; keys are ignored until `Resume`.
- Every `Write` ends with `SYN_REPORT`.
