# Switching from Toshy to keycap (Plasma 6, Keychron K5 Max over Bluetooth)

The aim: at no point do two remappers fight over the keyboard, and the time with none is a few
seconds. Two cannot both hold it anyway: a keyboard is grabbed by one program at a time, and
keycap reports `held by another remapper` and waits instead of grabbing. The order below makes
use of that: keycap goes in first and waits, Toshy comes off, keycap takes over.

**Emergency chord: hold both Shift keys and press Esc.** keycap gives the keyboards back to the
kernel at once (raw keys, no remapping). `keycap resume` takes them again. If keys misbehave in
any other way, `sudo systemctl stop keycap` does the same for good; a crash or `kill -9` does it
too, since the kernel drops the grab with the process. Keep a terminal open for this whole
procedure, and know that Cmd is Ctrl and Ctrl is Super while keycap runs.

Before you start: the Magic Mouse is not touched (keycap grabs only nodes that have letter keys;
the K5 Max's own `Mouse` node and the consumer-control bits are left alone too).

## 1. Look at what will happen (changes nothing)

```bash
cd ~/keycap          # the checkout with this branch merged
./install.sh --dry-run --desktop plasma --plasma-focus
```

It prints every command it would run and runs none (it does not even build or ask for sudo).
Read it. `--desktop plasma` installs `dist/keycap.plasma.toml` as `/etc/keycap/keycap.toml` if
you have none (it never overwrites yours): Cmd+Space opens KRunner, plus the Plasma equivalents
of macOS shortcuts (Cmd+Ctrl+F, Cmd+Ctrl+Space for emoji, Cmd+H, Cmd+Shift+3/4/5, Ctrl+Left/Right
for desktops). Two things Toshy did for you are personal, so they are not shipped; step 2 adds them.
`--plasma-focus` installs the focus feed: without it, keycap does not know a terminal has
focus, and **Cmd+C in Konsole would be Ctrl+C (SIGINT)**. Do not run keycap without it.

Needs `python3-gobject` for the focus feed (`python3 -c "import gi"`; Fedora: `sudo dnf install python3-gobject`).

## 2. Install keycap while Toshy is still running

```bash
./install.sh --desktop plasma --plasma-focus
```

It builds, copies the files (with sudo), reloads systemd and udev, and starts `keycap.service`
and the user unit `keycap-focus.service`. Running it again later changes nothing that is already
in place. Toshy is not touched.

### Your own settings (K5 Max layout, Caps Lock)

keycap cannot see the K5 Max's Mac/Windows switch, so it treats the keyboard as a PC one
(Cmd and Option swapped) until you say it is a Mac one. Append this to `/etc/keycap/keycap.toml`
(`sudoedit`; keycap re-reads the file when it changes, no restart needed), together with the
Caps Lock block if you want Caps Lock to switch fcitx5 as it did under Toshy:

```toml
[[device]]
match = "3434:0a53"      # Keychron K5 Max, USB and Bluetooth; layout "apple" = switch on Mac
layout = "apple"

[profile.default]
rules = [
  { when = "capslock", then = "ctrl+space" },
  { when = "shift+capslock", then = "capslock" },
]
```

(`docs/personal-config.md` explains both.) Do this before step 4: on step 3, `keycap status`
should already show the keyboard, and Cmd must be the key beside the space bar after the switch.

## 3. Check it, still before switching

```bash
keycap status
```

Expected, because Toshy still holds the keyboard:

```
keycap: running
  Keychron K5 Max Keyboard (/dev/input/eventNN, 3434:0a53): held by another remapper (Toshy, keyd, ...)
```

(`keycap status` exits 1 until a keyboard is grabbed; that is not an error here.) The Magic
Mouse and `Keychron K5 Max Mouse` must not appear. Also `systemctl status keycap` is `active
(running)`, `systemctl --user status keycap-focus` is active, and `journalctl -u keycap -b`
shows `keycap: ready`.

## 4. Switch: Toshy off, keycap on (do these in one go)

Stop the session monitor first, so nothing restarts Toshy behind you, then the rest:

```bash
systemctl --user disable --now toshy-session-monitor.service
systemctl --user disable --now toshy-config.service toshy-kwin-dbus.service \
    toshy-cosmic-dbus.service toshy-wlroots-dbus.service
```

The five units are `toshy-config`, `toshy-cosmic-dbus`, `toshy-kwin-dbus`,
`toshy-session-monitor` and `toshy-wlroots-dbus` (the cosmic and wlroots ones are already
inactive on Plasma; disabling them keeps them from starting on another desktop). The
keyboard is raw from now until the next command; typing still works.

```bash
sudo systemctl restart keycap      # take the keyboard now, not after keycap's retry delay
keycap status                      # must say: grabbed, remapping
```

Then keep Toshy from coming back at the next login. Its autostart entries run the tray and a
"kickstart" that restarts Toshy's services (it only fires when the units are enabled, so it is
inert now, but move all three aside so nothing starts Toshy by surprise):

