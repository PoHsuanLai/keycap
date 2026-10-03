#!/usr/bin/env bash
# Build keycap and install it: the binary, the systemd unit, the udev and polkit rules, and
# (for Plasma) the machine config. Safe to run again: a file already in place and identical is
# left alone, and /etc/keycap/keycap.toml is never overwritten.
#
#   ./install.sh [--dry-run] [--desktop plasma|cosmic|none] [--plasma-focus] [--no-start]
#
#   --dry-run       print every command, change nothing (does not build, does not use sudo)
#   --desktop D     which machine config to install: plasma (dist/keycap.plasma.toml, the
#                   default when $XDG_CURRENT_DESKTOP mentions KDE) or cosmic/none (the built-in
#                   mapping alone)
#   --plasma-focus  also install the Plasma focus feed (terminal vs default profile): the
#                   keycap-focus service for this user and its KWin script
#   --no-start      install and enable the service but do not start it now
#   --no-build      use the binary that is already built (or $KEYCAP_BIN)
#
# It does not stop Toshy or any other remapper; docs/switch-from-toshy.md has the order.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib.sh
source "$HERE/scripts/lib.sh"

DESKTOP=""
PLASMA_FOCUS=no
START=yes
BUILD=yes
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run) DRY_RUN=yes ;;
    --desktop) DESKTOP="${2:?--desktop needs plasma, cosmic or none}"; shift ;;
    --plasma-focus) PLASMA_FOCUS=yes ;;
    --no-start) START=no ;;
    --no-build) BUILD=no ;;
    -h|--help) sed -n '2,18p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) say "install.sh: unknown argument $1 (try --help)"; exit 2 ;;
  esac
  shift
done

if [[ -z "$DESKTOP" ]]; then
  case "${XDG_CURRENT_DESKTOP:-}" in
    *KDE*) DESKTOP=plasma ;;
    *) DESKTOP=none ;;
  esac
fi
case "$DESKTOP" in plasma|cosmic|none) ;; *) say "install.sh: --desktop must be plasma, cosmic or none"; exit 2 ;; esac

BIN="${KEYCAP_BIN:-$HERE/target/release/keycap}"
say "keycap install ($([[ $DRY_RUN == yes ]] && echo 'dry run: nothing is changed' || echo 'for real')); desktop: $DESKTOP"

say "1. build"
if [[ "$BUILD" == no ]]; then
  say "  skipped (--no-build): using $BIN"
elif [[ "$DRY_RUN" == yes ]]; then
  say "  would run: cargo build --release (in $HERE)"
else
  (cd "$HERE" && cargo build --release)
fi

say "2. files"
install_file 755 "$BIN" /usr/local/bin/keycap
BIN_CHANGED=$CHANGED; CHANGED=no
install_file 644 "$HERE/dist/keycap.service" /etc/systemd/system/keycap.service
UNIT_CHANGED=$CHANGED; CHANGED=no
install_file 644 "$HERE/dist/70-keycap.rules" /etc/udev/rules.d/70-keycap.rules
UDEV_CHANGED=$CHANGED; CHANGED=no
install_file 644 "$HERE/dist/keycap.polkit.rules" /etc/polkit-1/rules.d/keycap.polkit.rules
CHANGED=no

say "3. machine config (/etc/keycap/keycap.toml, never overwritten)"
if [[ "$DESKTOP" == plasma ]]; then
  if [[ -e "$(dest /etc/keycap/keycap.toml)" ]]; then
    if cmp -s "$HERE/dist/keycap.plasma.toml" "$(dest /etc/keycap/keycap.toml)"; then
      say "  unchanged: /etc/keycap/keycap.toml"
    else
      say "  kept your /etc/keycap/keycap.toml (it differs from dist/keycap.plasma.toml; diff them if you want the new lines)"
    fi
  else
    root_step "install /etc/keycap/keycap.toml (Plasma shortcuts)" \
      install -Dm644 "$HERE/dist/keycap.plasma.toml" "$(dest /etc/keycap/keycap.toml)"
  fi
else
  say "  none: the built-in mapping applies"
fi

say "4. systemd, udev, SELinux"
if [[ "$BIN_CHANGED$UNIT_CHANGED$UDEV_CHANGED" == *yes* ]] || ! unit_known keycap.service; then
  command -v restorecon >/dev/null 2>&1 &&
    system_step "give the installed files their default SELinux labels" \
      restorecon -F /usr/local/bin/keycap /etc/systemd/system/keycap.service \
        /etc/udev/rules.d/70-keycap.rules /etc/polkit-1/rules.d/keycap.polkit.rules
  system_step "reload systemd" systemctl daemon-reload
  [[ "$UDEV_CHANGED" == yes ]] &&
    system_step "reload udev rules" udevadm control --reload-rules &&
    system_step "re-apply them to /dev/uinput" udevadm trigger --subsystem-match=misc
else
  say "  nothing changed; nothing to reload"
fi

say "5. service"
if [[ "$START" == no ]]; then
  system_step "enable keycap (not started)" systemctl enable keycap.service
elif [[ -z "$DESTDIR" && "$BIN_CHANGED$UNIT_CHANGED" == *yes* ]] && systemctl is-active --quiet keycap.service; then
  # `enable --now` leaves a running service alone, so a new binary or unit needs a restart. A
  # changed /etc/keycap/keycap.toml does not: the daemon watches that directory and reloads it.
  system_step "a new binary or unit: restart so it runs (the grab gate waits for keys to be up)" systemctl restart keycap.service
else
  say "  keycap starts alongside any running remapper: it reports 'held by another remapper'"
  say "  and takes the keyboards when that one lets go (docs/switch-from-toshy.md)."
  system_step "enable and start keycap" systemctl enable --now keycap.service
fi

if [[ "$PLASMA_FOCUS" == yes ]]; then
  say "6. Plasma focus feed (this user)"
  step "the helper" install -Dm755 "$HERE/dist/plasma/keycap-focus" "$HOME/.local/bin/keycap-focus"
  step "its user unit" install -Dm644 "$HERE/dist/plasma/keycap-focus.service" "$HOME/.config/systemd/user/keycap-focus.service"
  step "reload the user manager" systemctl --user daemon-reload
  # Enable, then restart: `enable --now` leaves a helper that is already running on the old
  # script, and restart starts it when it is not running.
  step "start the helper at every login" systemctl --user enable keycap-focus.service
  step "(re)start the helper now, on the script just installed" systemctl --user restart keycap-focus.service
  if kpackagetool6 --type=KWin/Script --show keycap-focus >/dev/null 2>&1; then
    step "update the KWin script" kpackagetool6 --type=KWin/Script -u "$HERE/dist/plasma/kwin-script"
  else
    step "install the KWin script" kpackagetool6 --type=KWin/Script -i "$HERE/dist/plasma/kwin-script"
  fi
  step "enable the KWin script" kwriteconfig6 --file kwinrc --group Plugins --key keycap-focusEnabled true
  step "make KWin load it" gdbus call --session --dest org.kde.KWin --object-path /KWin --method org.kde.KWin.reconfigure
fi

say "done. Check it:  keycap status   (or: systemctl status keycap)"
say "Emergency: both Shift keys + Esc gives the keyboards back; \`keycap resume\` takes them again."
