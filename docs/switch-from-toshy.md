# Switching from Toshy to keycap

Written for KDE Plasma 6. The order below keeps two remappers from fighting over the keyboard: a
keyboard is held by one program at a time, so keycap goes in first and waits, Toshy comes off,
and keycap takes over.

**Emergency chord: hold both Shift keys and press Esc.** keycap gives the keyboards back to the
kernel at once; `keycap resume` takes them again. `sudo systemctl stop keycap` does the same
until the next start. Keep a terminal open throughout, and remember that Cmd is Ctrl and Ctrl is
Super while keycap runs.

## 1. Preview

```bash
./install.sh --dry-run --desktop plasma --plasma-focus
```

This prints every command and runs none. `--desktop plasma` installs Plasma's shortcut set as
`/etc/keycap/keycap.toml` if you have none (it never overwrites yours). `--plasma-focus` installs
the focus feed, which tells keycap when a terminal has focus. **Do not run keycap without it:
Cmd+C in Konsole would be Ctrl+C.** It needs `python3-gobject` (Fedora: `sudo dnf install
python3-gobject`).

## 2. Install while Toshy is still running

```bash
./install.sh --desktop plasma --plasma-focus
```

Toshy is not touched. Note that `/etc/keycap/keycap.toml` is the machine file: every session
shares it, so it holds the Plasma shortcuts (Cmd+Space is KRunner, Cmd+H minimises,
Cmd+Shift+3/4 are Spectacle). If you also log in to a desktop that forwards
`~/.config/keycap/keycap.toml` (sill does), that desktop's rules go in your own file, which
overrides the machine file in that session only; see
[personal-config.md](personal-config.md#which-file-when-you-use-more-than-one-desktop).

Then add your own settings to `/etc/keycap/keycap.toml` (`sudoedit`; changes apply on save): the
keyboard layout and, if Toshy made Caps Lock switch your input
method, the Caps Lock rules. Both are explained in [personal-config.md](personal-config.md).

## 3. Check

```bash
keycap status
```

Because Toshy still holds the keyboard, expect:

```
keycap: running
  Keychron K5 Max Keyboard (/dev/input/eventNN, 3434:0a53): held by another remapper
```

`keycap status` exits 1 until a keyboard is grabbed; that is not an error here. Also check that
`systemctl status keycap` and `systemctl --user status keycap-focus` are active.

## 4. Switch

```bash
systemctl --user disable --now toshy-session-monitor.service
systemctl --user disable --now toshy-config.service toshy-kwin-dbus.service \
    toshy-wlroots-dbus.service
sudo systemctl restart keycap
keycap status                      # must say: grabbed, remapping
```

The keyboard is raw for a few seconds in between, and Caps Lock may have been toggled on. If
letters come out in capitals, press Shift+Caps Lock once.

Then stop Toshy starting at login:

```bash
mkdir -p ~/.config/autostart.toshy-off
mv ~/.config/autostart/Toshy_Tray.desktop ~/.config/autostart/Toshy_Systemd_Service_Kickstart.desktop \
   ~/.config/autostart/Toshy_Import_Vars.desktop ~/.config/autostart.toshy-off/
pkill -u "$USER" -f toshy_tray.py
```

Nothing here edits Toshy's own files.

## 5. Verify

1. `pgrep -a xwaykeyz` prints nothing; `keycap status` says `grabbed, remapping`.
2. Click one window, then another, so the focus feed sends its first message.
3. Konsole: Ctrl+C stops `sleep 100`; Cmd+C and Cmd+V copy and paste; Cmd+D and Cmd+Z do not end
   or suspend the shell.
4. Kate or Firefox: Cmd+C/V/A/Z, Cmd+Left/Right (line start/end), Opt+Left/Right (word),
   Cmd+Backspace (delete to line start).
5. Cmd+Tab, Cmd+`, Cmd+Q, Cmd+Space (KRunner).
6. Dolphin: Cmd+Backspace trashes the selection.
7. If you set up Caps Lock as the input switch: it switches input, Shift+Caps Lock toggles the
   real lock.
8. Shift+Shift+Esc pauses keycap (`keycap status` says paused); `keycap resume` restores it.
9. Reconnect or sleep the keyboard: it is grabbed again a few seconds later.
10. Reboot: the keyboard is mapped at the login screen.

Toshy's forced numpad is not replicated; set "NumLock on Plasma Startup" in System Settings,
Keyboard, Hardware if you want it.

Not covered: Toshy's app-specific keymaps beyond terminals and file managers (browsers, VS Code,
Sublime, ...).

## 6. Rolling back

```bash
./uninstall.sh --restore-toshy --plasma-focus     # add --purge to delete /etc/keycap too
mv ~/.config/autostart.toshy-off/*.desktop ~/.config/autostart/ && rmdir ~/.config/autostart.toshy-off
nohup ~/.local/bin/toshy-tray >/dev/null 2>&1 &
```

To pause keycap without removing it:

```bash
sudo systemctl disable --now keycap.service
systemctl --user enable --now toshy-config.service toshy-kwin-dbus.service toshy-session-monitor.service
```

## Troubleshooting

| You see | What to do |
| --- | --- |
| `held by another remapper` | Stop the other remapper (`pgrep -a xwaykeyz`), then `sudo systemctl restart keycap` |
| Cmd+C in Konsole interrupts | The focus feed is not reaching keycap: check `systemctl --user status keycap-focus`, click another window, and make sure the KWin script is enabled in System Settings, Window Management, KWin Scripts |
| Cmd and Option swapped | Add the `[[device]]` layout entry, with the keyboard's switch on Mac |
| `cannot connect` | `systemctl status keycap`, `journalctl -u keycap -b` |
| No keyboard listed | A Bluetooth keyboard is asleep; wake it |
| Nothing mapped after the chord | `keycap resume` |
| Capital letters, Caps Lock stuck | Press Shift+Caps Lock once |
