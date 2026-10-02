# Personal configuration

Nothing shipped in keycap is specific to one person or one machine. Your own choices go in a
config file; keycap reads the same TOML in two places, later over earlier:

1. `/etc/keycap/keycap.toml` (machine-wide, applies at the login screen too, re-read when the
   file changes; `sudo install -Dm644 file /etc/keycap/keycap.toml`). On Plasma, `install.sh`
   puts `dist/keycap.plasma.toml` here first; add your blocks to the end of that file.
2. `~/.config/keycap/keycap.toml`, which a desktop forwards as `UserConfig` (sill does; the
   Plasma focus feed does not, so on Plasma use the machine file).

`keycap --print-config` prints the built-in defaults to copy from. A bad value costs only its
own key and is logged.

## Say what layout a keyboard has

Keyboards with a Mac/Windows switch (Keychron and others) send the same vendor and product id in
both positions, and the host cannot tell which one the switch is on, so keycap treats them as PC
keyboards until you say otherwise. Find the ids with `keycap status` (the `3434:0a53` part) or
by name, then:

```toml
# Keychron K5 Max with its switch on Mac, over USB or Bluetooth
[[device]]
match = "3434:0a53"        # vendor:product in hex, or the exact device name
layout = "apple"           # "apple" or "pc"
```

The name differs by link (`Keychron K5 Max Keyboard` over Bluetooth, `Keychron Keychron K5 Max`
over USB); the id is the same, so prefer it. Takes effect when the file is saved. If Cmd and
Option behave swapped, the layout is wrong.

## Caps Lock as the input-method switch

Like macOS: Caps Lock switches the input source (fcitx5's trigger is Ctrl+Space), Shift+Caps is
the real Caps Lock.

```toml
[profile.default]
rules = [
  { when = "capslock", then = "ctrl+space" },
  { when = "shift+capslock", then = "capslock" },
]
```