```bash
mkdir -p ~/.config/autostart.toshy-off
mv ~/.config/autostart/Toshy_Tray.desktop ~/.config/autostart/Toshy_Systemd_Service_Kickstart.desktop \
   ~/.config/autostart/Toshy_Import_Vars.desktop ~/.config/autostart.toshy-off/
systemctl --user stop app-Toshy_Tray@autostart.service 2>/dev/null; pkill -u "$USER" -f toshy_tray.py
```

(`pkill` kills only Toshy's tray, which runs as you.) Leave Toshy's KWin script
`toshy-dbus-notifyactivewindow` and your other KWin scripts (`macfullscreenspaces`, ...) alone:
the first now talks to nobody, which is harmless, and the second is what Cmd+Ctrl+F relies on.

Nothing here edits Toshy's files; `~/.config/toshy` stays exactly as it was.

## 5. Verify

Run these in order; each takes seconds.

1. `pgrep -a xwaykeyz` prints nothing; `keycap status` says `grabbed, remapping` for the K5 Max.
2. Click one window, then another (this is the first focus message the feed sends; if you
   restarted `keycap-focus` or keycap, do this again).
3. Konsole: `sleep 100` then Ctrl+C stops it (physical Control stays Control); select text,
   Cmd+C copies, Cmd+V pastes.
4. Kate or Firefox: Cmd+C / Cmd+V / Cmd+A / Cmd+Z; Cmd+Left / Cmd+Right (line start / end);
   Opt+Left / Opt+Right (word); Opt+Backspace; Cmd+Backspace (delete to line start).
5. Cmd+Tab (hold Cmd, press Tab repeatedly, release Cmd), Cmd+` (windows of one app), Cmd+Q
   (closes the window), Cmd+Space (KRunner), Cmd+W.
6. Dolphin: Cmd+Backspace moves the selection to the trash; Cmd+Left goes back.
7. Physical Control+Space switches the input method (fcitx5); with your Caps Lock block, Caps
   Lock does too and Shift+Caps toggles real Caps Lock.
8. Press and hold Shift, Shift, Esc together: the keyboard goes raw (Cmd+C does nothing useful).
   `keycap status` says paused. `keycap resume` brings the mapping back.
9. Put the keyboard to sleep (wait, or turn the K5 Max off and on, or switch it to another
   Bluetooth host and back): `keycap status` shows it grabbed again a few seconds after it
   reconnects, with no key stuck. Also `journalctl -u keycap -b` has no repeated errors.
10. Reboot. At the login screen (the greeter) the keyboard should already be mapped (`journalctl -u
    keycap -b`); after login repeat step 3. If the K5 Max is invisible before login, that is
    the Bluetooth-keyboard limitation in your notes, not keycap.
11. NumLock: Toshy's "forced numpad" made the numpad always a numpad. Set it in System
    Settings, Keyboard, Hardware, "NumLock on Plasma Startup: Turn on" if you want that.

Known differences from Toshy: app-specific keymaps beyond terminals and file managers (browsers'
Cmd+Shift+[ and Cmd+R, VS Code, Sublime, ...) do not exist; a terminal embedded in another
window (an editor's panel) gets the window's profile, not `terminal`.

## 6. Rolling back to Toshy

keycap off first, Toshy on second, so there are never two:

```bash
cd ~/keycap
./uninstall.sh --restore-toshy --plasma-focus     # add --purge to delete /etc/keycap too
```

That stops and disables keycap (the keyboards go straight back to the kernel), removes its
files and the focus feed, then enables and starts the five Toshy units in order. Then put the
autostart entries back and restart the tray:

```bash
mv ~/.config/autostart.toshy-off/*.desktop ~/.config/autostart/ && rmdir ~/.config/autostart.toshy-off
nohup ~/.local/bin/toshy-tray >/dev/null 2>&1 &
systemctl --user status toshy-config.service     # active
```

Rolling back without removing keycap (to try again later):

```bash
sudo systemctl disable --now keycap.service
systemctl --user enable --now toshy-config.service toshy-kwin-dbus.service toshy-session-monitor.service
```

If you are mid-switch and something is wrong: the emergency chord, then `sudo systemctl stop
keycap`, then the second command above.

## 7. If something looks wrong

| You see | Meaning | What to do |
| --- | --- | --- |
| `held by another remapper` after step 4 | Toshy (or keyd) still has the keyboard | `pgrep -a xwaykeyz`; stop it; `sudo systemctl restart keycap` |
| Cmd+C in Konsole interrupts | the focus feed is not reaching keycap | `systemctl --user status keycap-focus`; click another window; `journalctl --user -u keycap-focus`; the KWin script is enabled in System Settings, Window Management, KWin Scripts |
| Cmd and Option are swapped | keycap has no `[[device]]` layout for the K5 Max, or its side switch is on Windows | add the `[[device]]` block above, with the switch on Mac |
| `keycap status`: cannot connect | the service is not running | `systemctl status keycap`, `journalctl -u keycap -b` |
| no keyboard listed | the K5 Max is asleep or off | wake it; it appears when it reconnects |
| nothing mapped after the chord | paused | `keycap resume` |
