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

`install.sh` builds a release binary and installs it with sudo, along with a systemd unit and
the udev and polkit rules it needs. Running it again changes only what differs. Requires a Rust
toolchain; the Plasma focus feed also needs `python3-gobject`. To switch from Toshy, see
[docs/switch-from-toshy.md](docs/switch-from-toshy.md).

## Usage

```
keycap status         what keycap is doing; exits 0 only if a keyboard is grabbed
keycap resume         remap the keyboards again after the emergency chord
keycap --print-config print the built-in defaults, to copy from
keycap replay FILE    try a key script against the mapping without touching any device
```

**Emergency chord:** hold both Shift keys and press Esc. keycap gives every keyboard back to the
kernel immediately; `keycap resume` takes them again. `sudo systemctl stop keycap` does the same
until the next start.

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
| Cmd+Tab, Cmd+` | Alt+Tab, Alt+` (switch apps, windows of one app) |
| Cmd+Q | Alt+F4 |
| Ctrl+A / Ctrl+E | Home / End |

Profiles change this per app:

| Profile | For | Differences |
| --- | --- | --- |
| `terminal` | terminal emulators | Cmd+letter becomes Ctrl+Shift+letter, so Cmd+D is never end-of-file and Ctrl+C still interrupts; Cmd+Backspace kills to line start |
| `ghostty` | Ghostty | `terminal`, plus Ghostty's split, clear and inspector keys |
| `filemanager` | file managers | Cmd+Backspace trashes the selection; Cmd+arrows navigate |

On Plasma, `dist/keycap.plasma.toml` adds Plasma's equivalents of macOS shortcuts: Cmd+Space opens
KRunner, Cmd+Ctrl+Space the emoji picker, Cmd+H minimises, Cmd+Shift+3/4/5 take screenshots, and
Ctrl+Left/Right switch desktops. `keycap --print-config` lists every rule.

## Configuration

Settings are read in three layers, each overriding the one before:

1. The built-in defaults.
2. `/etc/keycap/keycap.toml`, for the whole machine; changes apply as soon as you save.
3. `~/.config/keycap/keycap.toml`, if your desktop forwards it.

A bad value is logged and ignored; the rest of the file still applies. See
[docs/personal-config.md](docs/personal-config.md) for common settings: telling keycap a
keyboard's layout, Caps Lock as the input-method switch, and terminal rules.

**Keyboard layout.** keycap treats Apple keyboards as Apple and everything else as PC. Keyboards
with a Mac/Windows switch look the same to the computer in both positions, so declare them in
your config with a `[[device]]` entry.

### Per-app profiles

The desktop tells keycap which profile to use for the focused window. Define your own with
`[profile.NAME]` and `inherits = "terminal"` (or any other profile). A profile nobody defined
falls back to `default`.

On Plasma, `--plasma-focus` installs a small KWin script and the `keycap-focus` helper that send
the focused window's class to keycap: `ghostty`, `terminal`, `filemanager` or `default`. The
class lists are at the top of `dist/plasma/keycap-focus`. A terminal embedded in another window
(an editor panel) gets that window's profile.

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
