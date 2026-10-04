# Personal configuration

Your own choices go in a TOML file. keycap reads two places, the later overriding the earlier:

1. `/etc/keycap/keycap.toml`: machine-wide, applies at the login screen too, reloaded when
   saved. On Plasma, `install.sh` puts `dist/keycap.plasma.toml` here first; it already has a
   `[profile.default]` table, so add your rules to the end of its `rules = [...]` list and put
   `[[device]]` blocks at the end of the file.
2. `~/.config/keycap/keycap.toml`, if your desktop forwards it (the Plasma focus feed does not,
   so on Plasma use the machine file).

`keycap --print-config` prints the built-in defaults to copy from. A bad value is logged and
ignored.

## Keyboard layout

Keyboards with a Mac/Windows switch (Keychron and others) look identical to the computer in both
positions, so keycap treats them as PC keyboards until you say otherwise. Find the ids with
`keycap status`, then:

```toml
# Keychron K5 Max with its switch on Mac, over USB or Bluetooth
[[device]]
match = "3434:0a53"        # vendor:product in hex, or the exact device name
layout = "apple"           # "apple" or "pc"
```

Prefer the id: the device name differs between USB and Bluetooth. If Cmd and Option behave
swapped, the layout is wrong.

## Caps Lock as the input-method switch

As on macOS: Caps Lock switches input source (fcitx5's trigger is Ctrl+Space), Shift+Caps Lock is
the real Caps Lock.

```toml
[profile.default]
rules = [
  { when = "capslock", then = "ctrl+space" },
  { when = "shift+capslock", then = "capslock" },
]
```

keycap keeps the lock off unless you set it with Shift+Caps Lock. `keycap status` shows the lamp.

## Terminals

In a terminal, Command is Ctrl+Shift, never plain Ctrl, so Cmd+D is not end-of-file. To change a
shortcut in every terminal, set it in `[profile.terminal]`, not `[profile.default]`:

```toml
[profile.terminal]
rules = [
  { when = "command+k", then = "ctrl+l" },   # Cmd+K clears the screen
]
```
