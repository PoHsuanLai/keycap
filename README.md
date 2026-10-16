# keycap

Mac keyboard shortcuts on Linux. keycap is a system service that remaps your keyboards so that
Command acts as Control, Option as Alt, Cmd+Left is Home, and so on. It starts at boot, so the
mapping also works at the login screen and on a text console, and it follows the focused window
so terminals get terminal-safe shortcuts. It works on any Linux with systemd.

## Before you start

- **Don't change your apps' own shortcuts.** keycap translates Mac shortcuts into the Linux ones
  your apps already have. Rebinding them in the apps as well will break the mapping.
- **Know the emergency chord.** keycap takes over your keyboards. If keys misbehave, hold both
  Shift keys and press Esc to get them back.
- **Run one remapper at a time.** Stop keyd, Toshy or similar before switching to keycap.
- **On Plasma, install the focus feed** (`--plasma-focus`). Without it keycap can't tell when a
  terminal has focus, and Cmd+C in a terminal sends Ctrl+C, which interrupts the running program.
- **Mac/Windows-switch keyboards** (Keychron and others) need a `[[device]]` entry, or Cmd and
  Option come out swapped.

## Install

```bash
./install.sh --dry-run          # print every command, change nothing
./install.sh                    # build, install, enable and start
./install.sh --desktop plasma --plasma-focus    # KDE Plasma: shortcuts and per-app profiles
./uninstall.sh [--purge] [--plasma-focus]
```

`install.sh` builds a release binary and installs it as root (through `$SUDO`, sudo, doas or
pkexec; with none it prints the commands), along with a systemd unit, the udev and polkit rules it
needs, and `modules-load.d/keycap.conf` so `uinput` loads at boot. The unit has no access to `/dev/uinput`
of its own on the desktop, where a root service hands it the virtual keyboards; `install.sh` is the standalone
install and adds the one line (`keycap.service.d/10-uinput.conf`) and the udev rule that let the unit open the node. `--prefix P` installs under P
(default `/usr/local`; any other prefix uses the vendor directories under it) and `--destdir D`
stages everything under D without root, for packages. Running it again changes only what differs. Requires a Rust
toolchain; the Plasma focus feed also needs `python3-gobject`. To switch from Toshy, see
[docs/switch-from-toshy.md](docs/switch-from-toshy.md).

## Usage

```
keycap status         what keycap is doing; exits 0 only if a keyboard is grabbed
keycap resume         remap the keyboards again after the emergency chord (only the desktop's shell is obeyed)
keycap --print-config print the built-in defaults, to copy from
keycap replay FILE    try a key script against the mapping without touching any device
```

**Emergency chord:** hold both Shift keys and press Esc. keycap gives every keyboard back to the
kernel immediately; the desktop's shell takes them again, and so does `sudo systemctl restart keycap`
(`keycap resume` from a terminal is refused: keycap obeys only the shell). `sudo systemctl stop keycap`
gives them back until the next start.

Only one program can hold a keyboard at a time. If another remapper (keyd, Toshy, ...) has it,
`keycap status` says `held by another remapper` and keycap waits.

## Default mapping

| Input | Output |
| --- | --- |
| Command, Option, Control | Control, Alt, Super |
| Cmd+Left / Right | Home / End |
| Cmd+Up / Down | Ctrl+Home / Ctrl+End |
| Option+Left / Right | Ctrl+Left / Ctrl+Right |
| Option+Backspace | Ctrl+Backspace |
| Cmd+Backspace | Delete to line start |
| Cmd+Tab, Cmd+` | Alt+Tab, Alt+` (switch apps, windows of one app); the Alt stays down while Cmd is held, so repeated Tab and Shift+Tab walk the switcher |
| Cmd+Q | Alt+F4 |
| Ctrl+A / Ctrl+E | Home / End |

Profiles change this per app:

| Profile | For | Differences |
| --- | --- | --- |
| `terminal` | terminal emulators | Cmd+letter becomes Ctrl+Shift+letter, so Cmd+D is never end-of-file and Ctrl+C still interrupts; Cmd+Backspace kills to line start |
| `ghostty` | Ghostty | `terminal`, plus Ghostty's split, clear and inspector keys |
| `keymap-aware` | apps that read the keymap themselves (temor) | Cmd stays Super, so Cmd+D and Cmd+Shift+D reach the app apart; only the desktop's own chords (Cmd+Tab, Cmd+Space, Cmd+`, Cmd+Q) are rewritten |
| `filemanager` | file managers | Cmd+Backspace trashes the selection; Cmd+arrows navigate |

On Plasma, `dist/keycap.plasma.toml` adds Plasma's equivalents of macOS shortcuts: Cmd+Space opens
KRunner, Cmd+Ctrl+Space the emoji picker, Cmd+H minimises, Cmd+Shift+3/4/5 take screenshots, and
Ctrl+Left/Right switch desktops. `keycap --print-config` lists every rule.

Those Plasma rules are the machine file, which every session on the machine shares, so they must
not be the whole story on a desktop that is not Plasma. What a few chords do:

| Chord | Built-in mapping | Plasma (`/etc/keycap/keycap.toml`) | sill session (its layer on top) |
| --- | --- | --- | --- |
| Cmd+Space | Super+Space | Alt+Space (KRunner) | Super+Space (sill's launcher) |
| Cmd+H | Ctrl+H | Super+PageDown (minimise) | Ctrl+H (nothing minimises) |
| Cmd+Shift+3 / 4 / 5 | Ctrl+Shift+3 / 4 / 5 | Shift+Print / Alt+Print / Print (Spectacle) | Print (sill's screenshot flow) |

The sill column is the layer sill ships in its `dist/keycap/keycap.toml`; see "Desktops that
forward a config" below.

## Configuration

Settings are read in three layers, each overriding the one before:

1. The built-in defaults.
2. `/etc/keycap/keycap.toml`, for the whole machine; changes apply as soon as you save. **It is
   shared by every session on the machine**, the login screen included.
3. `~/.config/keycap/keycap.toml`, if your desktop forwards it. This layer is per user and per
   session: only the desktop you are logged into reads the file and sends it to keycap, and keycap
   drops it again when that session ends.

A bad value is logged and ignored; the rest of the file still applies. See
[docs/personal-config.md](docs/personal-config.md) for common settings: telling keycap a
keyboard's layout, Caps Lock as the input-method switch, and terminal rules.

**Keyboard layout.** keycap treats Apple keyboards as Apple and everything else as PC. Keyboards
with a Mac/Windows switch look the same to the computer in both positions, so declare them in
your config with a `[[device]]` entry.

### Desktops that forward a config

On Plasma, `install.sh` puts `dist/keycap.plasma.toml` in `/etc/keycap/keycap.toml`. That file
sends Cmd+Space to Alt+Space (KRunner), Cmd+H to a Plasma minimise key and Cmd+Shift+3/4 to
Spectacle's Print variants, none of which another desktop binds. Editing the machine file for a
different desktop would break Plasma, because both desktops share it.

A desktop that forwards `~/.config/keycap/keycap.toml` can instead override those rules for its
own sessions only. sill does this: it reads the seat owner's file and sends it to keycap as the
user layer, and Plasma never does. The layer wins over the machine file, so a sill session
gets Cmd+Space as Super+Space, Cmd+Shift+3/4/5 as Print and Cmd+H as plain Ctrl+H, while a Plasma
session on the same machine keeps the Plasma rules. sill ships that layer in its
`dist/keycap/keycap.toml` and its installer copies it to `~/.config/keycap/keycap.toml` when you
have none. keycap cuts that layer down when it arrives: a rule on a key with no modifier held (Caps Lock alone, say), and
a rule that types several keys or none, belong in the machine file; so do more than 256 rules. The machine file can
let the second kind through for the user layer with `[user_layer]` / `sequences = "allow"`. `replay` takes one file, so to see both layers at once, copy the rules of the user file
after those of the machine file in a scratch file (later rules win). To check without touching a
keyboard:

```bash
keycap replay script.txt                                  # built-in defaults
keycap replay --config /etc/keycap/keycap.toml script.txt # a file laid over them
```

`install.sh` prints a reminder after its machine-config step when your desktop is not KDE and
`/etc/keycap/keycap.toml` is the Plasma overlay.

### Per-app profiles

The desktop tells keycap which profile to use for the focused window. Define your own with
`[profile.NAME]` and `inherits = "terminal"` (or any other profile). A profile nobody defined
falls back to `default`.

On Plasma, `--plasma-focus` installs a small KWin script and the `keycap-focus` helper that send
the focused window's class to keycap: `ghostty`, `keymap-aware`, `terminal`, `filemanager` or `default`. The
class lists are at the top of `dist/plasma/keycap-focus`. A terminal embedded in another window
(an editor panel) gets that window's profile.

## Keymap

Apps resolve their shortcuts through [chordkit](https://github.com/PoHsuanLai/chordkit) and never
check Command or Control themselves. keycap is this desktop's source for it. Your own changes
live in `~/.config/keycap/keymap` (or `$XDG_CONFIG_HOME/keycap/keymap`), one `action = shortcut`
per line (`copy = Primary+Alt+C`, `redo = none`; `#` starts a comment); Settings writes it and
no privilege is needed. An administrator may add machine-wide changes in `/etc/keycap/keymap`;
yours win. keycap watches the machine file, keeps its last good copy if an edit is broken, and
tells running apps when anything changes. A line it cannot read is skipped and reported; a chord
the desktop keeps for itself (Command+Space, Ctrl+1 to Ctrl+9) is refused with the reason.

`keycap-client` is what a program links: `KeycapSource::standard()` is a `chordkit::KeymapSource`
that works with or without the daemon.

## Safety

- keycap grabs your keyboard, but the kernel releases it if keycap crashes or is stopped, and
  systemd restarts it within a moment.
- keycap never records keystrokes.
- `install.sh` is the only thing that changes system files, and `--dry-run` shows exactly what.

## Roadmap

keycap is early software. What is not there yet:

- **Per-app profiles beyond terminals and file managers.** Browsers, editors and other apps
  currently get the `default` profile. You can add your own profiles today; shipped ones are
  planned.
- **Terminals inside other windows.** A terminal panel in an editor gets the editor's profile,
  because the desktop only reports the focused window.
- **Other desktops' focus feeds.** Only Plasma has a ready-made one. Any desktop can drive
  keycap with one socket message, `Focus { profile }`.
- **Hardware verification.** The mapping is tested end to end in CI with virtual keyboards. Real
  Apple keyboards, Bluetooth reconnects, receivers with a built-in touchpad and user switching
  under logind still need testing on real hardware.

## Development

See [docs/development.md](docs/development.md) for the architecture, the socket protocol and how
to run the tests.
