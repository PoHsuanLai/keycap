# Personal configuration

Your own choices go in a TOML file. keycap reads two places you edit, the later overriding the earlier. A third, `/usr/lib/keycap/keycap.toml`, is
what a desktop ships and lies under both; leave it alone and override it here.

1. `/etc/keycap/keycap.toml`: machine-wide, applies at the login screen too, reloaded when
   saved. **Every session on the machine shares it**, whichever desktop it runs. On Plasma,
   `install.sh` puts `dist/keycap.plasma.toml` here first; it already has a `[profile.default]`
   table, so add your rules to the end of its `rules = [...]` list and put `[[device]]` blocks at
   the end of the file.
2. `~/.config/keycap/keycap.toml`, if your desktop forwards it: this layer is yours alone and
   exists only while that desktop's session runs. The Plasma focus feed does not forward it, so
   on Plasma use the machine file. sill does forward it.

## What your own file may say

When a desktop forwards `~/.config/keycap/keycap.toml`, keycap keeps from it only rules that need a
modifier held (Command, Option, Control or Shift) and write one stroke, and at most 256 of them; the rest
are reported and left out. A rule on a key pressed alone, a rule whose `then` is a list of strokes or
`[]`, and a long list all belong in `/etc/keycap/keycap.toml`, which is not cut down. An administrator who
wants lists in your file too sets this in the machine file:

```toml
[user_layer]
sequences = "allow"
```

## Which file, when you use more than one desktop

Put a rule in the machine file when every session should have it: a keyboard's `[[device]]`
layout, Caps Lock as the input switch. Put it in `~/.config/keycap/keycap.toml` when it is about
one desktop, because the machine file may be the Plasma overlay and the Plasma shortcuts are
wrong elsewhere. Cmd+Space, for example, is Alt+Space on Plasma and Super+Space on sill:

```toml
# ~/.config/keycap/keycap.toml, read only by a desktop that forwards it (sill)
[profile.default]
rules = [
  { when = "command+space", then = "super+space" },    # the launcher, not KRunner
  { when = "command+shift+3", then = "print" },        # screenshot
  { when = "command+shift+4", then = "print" },
  { when = "command+shift+5", then = "print" },
  { when = "command+h", then = "ctrl+h" },             # plain Ctrl+H, not Plasma's minimise
]
```

A layer overrides per trigger: only the chords it lists change, the rest of the machine file
stays. Plasma never reads this file, so the same machine keeps `alt+space` there. sill ships
these rules in its `dist/keycap/keycap.toml`; you only write the file yourself to change them.
To see the result without touching a keyboard, `keycap replay --config FILE script.txt` lays one
file over the built-in mapping; to try both layers, paste the user file's rules after the machine
file's in a scratch file and replay that.

`keycap --print-config` prints the built-in defaults to copy from. A bad value is logged and
ignored.

## Chords that keep a modifier down: `hold`

An app switcher stays open while its modifier is held, and closes the moment it goes up. A rule
whose output is a single stroke takes `hold`:

| `hold` | The stroke's modifiers |
| --- | --- |
| `restore` (default) | come up with the key; the held roles' own outputs are put back |
| `while_held` | belong to the Command or Option or Control key you are holding, and come up only when it does |

With `while_held`, holding Cmd and pressing Tab, Tab, Shift+Tab sends the modifier once, then
Tab, Tab, Shift+Tab under it, with no Ctrl in between; releasing Cmd releases the modifier. Any
chord written with a held non-Shift role works, so a desktop whose switcher is on Super says:

```toml
# the switcher on Super+Tab, open while Cmd is held
[profile.default]
rules = [
  { when = "command+tab", then = "super+tab", hold = "while_held" },
  { when = "command+shift+tab", then = "super+shift+tab", hold = "while_held" },
]
```

Without `hold`, Super goes up after each Tab and the switcher commits at once. A trigger with
no held non-Shift role (`shift+y`) has nothing to own the modifier and behaves as `restore`.

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
the real Caps Lock. A desktop that ships keycap's defaults (sill does) already has the first rule below in
`/usr/lib/keycap/keycap.toml`. To change it, or on a machine without that desktop, put this in the
machine file, `/etc/keycap/keycap.toml`: a desktop that forwards
your own file has it cut down on arrival, and a rule on Caps Lock alone, with no modifier held, is
left out of it (keycap says so). `shift+capslock` is fine in either file.

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

## Apps that read the keymap themselves

temor and every quire app resolve Cmd through chordkit, so they must see it unchanged: the
`terminal` profile writes Cmd+D and Cmd+Shift+D both as Ctrl+Shift+D, and a keymap-aware terminal
could not tell a right split from a downward one. The `keymap-aware` profile writes Cmd as Super
and leaves Cmd+letter alone. temor gets it by default. To give it to another app, have the desktop
send that profile name for the app (on Plasma add its class to `OWN_PROFILES` in
`dist/plasma/keycap-focus`; on sill an app declares `X-Keycap-Profile=keymap-aware` in its
desktop entry, or you override it in sill's settings). To change one chord for those apps:

```toml
[profile.keymap-aware]
rules = [
  { when = "command+h", then = "super+h" },   # Cmd+H reaches the app instead of the desktop's rule
]
```
