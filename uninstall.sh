#!/usr/bin/env bash
# Remove keycap: the opposite of install.sh, safe to run again.
#
#   ./uninstall.sh [--dry-run] [--purge] [--plasma-focus] [--restore-toshy]
#
#   --dry-run        print every command, change nothing
#   --purge          also remove /etc/keycap (your machine config)
#   --plasma-focus   also remove the Plasma focus feed (user unit, helper, KWin script)
#   --restore-toshy  afterwards, enable and start Toshy's user units again. Without it, no
#                    remapper runs once keycap is gone: the keyboard falls back to its raw keys.
#
# Order matters: with --restore-toshy, keycap is stopped first (it lets go of the keyboards) and
# Toshy started after, so there is never a moment with two remappers.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib.sh
source "$HERE/scripts/lib.sh"

PURGE=no
PLASMA_FOCUS=no
RESTORE=no
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run) DRY_RUN=yes ;;
    --purge) PURGE=yes ;;
    --plasma-focus) PLASMA_FOCUS=yes ;;
    --restore-toshy) RESTORE=yes ;;
    -h|--help) sed -n '2,13p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) say "uninstall.sh: unknown argument $1 (try --help)"; exit 2 ;;
  esac
  shift
done

TOSHY_UNITS=(toshy-config.service toshy-kwin-dbus.service
  toshy-session-monitor.service toshy-wlroots-dbus.service)

say "keycap uninstall ($([[ $DRY_RUN == yes ]] && echo 'dry run: nothing is changed' || echo 'for real'))"

say "1. stop the service"
if unit_known keycap.service; then
  system_step "stop and disable keycap (the keyboards go back to the kernel)" systemctl disable --now keycap.service
  system_step "forget a failed state, if any" systemctl reset-failed keycap.service
else
  say "  keycap.service is not installed"
fi

say "2. files"
remove_file /usr/local/bin/keycap
remove_file /etc/systemd/system/keycap.service
remove_file /etc/udev/rules.d/70-keycap.rules
remove_file /etc/polkit-1/rules.d/keycap.polkit.rules
if [[ "$PURGE" == yes ]]; then
  if [[ -d "$(dest /etc/keycap)" ]]; then
    root_step "remove /etc/keycap" rm -rf "$(dest /etc/keycap)"
  else
    say "  absent: /etc/keycap"
  fi
else
  say "  kept /etc/keycap (use --purge to remove it)"
fi
if [[ "$CHANGED" == yes ]]; then
  system_step "reload systemd" systemctl daemon-reload
  system_step "reload udev rules" udevadm control --reload-rules
fi

if [[ "$PLASMA_FOCUS" == yes ]]; then
  say "3. Plasma focus feed"
  step "stop and disable the helper" systemctl --user disable --now keycap-focus.service || true
  step "remove its unit" rm -f "$HOME/.config/systemd/user/keycap-focus.service"
  step "remove the helper" rm -f "$HOME/.local/bin/keycap-focus"
  step "reload the user manager" systemctl --user daemon-reload
  step "disable the KWin script" kwriteconfig6 --file kwinrc --group Plugins --key keycap-focusEnabled false
  step "remove the KWin script" kpackagetool6 --type=KWin/Script -r keycap-focus || true
  step "make KWin reload" gdbus call --session --dest org.kde.KWin --object-path /KWin --method org.kde.KWin.reconfigure || true
fi

if [[ "$RESTORE" == yes ]]; then
  say "4. Toshy"
  for unit in "${TOSHY_UNITS[@]}"; do
    step "enable and start $unit" systemctl --user enable --now "$unit" || true
  done
  say "  Toshy's tray and autostart entries, if you moved them aside, go back with the steps in"
  say "  docs/switch-from-toshy.md (Rolling back)."
else
  say "No remapper is running now. To go back to Toshy: ./uninstall.sh --restore-toshy"
fi
say "done."
